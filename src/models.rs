use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::cache::CacheStrategy;

/// Launch probability as a percentage (0–100).
///
/// Constructed via `Probability::new` or `TryFrom<u8>`, which reject values
/// above 100. This prevents impossible states from propagating through the
/// UI rendering code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct Probability(u8);

impl Probability {
    /// Create a new `Probability`, returning `None` if `value > 100`.
    pub fn new(value: u8) -> Option<Self> {
        if value <= 100 {
            Some(Self(value))
        } else {
            None
        }
    }

    /// Returns the inner percentage value.
    pub fn value(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Probability {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value).ok_or_else(|| format!("probability {value} exceeds 100%"))
    }
}

impl From<Probability> for u8 {
    fn from(p: Probability) -> Self {
        p.0
    }
}

impl fmt::Display for Probability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}%", self.0)
    }
}

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
    pub provider_type: Option<String>,
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
    pub probability: Option<Probability>,
    pub weather_concerns: Option<String>,
    pub image_url: Option<String>,
    /// Failure reason text from the API (set on failed launches).
    #[serde(default)]
    pub failreason: Option<String>,
    /// Status updates posted to the launch (most recent first).
    #[serde(default)]
    pub updates: Vec<LaunchUpdate>,
    /// Crew assigned to the launch (empty for uncrewed missions).
    #[serde(default)]
    pub crew: Vec<CrewMember>,
    /// Recoverable stage landing entries (one per launcher stage).
    #[serde(default)]
    pub landings: Vec<StageLanding>,
    // Provider
    pub launch_service_provider: Provider,
    pub provider_total_launches: Option<u32>,
    pub provider_successful_launches: Option<u32>,
    pub provider_failed_launches: Option<u32>,
    #[serde(default)]
    pub provider_country_code: Option<String>,
    #[serde(default)]
    pub provider_founding_year: Option<u32>,
    #[serde(default)]
    pub provider_consecutive_successes: Option<u32>,
    // Vehicle
    pub rocket_full_name: Option<String>,
    #[serde(default)]
    pub rocket_variant: Option<String>,
    /// Stored for future use — not currently rendered.
    #[allow(dead_code)]
    #[serde(default)]
    pub rocket_description: Option<String>,
    #[serde(default)]
    pub rocket_length: Option<f64>,
    #[serde(default)]
    pub rocket_diameter: Option<f64>,
    #[serde(default)]
    pub rocket_launch_mass: Option<f64>,
    #[serde(default)]
    pub rocket_leo_capacity: Option<f64>,
    #[serde(default)]
    pub rocket_gto_capacity: Option<f64>,
    #[serde(default)]
    pub rocket_thrust: Option<f64>,
    #[serde(default)]
    pub rocket_maiden_flight: Option<String>,
    #[serde(default)]
    pub rocket_total_launches: Option<u32>,
    #[serde(default)]
    pub rocket_successful_launches: Option<u32>,
    #[serde(default)]
    pub rocket_failed_launches: Option<u32>,
    #[serde(default)]
    pub rocket_consecutive_successes: Option<u32>,
    // Location
    pub pad: PadInfo,
    #[serde(default)]
    pub pad_total_launch_count: Option<u32>,
    // Mission
    pub mission: Option<MissionSummary>,
    // Links
    pub vid_urls: Vec<UrlEntry>,
    pub info_urls: Vec<UrlEntry>,
    // Programs
    pub programs: Vec<String>,
}

/// A status update posted to a launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchUpdate {
    pub comment: String,
    pub created_on: DateTime<Utc>,
    pub info_url: Option<String>,
}

/// A crew member assigned to a launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrewMember {
    pub name: String,
    pub role: String,
    pub agency: String,
}

/// Landing information for a single recoverable stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageLanding {
    pub stage_type: Option<String>,
    pub landing_attempt: bool,
    pub landing_success: Option<bool>,
    pub landing_type: Option<String>,
    pub landing_location: Option<String>,
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
    /// API offset of the page this cache represents (`current_page * launches_per_page`).
    /// Defaults to 0 for caches written before this field was introduced.
    #[serde(default)]
    pub page_offset: u32,
    /// Filter selections active when this cache was written.
    /// Defaults to all-unfiltered for caches written before this field was introduced.
    #[serde(default)]
    pub active_filters: ActiveFilters,
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
    pub ttl_strategy: CacheStrategy,
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
pub const CACHE_VERSION: u32 = 2;

// ---------------------------------------------------------------------------
// Filter snapshot types (persisted in cache.json)
// ---------------------------------------------------------------------------

/// Launch status filter — persisted as part of [`ActiveFilters`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum StatusFilter {
    #[default]
    All,
    GoForLaunch,
    Tbd,
    Tbc,
    OnHold,
    InFlight,
}

/// Geographical region filter — persisted as part of [`ActiveFilters`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RegionFilter {
    #[default]
    All,
    US,
    Europe,
    RussiaKazakhstan,
    China,
    India,
    Japan,
    NewZealand,
}

/// Crewed mission filter — persisted as part of [`ActiveFilters`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CrewedFilter {
    #[default]
    All,
    CrewedOnly,
    UncrewedOnly,
}

/// Date range filter — persisted as part of [`ActiveFilters`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DateRangeFilter {
    #[default]
    All,
    Next7Days,
    Next30Days,
    Next90Days,
}

/// Snapshot of active filter selections, stored alongside the cached launch
/// list so filter context can be restored across sessions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActiveFilters {
    #[serde(default)]
    pub status: StatusFilter,
    #[serde(default)]
    pub region: RegionFilter,
    #[serde(default)]
    pub is_crewed: CrewedFilter,
    #[serde(default)]
    pub date_range: DateRangeFilter,
}

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
                        "provider_type": "Commercial"
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
        // Old-format JSON without new fields should default gracefully.
        assert_eq!(cache.page_offset, 0);
        assert_eq!(cache.active_filters.status, StatusFilter::All);
        assert_eq!(cache.active_filters.region, RegionFilter::All);
        assert_eq!(cache.active_filters.is_crewed, CrewedFilter::All);
        assert_eq!(cache.active_filters.date_range, DateRangeFilter::All);

        let serialized = serde_json::to_string(&cache).unwrap();
        let deserialized: LaunchListCache = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.launches[0].id, cache.launches[0].id);
    }

    #[test]
    fn launch_list_cache_page_offset_and_filters_round_trip() {
        let cache = LaunchListCache {
            version: 1,
            fetched_at: "2026-04-08T10:00:00Z".parse().unwrap(),
            expires_at: "2026-04-08T10:30:00Z".parse().unwrap(),
            total_count: 75,
            launches: vec![],
            page_offset: 25,
            active_filters: ActiveFilters {
                status: StatusFilter::GoForLaunch,
                region: RegionFilter::US,
                is_crewed: CrewedFilter::CrewedOnly,
                date_range: DateRangeFilter::Next30Days,
            },
        };

        let serialized = serde_json::to_string(&cache).unwrap();
        let deserialized: LaunchListCache = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.page_offset, 25);
        assert_eq!(deserialized.active_filters.status, StatusFilter::GoForLaunch);
        assert_eq!(deserialized.active_filters.region, RegionFilter::US);
        assert_eq!(deserialized.active_filters.is_crewed, CrewedFilter::CrewedOnly);
        assert_eq!(deserialized.active_filters.date_range, DateRangeFilter::Next30Days);
    }

    /// Round-trip test for `details/{uuid}.json`.
    #[test]
    fn launch_detail_cache_serde_round_trip() {
        let cache = LaunchDetailCache {
            version: 1,
            launch_id: "e3df2ecd-c239-472f-95e4-2b89b4f75800".into(),
            fetched_at: "2026-02-22T10:20:00Z".parse().unwrap(),
            expires_at: "2026-02-22T10:25:00Z".parse().unwrap(),
            ttl_strategy: CacheStrategy::ShortTerm,
            data: dummy_launch_detail(),
        };

        assert_eq!(cache.version, 1);
        assert_eq!(cache.launch_id, "e3df2ecd-c239-472f-95e4-2b89b4f75800");
        assert_eq!(cache.ttl_strategy, CacheStrategy::ShortTerm);
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
            failreason: None,
            updates: vec![],
            crew: vec![],
            landings: vec![],
            launch_service_provider: Provider {
                name: "SpaceX".into(),
                provider_type: None,
            },
            provider_total_launches: None,
            provider_successful_launches: None,
            provider_failed_launches: None,
            provider_country_code: None,
            provider_founding_year: None,
            provider_consecutive_successes: None,
            rocket_full_name: None,
            rocket_variant: None,
            rocket_description: None,
            rocket_length: None,
            rocket_diameter: None,
            rocket_launch_mass: None,
            rocket_leo_capacity: None,
            rocket_gto_capacity: None,
            rocket_thrust: None,
            rocket_maiden_flight: None,
            rocket_total_launches: None,
            rocket_successful_launches: None,
            rocket_failed_launches: None,
            rocket_consecutive_successes: None,
            pad: PadInfo {
                name: None,
                location: LocationInfo {
                    name: "Starbase, Texas".into(),
                    timezone_name: None,
                    country: None,
                },
            },
            pad_total_launch_count: None,
            mission: None,
            vid_urls: vec![],
            info_urls: vec![],
            programs: vec![],
        }
    }

    /// An old-format `LaunchDetail` JSON blob (pre-Step 1) must deserialize
    /// cleanly: all new fields default to `None`/empty so the version check
    /// can reject the cache without a serde error.
    #[test]
    fn launch_detail_deserializes_with_missing_new_fields() {
        let json = r#"{
            "id": "abc",
            "name": "Old Cache Launch",
            "net": "2026-01-01T00:00:00Z",
            "net_precision": null,
            "window_start": null,
            "window_end": null,
            "status": { "id": 1, "name": "Go for Launch", "abbrev": "Go" },
            "probability": null,
            "weather_concerns": null,
            "image_url": null,
            "launch_service_provider": { "name": "SpaceX", "provider_type": null },
            "provider_total_launches": null,
            "provider_successful_launches": null,
            "provider_failed_launches": null,
            "rocket_full_name": null,
            "pad": {
                "name": null,
                "location": {
                    "name": "Starbase",
                    "timezone_name": null,
                    "country": null
                }
            },
            "mission": null,
            "vid_urls": [],
            "info_urls": [],
            "programs": []
        }"#;

        let detail: LaunchDetail = serde_json::from_str(json).expect("deserialize old format");
        assert_eq!(detail.name, "Old Cache Launch");
        assert!(detail.failreason.is_none());
        assert!(detail.updates.is_empty());
        assert!(detail.crew.is_empty());
        assert!(detail.landings.is_empty());
        assert!(detail.rocket_variant.is_none());
        assert!(detail.rocket_description.is_none());
        assert!(detail.rocket_length.is_none());
        assert!(detail.rocket_thrust.is_none());
        assert!(detail.rocket_maiden_flight.is_none());
        assert!(detail.rocket_total_launches.is_none());
        assert!(detail.rocket_consecutive_successes.is_none());
        assert!(detail.provider_country_code.is_none());
        assert!(detail.provider_founding_year.is_none());
        assert!(detail.provider_consecutive_successes.is_none());
        assert!(detail.pad_total_launch_count.is_none());
    }
}
