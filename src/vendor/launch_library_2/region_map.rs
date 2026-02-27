//! Geographical region to `pad__location` ID mappings for Launch Library 2.
//!
//! These location IDs are used as filter values for the `pad__location` query
//! parameter when calling the LL2 launches API.

/// A named region with its associated LL2 location IDs.
#[derive(Debug, Clone)]
pub struct Region {
    pub name: &'static str,
    pub location_ids: &'static [u32],
}

/// All known geographical regions and their LL2 location IDs.
///
/// Location IDs sourced from the Launch Library 2 API `/location/` endpoint.
/// These group launch sites by broad geographical region for the filter panel.
pub static REGIONS: &[Region] = &[
    Region {
        name: "US",
        location_ids: &[
            12,  // Kennedy Space Center, FL
            27,  // Vandenberg SFB, CA
            20,  // Cape Canaveral SFS, FL
            143, // Wallops Island, VA
            184, // Starbase, TX (SpaceX Boca Chica)
            11,  // Mojave Air and Space Port, CA
            144, // Mid-Atlantic Regional Spaceport (MARS), VA
            98,  // Pacific Spaceport Complex, AK
        ],
    },
    Region {
        name: "Europe",
        location_ids: &[
            13, // Guiana Space Centre, French Guiana
            33, // Esrange, Sweden (Rocket Factory Augsburg)
            46, // Andøya, Norway
            36, // SaxaVord Spaceport, UK
        ],
    },
    Region {
        name: "Russia/Kazakhstan",
        location_ids: &[
            15, // Baikonur Cosmodrome, Kazakhstan
            18, // Plesetsk Cosmodrome, Russia
            35, // Vostochny Cosmodrome, Russia
        ],
    },
    Region {
        name: "China",
        location_ids: &[
            17,  // Jiuquan Satellite Launch Center
            16,  // Xichang Satellite Launch Center
            25,  // Wenchang Space Launch Site
            34,  // Taiyuan Satellite Launch Center
            199, // Haiyang (sea launch)
        ],
    },
    Region {
        name: "India",
        location_ids: &[
            14, // Satish Dhawan Space Centre (Sriharikota)
        ],
    },
    Region {
        name: "Japan",
        location_ids: &[
            24, // Tanegashima Space Center
            26, // Uchinoura Space Center
        ],
    },
    Region {
        name: "New Zealand",
        location_ids: &[
            188, // Rocket Lab Launch Complex, Mahia Peninsula
        ],
    },
];

/// Find a region by name (case-insensitive).
pub fn find_region(name: &str) -> Option<&'static Region> {
    REGIONS
        .iter()
        .find(|r| r.name.eq_ignore_ascii_case(name))
}

/// Build a comma-separated list of location IDs for use in the API query.
pub fn location_ids_param(region: &Region) -> String {
    region
        .location_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_us_region() {
        let region = find_region("US").expect("US region should exist");
        assert!(!region.location_ids.is_empty());
    }

    #[test]
    fn find_region_case_insensitive() {
        assert!(find_region("europe").is_some());
        assert!(find_region("EUROPE").is_some());
    }

    #[test]
    fn unknown_region_returns_none() {
        assert!(find_region("Antarctica").is_none());
    }

    #[test]
    fn location_ids_param_formatting() {
        let region = find_region("India").unwrap();
        assert_eq!(location_ids_param(region), "14");

        let us = find_region("US").unwrap();
        let param = location_ids_param(us);
        assert!(param.contains(','));
        assert!(param.contains("12"));
    }
}
