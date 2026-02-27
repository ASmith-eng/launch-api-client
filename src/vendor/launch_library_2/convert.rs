//! Conversion from LL2 vendor response models to core domain types.

use crate::models::{
    CountryInfo, LaunchStatus, LaunchSummary, LocationInfo, MissionSummary, NetPrecision, OrbitInfo,
    PadInfo, Provider, ProviderType,
};

use super::response_models::{
    Ll2Country, Ll2Launch, Ll2Location, Ll2Mission, Ll2NetPrecision, Ll2Orbit, Ll2Pad,
    Ll2Provider, Ll2Status,
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
}
