use chrono::{DateTime, Utc};
use std::time::Instant;

/// Application-wide error type.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("API request failed: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Rate limit exceeded, next available at {0}")]
    RateLimited(DateTime<Utc>),

    #[error("API returned error {status}: {message}")]
    ApiError { status: u16, message: String },

    #[error("Cache I/O error: {0}")]
    CacheIo(#[from] std::io::Error),

    #[error("Cache deserialization failed: {0}")]
    CacheParse(#[from] serde_json::Error),

    #[error("Configuration error: {0}")]
    Config(String),
}

impl AppError {
    /// Whether this error is transient and the operation may succeed on retry.
    ///
    /// Retryable: connection timeout/reset, DNS failure, HTTP 5xx.
    /// Non-retryable: HTTP 4xx (except 429 handled separately), deserialization,
    /// cache I/O, rate limit, config errors.
    pub fn is_retryable(&self) -> bool {
        match self {
            AppError::Network(e) => {
                e.is_timeout()
                    || e.is_connect()
                    || e.status()
                        .is_some_and(|s| s.is_server_error())
            }
            AppError::ApiError { status, .. } => (500..=599).contains(status),
            _ => false,
        }
    }

    /// Whether this error suggests the device is offline.
    ///
    /// Used by the UI layer to set `ErrorState::Offline`. Connection failures
    /// and timeouts are the strongest signals of no network connectivity.
    pub fn is_offline_signal(&self) -> bool {
        match self {
            AppError::Network(e) => e.is_connect() || e.is_timeout(),
            _ => false,
        }
    }
}

/// UI-facing error state used to control what the user sees.
#[derive(Debug)]
pub enum ErrorState {
    /// Transient error — auto-dismiss after a timeout, user can retry.
    Transient {
        message: String,
        dismiss_at: Instant,
    },
    /// Rate limit hit — persists until requests are available again.
    RateLimited { available_at: DateTime<Utc> },
    /// Network is unreachable — persists until a successful request.
    Offline,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limited_is_not_retryable() {
        let err = AppError::RateLimited(Utc::now());
        assert!(!err.is_retryable());
    }

    #[test]
    fn api_error_4xx_is_not_retryable() {
        let err = AppError::ApiError {
            status: 400,
            message: "Bad Request".into(),
        };
        assert!(!err.is_retryable());
    }

    #[test]
    fn api_error_5xx_is_retryable() {
        for status in [500, 502, 503, 504] {
            let err = AppError::ApiError {
                status,
                message: format!("Server Error {status}"),
            };
            assert!(err.is_retryable(), "status {status} should be retryable");
        }
    }

    #[test]
    fn api_error_429_is_not_retryable() {
        // 429 is handled separately via throttle sync, not generic retry.
        let err = AppError::ApiError {
            status: 429,
            message: "Too Many Requests".into(),
        };
        assert!(!err.is_retryable());
    }

    #[test]
    fn cache_io_is_not_retryable() {
        let err = AppError::CacheIo(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file not found",
        ));
        assert!(!err.is_retryable());
    }

    #[test]
    fn cache_parse_is_not_retryable() {
        let raw = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err = AppError::CacheParse(raw);
        assert!(!err.is_retryable());
    }

    #[test]
    fn config_error_is_not_retryable() {
        let err = AppError::Config("bad config".into());
        assert!(!err.is_retryable());
    }

    #[test]
    fn network_error_display() {
        // We can't easily construct a reqwest::Error, but we can verify the
        // non-network variants display correctly.
        let err = AppError::RateLimited(
            DateTime::parse_from_rfc3339("2026-01-01T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        );
        assert!(err.to_string().contains("2026-01-01"));
    }

    #[test]
    fn api_error_display() {
        let err = AppError::ApiError {
            status: 404,
            message: "Not Found".into(),
        };
        assert_eq!(err.to_string(), "API returned error 404: Not Found");
    }

    // --- is_offline_signal tests ---

    #[test]
    fn rate_limited_is_not_offline_signal() {
        let err = AppError::RateLimited(Utc::now());
        assert!(!err.is_offline_signal());
    }

    #[test]
    fn api_error_is_not_offline_signal() {
        let err = AppError::ApiError {
            status: 500,
            message: "Server Error".into(),
        };
        assert!(!err.is_offline_signal());
    }

    #[test]
    fn cache_io_is_not_offline_signal() {
        let err = AppError::CacheIo(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file not found",
        ));
        assert!(!err.is_offline_signal());
    }

    #[test]
    fn config_error_is_not_offline_signal() {
        let err = AppError::Config("bad config".into());
        assert!(!err.is_offline_signal());
    }
}
