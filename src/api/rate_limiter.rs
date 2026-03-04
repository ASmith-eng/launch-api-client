use std::collections::VecDeque;

use chrono::{DateTime, TimeDelta, Utc};

use crate::clock::Clock;
use crate::error::AppError;
use crate::models::RateLimitState;

/// Rolling-window duration: 1 hour.
const WINDOW_SECS: i64 = 3600;

/// Default limit for unauthenticated users.
const DEFAULT_LIMIT_UNAUTH: usize = 15;

/// Default limit for authenticated users.
const DEFAULT_LIMIT_AUTH: usize = 30;

/// Sync when remaining requests are at or below this threshold.
const SYNC_REMAINING_THRESHOLD: usize = 2;

/// Client-side rate limiter using a rolling-window log.
///
/// Tracks timestamps of API requests within a 1-hour rolling window.
/// Generic over `Clock` for deterministic testing.
#[derive(Debug)]
pub struct RateLimiter<C: Clock> {
    clock: C,
    limit: usize,
    requests: VecDeque<DateTime<Utc>>,
    last_sync: Option<DateTime<Utc>>,
    authenticated: bool,
}

impl<C: Clock> RateLimiter<C> {
    /// Create a new rate limiter with no request history.
    pub fn new(clock: C, authenticated: bool) -> Self {
        let limit = if authenticated {
            DEFAULT_LIMIT_AUTH
        } else {
            DEFAULT_LIMIT_UNAUTH
        };
        Self {
            clock,
            limit,
            requests: VecDeque::new(),
            last_sync: None,
            authenticated,
        }
    }

    /// Restore a rate limiter from persisted state.
    pub fn from_state(clock: C, state: &RateLimitState) -> Self {
        let limit = if state.authenticated {
            DEFAULT_LIMIT_AUTH
        } else {
            DEFAULT_LIMIT_UNAUTH
        };
        let mut limiter = Self {
            clock,
            limit,
            requests: VecDeque::from(state.requests.clone()),
            last_sync: Some(state.last_sync),
            authenticated: state.authenticated,
        };
        limiter.prune_expired();
        limiter
    }

    /// Export current state for persistence to `app_state.json`.
    pub fn to_state(&self) -> RateLimitState {
        RateLimitState {
            limit: self.limit,
            requests: self.requests.iter().copied().collect(),
            last_sync: self.last_sync.unwrap_or(self.clock.now()),
            authenticated: self.authenticated,
        }
    }

    /// Whether a request can be made without exceeding the rate limit.
    pub fn can_make_request(&mut self) -> bool {
        self.prune_expired();
        self.requests.len() < self.limit
    }

    /// Record that a request was just made. Returns `Err(AppError::RateLimited)`
    /// if the limit would be exceeded.
    pub fn record_request(&mut self) -> Result<(), AppError> {
        self.prune_expired();
        if self.requests.len() >= self.limit {
            let available = self.next_available_at();
            return Err(AppError::RateLimited(available));
        }
        self.requests.push_back(self.clock.now());
        Ok(())
    }

    /// Number of requests remaining in the current window.
    pub fn remaining(&mut self) -> usize {
        self.prune_expired();
        self.limit.saturating_sub(self.requests.len())
    }

    /// The current rate limit cap.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// When the next request will become available (the earliest time a
    /// request slot opens). Returns the current time if slots are available.
    pub fn next_available_at(&mut self) -> DateTime<Utc> {
        self.prune_expired();
        if self.requests.len() < self.limit {
            return self.clock.now();
        }
        // The oldest request in the window determines when a slot opens.
        match self.requests.front() {
            Some(&oldest) => oldest + TimeDelta::seconds(WINDOW_SECS),
            None => self.clock.now(),
        }
    }

    /// Whether the rate limiter should sync with the `/api-throttle/` endpoint.
    ///
    /// Sync conditions (from design doc §Rate Limit Calibration Strategy):
    /// 1. Never synced before
    /// 2. Remaining requests <= 2
    /// 3. Last sync was more than 1 hour ago
    pub fn should_sync(&mut self) -> bool {
        // Never synced
        let Some(last) = self.last_sync else {
            return true;
        };

        // Close to limit
        if self.remaining() <= SYNC_REMAINING_THRESHOLD {
            return true;
        }

        // More than 1 hour since last sync
        let now = self.clock.now();
        let elapsed = now.signed_duration_since(last);
        if elapsed >= TimeDelta::seconds(WINDOW_SECS) {
            return true;
        }

        false
    }

    /// Update the rate limiter after a successful `/api-throttle/` sync.
    ///
    /// `current_usage` is the number of requests the server reports as used
    /// in the current window. We reconcile by trusting the server count.
    pub fn record_sync(&mut self, server_remaining: usize, server_limit: usize) {
        let now = self.clock.now();
        self.last_sync = Some(now);
        self.limit = server_limit;

        // Reconcile local window with server-reported usage.
        // The server is authoritative — adjust our window to match.
        let server_used = server_limit.saturating_sub(server_remaining);
        self.prune_expired();
        let local_count = self.requests.len();

        if server_used < local_count {
            // Server sees fewer requests than us (some expired server-side).
            // Drop oldest entries to match.
            let to_drop = local_count - server_used;
            for _ in 0..to_drop {
                self.requests.pop_front();
            }
        } else if server_used > local_count {
            // Server sees more requests than us (e.g. another client instance).
            // Add synthetic entries at `now` to match the server count.
            let to_add = server_used - local_count;
            for _ in 0..to_add {
                self.requests.push_back(now);
            }
        }
    }

    /// Update authentication status (e.g. after learning an API key is set).
    pub fn set_authenticated(&mut self, authenticated: bool) {
        self.authenticated = authenticated;
        self.limit = if authenticated {
            DEFAULT_LIMIT_AUTH
        } else {
            DEFAULT_LIMIT_UNAUTH
        };
    }

    /// Remove request timestamps that have fallen outside the rolling window.
    fn prune_expired(&mut self) {
        let cutoff = self.clock.now() - TimeDelta::seconds(WINDOW_SECS);
        while let Some(&front) = self.requests.front() {
            if front <= cutoff {
                self.requests.pop_front();
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::testing::FakeClock;
    use chrono::TimeZone;

    fn base_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 1, 12, 0, 0).unwrap()
    }

    fn make_limiter() -> (RateLimiter<FakeClock>, FakeClock) {
        let clock = FakeClock::new(base_time());
        let limiter = RateLimiter::new(clock.clone(), false);
        (limiter, clock)
    }

    // --- Basic operations ---

    #[test]
    fn new_limiter_has_full_capacity() {
        let (mut limiter, _clock) = make_limiter();
        assert_eq!(limiter.remaining(), 15);
        assert!(limiter.can_make_request());
    }

    #[test]
    fn authenticated_limiter_has_higher_limit() {
        let clock = FakeClock::new(base_time());
        let mut limiter = RateLimiter::new(clock, true);
        assert_eq!(limiter.remaining(), 30);
        assert_eq!(limiter.limit(), 30);
    }

    #[test]
    fn record_request_decrements_remaining() {
        let (mut limiter, _clock) = make_limiter();
        limiter.record_request().unwrap();
        assert_eq!(limiter.remaining(), 14);
        limiter.record_request().unwrap();
        assert_eq!(limiter.remaining(), 13);
    }

    #[test]
    fn recording_15_requests_blocks_16th() {
        let (mut limiter, _clock) = make_limiter();
        for _ in 0..15 {
            limiter.record_request().unwrap();
        }
        assert_eq!(limiter.remaining(), 0);
        assert!(!limiter.can_make_request());

        let result = limiter.record_request();
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::RateLimited(at) => {
                // Should be base_time + 1 hour (when the first request expires)
                assert_eq!(at, base_time() + TimeDelta::hours(1));
            }
            other => panic!("expected RateLimited, got: {other:?}"),
        }
    }

    // --- Window expiry ---

    #[test]
    fn requests_older_than_one_hour_are_pruned() {
        let (mut limiter, clock) = make_limiter();
        // Record 10 requests
        for _ in 0..10 {
            limiter.record_request().unwrap();
        }
        assert_eq!(limiter.remaining(), 5);

        // Advance 1 hour — all requests should expire
        clock.advance(TimeDelta::hours(1));
        assert_eq!(limiter.remaining(), 15);
        assert!(limiter.can_make_request());
    }

    #[test]
    fn partial_expiry_frees_correct_slots() {
        let (mut limiter, clock) = make_limiter();

        // Record 5 requests at T+0
        for _ in 0..5 {
            limiter.record_request().unwrap();
        }

        // Advance 30 minutes, record 5 more
        clock.advance(TimeDelta::minutes(30));
        for _ in 0..5 {
            limiter.record_request().unwrap();
        }
        assert_eq!(limiter.remaining(), 5);

        // Advance another 31 minutes (total 61 min from start)
        // The first 5 requests should expire, the second 5 should remain
        clock.advance(TimeDelta::minutes(31));
        assert_eq!(limiter.remaining(), 10);
    }

    // --- next_available_at ---

    #[test]
    fn next_available_at_returns_now_when_slots_available() {
        let (mut limiter, _clock) = make_limiter();
        assert_eq!(limiter.next_available_at(), base_time());
    }

    #[test]
    fn next_available_at_returns_correct_time_when_full() {
        let (mut limiter, clock) = make_limiter();

        // Record 5 requests at T+0
        for _ in 0..5 {
            limiter.record_request().unwrap();
        }
        // Record 10 requests at T+10min
        clock.advance(TimeDelta::minutes(10));
        for _ in 0..10 {
            limiter.record_request().unwrap();
        }
        assert_eq!(limiter.remaining(), 0);

        // Next available should be when the oldest request (T+0) expires
        let expected = base_time() + TimeDelta::hours(1);
        assert_eq!(limiter.next_available_at(), expected);
    }

    // --- should_sync ---

    #[test]
    fn should_sync_when_never_synced() {
        let (mut limiter, _clock) = make_limiter();
        assert!(limiter.should_sync());
    }

    #[test]
    fn should_not_sync_after_recent_sync_with_capacity() {
        let (mut limiter, _clock) = make_limiter();
        limiter.record_sync(15, 15);
        assert!(!limiter.should_sync());
    }

    #[test]
    fn should_sync_when_close_to_limit() {
        let (mut limiter, _clock) = make_limiter();
        limiter.record_sync(15, 15);

        // Use up requests until remaining <= 2
        for _ in 0..13 {
            limiter.record_request().unwrap();
        }
        assert_eq!(limiter.remaining(), 2);
        assert!(limiter.should_sync());
    }

    #[test]
    fn should_sync_after_one_hour() {
        let (mut limiter, clock) = make_limiter();
        limiter.record_sync(15, 15);
        assert!(!limiter.should_sync());

        clock.advance(TimeDelta::hours(1));
        assert!(limiter.should_sync());
    }

    // --- record_sync ---

    #[test]
    fn record_sync_updates_last_sync_and_limit() {
        let (mut limiter, clock) = make_limiter();
        clock.advance(TimeDelta::minutes(5));
        limiter.record_sync(28, 30);

        assert_eq!(limiter.limit(), 30);
        assert_eq!(limiter.remaining(), 28);
        // Should not need sync immediately after syncing
        assert!(!limiter.should_sync());
    }

    #[test]
    fn record_sync_drops_excess_local_entries() {
        let (mut limiter, _clock) = make_limiter();
        // Record 10 requests locally
        for _ in 0..10 {
            limiter.record_request().unwrap();
        }
        assert_eq!(limiter.remaining(), 5);

        // Server says only 3 are used — drop 7 local entries
        limiter.record_sync(12, 15);
        assert_eq!(limiter.remaining(), 12);
    }

    #[test]
    fn record_sync_adds_synthetic_entries_when_server_sees_more() {
        let (mut limiter, _clock) = make_limiter();
        // No local requests, but server says 5 are used
        limiter.record_sync(10, 15);
        assert_eq!(limiter.remaining(), 10);
    }

    // --- Persistence round-trip ---

    #[test]
    fn to_state_and_from_state_round_trip() {
        let (mut limiter, clock) = make_limiter();
        limiter.record_request().unwrap();
        clock.advance(TimeDelta::minutes(5));
        limiter.record_request().unwrap();
        limiter.record_sync(13, 15);

        let state = limiter.to_state();
        assert_eq!(state.requests.len(), 2);
        assert_eq!(state.limit, 15);
        assert!(!state.authenticated);

        // Restore from state
        let clock2 = FakeClock::new(clock.now());
        let mut restored = RateLimiter::from_state(clock2, &state);
        assert_eq!(restored.remaining(), 13);
        assert!(!restored.should_sync()); // last_sync was just set
    }

    #[test]
    fn from_state_prunes_expired_requests() {
        let clock = FakeClock::new(base_time());
        let state = RateLimitState {
            limit: 15,
            requests: vec![
                // 2 hours ago — should be pruned
                base_time() - TimeDelta::hours(2),
                // 30 minutes ago — should remain
                base_time() - TimeDelta::minutes(30),
            ],
            last_sync: base_time() - TimeDelta::hours(2),
            authenticated: false,
        };

        let mut limiter = RateLimiter::from_state(clock, &state);
        assert_eq!(limiter.remaining(), 14); // only 1 request in window
    }

    // --- set_authenticated ---

    #[test]
    fn set_authenticated_updates_limit() {
        let (mut limiter, _clock) = make_limiter();
        assert_eq!(limiter.limit(), 15);

        limiter.set_authenticated(true);
        assert_eq!(limiter.limit(), 30);
        assert_eq!(limiter.remaining(), 30);

        limiter.set_authenticated(false);
        assert_eq!(limiter.limit(), 15);
    }

    // --- Edge cases ---

    #[test]
    fn clock_backward_jump_does_not_block_forever() {
        let (mut limiter, clock) = make_limiter();
        // Record some requests
        for _ in 0..5 {
            limiter.record_request().unwrap();
        }

        // Jump clock backward by 30 minutes
        clock.set(base_time() - TimeDelta::minutes(30));

        // All 5 requests are now "in the future" relative to clock —
        // prune_expired uses `front < cutoff` where cutoff = now - 1h,
        // so future timestamps won't be pruned. But they should still
        // be counted toward the limit.
        assert_eq!(limiter.remaining(), 10);
        assert!(limiter.can_make_request());
    }

    #[test]
    fn empty_limiter_next_available_at_returns_now() {
        let (mut limiter, _clock) = make_limiter();
        assert_eq!(limiter.next_available_at(), base_time());
    }

    #[test]
    fn to_state_with_no_sync_uses_current_time() {
        let (limiter, _clock) = make_limiter();
        let state = limiter.to_state();
        assert_eq!(state.last_sync, base_time());
    }
}
