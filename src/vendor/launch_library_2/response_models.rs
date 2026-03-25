//! Serde models matching Launch Library 2 API response shapes.
//!
//! These types exist solely to deserialize LL2 JSON responses. Application
//! logic should use the core domain types from [`crate::models`] instead.

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// Paginated response wrapper used by all LL2 list endpoints.
#[derive(Debug, Deserialize)]
pub struct PaginatedResponse<T> {
    pub count: u32,
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<T>,
}

/// A launch in "normal" response mode (used by `/launches/upcoming/`).
#[derive(Debug, Deserialize)]
pub struct Ll2Launch {
    pub id: String,
    pub name: String,
    pub net: DateTime<Utc>,
    pub net_precision: Option<Ll2NetPrecision>,
    pub window_start: Option<DateTime<Utc>>,
    pub window_end: Option<DateTime<Utc>>,
    pub status: Ll2Status,
    pub launch_service_provider: Ll2Provider,
    pub pad: Ll2Pad,
    pub mission: Option<Ll2Mission>,
}

#[derive(Debug, Deserialize)]
pub struct Ll2Status {
    pub id: u32,
    pub name: String,
    pub abbrev: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Ll2NetPrecision {
    pub id: u32,
    pub name: String,
    pub abbrev: String,
}

#[derive(Debug, Deserialize)]
pub struct Ll2Provider {
    pub name: String,
    #[serde(rename = "type", default)]
    pub provider_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Ll2Pad {
    #[serde(default)]
    pub name: Option<String>,
    pub location: Ll2Location,
}

#[derive(Debug, Deserialize)]
pub struct Ll2Location {
    pub name: String,
    #[serde(default)]
    pub timezone_name: Option<String>,
    #[serde(default)]
    pub country: Option<Ll2Country>,
}

#[derive(Debug, Deserialize)]
pub struct Ll2Country {
    pub name: String,
    pub alpha_2_code: String,
}

#[derive(Debug, Deserialize)]
pub struct Ll2Mission {
    pub name: String,
    #[serde(rename = "type", default)]
    pub mission_type: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub orbit: Option<Ll2Orbit>,
}

#[derive(Debug, Deserialize)]
pub struct Ll2Orbit {
    pub id: u32,
    pub name: String,
    pub abbrev: String,
}

// ---------------------------------------------------------------------------
// Detail response models (from `/launch/{id}/`)
// ---------------------------------------------------------------------------

/// A launch in "detailed" response mode.
///
/// The detailed mode expands the launch service provider into a full agency
/// with launch statistics, includes `info_urls`/`vid_urls` on the mission,
/// and adds top-level fields like `probability` and `weather_concerns`.
#[derive(Debug, Deserialize)]
pub struct Ll2LaunchDetail {
    pub id: String,
    pub name: String,
    pub net: DateTime<Utc>,
    #[serde(default)]
    pub net_precision: Option<Ll2NetPrecision>,
    #[serde(default)]
    pub window_start: Option<DateTime<Utc>>,
    #[serde(default)]
    pub window_end: Option<DateTime<Utc>>,
    pub status: Ll2Status,
    #[serde(default)]
    pub probability: Option<i32>,
    #[serde(default)]
    pub weather_concerns: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    pub launch_service_provider: Ll2ProviderDetail,
    #[serde(default)]
    pub rocket: Option<Ll2Rocket>,
    pub pad: Ll2Pad,
    #[serde(default)]
    pub mission: Option<Ll2MissionDetail>,
    #[serde(default)]
    pub program: Vec<Ll2Program>,
    #[serde(rename = "vidURLs", default)]
    pub vid_urls: Vec<Ll2UrlEntry>,
    #[serde(rename = "infoURLs", default)]
    pub info_urls: Vec<Ll2UrlEntry>,
}

/// Provider in detailed response mode (full agency with launch statistics).
#[derive(Debug, Deserialize)]
pub struct Ll2ProviderDetail {
    pub name: String,
    #[serde(rename = "type", default)]
    pub provider_type: Option<String>,
    #[serde(default)]
    pub total_launch_count: Option<u32>,
    #[serde(default)]
    pub successful_launches: Option<u32>,
    #[serde(default)]
    pub failed_launches: Option<u32>,
}

/// Rocket configuration in detailed response mode.
#[derive(Debug, Deserialize)]
pub struct Ll2Rocket {
    #[serde(default)]
    pub configuration: Option<Ll2RocketConfiguration>,
}

/// Rocket configuration details.
#[derive(Debug, Deserialize)]
pub struct Ll2RocketConfiguration {
    #[serde(default)]
    pub full_name: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

/// Mission in detailed response mode (includes `info_urls` and `vid_urls`).
#[derive(Debug, Deserialize)]
pub struct Ll2MissionDetail {
    pub name: String,
    #[serde(rename = "type", default)]
    pub mission_type: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub orbit: Option<Ll2Orbit>,
    #[serde(default)]
    pub info_urls: Vec<Ll2UrlEntry>,
    #[serde(default)]
    pub vid_urls: Vec<Ll2UrlEntry>,
}

/// A URL entry (webcast, info link, etc.).
#[derive(Debug, Deserialize)]
pub struct Ll2UrlEntry {
    #[serde(default)]
    pub title: Option<String>,
    pub url: String,
}

/// Program associated with a launch.
#[derive(Debug, Deserialize)]
pub struct Ll2Program {
    pub name: String,
}

// ---------------------------------------------------------------------------
// Throttle response model (from `/api-throttle/`)
// ---------------------------------------------------------------------------

/// Response from the `/api-throttle/` endpoint.
///
/// Verified against `https://lldev.thespacedevs.com/2.3.0/api-throttle/`:
/// ```json
/// {
///   "your_request_limit": 15,
///   "limit_frequency_secs": 3600,
///   "current_use": 0,
///   "next_use_secs": 0,
///   "ident": "[ipv4]"
/// }
/// ```
#[derive(Debug, Deserialize)]
pub struct Ll2ThrottleResponse {
    pub your_request_limit: u32,
    pub current_use: u32,
    #[serde(default)]
    pub limit_frequency_secs: Option<u32>,
    #[serde(default)]
    pub next_use_secs: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// JSON payload matching the LL2 `/launches/upcoming/?mode=normal` shape,
    /// based on example data from the design document.
    const SAMPLE_LIST_JSON: &str = r#"{
        "count": 147,
        "next": "https://lldev.thespacedevs.com/2.3.0/launches/upcoming/?limit=10&offset=10",
        "previous": null,
        "results": [
            {
                "id": "e3df2ecd-c239-472f-95e4-2b89b4f75800",
                "name": "Starship IFT-7",
                "net": "2026-02-28T09:00:00Z",
                "net_precision": {
                    "id": 1,
                    "name": "Day",
                    "abbrev": "Day"
                },
                "window_start": "2026-02-28T09:00:00Z",
                "window_end": "2026-02-28T12:00:00Z",
                "status": {
                    "id": 1,
                    "name": "Go for Launch",
                    "abbrev": "Go",
                    "description": "Current launch date is a go."
                },
                "launch_service_provider": {
                    "name": "SpaceX",
                    "type": "Commercial"
                },
                "pad": {
                    "name": "Orbital Launch Mount A",
                    "location": {
                        "name": "Starbase, Texas",
                        "timezone_name": "America/Chicago",
                        "country_code": "US"
                    }
                },
                "mission": {
                    "name": "Starship IFT-7",
                    "type": "Test Flight",
                    "description": "Seventh integrated flight test of the Starship system.",
                    "orbit": {
                        "id": 8,
                        "name": "Low Earth Orbit",
                        "abbrev": "LEO"
                    }
                }
            }
        ]
    }"#;

    #[test]
    fn deserialize_paginated_launch_list() {
        let response: PaginatedResponse<Ll2Launch> =
            serde_json::from_str(SAMPLE_LIST_JSON).expect("should deserialize");

        assert_eq!(response.count, 147);
        assert!(response.next.is_some());
        assert!(response.previous.is_none());
        assert_eq!(response.results.len(), 1);

        let launch = &response.results[0];
        assert_eq!(launch.id, "e3df2ecd-c239-472f-95e4-2b89b4f75800");
        assert_eq!(launch.name, "Starship IFT-7");
        assert_eq!(launch.status.id, 1);
        assert_eq!(launch.status.abbrev, "Go");
        assert_eq!(launch.launch_service_provider.name, "SpaceX");
        assert_eq!(
            launch.launch_service_provider.provider_type.as_deref(),
            Some("Commercial")
        );
        assert_eq!(launch.pad.location.name, "Starbase, Texas");

        let mission = launch.mission.as_ref().unwrap();
        assert_eq!(mission.name, "Starship IFT-7");
        assert_eq!(mission.mission_type.as_deref(), Some("Test Flight"));
        assert_eq!(mission.orbit.as_ref().unwrap().abbrev, "LEO");
    }

    #[test]
    fn deserialize_launch_with_null_optionals() {
        let json = r#"{
            "id": "abc-123",
            "name": "Unknown Launch",
            "net": "2026-03-01T00:00:00Z",
            "net_precision": null,
            "window_start": null,
            "window_end": null,
            "status": { "id": 2, "name": "To Be Determined", "abbrev": "TBD" },
            "launch_service_provider": { "name": "Unknown" },
            "pad": { "location": { "name": "Unknown" } },
            "mission": null
        }"#;

        let launch: Ll2Launch = serde_json::from_str(json).expect("should deserialize");
        assert!(launch.net_precision.is_none());
        assert!(launch.mission.is_none());
        assert!(launch.launch_service_provider.provider_type.is_none());
        assert!(launch.pad.name.is_none());
        assert!(launch.pad.location.timezone_name.is_none());
        assert!(launch.pad.location.country.is_none());
    }

    /// JSON payload matching the LL2 `/launch/{id}/` detailed response shape.
    const SAMPLE_DETAIL_JSON: &str = r#"{
        "id": "e3df2ecd-c239-472f-95e4-2b89b4f75800",
        "url": "https://ll.thespacedevs.com/2.3.0/launches/e3df2ecd/",
        "name": "Starship IFT-7",
        "response_mode": "detailed",
        "slug": "starship-ift-7",
        "status": {
            "id": 1,
            "name": "Go for Launch",
            "abbrev": "Go",
            "description": "Current launch date is a go."
        },
        "net": "2026-02-28T09:00:00Z",
        "net_precision": { "id": 1, "name": "Day", "abbrev": "Day" },
        "window_start": "2026-02-28T09:00:00Z",
        "window_end": "2026-02-28T12:00:00Z",
        "image": "https://example.com/starship.jpg",
        "probability": 90,
        "weather_concerns": "No concerns",
        "failreason": "",
        "hashtag": null,
        "launch_service_provider": {
            "response_mode": "normal",
            "id": 121,
            "name": "SpaceX",
            "type": "Commercial",
            "total_launch_count": 301,
            "successful_launches": 295,
            "failed_launches": 6
        },
        "rocket": {
            "id": 1000,
            "configuration": {
                "response_mode": "detailed",
                "id": 207,
                "name": "Starship",
                "full_name": "Starship (Super Heavy + Starship)",
                "variant": "Block 1"
            }
        },
        "pad": {
            "name": "Orbital Launch Mount A",
            "location": {
                "name": "Starbase, Texas",
                "timezone_name": "America/Chicago",
                "country": {
                    "name": "United States of America",
                    "alpha_2_code": "US"
                }
            }
        },
        "mission": {
            "id": 5000,
            "name": "Starship IFT-7",
            "type": "Test Flight",
            "description": "Seventh integrated flight test of the Starship system.",
            "orbit": { "id": 8, "name": "Low Earth Orbit", "abbrev": "LEO" },
            "info_urls": [],
            "vid_urls": []
        },
        "program": [
            { "id": 1, "name": "Starship Development", "url": "https://example.com" }
        ],
        "infoURLs": [
            { "priority": 1, "title": "SpaceX Info", "url": "https://spacex.com/ift7" }
        ],
        "vidURLs": [
            { "priority": 1, "title": "Webcast", "url": "https://youtube.com/watch?v=abc", "source": "YouTube" }
        ],
        "webcast_live": false
    }"#;

    #[test]
    fn deserialize_launch_detail() {
        let detail: Ll2LaunchDetail =
            serde_json::from_str(SAMPLE_DETAIL_JSON).expect("should deserialize detail");

        assert_eq!(detail.id, "e3df2ecd-c239-472f-95e4-2b89b4f75800");
        assert_eq!(detail.name, "Starship IFT-7");
        assert_eq!(detail.status.id, 1);
        assert_eq!(detail.probability, Some(90));
        assert_eq!(detail.weather_concerns.as_deref(), Some("No concerns"));
        assert_eq!(
            detail.image.as_deref(),
            Some("https://example.com/starship.jpg")
        );

        // Provider with launch stats
        assert_eq!(detail.launch_service_provider.name, "SpaceX");
        assert_eq!(detail.launch_service_provider.total_launch_count, Some(301));
        assert_eq!(
            detail.launch_service_provider.successful_launches,
            Some(295)
        );
        assert_eq!(detail.launch_service_provider.failed_launches, Some(6));

        // Rocket
        let config = detail
            .rocket
            .as_ref()
            .unwrap()
            .configuration
            .as_ref()
            .unwrap();
        assert_eq!(
            config.full_name.as_deref(),
            Some("Starship (Super Heavy + Starship)")
        );

        // Mission
        let mission = detail.mission.as_ref().unwrap();
        assert_eq!(mission.name, "Starship IFT-7");

        // Top-level URLs (real API puts them here, not in mission)
        assert_eq!(detail.info_urls.len(), 1);
        assert_eq!(detail.info_urls[0].url, "https://spacex.com/ift7");
        assert_eq!(detail.vid_urls.len(), 1);
        assert_eq!(detail.vid_urls[0].title.as_deref(), Some("Webcast"));

        // Programs
        assert_eq!(detail.program.len(), 1);
        assert_eq!(detail.program[0].name, "Starship Development");
    }

    #[test]
    fn deserialize_launch_detail_with_nulls() {
        let json = r#"{
            "id": "abc-123",
            "name": "Unknown Launch",
            "net": "2026-03-01T00:00:00Z",
            "status": { "id": 2, "name": "TBD", "abbrev": "TBD" },
            "launch_service_provider": { "name": "Unknown" },
            "pad": { "location": { "name": "Unknown" } }
        }"#;

        let detail: Ll2LaunchDetail = serde_json::from_str(json).expect("should deserialize");
        assert!(detail.probability.is_none());
        assert!(detail.weather_concerns.is_none());
        assert!(detail.image.is_none());
        assert!(detail.rocket.is_none());
        assert!(detail.mission.is_none());
        assert!(detail.program.is_empty());
    }

    #[test]
    fn deserialize_throttle_response() {
        let json = r#"{
            "your_request_limit": 15,
            "limit_frequency_secs": 3600,
            "current_use": 5,
            "next_use_secs": 0,
            "ident": "88.97.214.56"
        }"#;

        let resp: Ll2ThrottleResponse = serde_json::from_str(json).expect("should deserialize");
        assert_eq!(resp.your_request_limit, 15);
        assert_eq!(resp.current_use, 5);
        assert_eq!(resp.limit_frequency_secs, Some(3600));
        assert_eq!(resp.next_use_secs, Some(0));
    }

    #[test]
    fn deserialize_ignores_unknown_fields() {
        let json = r#"{
            "id": "abc-123",
            "name": "Test Launch",
            "net": "2026-03-01T00:00:00Z",
            "status": { "id": 1, "name": "Go", "abbrev": "Go" },
            "launch_service_provider": { "name": "SpaceX" },
            "pad": { "location": { "name": "KSC" } },
            "mission": null,
            "slug": "test-launch",
            "url": "https://example.com",
            "webcast_live": false,
            "image": null,
            "program": [],
            "rocket": { "id": 1 }
        }"#;

        // Should not fail on unknown fields — serde defaults to ignoring them.
        let launch: Ll2Launch = serde_json::from_str(json).expect("should ignore unknown fields");
        assert_eq!(launch.name, "Test Launch");
    }
}
