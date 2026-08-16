//! Geographical region to location ID mappings for Launch Library 2.
//!
//! These location IDs are used as filter values for the `location__ids` query
//! parameter when calling the LL2 launches API.
//!
//! LL2 has three locations with no country — "Sea Launch", "Air launch to
//! orbit", and "Air launch to Suborbital flight". They belong to no region, so
//! air- and sea-launched missions are absent from every region filter.

/// A named region with its associated LL2 location IDs.
#[derive(Debug, Clone)]
pub struct Region {
    pub name: &'static str,
    pub location_ids: &'static [u32],
}

/// All known geographical regions and their LL2 location IDs.
///
/// Location IDs sourced from the Launch Library 2 API `/locations/` endpoint,
/// filtered to `active` sites — inactive ones (Svobodny, the French Algeria
/// test centre) never appear in upcoming launches.
pub static REGIONS: &[Region] = &[
    Region {
        name: "US",
        location_ids: &[
            12,  // Cape Canaveral SFS, FL
            11,  // Vandenberg SFB, CA
            27,  // Kennedy Space Center, FL
            21,  // Wallops Flight Facility, VA
            29,  // Corn Ranch, Van Horn, TX (Blue Origin New Shepard)
            143, // SpaceX Starbase, TX
            25,  // Pacific Spaceport Complex, AK
            155, // White Sands Missile Range, NM
            31,  // Spaceport America, NM
            1,   // Pacific Missile Range Facility, HI
            162, // Edwards Air Force Base, CA
        ],
    },
    Region {
        name: "Europe",
        location_ids: &[
            13,  // Guiana Space Centre, French Guiana (ESA)
            178, // Esrange Space Center, Sweden
            161, // Andøya Spaceport, Norway
            157, // SaxaVord Spaceport, UK
            159, // Sutherland Spaceport, UK
            163, // El Arenosillo Test Centre, Spain
        ],
    },
    Region {
        name: "Russia/Kazakhstan",
        location_ids: &[
            6,  // Plesetsk Cosmodrome, Russia
            15, // Baikonur Cosmodrome, Kazakhstan
            30, // Kapustin Yar, Russia
            18, // Vostochny Cosmodrome, Russia
            5,  // Dombarovskiy, Russia
        ],
    },
    Region {
        name: "China",
        location_ids: &[
            17,  // Jiuquan Satellite Launch Center
            16,  // Xichang Satellite Launch Center
            19,  // Taiyuan Satellite Launch Center
            8,   // Wenchang Space Launch Site
            185, // Haiyang Oriental Spaceport
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
            26,  // Tanegashima Space Center
            24,  // Uchinoura Space Center
            32,  // Hokkaido Spaceport
            166, // Spaceport Kii
        ],
    },
    Region {
        name: "New Zealand",
        location_ids: &[
            10, // Rocket Lab Launch Complex 1, Mahia Peninsula
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
