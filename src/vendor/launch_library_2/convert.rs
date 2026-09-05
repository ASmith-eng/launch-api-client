//! Conversion from LL2 vendor response models to core domain types.

use crate::models::{
    CountryInfo, CrewMember, LaunchDetail, LaunchStatus, LaunchSummary, LaunchUpdate, LocationInfo,
    MissionSummary, NetPrecision, OrbitInfo, PadInfo, Probability, Provider, StageLanding,
    ThrottleStatus, UrlEntry,
};

use super::response_models::{
    Ll2Country, Ll2CrewEntry, Ll2Launch, Ll2LaunchDetail, Ll2LaunchUpdate, Ll2LauncherStage,
    Ll2Location, Ll2Mission, Ll2MissionDetail, Ll2NetPrecision, Ll2Orbit, Ll2Pad, Ll2Provider,
    Ll2ProviderDetail, Ll2RocketConfiguration, Ll2Status, Ll2ThrottleResponse, Ll2UrlEntry,
};

impl From<Ll2Launch> for LaunchSummary {
    fn from(ll2: Ll2Launch) -> Self {
        Self {
            id: ll2.id,
            name: ll2.name,
            net: ll2.net,
            net_precision: ll2.net_precision.map(Into::into),
            window_start: ll2.window_start,
            window_end: ll2.window_end,
            status: ll2.status.into(),
            launch_service_provider: ll2.launch_service_provider.into(),
            pad: ll2.pad.into(),
            mission: ll2.mission.map(Into::into),
        }
    }
}

impl From<Ll2Status> for LaunchStatus {
    fn from(ll2: Ll2Status) -> Self {
        Self {
            id: ll2.id,
            name: ll2.name,
            abbrev: ll2.abbrev,
        }
    }
}

impl From<Ll2NetPrecision> for NetPrecision {
    fn from(ll2: Ll2NetPrecision) -> Self {
        Self {
            id: ll2.id,
            name: ll2.name,
            abbrev: ll2.abbrev,
        }
    }
}

impl From<Ll2Provider> for Provider {
    fn from(ll2: Ll2Provider) -> Self {
        Self {
            name: ll2.name,
            provider_type: ll2.provider_type,
        }
    }
}

impl From<Ll2Pad> for PadInfo {
    fn from(ll2: Ll2Pad) -> Self {
        Self {
            name: ll2.name,
            location: ll2.location.into(),
        }
    }
}

impl From<Ll2Location> for LocationInfo {
    fn from(ll2: Ll2Location) -> Self {
        Self {
            name: ll2.name,
            timezone_name: ll2.timezone_name,
            country: ll2.country.map(Into::into),
        }
    }
}

impl From<Ll2Country> for CountryInfo {
    fn from(ll2: Ll2Country) -> Self {
        Self {
            name: ll2.name,
            alpha_2_code: ll2.alpha_2_code,
        }
    }
}

impl From<Ll2Mission> for MissionSummary {
    fn from(ll2: Ll2Mission) -> Self {
        Self {
            name: ll2.name,
            mission_type: ll2.mission_type.unwrap_or_default(),
            description: ll2.description,
            orbit: ll2.orbit.map(Into::into),
        }
    }
}

impl From<Ll2Orbit> for OrbitInfo {
    fn from(ll2: Ll2Orbit) -> Self {
        Self {
            id: ll2.id,
            name: ll2.name,
            abbrev: ll2.abbrev,
        }
    }
}

/// Convert an empty string to `None`.
///
/// Several LL2 fields (notably `failreason` and the rocket `variant`) use
/// `""` rather than `null` to signal an absent value.
fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.is_empty())
}

impl From<Ll2LaunchUpdate> for LaunchUpdate {
    fn from(ll2: Ll2LaunchUpdate) -> Self {
        Self {
            comment: ll2.comment,
            created_on: ll2.created_on,
            info_url: ll2.info_url,
        }
    }
}

impl From<Ll2CrewEntry> for CrewMember {
    fn from(ll2: Ll2CrewEntry) -> Self {
        let role = ll2.role.and_then(|r| r.role).unwrap_or_default();
        let (name, agency) = match ll2.astronaut {
            Some(astronaut) => {
                // Prefer the compact agency abbreviation for the TUI crew
                // column; fall back to the full agency name when absent.
                let agency = astronaut
                    .agency
                    .and_then(|a| a.abbrev.or(a.name))
                    .unwrap_or_default();
                (astronaut.name, agency)
            }
            None => (String::new(), String::new()),
        };
        Self { name, role, agency }
    }
}

impl From<Ll2LauncherStage> for StageLanding {
    fn from(ll2: Ll2LauncherStage) -> Self {
        let (landing_attempt, landing_success, landing_type, landing_location) = match ll2.landing {
            Some(landing) => (
                landing.attempt,
                landing.success,
                landing.landing_type.and_then(|t| t.abbrev),
                landing.landing_location.and_then(|l| l.name),
            ),
            None => (false, None, None, None),
        };
        Self {
            stage_type: ll2.stage_type,
            landing_attempt,
            landing_success,
            landing_type,
            landing_location,
        }
    }
}

impl From<Ll2LaunchDetail> for LaunchDetail {
    fn from(ll2: Ll2LaunchDetail) -> Self {
        // Unpack the rocket once: `configuration` carries the vehicle specs,
        // `launcher_stage` the landing records, `spacecraft_stage` the crew.
        let (config, launcher_stages, spacecraft_stages) = match ll2.rocket {
            Some(rocket) => (
                rocket.configuration.unwrap_or_default(),
                rocket.launcher_stage,
                rocket.spacecraft_stage,
            ),
            None => (Ll2RocketConfiguration::default(), Vec::new(), Vec::new()),
        };

        let rocket_full_name = config.full_name.or(config.name);

        // Crew rides on the first spacecraft stage (crewed flights only);
        // uncrewed flights have no spacecraft stage, hence an empty crew.
        let crew = spacecraft_stages
            .into_iter()
            .next()
            .map(|stage| {
                stage
                    .launch_crew
                    .into_iter()
                    .map(CrewMember::from)
                    .collect()
            })
            .unwrap_or_default();

        let landings = launcher_stages
            .into_iter()
            .map(StageLanding::from)
            .collect();

        let updates = ll2.updates.into_iter().map(LaunchUpdate::from).collect();

        // Provider country code is the first `country[]` entry's three-letter
        // code; `None` when the array is empty.
        let provider_country_code = ll2
            .launch_service_provider
            .country
            .into_iter()
            .next()
            .and_then(|c| c.alpha_3_code);

        // Read the pad launch count before `pad` is moved into `Self` below.
        let pad_total_launch_count = ll2.pad.total_launch_count;

        // Prefer top-level vidURLs/infoURLs (where the real data lives),
        // falling back to mission-level urls if top-level is empty.
        let vid_urls = if !ll2.vid_urls.is_empty() {
            ll2.vid_urls.iter().map(UrlEntry::from).collect()
        } else {
            ll2.mission
                .as_ref()
                .map(|m| m.vid_urls.iter().map(UrlEntry::from).collect())
                .unwrap_or_default()
        };

        let info_urls = if !ll2.info_urls.is_empty() {
            ll2.info_urls.iter().map(UrlEntry::from).collect()
        } else {
            ll2.mission
                .as_ref()
                .map(|m| m.info_urls.iter().map(UrlEntry::from).collect())
                .unwrap_or_default()
        };

        let programs = ll2.program.into_iter().map(|p| p.name).collect();

        Self {
            id: ll2.id,
            name: ll2.name,
            net: ll2.net,
            net_precision: ll2.net_precision.map(Into::into),
            window_start: ll2.window_start,
            window_end: ll2.window_end,
            status: ll2.status.into(),
            // LL2 API uses -1 for "unknown"; we normalize to Option<Probability>.
            probability: ll2
                .probability
                .and_then(|p| u8::try_from(p).ok())
                .and_then(Probability::new),
            weather_concerns: ll2.weather_concerns,
            image_url: ll2.image,
            failreason: non_empty(ll2.failreason),
            updates,
            crew,
            landings,
            launch_service_provider: Provider {
                name: ll2.launch_service_provider.name.clone(),
                provider_type: ll2.launch_service_provider.provider_type.clone(),
            },
            provider_total_launches: ll2.launch_service_provider.total_launch_count,
            provider_successful_launches: ll2.launch_service_provider.successful_launches,
            provider_failed_launches: ll2.launch_service_provider.failed_launches,
            provider_country_code,
            provider_founding_year: ll2.launch_service_provider.founding_year,
            provider_consecutive_successes: ll2
                .launch_service_provider
                .consecutive_successful_launches,
            rocket_full_name,
            rocket_variant: non_empty(config.variant),
            rocket_description: config.description,
            rocket_length: config.length,
            rocket_diameter: config.diameter,
            rocket_launch_mass: config.launch_mass,
            rocket_leo_capacity: config.leo_capacity,
            rocket_gto_capacity: config.gto_capacity,
            rocket_thrust: config.to_thrust,
            rocket_maiden_flight: config.maiden_flight,
            rocket_total_launches: config.total_launch_count,
            rocket_successful_launches: config.successful_launches,
            rocket_failed_launches: config.failed_launches,
            rocket_consecutive_successes: config.consecutive_successful_launches,
            pad: ll2.pad.into(),
            pad_total_launch_count,
            mission: ll2.mission.map(Into::into),
            vid_urls,
            info_urls,
            programs,
        }
    }
}

impl From<Ll2MissionDetail> for MissionSummary {
    fn from(ll2: Ll2MissionDetail) -> Self {
        Self {
            name: ll2.name,
            mission_type: ll2.mission_type.unwrap_or_default(),
            description: ll2.description,
            orbit: ll2.orbit.map(Into::into),
        }
    }
}

impl From<Ll2ProviderDetail> for Provider {
    fn from(ll2: Ll2ProviderDetail) -> Self {
        Self {
            name: ll2.name,
            provider_type: ll2.provider_type,
        }
    }
}

impl From<&Ll2UrlEntry> for UrlEntry {
    fn from(ll2: &Ll2UrlEntry) -> Self {
        Self {
            title: ll2.title.clone(),
            url: ll2.url.clone(),
        }
    }
}

impl From<Ll2ThrottleResponse> for ThrottleStatus {
    fn from(ll2: Ll2ThrottleResponse) -> Self {
        let limit = ll2.your_request_limit as usize;
        let remaining = limit.saturating_sub(ll2.current_use as usize);
        Self { remaining, limit }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vendor::launch_library_2::fixtures;
    use crate::vendor::launch_library_2::response_models::*;
    use chrono::{DateTime, Utc};

    fn sample_ll2_launch() -> Ll2Launch {
        Ll2Launch {
            id: "e3df2ecd-c239-472f-95e4-2b89b4f75800".into(),
            name: "Starship IFT-7".into(),
            net: "2026-02-28T09:00:00Z".parse::<DateTime<Utc>>().unwrap(),
            net_precision: Some(Ll2NetPrecision {
                id: 1,
                name: "Day".into(),
                abbrev: "Day".into(),
            }),
            window_start: Some("2026-02-28T09:00:00Z".parse().unwrap()),
            window_end: Some("2026-02-28T12:00:00Z".parse().unwrap()),
            status: Ll2Status {
                id: 1,
                name: "Go for Launch".into(),
                abbrev: "Go".into(),
                description: None,
            },
            launch_service_provider: Ll2Provider {
                name: "SpaceX".into(),
                provider_type: Some("Commercial".into()),
            },
            pad: Ll2Pad {
                name: Some("Orbital Launch Mount A".into()),
                location: Ll2Location {
                    name: "Starbase, Texas".into(),
                    timezone_name: Some("America/Chicago".into()),
                    country: Some(Ll2Country {
                        name: "United States of America".into(),
                        alpha_2_code: "US".into(),
                        alpha_3_code: None,
                    }),
                },
                total_launch_count: None,
            },
            mission: Some(Ll2Mission {
                name: "Starship IFT-7".into(),
                mission_type: Some("Test Flight".into()),
                description: Some("Seventh integrated flight test.".into()),
                orbit: Some(Ll2Orbit {
                    id: 8,
                    name: "Low Earth Orbit".into(),
                    abbrev: "LEO".into(),
                }),
            }),
        }
    }

    #[test]
    fn convert_full_launch() {
        let ll2 = sample_ll2_launch();
        let summary: LaunchSummary = ll2.into();

        assert_eq!(summary.id, "e3df2ecd-c239-472f-95e4-2b89b4f75800");
        assert_eq!(summary.name, "Starship IFT-7");
        assert_eq!(summary.status.id, 1);
        assert_eq!(summary.status.abbrev, "Go");
        assert_eq!(summary.launch_service_provider.name, "SpaceX");
        assert_eq!(summary.launch_service_provider.provider_type.as_deref(), Some("Commercial"));
        assert_eq!(summary.pad.location.name, "Starbase, Texas");
        assert_eq!(summary.pad.location.timezone_name.as_deref(), Some("America/Chicago"));

        let mission = summary.mission.unwrap();
        assert_eq!(mission.name, "Starship IFT-7");
        assert_eq!(mission.mission_type, "Test Flight");
        assert_eq!(mission.orbit.unwrap().abbrev, "LEO");
    }

    #[test]
    fn convert_launch_with_missing_optionals() {
        let ll2 = Ll2Launch {
            id: "abc-123".into(),
            name: "Mystery Launch".into(),
            net: Utc::now(),
            net_precision: None,
            window_start: None,
            window_end: None,
            status: Ll2Status {
                id: 2,
                name: "To Be Determined".into(),
                abbrev: "TBD".into(),
                description: None,
            },
            launch_service_provider: Ll2Provider {
                name: "Unknown Corp".into(),
                provider_type: None,
            },
            pad: Ll2Pad {
                name: None,
                location: Ll2Location {
                    name: "Undisclosed".into(),
                    timezone_name: None,
                    country: None,
                },
                total_launch_count: None,
            },
            mission: None,
        };

        let summary: LaunchSummary = ll2.into();
        assert!(summary.net_precision.is_none());
        assert!(summary.window_start.is_none());
        assert!(summary.mission.is_none());
        assert!(summary.pad.name.is_none());
        assert!(summary.pad.location.country.is_none());
        assert!(summary.launch_service_provider.provider_type.is_none());
    }

    #[test]
    fn mission_type_defaults_to_empty_when_null() {
        let ll2_mission = Ll2Mission {
            name: "Test".into(),
            mission_type: None,
            description: None,
            orbit: None,
        };
        let mission: MissionSummary = ll2_mission.into();
        assert_eq!(mission.mission_type, "");
    }

    // --- LaunchDetail conversion tests ---

    fn sample_ll2_launch_detail() -> Ll2LaunchDetail {
        Ll2LaunchDetail {
            id: "e3df2ecd-c239-472f-95e4-2b89b4f75800".into(),
            name: "Starship IFT-7".into(),
            net: "2026-02-28T09:00:00Z".parse::<DateTime<Utc>>().unwrap(),
            net_precision: Some(Ll2NetPrecision {
                id: 1,
                name: "Day".into(),
                abbrev: "Day".into(),
            }),
            window_start: Some("2026-02-28T09:00:00Z".parse().unwrap()),
            window_end: Some("2026-02-28T12:00:00Z".parse().unwrap()),
            status: Ll2Status {
                id: 1,
                name: "Go for Launch".into(),
                abbrev: "Go".into(),
                description: None,
            },
            probability: Some(90),
            weather_concerns: Some("No concerns".into()),
            image: Some("https://example.com/starship.jpg".into()),
            launch_service_provider: Ll2ProviderDetail {
                name: "SpaceX".into(),
                provider_type: Some("Commercial".into()),
                total_launch_count: Some(301),
                successful_launches: Some(295),
                failed_launches: Some(6),
                country: vec![],
                founding_year: None,
                consecutive_successful_launches: None,
            },
            rocket: Some(Ll2Rocket {
                configuration: Some(Ll2RocketConfiguration {
                    full_name: Some("Starship (Super Heavy + Starship)".into()),
                    name: Some("Starship".into()),
                    variant: None,
                    description: None,
                    length: None,
                    diameter: None,
                    launch_mass: None,
                    leo_capacity: None,
                    gto_capacity: None,
                    to_thrust: None,
                    maiden_flight: None,
                    total_launch_count: None,
                    successful_launches: None,
                    failed_launches: None,
                    consecutive_successful_launches: None,
                }),
                launcher_stage: vec![],
                spacecraft_stage: vec![],
            }),
            pad: Ll2Pad {
                name: Some("Orbital Launch Mount A".into()),
                location: Ll2Location {
                    name: "Starbase, Texas".into(),
                    timezone_name: Some("America/Chicago".into()),
                    country: Some(Ll2Country {
                        name: "United States of America".into(),
                        alpha_2_code: "US".into(),
                        alpha_3_code: None,
                    }),
                },
                total_launch_count: None,
            },
            mission: Some(Ll2MissionDetail {
                name: "Starship IFT-7".into(),
                mission_type: Some("Test Flight".into()),
                description: Some("Seventh flight test.".into()),
                orbit: Some(Ll2Orbit {
                    id: 8,
                    name: "Low Earth Orbit".into(),
                    abbrev: "LEO".into(),
                }),
                info_urls: vec![],
                vid_urls: vec![],
            }),
            program: vec![Ll2Program {
                name: "Starship Development".into(),
            }],
            vid_urls: vec![Ll2UrlEntry {
                title: Some("Webcast".into()),
                url: "https://youtube.com/watch?v=abc".into(),
            }],
            info_urls: vec![Ll2UrlEntry {
                title: Some("SpaceX Info".into()),
                url: "https://spacex.com/ift7".into(),
            }],
            failreason: None,
            updates: vec![],
        }
    }

    #[test]
    fn convert_full_launch_detail() {
        let ll2 = sample_ll2_launch_detail();
        let detail: LaunchDetail = ll2.into();

        assert_eq!(detail.id, "e3df2ecd-c239-472f-95e4-2b89b4f75800");
        assert_eq!(detail.name, "Starship IFT-7");
        assert_eq!(detail.status.id, 1);
        assert_eq!(detail.probability, Probability::new(90));
        assert_eq!(detail.weather_concerns.as_deref(), Some("No concerns"));
        assert_eq!(detail.image_url.as_deref(), Some("https://example.com/starship.jpg"));
        assert_eq!(detail.provider_total_launches, Some(301));
        assert_eq!(detail.provider_successful_launches, Some(295));
        assert_eq!(detail.provider_failed_launches, Some(6));
        assert_eq!(detail.rocket_full_name.as_deref(), Some("Starship (Super Heavy + Starship)"));
        assert_eq!(detail.vid_urls.len(), 1);
        assert_eq!(detail.vid_urls[0].url, "https://youtube.com/watch?v=abc");
        assert_eq!(detail.info_urls.len(), 1);
        assert_eq!(detail.programs, vec!["Starship Development"]);
    }

    #[test]
    fn convert_detail_with_missing_optionals() {
        let ll2 = Ll2LaunchDetail {
            id: "abc-123".into(),
            name: "Unknown".into(),
            net: Utc::now(),
            net_precision: None,
            window_start: None,
            window_end: None,
            status: Ll2Status {
                id: 2,
                name: "TBD".into(),
                abbrev: "TBD".into(),
                description: None,
            },
            probability: None,
            weather_concerns: None,
            image: None,
            launch_service_provider: Ll2ProviderDetail {
                name: "Unknown".into(),
                provider_type: None,
                total_launch_count: None,
                successful_launches: None,
                failed_launches: None,
                country: vec![],
                founding_year: None,
                consecutive_successful_launches: None,
            },
            rocket: None,
            pad: Ll2Pad {
                name: None,
                location: Ll2Location {
                    name: "Unknown".into(),
                    timezone_name: None,
                    country: None,
                },
                total_launch_count: None,
            },
            mission: None,
            program: vec![],
            vid_urls: vec![],
            info_urls: vec![],
            failreason: None,
            updates: vec![],
        };

        let detail: LaunchDetail = ll2.into();
        assert!(detail.probability.is_none());
        assert!(detail.image_url.is_none());
        assert!(detail.rocket_full_name.is_none());
        assert!(detail.provider_total_launches.is_none());
        assert!(detail.vid_urls.is_empty());
        assert!(detail.info_urls.is_empty());
        assert!(detail.programs.is_empty());

        // New detail fields all default to absent when the API omits them.
        assert!(detail.failreason.is_none());
        assert!(detail.updates.is_empty());
        assert!(detail.crew.is_empty());
        assert!(detail.landings.is_empty());
        assert!(detail.rocket_variant.is_none());
        assert!(detail.rocket_length.is_none());
        assert!(detail.rocket_total_launches.is_none());
        assert!(detail.rocket_consecutive_successes.is_none());
        assert!(detail.provider_country_code.is_none());
        assert!(detail.provider_founding_year.is_none());
        assert!(detail.provider_consecutive_successes.is_none());
        assert!(detail.pad_total_launch_count.is_none());
    }

    #[test]
    fn convert_rocket_falls_back_to_name() {
        let ll2 = Ll2LaunchDetail {
            id: "test".into(),
            name: "Test".into(),
            net: Utc::now(),
            net_precision: None,
            window_start: None,
            window_end: None,
            status: Ll2Status {
                id: 1,
                name: "Go".into(),
                abbrev: "Go".into(),
                description: None,
            },
            probability: None,
            weather_concerns: None,
            image: None,
            launch_service_provider: Ll2ProviderDetail {
                name: "Test".into(),
                provider_type: None,
                total_launch_count: None,
                successful_launches: None,
                failed_launches: None,
                country: vec![],
                founding_year: None,
                consecutive_successful_launches: None,
            },
            rocket: Some(Ll2Rocket {
                configuration: Some(Ll2RocketConfiguration {
                    full_name: None,
                    name: Some("Falcon 9".into()),
                    variant: None,
                    description: None,
                    length: None,
                    diameter: None,
                    launch_mass: None,
                    leo_capacity: None,
                    gto_capacity: None,
                    to_thrust: None,
                    maiden_flight: None,
                    total_launch_count: None,
                    successful_launches: None,
                    failed_launches: None,
                    consecutive_successful_launches: None,
                }),
                launcher_stage: vec![],
                spacecraft_stage: vec![],
            }),
            pad: Ll2Pad {
                name: None,
                location: Ll2Location {
                    name: "KSC".into(),
                    timezone_name: None,
                    country: None,
                },
                total_launch_count: None,
            },
            mission: None,
            program: vec![],
            vid_urls: vec![],
            info_urls: vec![],
            failreason: None,
            updates: vec![],
        };

        let detail: LaunchDetail = ll2.into();
        assert_eq!(detail.rocket_full_name.as_deref(), Some("Falcon 9"));
    }

    #[test]
    fn non_empty_treats_empty_string_as_none() {
        assert_eq!(non_empty(Some(String::new())), None);
        assert_eq!(non_empty(None), None);
        assert_eq!(non_empty(Some("Block 5".into())).as_deref(), Some("Block 5"));
    }

    #[test]
    fn convert_falcon9_fixture_populates_new_fields() {
        let ll2: Ll2LaunchDetail = serde_json::from_str(fixtures::FALCON9_DETAIL)
            .expect("Falcon 9 fixture should deserialize");
        let detail: LaunchDetail = ll2.into();

        // `failreason` is `""` in the fixture and normalises to `None`.
        assert!(detail.failreason.is_none());

        // Updates carry over in order (15 in the fixture).
        assert_eq!(detail.updates.len(), 15);
        assert_eq!(detail.updates[0].comment, "Now targeting Apr 10 at 02:39 UTC");

        // Uncrewed flight — no crew.
        assert!(detail.crew.is_empty());

        // One Core booster landing (ASDS on OCISLY).
        assert_eq!(detail.landings.len(), 1);
        let landing = &detail.landings[0];
        assert_eq!(landing.stage_type.as_deref(), Some("Core"));
        assert!(landing.landing_attempt);
        assert_eq!(landing.landing_success, Some(true));
        assert_eq!(landing.landing_type.as_deref(), Some("ASDS"));
        assert_eq!(landing.landing_location.as_deref(), Some("Of Course I Still Love You"));

        // Vehicle specs.
        assert_eq!(detail.rocket_variant.as_deref(), Some("Block 5"));
        assert_eq!(detail.rocket_length, Some(70.0));
        assert_eq!(detail.rocket_diameter, Some(3.65));
        assert_eq!(detail.rocket_launch_mass, Some(549.0));
        assert_eq!(detail.rocket_leo_capacity, Some(22800.0));
        assert_eq!(detail.rocket_gto_capacity, Some(8300.0));
        assert_eq!(detail.rocket_thrust, Some(7607.0));
        assert_eq!(detail.rocket_maiden_flight.as_deref(), Some("2018-05-11"));
        assert_eq!(detail.rocket_total_launches, Some(570));
        assert_eq!(detail.rocket_successful_launches, Some(569));
        assert_eq!(detail.rocket_failed_launches, Some(1));
        assert_eq!(detail.rocket_consecutive_successes, Some(272));

        // Provider + pad.
        assert_eq!(detail.provider_country_code.as_deref(), Some("USA"));
        assert_eq!(detail.provider_founding_year, Some(2002));
        assert_eq!(detail.provider_consecutive_successes, Some(148));
        assert_eq!(detail.pad_total_launch_count, Some(259));
    }

    #[test]
    fn convert_soyuz_fixture_populates_crew() {
        let ll2: Ll2LaunchDetail = serde_json::from_str(fixtures::SOYUZ_CREWED_DETAIL)
            .expect("Soyuz fixture should deserialize");
        let detail: LaunchDetail = ll2.into();

        assert_eq!(detail.crew.len(), 3);
        let first = &detail.crew[0];
        assert_eq!(first.name, "Pyotr Dubrov");
        assert_eq!(first.role, "Commander");
        // Compact agency abbreviation is preferred over the full name.
        assert_eq!(first.agency, "RFSA");

        // Crewed Soyuz flight has no recoverable launcher stages.
        assert!(detail.landings.is_empty());
    }

    #[test]
    fn crew_agency_falls_back_to_name_when_abbrev_absent() {
        let entry = Ll2CrewEntry {
            role: Some(Ll2Role {
                role: Some("Flight Engineer".into()),
            }),
            astronaut: Some(Ll2Astronaut {
                name: "Jane Doe".into(),
                agency: Some(Ll2AstronautAgency {
                    name: Some("European Space Agency".into()),
                    abbrev: None,
                }),
            }),
        };

        let member = CrewMember::from(entry);
        assert_eq!(member.name, "Jane Doe");
        assert_eq!(member.role, "Flight Engineer");
        assert_eq!(member.agency, "European Space Agency");
    }

    #[test]
    fn convert_throttle_response() {
        let resp = Ll2ThrottleResponse {
            your_request_limit: 15,
            current_use: 5,
            limit_frequency_secs: Some(3600),
            next_use_secs: Some(0),
            ident: Some("88.97.214.56".into()),
        };

        let status: ThrottleStatus = resp.into();
        assert_eq!(status.limit, 15);
        assert_eq!(status.remaining, 10);
    }

    #[test]
    fn convert_throttle_response_at_limit() {
        let resp = Ll2ThrottleResponse {
            your_request_limit: 15,
            current_use: 15,
            limit_frequency_secs: None,
            next_use_secs: None,
            ident: None,
        };

        let status: ThrottleStatus = resp.into();
        assert_eq!(status.remaining, 0);
    }

    #[test]
    fn convert_throttle_response_over_limit() {
        let resp = Ll2ThrottleResponse {
            your_request_limit: 15,
            current_use: 20,
            limit_frequency_secs: None,
            next_use_secs: None,
            ident: None,
        };

        let status: ThrottleStatus = resp.into();
        assert_eq!(status.remaining, 0); // saturating_sub prevents underflow
    }
}
