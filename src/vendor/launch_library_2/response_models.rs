//! Serde models matching Launch Library 2 API response shapes.
//!
//! These types exist solely to deserialize LL2 JSON responses. Application
//! logic should use the core domain types from [`crate::models`] instead.

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// Deserialize a field that may be either a plain string or an object with a
/// `name` key (e.g. `"Commercial"` vs `{"id":3,"name":"Commercial"}`).
/// Returns `Option<String>` — the name in both cases, or `None` if null/absent.
mod string_or_named_object {
    use serde::de::{self, Deserializer, MapAccess, Visitor};
    use std::fmt;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StringOrObject;

        impl<'de> Visitor<'de> for StringOrObject {
            type Value = Option<String>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a string, an object with a \"name\" field, or null")
            }

            fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(None)
            }

            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(None)
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(Some(v.to_owned()))
            }

            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(Some(v))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut name: Option<String> = None;
                while let Some(key) = map.next_key::<&str>()? {
                    if key == "name" {
                        name = Some(map.next_value()?);
                    } else {
                        map.next_value::<de::IgnoredAny>()?;
                    }
                }
                Ok(name)
            }
        }

        deserializer.deserialize_any(StringOrObject)
    }
}

/// Deserialize an image field that may be either a URL string or an object
/// with an `image_url` key. Returns `Option<String>` — the URL in both cases.
mod string_or_image_object {
    use serde::de::{self, Deserializer, MapAccess, Visitor};
    use std::fmt;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StringOrImage;

        impl<'de> Visitor<'de> for StringOrImage {
            type Value = Option<String>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a URL string, an object with an \"image_url\" field, or null")
            }

            fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(None)
            }

            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(None)
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(Some(v.to_owned()))
            }

            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(Some(v))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut image_url: Option<String> = None;
                while let Some(key) = map.next_key::<&str>()? {
                    if key == "image_url" {
                        image_url = Some(map.next_value()?);
                    } else {
                        map.next_value::<de::IgnoredAny>()?;
                    }
                }
                Ok(image_url)
            }
        }

        deserializer.deserialize_any(StringOrImage)
    }
}

/// Paginated response wrapper used by all LL2 list endpoints.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
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
#[allow(dead_code)]
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
    #[serde(rename = "type", default, deserialize_with = "string_or_named_object::deserialize")]
    pub provider_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Ll2Pad {
    #[serde(default)]
    pub name: Option<String>,
    pub location: Ll2Location,
    #[serde(default)]
    pub total_launch_count: Option<u32>,
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
    /// Three-letter ISO country code. Present on provider-level `country[]`
    /// entries (used for the provider's "Founded {year} · {alpha_3}" line);
    /// absent on pad/location country objects.
    #[serde(default)]
    pub alpha_3_code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Ll2Mission {
    pub name: String,
    #[serde(rename = "type", default, deserialize_with = "string_or_named_object::deserialize")]
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
    pub failreason: Option<String>,
    #[serde(default, deserialize_with = "string_or_image_object::deserialize")]
    pub image: Option<String>,
    pub launch_service_provider: Ll2ProviderDetail,
    #[serde(default)]
    pub rocket: Option<Ll2Rocket>,
    pub pad: Ll2Pad,
    #[serde(default)]
    pub mission: Option<Ll2MissionDetail>,
    #[serde(default)]
    pub program: Vec<Ll2Program>,
    #[serde(default)]
    pub updates: Vec<Ll2LaunchUpdate>,
    // The LL2 v2.3.0 API uses snake_case (`vid_urls` / `info_urls`); older
    // v2.2.0 responses (and any caches written against them) use camelCase.
    // Accept both via `alias` so the active field name is canonical.
    #[serde(alias = "vidURLs", default)]
    pub vid_urls: Vec<Ll2UrlEntry>,
    #[serde(alias = "infoURLs", default)]
    pub info_urls: Vec<Ll2UrlEntry>,
}

/// Provider in detailed response mode (full agency with launch statistics).
#[derive(Debug, Deserialize)]
pub struct Ll2ProviderDetail {
    pub name: String,
    #[serde(rename = "type", default, deserialize_with = "string_or_named_object::deserialize")]
    pub provider_type: Option<String>,
    #[serde(default)]
    pub total_launch_count: Option<u32>,
    #[serde(default)]
    pub successful_launches: Option<u32>,
    #[serde(default)]
    pub failed_launches: Option<u32>,
    /// Country array (each entry has `alpha_3_code`). Empty when absent.
    #[serde(default)]
    pub country: Vec<Ll2Country>,
    #[serde(default)]
    pub founding_year: Option<u32>,
    #[serde(default)]
    pub consecutive_successful_launches: Option<u32>,
}

/// Rocket configuration in detailed response mode.
#[derive(Debug, Deserialize)]
pub struct Ll2Rocket {
    #[serde(default)]
    pub configuration: Option<Ll2RocketConfiguration>,
    #[serde(default)]
    pub launcher_stage: Vec<Ll2LauncherStage>,
    #[serde(default)]
    pub spacecraft_stage: Vec<Ll2SpacecraftStage>,
}

/// Rocket configuration details.
#[derive(Debug, Default, Deserialize)]
pub struct Ll2RocketConfiguration {
    #[serde(default)]
    pub full_name: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub length: Option<f64>,
    #[serde(default)]
    pub diameter: Option<f64>,
    #[serde(default)]
    pub launch_mass: Option<f64>,
    #[serde(default)]
    pub leo_capacity: Option<f64>,
    #[serde(default)]
    pub gto_capacity: Option<f64>,
    #[serde(default)]
    pub to_thrust: Option<f64>,
    #[serde(default)]
    pub maiden_flight: Option<String>,
    #[serde(default)]
    pub total_launch_count: Option<u32>,
    #[serde(default)]
    pub successful_launches: Option<u32>,
    #[serde(default)]
    pub failed_launches: Option<u32>,
    #[serde(default)]
    pub consecutive_successful_launches: Option<u32>,
}

/// Launcher (booster) stage entry in `rocket.launcher_stage[]`.
#[derive(Debug, Deserialize)]
pub struct Ll2LauncherStage {
    /// Stage type label (e.g. "Core", "Strap-on Booster"). `type` is a Rust
    /// keyword so the JSON key is renamed.
    #[serde(rename = "type", default)]
    pub stage_type: Option<String>,
    #[serde(default)]
    pub landing: Option<Ll2Landing>,
}

/// Landing record attached to a launcher stage.
#[derive(Debug, Deserialize)]
pub struct Ll2Landing {
    #[serde(default)]
    pub attempt: bool,
    #[serde(default)]
    pub success: Option<bool>,
    #[serde(rename = "type", default)]
    pub landing_type: Option<Ll2LandingType>,
    #[serde(default)]
    pub landing_location: Option<Ll2LandingLocation>,
}

/// Landing type (e.g. ASDS, RTLS).
#[derive(Debug, Deserialize)]
pub struct Ll2LandingType {
    #[serde(default)]
    pub abbrev: Option<String>,
}

/// Specific landing location (e.g. drone ship name).
#[derive(Debug, Deserialize)]
pub struct Ll2LandingLocation {
    #[serde(default)]
    pub name: Option<String>,
}

/// Spacecraft stage entry in `rocket.spacecraft_stage[]` (used for crewed
/// missions to expose `launch_crew`).
#[derive(Debug, Deserialize)]
pub struct Ll2SpacecraftStage {
    #[serde(default)]
    pub launch_crew: Vec<Ll2CrewEntry>,
}

/// One crew entry from a spacecraft stage's `launch_crew[]`.
#[derive(Debug, Deserialize)]
pub struct Ll2CrewEntry {
    #[serde(default)]
    pub role: Option<Ll2Role>,
    #[serde(default)]
    pub astronaut: Option<Ll2Astronaut>,
}

/// Crew role wrapper. The display string is at `.role` (not `.name`).
#[derive(Debug, Deserialize)]
pub struct Ll2Role {
    #[serde(default)]
    pub role: Option<String>,
}

/// Astronaut record (subset — only what we render).
#[derive(Debug, Deserialize)]
pub struct Ll2Astronaut {
    pub name: String,
    #[serde(default)]
    pub agency: Option<Ll2AstronautAgency>,
}

/// Astronaut's agency (subset — name + abbreviation only).
#[derive(Debug, Deserialize)]
pub struct Ll2AstronautAgency {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub abbrev: Option<String>,
}

/// Status update posted against a launch (LL2 `updates[]`).
#[derive(Debug, Deserialize)]
pub struct Ll2LaunchUpdate {
    pub comment: String,
    pub created_on: DateTime<Utc>,
    #[serde(default)]
    pub info_url: Option<String>,
}

/// Mission in detailed response mode (includes `info_urls` and `vid_urls`).
#[derive(Debug, Deserialize)]
pub struct Ll2MissionDetail {
    pub name: String,
    #[serde(rename = "type", default, deserialize_with = "string_or_named_object::deserialize")]
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
    /// Whichever identity the server rate-limited this call under. For an
    /// authenticated client that is the API key itself, so this must never
    /// reach a log verbatim — see `ident_for_log`.
    #[serde(default)]
    pub ident: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vendor::launch_library_2::fixtures;

    #[test]
    fn deserialize_paginated_launch_list() {
        let response: PaginatedResponse<Ll2Launch> =
            serde_json::from_str(fixtures::STARSHIP_LIST).expect("should deserialize");

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

    #[test]
    fn deserialize_launch_detail() {
        let detail: Ll2LaunchDetail =
            serde_json::from_str(fixtures::STARSHIP_DETAIL).expect("should deserialize detail");

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

    /// Older LL2 v2.2.0 responses (and any pre-v2.3.0 cached payloads) use
    /// camelCase `vidURLs` / `infoURLs`. The serde alias must keep these
    /// deserializing into the same fields as the canonical snake_case names.
    #[test]
    fn deserialize_launch_detail_accepts_legacy_camelcase_url_fields() {
        let json = r#"{
            "id": "legacy-1",
            "name": "Legacy Launch",
            "net": "2026-03-01T00:00:00Z",
            "status": { "id": 1, "name": "Go for Launch", "abbrev": "Go" },
            "launch_service_provider": { "name": "SpaceX" },
            "pad": { "location": { "name": "Starbase" } },
            "infoURLs": [
                { "priority": 1, "title": "Info", "url": "https://example.com/info" }
            ],
            "vidURLs": [
                { "priority": 1, "title": "Stream", "url": "https://example.com/stream" }
            ]
        }"#;

        let detail: Ll2LaunchDetail =
            serde_json::from_str(json).expect("legacy camelCase URLs should deserialize");
        assert_eq!(detail.info_urls.len(), 1);
        assert_eq!(detail.info_urls[0].url, "https://example.com/info");
        assert_eq!(detail.vid_urls.len(), 1);
        assert_eq!(detail.vid_urls[0].title.as_deref(), Some("Stream"));
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
        assert_eq!(resp.ident.as_deref(), Some("88.97.214.56"));
    }

    #[test]
    fn deserialize_throttle_response_without_optional_fields() {
        let json = r#"{ "your_request_limit": 15, "current_use": 5 }"#;

        let resp: Ll2ThrottleResponse = serde_json::from_str(json).expect("should deserialize");
        assert_eq!(resp.your_request_limit, 15);
        assert!(resp.limit_frequency_secs.is_none());
        assert!(resp.next_use_secs.is_none());
        assert!(resp.ident.is_none());
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

    // --- string_or_named_object resilience tests ---

    #[test]
    fn deserialize_provider_type_as_object() {
        let json = r#"{
            "id": "abc-123",
            "name": "Test Launch",
            "net": "2026-03-01T00:00:00Z",
            "status": { "id": 1, "name": "Go", "abbrev": "Go" },
            "launch_service_provider": {
                "name": "SpaceX",
                "type": { "id": 3, "name": "Commercial" }
            },
            "pad": { "location": { "name": "KSC" } },
            "mission": null
        }"#;

        let launch: Ll2Launch = serde_json::from_str(json).expect("should handle object type");
        assert_eq!(
            launch.launch_service_provider.provider_type.as_deref(),
            Some("Commercial")
        );
    }

    #[test]
    fn deserialize_mission_type_as_object() {
        let json = r#"{
            "id": "abc-123",
            "name": "Test Launch",
            "net": "2026-03-01T00:00:00Z",
            "status": { "id": 1, "name": "Go", "abbrev": "Go" },
            "launch_service_provider": { "name": "SpaceX" },
            "pad": { "location": { "name": "KSC" } },
            "mission": {
                "name": "Test Mission",
                "type": { "id": 5, "name": "Communications" },
                "description": null,
                "orbit": null
            }
        }"#;

        let launch: Ll2Launch = serde_json::from_str(json).expect("should handle object type");
        let mission = launch.mission.unwrap();
        assert_eq!(mission.mission_type.as_deref(), Some("Communications"));
    }

    #[test]
    fn deserialize_detail_with_object_types() {
        let json = r#"{
            "id": "abc-123",
            "name": "Test Launch",
            "net": "2026-03-01T00:00:00Z",
            "status": { "id": 1, "name": "Go", "abbrev": "Go" },
            "launch_service_provider": {
                "name": "SpaceX",
                "type": { "id": 3, "name": "Commercial" },
                "total_launch_count": 301,
                "successful_launches": 295,
                "failed_launches": 6
            },
            "pad": { "location": { "name": "KSC" } },
            "image": {
                "id": 42,
                "name": "Starship Launch",
                "image_url": "https://example.com/starship.jpg",
                "thumbnail_url": "https://example.com/starship_thumb.jpg"
            },
            "mission": {
                "name": "Test Mission",
                "type": { "id": 5, "name": "Communications" },
                "description": null,
                "orbit": null,
                "info_urls": [],
                "vid_urls": []
            }
        }"#;

        let detail: Ll2LaunchDetail =
            serde_json::from_str(json).expect("should handle object types in detail");
        assert_eq!(
            detail.launch_service_provider.provider_type.as_deref(),
            Some("Commercial")
        );
        assert_eq!(
            detail.image.as_deref(),
            Some("https://example.com/starship.jpg")
        );
        assert_eq!(
            detail.mission.unwrap().mission_type.as_deref(),
            Some("Communications")
        );
    }

    #[test]
    fn deserialize_image_as_string() {
        let json = r#"{
            "id": "abc-123",
            "name": "Test",
            "net": "2026-03-01T00:00:00Z",
            "status": { "id": 1, "name": "Go", "abbrev": "Go" },
            "launch_service_provider": { "name": "Test" },
            "pad": { "location": { "name": "KSC" } },
            "image": "https://example.com/image.jpg"
        }"#;

        let detail: Ll2LaunchDetail =
            serde_json::from_str(json).expect("should handle string image");
        assert_eq!(
            detail.image.as_deref(),
            Some("https://example.com/image.jpg")
        );
    }

    #[test]
    fn deserialize_falcon9_sample_detail() {
        let detail: Ll2LaunchDetail = serde_json::from_str(fixtures::FALCON9_DETAIL)
            .expect("Falcon 9 fixture should deserialize");

        // `failreason` is present as an empty string on a non-failed launch.
        assert_eq!(detail.failreason.as_deref(), Some(""));

        // Updates: 15 entries; first carries a known comment + timestamp.
        assert_eq!(detail.updates.len(), 15);
        let first_update = &detail.updates[0];
        assert_eq!(first_update.comment, "Now targeting Apr 10 at 02:39 UTC");
        assert_eq!(
            first_update.created_on,
            "2026-04-02T20:45:00Z".parse::<DateTime<Utc>>().unwrap()
        );

        // Launcher stage: one Core booster with an ASDS landing on OCISLY.
        let rocket = detail.rocket.as_ref().expect("rocket present");
        assert_eq!(rocket.launcher_stage.len(), 1);
        let stage = &rocket.launcher_stage[0];
        assert_eq!(stage.stage_type.as_deref(), Some("Core"));
        let landing = stage.landing.as_ref().expect("landing present");
        assert!(landing.attempt);
        assert_eq!(landing.success, Some(true));
        assert_eq!(
            landing.landing_type.as_ref().and_then(|t| t.abbrev.as_deref()),
            Some("ASDS")
        );
        assert_eq!(
            landing
                .landing_location
                .as_ref()
                .and_then(|l| l.name.as_deref()),
            Some("Of Course I Still Love You")
        );

        // Spacecraft stage absent for an uncrewed mission.
        assert!(rocket.spacecraft_stage.is_empty());

        // Configuration specs — the headline numbers shown in the Vehicle box.
        let config = rocket.configuration.as_ref().expect("configuration present");
        assert_eq!(config.variant.as_deref(), Some("Block 5"));
        assert_eq!(config.length, Some(70.0));
        assert_eq!(config.diameter, Some(3.65));
        assert_eq!(config.launch_mass, Some(549.0));
        assert_eq!(config.leo_capacity, Some(22800.0));
        assert_eq!(config.gto_capacity, Some(8300.0));
        assert_eq!(config.to_thrust, Some(7607.0));
        assert_eq!(config.maiden_flight.as_deref(), Some("2018-05-11"));
        assert_eq!(config.total_launch_count, Some(570));
        assert_eq!(config.successful_launches, Some(569));
        assert_eq!(config.failed_launches, Some(1));
        assert_eq!(config.consecutive_successful_launches, Some(272));

        // Provider — country array with alpha_3, founding year, consecutive.
        let provider = &detail.launch_service_provider;
        assert_eq!(provider.country.len(), 1);
        assert_eq!(provider.country[0].alpha_3_code.as_deref(), Some("USA"));
        assert_eq!(provider.founding_year, Some(2002));
        assert_eq!(provider.consecutive_successful_launches, Some(148));

        // Pad-level launch count.
        assert_eq!(detail.pad.total_launch_count, Some(259));
    }

    #[test]
    fn deserialize_soyuz_crewed_sample_detail() {
        let detail: Ll2LaunchDetail = serde_json::from_str(fixtures::SOYUZ_CREWED_DETAIL)
            .expect("Soyuz fixture should deserialize");

        let rocket = detail.rocket.as_ref().expect("rocket present");

        // Crewed launch: spacecraft_stage carries the crew, launcher_stage is empty.
        assert!(rocket.launcher_stage.is_empty());
        assert_eq!(rocket.spacecraft_stage.len(), 1);

        let crew = &rocket.spacecraft_stage[0].launch_crew;
        assert_eq!(crew.len(), 3);

        let first = &crew[0];
        let astronaut = first.astronaut.as_ref().expect("astronaut present");
        assert_eq!(astronaut.name, "Pyotr Dubrov");
        assert_eq!(
            first.role.as_ref().and_then(|r| r.role.as_deref()),
            Some("Commander")
        );
        assert_eq!(
            astronaut.agency.as_ref().and_then(|a| a.abbrev.as_deref()),
            Some("RFSA")
        );
    }

    /// All Step 2 fields are optional / `#[serde(default)]` — a minimal
    /// payload with none of them present must still deserialize.
    #[test]
    fn deserialize_launch_detail_tolerates_missing_step2_fields() {
        let json = r#"{
            "id": "minimal",
            "name": "Minimal Launch",
            "net": "2026-03-01T00:00:00Z",
            "status": { "id": 1, "name": "Go", "abbrev": "Go" },
            "launch_service_provider": { "name": "Unknown" },
            "pad": { "location": { "name": "Unknown" } }
        }"#;

        let detail: Ll2LaunchDetail = serde_json::from_str(json).expect("should deserialize");
        assert!(detail.failreason.is_none());
        assert!(detail.updates.is_empty());
        assert!(detail.launch_service_provider.country.is_empty());
        assert!(detail.launch_service_provider.founding_year.is_none());
        assert!(detail
            .launch_service_provider
            .consecutive_successful_launches
            .is_none());
        assert!(detail.pad.total_launch_count.is_none());
    }

    #[test]
    fn deserialize_image_as_null() {
        let json = r#"{
            "id": "abc-123",
            "name": "Test",
            "net": "2026-03-01T00:00:00Z",
            "status": { "id": 1, "name": "Go", "abbrev": "Go" },
            "launch_service_provider": { "name": "Test" },
            "pad": { "location": { "name": "KSC" } },
            "image": null
        }"#;

        let detail: Ll2LaunchDetail =
            serde_json::from_str(json).expect("should handle null image");
        assert!(detail.image.is_none());
    }
}
