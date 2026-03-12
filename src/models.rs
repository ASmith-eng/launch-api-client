use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Summary of a launch for use in the list view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchSummary {
    pub id: String,
    pub name: String,
    pub net: DateTime<Utc>,
    pub net_precision: Option<NetPrecision>,
    pub window_start: Option<DateTime<Utc>>,
    pub window_end: Option<DateTime<Utc>>,
    pub status: LaunchStatus,
    pub launch_service_provider: Provider,
    pub pad: PadInfo,
    pub mission: Option<MissionSummary>,
}

/// Launch status (e.g. "Go for Launch", "TBD").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchStatus {
    pub id: u32,
    pub name: String,
    pub abbrev: String,
}

/// NET (No Earlier Than) precision indicator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetPrecision {
    pub id: u32,
    pub name: String,
    pub abbrev: String,
}

/// Launch service provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub name: String,
    pub provider_type: Option<ProviderType>,
}

/// Provider classification (e.g. "Commercial", "Government").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderType {
    pub name: String,
}

/// Launch pad information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PadInfo {
    pub name: Option<String>,
    pub location: LocationInfo,
}

/// Launch site location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationInfo {
    pub name: String,
    pub timezone_name: Option<String>,
    pub country: Option<CountryInfo>,
}

/// Country associated with a location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountryInfo {
    pub name: String,
    pub alpha_2_code: String,
}

/// Mission summary for the list view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionSummary {
    pub name: String,
    pub mission_type: String,
    pub description: Option<String>,
    pub orbit: Option<OrbitInfo>,
}

/// Orbital destination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrbitInfo {
    pub id: u32,
    pub name: String,
    pub abbrev: String,
}

/// Full launch detail for the detail view.
///
/// Contains all fields from `LaunchSummary` plus additional data only
/// available from the `/launch/{id}/` endpoint (detailed response mode).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchDetail {
    pub id: String,
    pub name: String,
    pub net: DateTime<Utc>,
    pub net_precision: Option<NetPrecision>,
    pub window_start: Option<DateTime<Utc>>,
    pub window_end: Option<DateTime<Utc>>,
    pub status: LaunchStatus,
    pub probability: Option<i32>,
    pub weather_concerns: Option<String>,
    pub image_url: Option<String>,
    // Provider
    pub launch_service_provider: Provider,
    pub provider_total_launches: Option<u32>,
    pub provider_successful_launches: Option<u32>,
    pub provider_failed_launches: Option<u32>,
    // Vehicle
    pub rocket_full_name: Option<String>,
    // Location
    pub pad: PadInfo,
    // Mission
    pub mission: Option<MissionSummary>,
    // Links
    pub vid_urls: Vec<UrlEntry>,
    pub info_urls: Vec<UrlEntry>,
    // Programs
    pub programs: Vec<String>,
}

/// A URL entry from the API (webcast link, info link, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlEntry {
    pub title: Option<String>,
    pub url: String,
}

/// Rate limit status returned by the `/api-throttle/` endpoint.
#[derive(Debug, Clone)]
pub struct ThrottleStatus {
    pub remaining: usize,
    pub limit: usize,
}

/// Cached launch list with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchListCache {
    pub version: u32,
    pub fetched_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub total_count: u32,
    pub launches: Vec<LaunchSummary>,
}

impl LaunchListCache {
    /// Whether this cache entry has expired at the given time.
    pub fn is_stale(&self, now: DateTime<Utc>) -> bool {
        now >= self.expires_at
    }
}

/// Cached launch detail with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchDetailCache {
    pub version: u32,
    pub launch_id: String,
    pub fetched_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub ttl_strategy: String,
    pub data: LaunchDetail,
}

impl LaunchDetailCache {
    /// Whether this cache entry has expired at the given time.
    pub fn is_stale(&self, now: DateTime<Utc>) -> bool {
        now >= self.expires_at
    }
}

/// Persistent application state saved across sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub version: u32,
    pub rate_limit: RateLimitState,
    pub last_startup: DateTime<Utc>,
    pub startup_count: u32,
    pub last_prune: Option<DateTime<Utc>>,
}

/// Rate limiter state persisted between sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitState {
    pub limit: usize,
    pub requests: Vec<DateTime<Utc>>,
    pub last_sync: DateTime<Utc>,
    pub authenticated: bool,
}

/// Current cache version. Bumped when the cache format changes.
pub const CACHE_VERSION: u32 = 1;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Round-trip test using the `app_state.json` example from the design doc.
    #[test]
    fn app_state_serde_round_trip() {
        let json = r#"{
            "version": 1,
            "rate_limit": {
                "limit": 15,
                "requests": [
                    "2026-02-22T10:15:00Z",
                    "2026-02-22T10:20:00Z"
                ],
                "last_sync": "2026-02-22T10:25:00Z",
                "authenticated": false
            },
            "last_startup": "2026-02-22T10:00:00Z",
            "startup_count": 42,
            "last_prune": "2026-02-20T08:00:00Z"
        }"#;

        let state: AppState = serde_json::from_str(json).expect("deserialize");
        assert_eq!(state.version, 1);
        assert_eq!(state.rate_limit.limit, 15);
        assert_eq!(state.rate_limit.requests.len(), 2);
        assert!(!state.rate_limit.authenticated);
        assert_eq!(state.startup_count, 42);
        assert!(state.last_prune.is_some());

        // Round-trip: serialize then deserialize again
        let serialized = serde_json::to_string(&state).unwrap();
        let deserialized: AppState = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.version, state.version);
        assert_eq!(deserialized.startup_count, state.startup_count);
    }

    /// Round-trip test using the `cache.json` example from the design doc.
    #[test]
    fn launch_list_cache_serde_round_trip() {
        let json = r#"{
            "version": 1,
            "fetched_at": "2026-02-22T10:15:00Z",
            "expires_at": "2026-02-22T10:45:00Z",
            "total_count": 147,
            "launches": [
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
                        "provider_type": { "name": "Commercial" }
                    },
                    "pad": {
                        "location": {
                            "name": "Starbase, Texas",
                            "timezone_name": "America/Chicago"
                        }
                    },
                    "mission": {
                        "name": "Starship IFT-7",
                        "mission_type": "Test Flight"
                    }
                }
            ]
        }"#;

        let cache: LaunchListCache = serde_json::from_str(json).expect("deserialize");
        assert_eq!(cache.version, 1);
        assert_eq!(cache.total_count, 147);
        assert_eq!(cache.launches.len(), 1);
        assert_eq!(cache.launches[0].name, "Starship IFT-7");
        assert_eq!(cache.launches[0].status.abbrev, "Go");

        let serialized = serde_json::to_string(&cache).unwrap();
        let deserialized: LaunchListCache = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.launches[0].id, cache.launches[0].id);
    }

    /// Round-trip test for `details/{uuid}.json`.
    #[test]
    fn launch_detail_cache_serde_round_trip() {
        let cache = LaunchDetailCache {
            version: 1,
            launch_id: "e3df2ecd-c239-472f-95e4-2b89b4f75800".into(),
            fetched_at: "2026-02-22T10:20:00Z".parse().unwrap(),
            expires_at: "2026-02-22T10:25:00Z".parse().unwrap(),
            ttl_strategy: "imminent".into(),
            data: dummy_launch_detail(),
        };

        assert_eq!(cache.version, 1);
        assert_eq!(cache.launch_id, "e3df2ecd-c239-472f-95e4-2b89b4f75800");
        assert_eq!(cache.ttl_strategy, "imminent");
        assert_eq!(cache.data.name, "Starship IFT-7");

        let serialized = serde_json::to_string(&cache).unwrap();
        let deserialized: LaunchDetailCache = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.launch_id, cache.launch_id);
        assert_eq!(deserialized.data.name, "Starship IFT-7");
    }

    /// Helper to create a minimal `LaunchDetail` for tests.
    pub(crate) fn dummy_launch_detail() -> LaunchDetail {
        LaunchDetail {
            id: "e3df2ecd-c239-472f-95e4-2b89b4f75800".into(),
            name: "Starship IFT-7".into(),
            net: "2026-02-28T09:00:00Z".parse().unwrap(),
            net_precision: None,
            window_start: None,
            window_end: None,
            status: LaunchStatus {
                id: 1,
                name: "Go for Launch".into(),
                abbrev: "Go".into(),
            },
            probability: None,
            weather_concerns: None,
            image_url: None,
            launch_service_provider: Provider {
                name: "SpaceX".into(),
                provider_type: None,
            },
            provider_total_launches: None,
            provider_successful_launches: None,
            provider_failed_launches: None,
            rocket_full_name: None,
            pad: PadInfo {
                name: None,
                location: LocationInfo {
                    name: "Starbase, Texas".into(),
                    timezone_name: None,
                    country: None,
                },
            },
            mission: None,
            vid_urls: vec![],
            info_urls: vec![],
            programs: vec![],
        }
    }
}
