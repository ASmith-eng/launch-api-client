use chrono::{DateTime, Utc};

/// Abstraction over time for testability.
///
/// Components that need the current time (rate limiter, cache manager)
/// accept a `Clock` implementation rather than calling `Utc::now()` directly.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// Production clock that delegates to `chrono::Utc::now()`.
#[derive(Debug, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use chrono::TimeDelta;
    use std::sync::Mutex;

    /// Test clock with manually controlled time.
    #[derive(Debug)]
    pub struct FakeClock {
        now: Mutex<DateTime<Utc>>,
    }

    impl FakeClock {
        pub fn new(start: DateTime<Utc>) -> Self {
            Self {
                now: Mutex::new(start),
            }
        }

        pub fn advance(&self, duration: TimeDelta) {
            let mut now = self.now.lock().unwrap();
            *now = *now + duration;
        }

        pub fn set(&self, time: DateTime<Utc>) {
            let mut now = self.now.lock().unwrap();
            *now = time;
        }
    }

    impl Clock for FakeClock {
        fn now(&self) -> DateTime<Utc> {
            *self.now.lock().unwrap()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;
    use testing::FakeClock;

    #[test]
    fn system_clock_returns_current_time() {
        let clock = SystemClock;
        let before = Utc::now();
        let now = clock.now();
        let after = Utc::now();
        assert!(now >= before && now <= after);
    }

    #[test]
    fn fake_clock_starts_at_given_time() {
        let start = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let clock = FakeClock::new(start);
        assert_eq!(clock.now(), start);
    }

    #[test]
    fn fake_clock_advance() {
        let start = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let clock = FakeClock::new(start);
        clock.advance(TimeDelta::hours(1));
        assert_eq!(clock.now(), start + TimeDelta::hours(1));
    }

    #[test]
    fn fake_clock_set() {
        let start = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let new_time = DateTime::parse_from_rfc3339("2026-06-15T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let clock = FakeClock::new(start);
        clock.set(new_time);
        assert_eq!(clock.now(), new_time);
    }
}
