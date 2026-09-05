//! Resolves the region filter to LL2 `location__ids` query values.
//!
//! LL2 has no country or region filter on launches, so a region has to be
//! expressed as the set of numeric location IDs it contains. That table is
//! generated — see [`table`] and `tools/launch_library_2/region-map/` — because the IDs are
//! rows in someone else's database and have silently moved before now.
//!
//! Every active launch site LL2 lists is reachable through some region:
//! `RegionFilter::Other` is generated as the complement of the named ones, so
//! air- and sea-launched missions and any country we have not thought to name
//! land there rather than nowhere. The exception is LL2's lunar landing sites,
//! which carry no country and are not launch sites; the generated table's
//! header names them.

mod table;

use crate::models::{RegionFilter, RegionSites};

/// Resolve a region to the location IDs to filter on.
pub fn resolve(region: RegionFilter) -> RegionSites {
    sites_from_ids(region, table::location_ids(region))
}

/// Every region, in the order the filter panel cycles through them.
pub fn all() -> &'static [RegionFilter] {
    &table::ALL
}

/// Split out from [`resolve`] so the empty case stays testable — no region in
/// the generated table has an empty ID list today, but the branch is what stops
/// one from silently degrading into an unfiltered query if that changes.
fn sites_from_ids(region: RegionFilter, ids: &[u32]) -> RegionSites {
    if region == RegionFilter::All {
        return RegionSites::Unfiltered;
    }
    // Only meaningful once All is out of the way: All is empty because it
    // filters on nothing, whereas any other empty region has no sites to
    // filter on. Same slice, opposite meanings.
    match ids {
        [] => RegionSites::NoneRegistered,
        ids => RegionSites::Sites(join_ids(ids)),
    }
}

/// Display name for a region, defined alongside its countries in
/// `tools/launch_library_2/region-map/regions.toml`.
pub fn label(region: RegionFilter) -> &'static str {
    table::label(region)
}

fn join_ids(ids: &[u32]) -> String {
    ids.iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_is_unfiltered() {
        assert_eq!(resolve(RegionFilter::All), RegionSites::Unfiltered);
    }

    #[test]
    fn region_with_no_ids_is_none_registered() {
        assert_eq!(
            sites_from_ids(RegionFilter::Europe, &[]),
            RegionSites::NoneRegistered
        );
    }

    /// `All` is empty for the opposite reason, and must not be mistaken for a
    /// region the provider has no sites in.
    #[test]
    fn all_stays_unfiltered_when_ids_are_empty() {
        assert_eq!(
            sites_from_ids(RegionFilter::All, &[]),
            RegionSites::Unfiltered
        );
    }

    #[test]
    fn single_site_region_has_no_separator() {
        assert_eq!(
            resolve(RegionFilter::India),
            RegionSites::Sites("14".to_string())
        );
    }

    #[test]
    fn multi_site_region_is_comma_separated() {
        let RegionSites::Sites(ids) = resolve(RegionFilter::US) else {
            panic!("US should resolve to sites");
        };
        assert!(ids.contains(','));
        assert!(ids.split(',').all(|id| id.parse::<u32>().is_ok()));
    }

    /// Everything but `All`, which is empty by definition.
    fn filterable() -> impl Iterator<Item = RegionFilter> {
        all().iter().copied().filter(|&r| r != RegionFilter::All)
    }

    /// The IDs are only meaningful to LL2, so the most we can assert locally is
    /// that no region silently resolves to nothing. A region that genuinely has
    /// no sites is a supported state, but none of ours should be in it — if this
    /// fails after a regeneration, LL2 dropped every site in that region.
    #[test]
    fn every_region_resolves_to_at_least_one_site() {
        for region in filterable() {
            assert!(
                matches!(resolve(region), RegionSites::Sites(_)),
                "{} resolved to no sites",
                label(region)
            );
        }
    }

    #[test]
    fn regions_do_not_share_locations() {
        let mut seen = std::collections::HashMap::new();
        for region in filterable() {
            for id in table::location_ids(region) {
                if let Some(other) = seen.insert(*id, region) {
                    panic!(
                        "location {id} is in both {} and {}",
                        label(other),
                        label(region)
                    );
                }
            }
        }
    }

    /// `All` must lead: `FilterOption` treats the first entry as the default,
    /// and the generator puts it there rather than `regions.toml` doing so.
    #[test]
    fn all_leads_the_cycling_order() {
        assert_eq!(all().first(), Some(&RegionFilter::All));
    }
}
