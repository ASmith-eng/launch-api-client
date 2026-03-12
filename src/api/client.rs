//! API client trait and Launch Library 2 implementation.
//!
//! The [`LaunchApi`] trait defines vendor-agnostic methods for fetching launch
//! data. [`Ll2Client`] implements it using `reqwest` against the LL2 API, with
//! integrated client-side rate limiting.

use std::sync::Mutex;

use tracing::debug;

use crate::api::rate_limiter::RateLimiter;
use crate::clock::Clock;
use crate::error::AppError;
use crate::models::{LaunchDetail, LaunchSummary, ThrottleStatus};
use crate::vendor::launch_library_2::endpoints::{self, ListParams};
use crate::vendor::launch_library_2::response_models::{
    Ll2LaunchDetail, Ll2Launch, Ll2ThrottleResponse, PaginatedResponse,
};

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

    fn fetch_throttle_status(
        &self,
    ) -> impl std::future::Future<Output = Result<ThrottleStatus, AppError>> + Send;
}

/// Launch Library 2 API client.
///
/// Wraps `reqwest::Client` with rate limiting and API key injection.
/// The rate limiter is behind a `Mutex` (never held across await points)
/// to allow `&self` on trait methods.
#[derive(Debug)]
pub struct Ll2Client<C: Clock> {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    rate_limiter: Mutex<RateLimiter<C>>,
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

    /// Send a request and map non-success status codes to `AppError`.
    async fn send_and_check(&self, url: &str) -> Result<reqwest::Response, AppError> {
        debug!(url, "sending API request");
        let response = self.request(url).send().await?;

        let status = response.status();
        if status.is_success() {
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
}

impl<C: Clock + Send + Sync> LaunchApi for Ll2Client<C> {
    async fn fetch_launch_list(
        &self,
        params: &ListParams,
    ) -> Result<LaunchListResponse, AppError> {
        self.check_and_record_request()?;

        let url = endpoints::launches_upcoming_url(&self.base_url, params);
        let response = self.send_and_check(&url).await?;
        let paginated: PaginatedResponse<Ll2Launch> = response.json().await?;

        let launches = paginated.results.into_iter().map(Into::into).collect();
        Ok(LaunchListResponse {
            launches,
            total_count: paginated.count,
        })
    }

    async fn fetch_launch_detail(&self, id: &str) -> Result<LaunchDetail, AppError> {
        self.check_and_record_request()?;

        let url = endpoints::launch_detail_url(&self.base_url, id);
        let response = self.send_and_check(&url).await?;
        let ll2_detail: Ll2LaunchDetail = response.json().await?;

        Ok(ll2_detail.into())
    }

    async fn fetch_throttle_status(&self) -> Result<ThrottleStatus, AppError> {
        // Throttle endpoint does NOT count against the rate limit.
        let url = endpoints::api_throttle_url(&self.base_url);
        let response = self.send_and_check(&url).await?;
        let ll2_throttle: Ll2ThrottleResponse = response.json().await?;

        let status: ThrottleStatus = ll2_throttle.into();
        debug!(
            remaining = status.remaining,
            limit = status.limit,
            "throttle sync complete"
        );

        // Update the local rate limiter with server state.
        let mut limiter = self.rate_limiter.lock().expect("rate limiter lock poisoned");
        limiter.record_sync(status.remaining, status.limit);

        Ok(status)
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
                    "type": { "name": "Commercial" }
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
        "image": { "id": 1, "image_url": "https://example.com/img.jpg" },
        "launch_service_provider": {
            "name": "SpaceX",
            "type": { "name": "Commercial" },
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
            "info_urls": [{ "title": "Info", "url": "https://spacex.com/ift7" }],
            "vid_urls": [{ "title": "Webcast", "url": "https://youtube.com/watch?v=abc" }]
        },
        "program": [{ "name": "Starship Development" }]
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
            .and(path("/launch/e3df2ecd-c239-472f-95e4-2b89b4f75800/"))
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
            .and(path("/launch/nonexistent-id/"))
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

    // --- Error handling ---

    #[tokio::test]
    async fn server_error_returns_api_error() {
        let (client, server) = setup().await;

        Mock::given(method("GET"))
            .and(path("/launches/upcoming/"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&server)
            .await;

        let result = client.fetch_launch_list(&ListParams::default()).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            AppError::ApiError { status, message } => {
                assert_eq!(status, 500);
                assert_eq!(message, "Internal Server Error");
            }
            other => panic!("expected ApiError, got: {other:?}"),
        }
    }
}
