//! Conversion from LL2 vendor response models to core domain types.

use crate::models::{
    CountryInfo, LaunchDetail, LaunchStatus, LaunchSummary, LocationInfo, MissionSummary,
    NetPrecision, OrbitInfo, PadInfo, Provider, ProviderType, ThrottleStatus, UrlEntry,
};

use super::response_models::{
    Ll2Country, Ll2Launch, Ll2LaunchDetail, Ll2Location, Ll2Mission, Ll2MissionDetail,
    Ll2NetPrecision, Ll2Orbit, Ll2Pad, Ll2Provider, Ll2ProviderDetail, Ll2Status,
    Ll2ThrottleResponse, Ll2UrlEntry,
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
            provider_type: ll2.provider_type.map(|t| ProviderType { name: t.name }),
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

impl From<Ll2LaunchDetail> for LaunchDetail {
    fn from(ll2: Ll2LaunchDetail) -> Self {
        let rocket_full_name = ll2
            .rocket
            .and_then(|r| r.configuration)
            .and_then(|c| c.full_name.or(c.name));

        let image_url = ll2.image.and_then(|img| img.image_url);

        let (vid_urls, info_urls) = match &ll2.mission {
            Some(m) => (
                m.vid_urls.iter().map(UrlEntry::from).collect(),
                m.info_urls.iter().map(UrlEntry::from).collect(),
            ),
            None => (vec![], vec![]),
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
            probability: ll2.probability,
            weather_concerns: ll2.weather_concerns,
            image_url,
            launch_service_provider: Provider {
                name: ll2.launch_service_provider.name.clone(),
                provider_type: ll2
                    .launch_service_provider
                    .provider_type
                    .map(|t| ProviderType { name: t.name }),
            },
            provider_total_launches: ll2.launch_service_provider.total_launch_count,
            provider_successful_launches: ll2.launch_service_provider.successful_launches,
            provider_failed_launches: ll2.launch_service_provider.failed_launches,
            rocket_full_name,
            pad: ll2.pad.into(),
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
            provider_type: ll2.provider_type.map(|t| ProviderType { name: t.name }),
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
                provider_type: Some(Ll2ProviderType {
                    name: "Commercial".into(),
                }),
            },
            pad: Ll2Pad {
                name: Some("Orbital Launch Mount A".into()),
                location: Ll2Location {
                    name: "Starbase, Texas".into(),
                    timezone_name: Some("America/Chicago".into()),
                    country: Some(Ll2Country {
                        name: "United States of America".into(),
                        alpha_2_code: "US".into(),
                    }),
                },
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
        assert_eq!(
            summary.launch_service_provider.provider_type.as_ref().unwrap().name,
            "Commercial"
        );
        assert_eq!(summary.pad.location.name, "Starbase, Texas");
        assert_eq!(
            summary.pad.location.timezone_name.as_deref(),
            Some("America/Chicago")
        );

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
            image: Some(Ll2Image {
                image_url: Some("https://example.com/starship.jpg".into()),
            }),
            launch_service_provider: Ll2ProviderDetail {
                name: "SpaceX".into(),
                provider_type: Some(Ll2ProviderType {
                    name: "Commercial".into(),
                }),
                total_launch_count: Some(301),
                successful_launches: Some(295),
                failed_launches: Some(6),
            },
            rocket: Some(Ll2Rocket {
                configuration: Some(Ll2RocketConfiguration {
                    full_name: Some("Starship (Super Heavy + Starship)".into()),
                    name: Some("Starship".into()),
                }),
            }),
            pad: Ll2Pad {
                name: Some("Orbital Launch Mount A".into()),
                location: Ll2Location {
                    name: "Starbase, Texas".into(),
                    timezone_name: Some("America/Chicago".into()),
                    country: Some(Ll2Country {
                        name: "United States of America".into(),
                        alpha_2_code: "US".into(),
                    }),
                },
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
                info_urls: vec![Ll2UrlEntry {
                    title: Some("SpaceX Info".into()),
                    url: "https://spacex.com/ift7".into(),
                }],
                vid_urls: vec![Ll2UrlEntry {
                    title: Some("Webcast".into()),
                    url: "https://youtube.com/watch?v=abc".into(),
                }],
            }),
            program: vec![Ll2Program {
                name: "Starship Development".into(),
            }],
        }
    }

    #[test]
    fn convert_full_launch_detail() {
        let ll2 = sample_ll2_launch_detail();
        let detail: LaunchDetail = ll2.into();

        assert_eq!(detail.id, "e3df2ecd-c239-472f-95e4-2b89b4f75800");
        assert_eq!(detail.name, "Starship IFT-7");
        assert_eq!(detail.status.id, 1);
        assert_eq!(detail.probability, Some(90));
        assert_eq!(detail.weather_concerns.as_deref(), Some("No concerns"));
        assert_eq!(
            detail.image_url.as_deref(),
            Some("https://example.com/starship.jpg")
        );
        assert_eq!(detail.provider_total_launches, Some(301));
        assert_eq!(detail.provider_successful_launches, Some(295));
        assert_eq!(detail.provider_failed_launches, Some(6));
        assert_eq!(
            detail.rocket_full_name.as_deref(),
            Some("Starship (Super Heavy + Starship)")
        );
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
            },
            rocket: None,
            pad: Ll2Pad {
                name: None,
                location: Ll2Location {
                    name: "Unknown".into(),
                    timezone_name: None,
                    country: None,
                },
            },
            mission: None,
            program: vec![],
        };

        let detail: LaunchDetail = ll2.into();
        assert!(detail.probability.is_none());
        assert!(detail.image_url.is_none());
        assert!(detail.rocket_full_name.is_none());
        assert!(detail.provider_total_launches.is_none());
        assert!(detail.vid_urls.is_empty());
        assert!(detail.info_urls.is_empty());
        assert!(detail.programs.is_empty());
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
            },
            rocket: Some(Ll2Rocket {
                configuration: Some(Ll2RocketConfiguration {
                    full_name: None,
                    name: Some("Falcon 9".into()),
                }),
            }),
            pad: Ll2Pad {
                name: None,
                location: Ll2Location {
                    name: "KSC".into(),
                    timezone_name: None,
                    country: None,
                },
            },
            mission: None,
            program: vec![],
        };

        let detail: LaunchDetail = ll2.into();
        assert_eq!(detail.rocket_full_name.as_deref(), Some("Falcon 9"));
    }

    #[test]
    fn convert_throttle_response() {
        let resp = Ll2ThrottleResponse {
            your_request_limit: 15,
            current_use: 5,
            limit_frequency_secs: Some(3600),
            next_use_secs: Some(0),
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
        };

        let status: ThrottleStatus = resp.into();
        assert_eq!(status.remaining, 0); // saturating_sub prevents underflow
    }
}
