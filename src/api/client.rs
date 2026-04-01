//! API client trait and Launch Library 2 implementation.
//!
//! The [`LaunchApi`] trait defines vendor-agnostic methods for fetching launch
//! data. [`Ll2Client`] implements it using `reqwest` against the LL2 API, with
//! integrated client-side rate limiting.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tracing::{debug, error, info, warn};

use crate::api::rate_limiter::RateLimiter;
use crate::clock::Clock;
use crate::error::AppError;
use crate::models::{LaunchDetail, LaunchSummary, ThrottleStatus};
use crate::vendor::launch_library_2::endpoints::{self, ListParams};
use crate::vendor::launch_library_2::response_models::{
    Ll2LaunchDetail, Ll2Launch, Ll2ThrottleResponse, PaginatedResponse,
};

/// Delay between the first failed attempt and the automatic retry.
const RETRY_DELAY: Duration = Duration::from_secs(1);

/// Hard ceiling on HTTP requests within [`REQUEST_CAP_WINDOW`].
///
/// This is a transport-layer safety net, independent of the application-layer
/// rate limiter. It exists solely to prevent runaway request loops caused by
/// bugs in fetch dispatch or sync logic. Normal usage (15–30 req/hour) will
/// never approach this limit.
const REQUEST_CAP: usize = 50;

/// Rolling window for the request cap.
const REQUEST_CAP_WINDOW: Duration = Duration::from_secs(600); // 10 minutes

/// Result of a successful launch list fetch.
#[derive(Debug)]
pub struct LaunchListResponse {
    pub launches: Vec<LaunchSummary>,
    pub total_count: u32,
}

/// Vendor-agnostic API for fetching launch data.
pub trait LaunchApi {
    fn fetch_launch_list(
        &self,
        params: &ListParams,
    ) -> impl std::future::Future<Output = Result<LaunchListResponse, AppError>> + Send;

    fn fetch_launch_detail(
        &self,
        id: &str,
    ) -> impl std::future::Future<Output = Result<LaunchDetail, AppError>> + Send;
}

/// Launch Library 2 API client.
///
/// Wraps `reqwest::Client` with rate limiting, API key injection, and a
/// transport-layer request cap as a safety net against runaway loops.
#[derive(Debug)]
pub struct Ll2Client<C: Clock> {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    rate_limiter: Mutex<RateLimiter<C>>,
    /// Transport-layer safety net: timestamps of all HTTP requests sent
    /// within the rolling window. Independent of the rate limiter.
    request_log: Mutex<VecDeque<Instant>>,
}

impl<C: Clock> Ll2Client<C> {
    /// Create a new LL2 client.
    pub fn new(
        base_url: String,
        api_key: Option<String>,
        rate_limiter: RateLimiter<C>,
    ) -> Result<Self, AppError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(AppError::Network)?;

        Ok(Self {
            http,
            base_url,
            api_key,
            rate_limiter: Mutex::new(rate_limiter),
            request_log: Mutex::new(VecDeque::new()),
        })
    }

    /// Access the rate limiter for UI queries (remaining, limit) or persistence.
    pub fn rate_limiter(&self) -> std::sync::MutexGuard<'_, RateLimiter<C>> {
        self.rate_limiter.lock().expect("rate limiter lock poisoned")
    }

    /// Check rate limiter and record a request. Returns `Err` if rate limited.
    fn check_and_record_request(&self) -> Result<(), AppError> {
        self.rate_limiter
            .lock()
            .expect("rate limiter lock poisoned")
            .record_request()
    }

    /// Build an HTTP request with the API key header if configured.
    fn request(&self, url: &str) -> reqwest::RequestBuilder {
        let req = self.http.get(url);
        match &self.api_key {
            Some(key) if !key.is_empty() => {
                req.header("Authorization", format!("Token {key}"))
            }
            _ => req,
        }
    }

    /// Check and record against the transport-layer request cap.
    ///
    /// Returns `Err` if the cap has been exceeded. This is a safety net —
    /// if this ever fires, there is a bug in the application-layer logic.
    fn check_request_cap(&self) -> Result<(), AppError> {
        let now = Instant::now();
        let mut log = self.request_log.lock().expect("request_log lock poisoned");

        // Prune entries outside the rolling window.
        while let Some(&front) = log.front() {
            if now.duration_since(front) > REQUEST_CAP_WINDOW {
                log.pop_front();
            } else {
                break;
            }
        }

        if log.len() >= REQUEST_CAP {
            error!(
                cap = REQUEST_CAP,
                window_secs = REQUEST_CAP_WINDOW.as_secs(),
                "transport-layer request cap exceeded — this indicates a bug in fetch dispatch"
            );
            return Err(AppError::RequestCapExceeded(format!(
                "Internal safety limit reached ({REQUEST_CAP} requests in {} minutes). \
                 Please restart the app.",
                REQUEST_CAP_WINDOW.as_secs() / 60,
            )));
        }

        log.push_back(now);
        Ok(())
    }

    /// Send a request and map non-success status codes to `AppError`.
    async fn send_and_check(&self, url: &str) -> Result<reqwest::Response, AppError> {
        self.check_request_cap()?;
        debug!(url, "sending API request");
        let response = self.request(url).send().await?;

        let status = response.status();
        if status.is_success() {
            debug!(url, status = status.as_u16(), "API response OK");
            return Ok(response);
        }

        // Read body for error message (best-effort).
        let message = response
            .text()
            .await
            .unwrap_or_else(|_| String::from("(no response body)"));

        Err(AppError::ApiError {
            status: status.as_u16(),
            message,
        })
    }

    /// Send a request with automatic retry for retryable errors and 429 handling.
    ///
    /// - **429**: syncs with `/api-throttle/` and returns `AppError::RateLimited`
    /// - **Retryable** (5xx, timeout, connect): waits 1 second, retries once
    /// - **Non-retryable** (4xx, deserialization): returns immediately
    async fn send_with_retry(&self, url: &str) -> Result<reqwest::Response, AppError> {
        match self.send_and_check(url).await {
            Ok(resp) => Ok(resp),
            Err(AppError::ApiError { status: 429, .. }) => self.handle_429().await,
            Err(e) if e.is_retryable() => {
                warn!(error = %e, "retryable error, retrying in 1s");
                tokio::time::sleep(RETRY_DELAY).await;
                // On retry, a 429 also triggers throttle sync.
                match self.send_and_check(url).await {
                    Err(AppError::ApiError { status: 429, .. }) => self.handle_429().await,
                    other => other,
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Handle a 429 response: best-effort throttle sync, then return `RateLimited`.
    async fn handle_429(&self) -> Result<reqwest::Response, AppError> {
        warn!("received 429, syncing with throttle endpoint");

        // Best-effort sync — if it fails, fall back to local rate limit state.
        match self.fetch_throttle_raw().await {
            Ok(status) => {
                let mut limiter = self.rate_limiter.lock().expect("rate limiter lock poisoned");
                limiter.record_sync(status.remaining, status.limit);
            }
            Err(e) => {
                warn!(error = %e, "throttle sync failed after 429, using local state");
            }
        }

        let mut limiter = self.rate_limiter.lock().expect("rate limiter lock poisoned");
        let available_at = limiter.next_available_at();
        Err(AppError::RateLimited(available_at))
    }

    /// Raw HTTP call to the throttle endpoint — no rate limiter interaction.
    async fn fetch_throttle_raw(&self) -> Result<ThrottleStatus, AppError> {
        let url = endpoints::api_throttle_url(&self.base_url);
        let response = self.send_and_check(&url).await?;
        let body = response.text().await.map_err(AppError::Network)?;
        let ll2_throttle: Ll2ThrottleResponse = match serde_json::from_str(&body) {
            Ok(parsed) => parsed,
            Err(e) => {
                let preview = truncate_for_log(&body, 500);
                warn!(
                    error = %e,
                    url,
                    body_preview = preview,
                    "failed to deserialize throttle response"
                );
                return Err(AppError::ApiParse(e));
            }
        };
        Ok(ll2_throttle.into())
    }

    /// Sync with the `/api-throttle/` endpoint, retrying once on transient failure.
    ///
    /// Intended for use during startup (design doc §Startup Sequence step 3).
    /// On permanent failure, logs a warning and returns the error — the caller
    /// should continue with local rate limit tracking.
    pub async fn sync_throttle_with_retry(&self) -> Result<ThrottleStatus, AppError> {
        let apply_sync = |this: &Self, status: &ThrottleStatus| {
            let mut limiter = this.rate_limiter.lock().expect("rate limiter lock poisoned");
            limiter.record_sync(status.remaining, status.limit);
            debug!(
                remaining = status.remaining,
                limit = status.limit,
                "startup throttle sync complete"
            );
        };

        match self.fetch_throttle_raw().await {
            Ok(status) => {
                apply_sync(self, &status);
                Ok(status)
            }
            Err(e) if e.is_retryable() => {
                warn!(error = %e, "startup throttle sync failed, retrying in 1s");
                tokio::time::sleep(RETRY_DELAY).await;
                match self.fetch_throttle_raw().await {
                    Ok(status) => {
                        apply_sync(self, &status);
                        Ok(status)
                    }
                    Err(e) => {
                        warn!(
                            error = %e,
                            "startup throttle sync failed after retry, \
                             continuing with local tracking"
                        );
                        Err(e)
                    }
                }
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "startup throttle sync failed (non-retryable), \
                     continuing with local tracking"
                );
                Err(e)
            }
        }
    }

    /// Fetch the current rate limit status from the `/api-throttle/` endpoint
    /// and sync the local rate limiter.
    ///
    /// The throttle endpoint does NOT count against the rate limit.
    #[cfg(test)]
    pub async fn fetch_throttle_status(&self) -> Result<ThrottleStatus, AppError> {
        let status = self.fetch_throttle_raw().await?;
        debug!(
            remaining = status.remaining,
            limit = status.limit,
            "throttle sync complete"
        );

        let mut limiter = self.rate_limiter.lock().expect("rate limiter lock poisoned");
        limiter.record_sync(status.remaining, status.limit);

        Ok(status)
    }
}

impl<C: Clock + Send + Sync> LaunchApi for Ll2Client<C> {
    async fn fetch_launch_list(
        &self,
        params: &ListParams,
    ) -> Result<LaunchListResponse, AppError> {
        self.check_and_record_request()?;

        let url = endpoints::launches_upcoming_url(&self.base_url, params);
        let response = self.send_with_retry(&url).await?;
        let body = response.text().await.map_err(AppError::Network)?;
        let paginated: PaginatedResponse<Ll2Launch> = match serde_json::from_str(&body) {
            Ok(parsed) => parsed,
            Err(e) => {
                let preview = truncate_for_log(&body, 500);
                warn!(
                    error = %e,
                    url,
                    body_preview = preview,
                    "failed to deserialize launch list response"
                );
                return Err(AppError::ApiParse(e));
            }
        };

        let count = paginated.results.len();
        let launches: Vec<LaunchSummary> =
            paginated.results.into_iter().map(Into::into).collect();
        info!(count, total = paginated.count, "fetched launch list");
        Ok(LaunchListResponse {
            launches,
            total_count: paginated.count,
        })
    }

    async fn fetch_launch_detail(&self, id: &str) -> Result<LaunchDetail, AppError> {
        self.check_and_record_request()?;

        let url = endpoints::launch_detail_url(&self.base_url, id);
        let response = self.send_with_retry(&url).await?;
        let body = response.text().await.map_err(AppError::Network)?;
        let ll2_detail: Ll2LaunchDetail = match serde_json::from_str(&body) {
            Ok(parsed) => parsed,
            Err(e) => {
                let preview = truncate_for_log(&body, 500);
                warn!(
                    error = %e,
                    url,
                    launch_id = id,
                    body_preview = preview,
                    "failed to deserialize launch detail response"
                );
                return Err(AppError::ApiParse(e));
            }
        };

        let detail: LaunchDetail = ll2_detail.into();
        info!(launch_id = id, name = %detail.name, "fetched launch detail");
        Ok(detail)
    }
}

/// Truncate a string for log output, appending "..." if it exceeds `max_len`.
fn truncate_for_log(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut truncated = s[..max_len].to_string();
        truncated.push_str("...");
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::testing::FakeClock;
    use chrono::{TimeZone, Utc};
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn base_time() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 1, 12, 0, 0).unwrap()
    }

    async fn setup() -> (Ll2Client<FakeClock>, MockServer) {
        let server = MockServer::start().await;
        let clock = FakeClock::new(base_time());
        let limiter = RateLimiter::new(clock, false);
        let client =
            Ll2Client::new(server.uri(), None, limiter).expect("client creation should succeed");
        (client, server)
    }

    const SAMPLE_LIST_RESPONSE: &str = r#"{
        "count": 147,
        "next": null,
        "previous": null,
        "results": [
            {
                "id": "e3df2ecd-c239-472f-95e4-2b89b4f75800",
                "name": "Starship IFT-7",
                "net": "2026-02-28T09:00:00Z",
                "net_precision": { "id": 1, "name": "Day", "abbrev": "Day" },
                "window_start": "2026-02-28T09:00:00Z",
                "window_end": "2026-02-28T12:00:00Z",
                "status": { "id": 1, "name": "Go for Launch", "abbrev": "Go" },
                "launch_service_provider": {
                    "name": "SpaceX",
                    "type": "Commercial"
                },
                "pad": {
                    "name": "OLM A",
                    "location": {
                        "name": "Starbase, Texas",
                        "timezone_name": "America/Chicago",
                        "country": { "name": "United States", "alpha_2_code": "US" }
                    }
                },
                "mission": {
                    "name": "Starship IFT-7",
                    "type": "Test Flight",
                    "description": "Seventh test.",
                    "orbit": { "id": 8, "name": "Low Earth Orbit", "abbrev": "LEO" }
                }
            }
        ]
    }"#;

    const SAMPLE_DETAIL_RESPONSE: &str = r#"{
        "id": "e3df2ecd-c239-472f-95e4-2b89b4f75800",
        "name": "Starship IFT-7",
        "response_mode": "detailed",
        "net": "2026-02-28T09:00:00Z",
        "status": { "id": 1, "name": "Go for Launch", "abbrev": "Go" },
        "probability": 90,
        "weather_concerns": "No concerns",
        "image": "https://example.com/img.jpg",
        "launch_service_provider": {
            "name": "SpaceX",
            "type": "Commercial",
            "total_launch_count": 301,
            "successful_launches": 295,
            "failed_launches": 6
        },
        "rocket": {
            "id": 1,
            "configuration": {
                "full_name": "Starship (Super Heavy + Starship)",
                "name": "Starship"
            }
        },
        "pad": {
            "name": "OLM A",
            "location": {
                "name": "Starbase, Texas",
                "timezone_name": "America/Chicago",
                "country": { "name": "United States", "alpha_2_code": "US" }
            }
        },
        "mission": {
            "name": "Starship IFT-7",
            "type": "Test Flight",
            "description": "Seventh flight test.",
            "orbit": { "id": 8, "name": "Low Earth Orbit", "abbrev": "LEO" },
            "info_urls": [],
            "vid_urls": []
        },
        "program": [{ "name": "Starship Development" }],
        "infoURLs": [{ "title": "Info", "url": "https://spacex.com/ift7" }],
        "vidURLs": [{ "title": "Webcast", "url": "https://youtube.com/watch?v=abc" }]
    }"#;

    const SAMPLE_THROTTLE_RESPONSE: &str = r#"{
        "your_request_limit": 15,
        "limit_frequency_secs": 3600,
        "current_use": 3,
        "next_use_secs": 0,
        "ident": "127.0.0.1"
    }"#;

    // --- List endpoint tests ---

    #[tokio::test]
    async fn fetch_launch_list_success() {
        let (client, server) = setup().await;

        Mock::given(method("GET"))
            .and(path("/launches/upcoming/"))
            .and(query_param("mode", "normal"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(SAMPLE_LIST_RESPONSE, "application/json"))
            .mount(&server)
            .await;

        let result = client
            .fetch_launch_list(&ListParams::default())
            .await
            .expect("should succeed");

        assert_eq!(result.total_count, 147);
        assert_eq!(result.launches.len(), 1);
        assert_eq!(result.launches[0].name, "Starship IFT-7");
        assert_eq!(result.launches[0].status.abbrev, "Go");
    }

    #[tokio::test]
    async fn fetch_launch_list_records_rate_limit() {
        let (client, server) = setup().await;

        Mock::given(method("GET"))
            .and(path("/launches/upcoming/"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(SAMPLE_LIST_RESPONSE, "application/json"))
            .mount(&server)
            .await;

        assert_eq!(client.rate_limiter().remaining(), 15);

        client
            .fetch_launch_list(&ListParams::default())
            .await
            .unwrap();

        assert_eq!(client.rate_limiter().remaining(), 14);
    }

    // --- Detail endpoint tests ---

    #[tokio::test]
    async fn fetch_launch_detail_success() {
        let (client, server) = setup().await;

        Mock::given(method("GET"))
            .and(path("/launches/e3df2ecd-c239-472f-95e4-2b89b4f75800/"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(SAMPLE_DETAIL_RESPONSE, "application/json"))
            .mount(&server)
            .await;

        let detail = client
            .fetch_launch_detail("e3df2ecd-c239-472f-95e4-2b89b4f75800")
            .await
            .expect("should succeed");

        assert_eq!(detail.name, "Starship IFT-7");
        assert_eq!(detail.probability, Some(90));
        assert_eq!(detail.weather_concerns.as_deref(), Some("No concerns"));
        assert_eq!(detail.provider_total_launches, Some(301));
        assert_eq!(detail.provider_successful_launches, Some(295));
        assert_eq!(
            detail.rocket_full_name.as_deref(),
            Some("Starship (Super Heavy + Starship)")
        );
        assert_eq!(detail.vid_urls.len(), 1);
        assert_eq!(detail.info_urls.len(), 1);
        assert_eq!(detail.programs, vec!["Starship Development"]);
    }

    #[tokio::test]
    async fn fetch_launch_detail_404() {
        let (client, server) = setup().await;

        Mock::given(method("GET"))
            .and(path("/launches/nonexistent-id/"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
            .mount(&server)
            .await;

        let result = client.fetch_launch_detail("nonexistent-id").await;
        assert!(result.is_err());

        match result.unwrap_err() {
            AppError::ApiError { status, .. } => assert_eq!(status, 404),
            other => panic!("expected ApiError, got: {other:?}"),
        }
    }

    // --- Throttle endpoint tests ---

    #[tokio::test]
    async fn fetch_throttle_status_success() {
        let (client, server) = setup().await;

        Mock::given(method("GET"))
            .and(path("/api-throttle/"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(SAMPLE_THROTTLE_RESPONSE, "application/json"))
            .mount(&server)
            .await;

        let status = client.fetch_throttle_status().await.expect("should succeed");

        assert_eq!(status.limit, 15);
        assert_eq!(status.remaining, 12);
    }

    #[tokio::test]
    async fn throttle_does_not_count_against_rate_limit() {
        let (client, server) = setup().await;

        Mock::given(method("GET"))
            .and(path("/api-throttle/"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(SAMPLE_THROTTLE_RESPONSE, "application/json"))
            .mount(&server)
            .await;

        let before = client.rate_limiter().remaining();
        client.fetch_throttle_status().await.unwrap();

        // The throttle endpoint updates the rate limiter via record_sync,
        // but it should NOT record a request against the limit.
        // The server reports current_use=3, so remaining should be 12.
        assert_eq!(before, 15);
        assert_eq!(client.rate_limiter().remaining(), 12);
    }

    #[tokio::test]
    async fn throttle_updates_rate_limiter() {
        let (client, server) = setup().await;

        Mock::given(method("GET"))
            .and(path("/api-throttle/"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(SAMPLE_THROTTLE_RESPONSE, "application/json"))
            .mount(&server)
            .await;

        client.fetch_throttle_status().await.unwrap();

        // After sync, should_sync() should return false (just synced).
        assert!(!client.rate_limiter().should_sync());
    }

    // --- Rate limiting tests ---

    #[tokio::test]
    async fn rate_limiter_blocks_when_exhausted() {
        let (client, server) = setup().await;

        Mock::given(method("GET"))
            .and(path("/launches/upcoming/"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(SAMPLE_LIST_RESPONSE, "application/json"))
            .mount(&server)
            .await;

        // Exhaust the rate limit (15 unauth requests).
        for _ in 0..15 {
            client
                .fetch_launch_list(&ListParams::default())
                .await
                .unwrap();
        }

        // 16th request should fail without making an HTTP call.
        let result = client.fetch_launch_list(&ListParams::default()).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::RateLimited(_) => {} // expected
            other => panic!("expected RateLimited, got: {other:?}"),
        }
    }

    // --- API key injection ---

    #[tokio::test]
    async fn api_key_sent_in_header() {
        let server = MockServer::start().await;
        let clock = FakeClock::new(base_time());
        let limiter = RateLimiter::new(clock, true);
        let client = Ll2Client::new(
            server.uri(),
            Some("test-api-key-123".into()),
            limiter,
        )
        .unwrap();

        Mock::given(method("GET"))
            .and(path("/launches/upcoming/"))
            .and(header("Authorization", "Token test-api-key-123"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(SAMPLE_LIST_RESPONSE, "application/json"))
            .mount(&server)
            .await;

        let result = client
            .fetch_launch_list(&ListParams::default())
            .await;

        // If the header doesn't match, wiremock returns 404, so success means
        // the header was sent correctly.
        assert!(result.is_ok());
    }

    // --- Retry behavior tests ---

    #[tokio::test]
    async fn server_5xx_triggers_retry_then_returns_error() {
        let (client, server) = setup().await;

        Mock::given(method("GET"))
            .and(path("/launches/upcoming/"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .expect(2) // Original + 1 retry
            .mount(&server)
            .await;

        let result = client.fetch_launch_list(&ListParams::default()).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            AppError::ApiError { status, message } => {
                assert_eq!(status, 500);
                assert_eq!(message, "Internal Server Error");
            }
            other => panic!("expected ApiError 500, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn server_5xx_retry_succeeds_on_second_attempt() {
        let (client, server) = setup().await;

        // Mount 200 response first (lower priority — fallback).
        Mock::given(method("GET"))
            .and(path("/launches/upcoming/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(SAMPLE_LIST_RESPONSE, "application/json"),
            )
            .mount(&server)
            .await;

        // Mount 500 response second (higher priority), matches only once.
        Mock::given(method("GET"))
            .and(path("/launches/upcoming/"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Temporary Error"))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        let result = client
            .fetch_launch_list(&ListParams::default())
            .await
            .expect("should succeed on retry");

        assert_eq!(result.total_count, 147);
        assert_eq!(result.launches.len(), 1);
    }

    #[tokio::test]
    async fn client_4xx_does_not_retry() {
        let (client, server) = setup().await;

        Mock::given(method("GET"))
            .and(path("/launches/upcoming/"))
            .respond_with(ResponseTemplate::new(400).set_body_string("Bad Request"))
            .expect(1) // Exactly 1 request — no retry
            .mount(&server)
            .await;

        let result = client.fetch_launch_list(&ListParams::default()).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            AppError::ApiError { status: 400, .. } => {}
            other => panic!("expected ApiError 400, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn detail_404_does_not_retry() {
        let (client, server) = setup().await;

        Mock::given(method("GET"))
            .and(path("/launches/nonexistent-id/"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
            .expect(1) // Exactly 1 request — no retry
            .mount(&server)
            .await;

        let result = client.fetch_launch_detail("nonexistent-id").await;

        match result.unwrap_err() {
            AppError::ApiError { status: 404, .. } => {}
            other => panic!("expected ApiError 404, got: {other:?}"),
        }
    }

    // --- 429 / throttle sync tests ---

    #[tokio::test]
    async fn server_429_triggers_throttle_sync_and_returns_rate_limited() {
        let (client, server) = setup().await;

        Mock::given(method("GET"))
            .and(path("/launches/upcoming/"))
            .respond_with(ResponseTemplate::new(429).set_body_string("Too Many Requests"))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api-throttle/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(SAMPLE_THROTTLE_RESPONSE, "application/json"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let result = client.fetch_launch_list(&ListParams::default()).await;

        match result.unwrap_err() {
            AppError::RateLimited(_) => {}
            other => panic!("expected RateLimited, got: {other:?}"),
        }

        // Throttle sync should have updated the rate limiter.
        // Server reports current_use=3, limit=15 → remaining=12.
        // But we also recorded 1 request via check_and_record_request before
        // the 429, and record_sync reconciles to server state.
        assert_eq!(client.rate_limiter().remaining(), 12);
    }

    #[tokio::test]
    async fn server_429_with_throttle_failure_still_returns_rate_limited() {
        let (client, server) = setup().await;

        Mock::given(method("GET"))
            .and(path("/launches/upcoming/"))
            .respond_with(ResponseTemplate::new(429).set_body_string("Too Many Requests"))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api-throttle/"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Server Error"))
            .mount(&server)
            .await;

        let result = client.fetch_launch_list(&ListParams::default()).await;

        // Should still return RateLimited even if throttle sync fails.
        match result.unwrap_err() {
            AppError::RateLimited(_) => {}
            other => panic!("expected RateLimited, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn server_429_on_retry_triggers_throttle_sync() {
        let (client, server) = setup().await;

        // First request returns 502 (retryable), retry returns 429.
        Mock::given(method("GET"))
            .and(path("/launches/upcoming/"))
            .respond_with(ResponseTemplate::new(429).set_body_string("Too Many Requests"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/launches/upcoming/"))
            .respond_with(ResponseTemplate::new(502).set_body_string("Bad Gateway"))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api-throttle/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(SAMPLE_THROTTLE_RESPONSE, "application/json"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let result = client.fetch_launch_list(&ListParams::default()).await;

        match result.unwrap_err() {
            AppError::RateLimited(_) => {}
            other => panic!("expected RateLimited, got: {other:?}"),
        }
    }

    // --- Timeout tests ---

    #[tokio::test]
    async fn timeout_returns_retryable_offline_error() {
        let server = MockServer::start().await;
        let clock = FakeClock::new(base_time());
        let limiter = RateLimiter::new(clock, false);

        // Build a client with a very short timeout (100ms).
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(100))
            .build()
            .unwrap();
        let client = Ll2Client {
            http,
            base_url: server.uri(),
            api_key: None,
            rate_limiter: Mutex::new(limiter),
            request_log: Mutex::new(VecDeque::new()),
        };

        // Respond with a 5-second delay — exceeds our 100ms timeout.
        Mock::given(method("GET"))
            .and(path("/launches/upcoming/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(SAMPLE_LIST_RESPONSE, "application/json")
                    .set_delay(Duration::from_secs(5)),
            )
            .mount(&server)
            .await;

        let result = client.fetch_launch_list(&ListParams::default()).await;
        let err = result.unwrap_err();

        // Timeout is both retryable and an offline signal.
        assert!(err.is_retryable(), "timeout should be retryable");
        assert!(err.is_offline_signal(), "timeout should be offline signal");
    }

    // --- Empty response tests ---

    #[tokio::test]
    async fn fetch_launch_list_empty_results() {
        let (client, server) = setup().await;

        let empty_response = r#"{
            "count": 0,
            "next": null,
            "previous": null,
            "results": []
        }"#;

        Mock::given(method("GET"))
            .and(path("/launches/upcoming/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(empty_response, "application/json"),
            )
            .mount(&server)
            .await;

        let result = client
            .fetch_launch_list(&ListParams::default())
            .await
            .expect("empty list should parse successfully");

        assert_eq!(result.total_count, 0);
        assert!(result.launches.is_empty());
    }

    // --- Query parameter tests ---

    #[tokio::test]
    async fn filter_params_appear_in_request() {
        let (client, server) = setup().await;

        Mock::given(method("GET"))
            .and(path("/launches/upcoming/"))
            .and(query_param("status__ids", "1,2"))
            .and(query_param("is_crewed", "true"))
            .and(query_param("pad__location", "27,12"))
            .and(query_param("limit", "10"))
            .and(query_param("offset", "5"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(SAMPLE_LIST_RESPONSE, "application/json"),
            )
            .mount(&server)
            .await;

        let params = ListParams {
            limit: 10,
            offset: 5,
            status_ids: Some("1,2".into()),
            is_crewed: Some(true),
            pad_location: Some("27,12".into()),
            net_gt: None,
            net_lt: None,
            search: None,
        };

        // If any query param doesn't match, wiremock returns 404.
        let result = client.fetch_launch_list(&params).await;
        assert!(result.is_ok(), "query params should match wiremock expectations");
    }

    // --- Startup throttle sync tests ---

    #[tokio::test]
    async fn startup_sync_retries_on_5xx_and_succeeds() {
        let server = MockServer::start().await;
        let clock = FakeClock::new(base_time());
        let limiter = RateLimiter::new(clock, false);
        let client = Ll2Client::new(server.uri(), None, limiter).unwrap();

        // Mount 200 first (lower priority — fallback after first failure).
        Mock::given(method("GET"))
            .and(path("/api-throttle/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(SAMPLE_THROTTLE_RESPONSE, "application/json"),
            )
            .mount(&server)
            .await;

        // Mount 500 second (higher priority), matches only once.
        Mock::given(method("GET"))
            .and(path("/api-throttle/"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Temporary Error"))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        let result = client.sync_throttle_with_retry().await;
        let status = result.expect("should succeed on retry");
        assert_eq!(status.remaining, 12);
        assert_eq!(status.limit, 15);

        // Rate limiter should be updated.
        assert_eq!(client.rate_limiter().remaining(), 12);
    }

    #[tokio::test]
    async fn startup_sync_degrades_gracefully_on_persistent_failure() {
        let server = MockServer::start().await;
        let clock = FakeClock::new(base_time());
        let limiter = RateLimiter::new(clock, false);
        let client = Ll2Client::new(server.uri(), None, limiter).unwrap();

        Mock::given(method("GET"))
            .and(path("/api-throttle/"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Server Error"))
            .expect(2) // Original + 1 retry
            .mount(&server)
            .await;

        let result = client.sync_throttle_with_retry().await;
        assert!(result.is_err());

        // Rate limiter should still work with local defaults.
        assert_eq!(client.rate_limiter().remaining(), 15);
    }

    #[tokio::test]
    async fn startup_sync_no_retry_on_4xx() {
        let server = MockServer::start().await;
        let clock = FakeClock::new(base_time());
        let limiter = RateLimiter::new(clock, false);
        let client = Ll2Client::new(server.uri(), None, limiter).unwrap();

        Mock::given(method("GET"))
            .and(path("/api-throttle/"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
            .expect(1) // No retry for 4xx
            .mount(&server)
            .await;

        let result = client.sync_throttle_with_retry().await;
        assert!(result.is_err());
    }

    // --- Transport-layer request cap tests ---

    #[test]
    fn request_cap_allows_requests_under_limit() {
        let clock = FakeClock::new(base_time());
        let limiter = RateLimiter::new(clock, false);
        let client = Ll2Client::new("http://unused".into(), None, limiter).unwrap();

        for i in 0..REQUEST_CAP {
            assert!(
                client.check_request_cap().is_ok(),
                "request {i} should be allowed"
            );
        }
    }

    #[test]
    fn request_cap_blocks_at_limit() {
        let clock = FakeClock::new(base_time());
        let limiter = RateLimiter::new(clock, false);
        let client = Ll2Client::new("http://unused".into(), None, limiter).unwrap();

        // Fill up to the cap.
        for _ in 0..REQUEST_CAP {
            client.check_request_cap().unwrap();
        }

        // Next request should be blocked.
        let result = client.check_request_cap();
        assert!(result.is_err(), "request at cap should be blocked");

        match result.unwrap_err() {
            AppError::RequestCapExceeded(message) => {
                assert!(
                    message.contains("safety limit"),
                    "error message should mention safety limit, got: {message}"
                );
            }
            other => panic!("expected RequestCapExceeded, got: {other:?}"),
        }
    }

    #[test]
    fn request_cap_prunes_expired_entries() {
        let clock = FakeClock::new(base_time());
        let limiter = RateLimiter::new(clock, false);
        let client = Ll2Client::new("http://unused".into(), None, limiter).unwrap();

        // Backdate entries to just outside the window.
        {
            let mut log = client.request_log.lock().unwrap();
            let past = Instant::now() - REQUEST_CAP_WINDOW - Duration::from_secs(1);
            for _ in 0..REQUEST_CAP {
                log.push_back(past);
            }
        }

        // Despite the log being full, all entries are expired — should succeed.
        assert!(
            client.check_request_cap().is_ok(),
            "expired entries should be pruned, allowing new requests"
        );
    }
}
