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
    #[serde(rename = "type")]
    pub provider_type: Option<Ll2ProviderType>,
}

#[derive(Debug, Deserialize)]
pub struct Ll2ProviderType {
    pub name: String,
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
                    "type": {
                        "name": "Commercial"
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
            launch.launch_service_provider.provider_type.as_ref().unwrap().name,
            "Commercial"
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
