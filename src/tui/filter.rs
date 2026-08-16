//! Filter enums and state for the inline filter panel.
//!
//! All filter categories implement [`FilterOption`], which provides cycling
//! (`next`/`prev`) and default-detection for free — each enum only needs to
//! define [`all()`](FilterOption::all) (the variant list in display order) and
//! [`label()`](FilterOption::label). Domain-specific API param methods live on
//! each enum directly.
//!
//! [`FilterState`] groups the four categories together and provides helpers for
//! cycling, active-filter detection, and applying to [`ListParams`].

use chrono::{DateTime, Duration, Utc};

pub use crate::models::{ActiveFilters, CrewedFilter, DateRangeFilter, RegionFilter, StatusFilter};
use crate::vendor::launch_library_2::endpoints::ListParams;
use crate::vendor::launch_library_2::region_map;

/// Number of filter categories (Status, Region, Crewed, Date Range).
pub const NUM_FILTER_CATEGORIES: usize = 4;

// ---------------------------------------------------------------------------
// FilterOption trait
// ---------------------------------------------------------------------------

/// Shared behaviour for cyclable filter enums.
///
/// Implementors provide [`all()`](Self::all) (ordered variant list, first
/// element must be the default) and [`label()`](Self::label). Cycling and
/// default-detection are derived automatically.
pub trait FilterOption: Copy + Default + PartialEq + 'static {
    /// All variants in display/cycling order. The first element **must** be
    /// the `#[default]` variant.
    fn all() -> &'static [Self];

    /// Human-readable label shown in the filter panel UI.
    fn label(self) -> &'static str;

    /// Advance to the next variant, wrapping around.
    fn next(self) -> Self {
        let all = Self::all();
        let Some(idx) = all.iter().position(|&v| v == self) else {
            return self;
        };
        all[(idx + 1) % all.len()]
    }

    /// Go back to the previous variant, wrapping around.
    fn prev(self) -> Self {
        let all = Self::all();
        let Some(idx) = all.iter().position(|&v| v == self) else {
            return self;
        };
        all[(idx + all.len() - 1) % all.len()]
    }

    /// Whether this value is the default (i.e. "All" / unfiltered).
    fn is_default(self) -> bool {
        self == Self::default()
    }
}

// ---------------------------------------------------------------------------
// FilterOption implementations
// ---------------------------------------------------------------------------

impl FilterOption for StatusFilter {
    fn all() -> &'static [Self] {
        &[
            Self::All,
            Self::GoForLaunch,
            Self::Tbd,
            Self::Tbc,
            Self::OnHold,
            Self::InFlight,
        ]
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::GoForLaunch => "Go for Launch",
            Self::Tbd => "TBD",
            Self::Tbc => "TBC",
            Self::OnHold => "On Hold",
            Self::InFlight => "In Flight",
        }
    }
}

impl StatusFilter {
    /// LL2 `status__ids` query parameter value, if filtering.
    pub fn status_ids(self) -> Option<String> {
        match self {
            Self::All => None,
            Self::GoForLaunch => Some("1".into()),
            Self::Tbd => Some("2".into()),
            Self::Tbc => Some("8".into()),
            Self::OnHold => Some("5".into()),
            Self::InFlight => Some("6".into()),
        }
    }
}

impl FilterOption for RegionFilter {
    fn all() -> &'static [Self] {
        &[
            Self::All,
            Self::US,
            Self::Europe,
            Self::RussiaKazakhstan,
            Self::China,
            Self::India,
            Self::Japan,
            Self::NewZealand,
        ]
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::US => "US",
            Self::Europe => "Europe",
            Self::RussiaKazakhstan => "Russia/Kaz.",
            Self::China => "China",
            Self::India => "India",
            Self::Japan => "Japan",
            Self::NewZealand => "New Zealand",
        }
    }
}

impl RegionFilter {
    /// LL2 `location__ids` query parameter value, if filtering.
    pub fn location_ids(self) -> Option<String> {
        let name = match self {
            Self::All => return None,
            Self::US => "US",
            Self::Europe => "Europe",
            Self::RussiaKazakhstan => "Russia/Kazakhstan",
            Self::China => "China",
            Self::India => "India",
            Self::Japan => "Japan",
            Self::NewZealand => "New Zealand",
        };
        region_map::find_region(name).map(region_map::location_ids_param)
    }
}

impl FilterOption for CrewedFilter {
    fn all() -> &'static [Self] {
        &[Self::All, Self::CrewedOnly, Self::UncrewedOnly]
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::CrewedOnly => "Crewed",
            Self::UncrewedOnly => "Uncrewed",
        }
    }
}

impl CrewedFilter {
    /// LL2 `is_crewed` query parameter value, if filtering.
    pub fn is_crewed(self) -> Option<bool> {
        match self {
            Self::All => None,
            Self::CrewedOnly => Some(true),
            Self::UncrewedOnly => Some(false),
        }
    }
}

impl FilterOption for DateRangeFilter {
    fn all() -> &'static [Self] {
        &[Self::All, Self::Next7Days, Self::Next30Days, Self::Next90Days]
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Next7Days => "7 days",
            Self::Next30Days => "30 days",
            Self::Next90Days => "90 days",
        }
    }
}

impl DateRangeFilter {
    /// LL2 `net__lt` query parameter value, if filtering.
    pub fn net_lt(self, now: DateTime<Utc>) -> Option<String> {
        let days = match self {
            Self::All => return None,
            Self::Next7Days => 7,
            Self::Next30Days => 30,
            Self::Next90Days => 90,
        };
        Some(
            (now + Duration::days(days))
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string(),
        )
    }
}

// ---------------------------------------------------------------------------
// FilterState
// ---------------------------------------------------------------------------

/// Names shown in the filter panel for each category, in display order.
const CATEGORY_NAMES: [&str; NUM_FILTER_CATEGORIES] = ["Status", "Region", "Crewed", "Date"];

/// Filter state for the inline filter panel.
#[derive(Debug, Clone, Default)]
pub struct FilterState {
    /// Which filter category is currently focused (0–3).
    pub active_category: usize,
    /// Launch status filter.
    pub status: StatusFilter,
    /// Geographical region filter.
    pub region: RegionFilter,
    /// Crewed mission filter.
    pub is_crewed: CrewedFilter,
    /// Date range filter.
    pub date_range: DateRangeFilter,
}

impl FilterState {
    /// Cycle the active category's value forward.
    pub fn cycle_next(&mut self) {
        match self.active_category {
            0 => self.status = self.status.next(),
            1 => self.region = self.region.next(),
            2 => self.is_crewed = self.is_crewed.next(),
            3 => self.date_range = self.date_range.next(),
            _ => {}
        }
    }

    /// Cycle the active category's value backward.
    pub fn cycle_prev(&mut self) {
        match self.active_category {
            0 => self.status = self.status.prev(),
            1 => self.region = self.region.prev(),
            2 => self.is_crewed = self.is_crewed.prev(),
            3 => self.date_range = self.date_range.prev(),
            _ => {}
        }
    }

    /// Advance to the next filter category (wraps around).
    pub fn next_category(&mut self) {
        self.active_category = (self.active_category + 1) % NUM_FILTER_CATEGORIES;
    }

    /// Reset all filter categories to their defaults.
    pub fn clear_all(&mut self) {
        self.status = StatusFilter::default();
        self.region = RegionFilter::default();
        self.is_crewed = CrewedFilter::default();
        self.date_range = DateRangeFilter::default();
    }

    /// Whether any filter is active (non-default).
    pub fn has_active_filters(&self) -> bool {
        !self.status.is_default()
            || !self.region.is_default()
            || !self.is_crewed.is_default()
            || !self.date_range.is_default()
    }

    /// Apply this filter state to a [`ListParams`] for API requests.
    pub fn apply_to_params(&self, params: &mut ListParams, now: DateTime<Utc>) {
        params.status_ids = self.status.status_ids();
        params.location_ids = self.region.location_ids();
        params.is_crewed = self.is_crewed.is_crewed();
        params.net_lt = self.date_range.net_lt(now);
    }

    /// Category name and current label for each filter, in display order.
    ///
    /// Used by the filter panel renderer so the name-to-field mapping is
    /// defined in one place.
    pub fn category_labels(&self) -> [(&'static str, &'static str); NUM_FILTER_CATEGORIES] {
        [
            (CATEGORY_NAMES[0], self.status.label()),
            (CATEGORY_NAMES[1], self.region.label()),
            (CATEGORY_NAMES[2], self.is_crewed.label()),
            (CATEGORY_NAMES[3], self.date_range.label()),
        ]
    }

    /// Snapshot the current filter selections for persistence.
    pub fn to_active_filters(&self) -> ActiveFilters {
        ActiveFilters {
            status: self.status,
            region: self.region,
            is_crewed: self.is_crewed,
            date_range: self.date_range,
        }
    }

    /// Restore filter selections from a persisted snapshot.
    ///
    /// `active_category` is always reset to 0 since the focused UI element
    /// is not meaningful to persist.
    pub fn from_active_filters(snapshot: &ActiveFilters) -> Self {
        Self {
            active_category: 0,
            status: snapshot.status,
            region: snapshot.region,
            is_crewed: snapshot.is_crewed,
            date_range: snapshot.date_range,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    // --- FilterOption trait: cycling ---

    #[test]
    fn status_filter_cycles_all_values_forward() {
        let mut f = StatusFilter::All;
        let labels = ["All", "Go for Launch", "TBD", "TBC", "On Hold", "In Flight"];
        for label in &labels {
            assert_eq!(f.label(), *label);
            f = f.next();
        }
        assert_eq!(f, StatusFilter::All); // wrapped around
    }

    #[test]
    fn status_filter_cycles_all_values_backward() {
        let mut f = StatusFilter::All;
        f = f.prev(); // wraps to InFlight
        assert_eq!(f, StatusFilter::InFlight);
        f = f.prev();
        assert_eq!(f, StatusFilter::OnHold);
    }

    #[test]
    fn region_filter_cycles_all_values() {
        let mut f = RegionFilter::All;
        let expected = [
            "All", "US", "Europe", "Russia/Kaz.", "China", "India", "Japan", "New Zealand",
        ];
        for label in &expected {
            assert_eq!(f.label(), *label);
            f = f.next();
        }
        assert_eq!(f, RegionFilter::All);
    }

    #[test]
    fn crewed_filter_cycles() {
        let mut f = CrewedFilter::All;
        assert_eq!(f.label(), "All");
        f = f.next();
        assert_eq!(f.label(), "Crewed");
        f = f.next();
        assert_eq!(f.label(), "Uncrewed");
        f = f.next();
        assert_eq!(f, CrewedFilter::All);
    }

    #[test]
    fn date_range_filter_cycles() {
        let mut f = DateRangeFilter::All;
        let labels = ["All", "7 days", "30 days", "90 days"];
        for label in &labels {
            assert_eq!(f.label(), *label);
            f = f.next();
        }
        assert_eq!(f, DateRangeFilter::All);
    }

    // --- FilterOption trait: is_default ---

    #[test]
    fn is_default_true_for_all_variants() {
        assert!(StatusFilter::All.is_default());
        assert!(RegionFilter::All.is_default());
        assert!(CrewedFilter::All.is_default());
        assert!(DateRangeFilter::All.is_default());
    }

    #[test]
    fn is_default_false_for_non_default() {
        assert!(!StatusFilter::GoForLaunch.is_default());
        assert!(!RegionFilter::US.is_default());
        assert!(!CrewedFilter::CrewedOnly.is_default());
        assert!(!DateRangeFilter::Next7Days.is_default());
    }

    // --- Domain-specific API param methods ---

    #[test]
    fn status_filter_api_ids() {
        assert_eq!(StatusFilter::All.status_ids(), None);
        assert_eq!(StatusFilter::GoForLaunch.status_ids(), Some("1".into()));
        assert_eq!(StatusFilter::Tbd.status_ids(), Some("2".into()));
        assert_eq!(StatusFilter::Tbc.status_ids(), Some("8".into()));
        assert_eq!(StatusFilter::OnHold.status_ids(), Some("5".into()));
        assert_eq!(StatusFilter::InFlight.status_ids(), Some("6".into()));
    }

    #[test]
    fn region_filter_location_ids_all_returns_none() {
        assert_eq!(RegionFilter::All.location_ids(), None);
    }

    #[test]
    fn region_filter_location_ids_us_returns_ids() {
        let loc = RegionFilter::US.location_ids().unwrap();
        assert!(loc.contains("12")); // Cape Canaveral SFS
        assert!(loc.contains(','));
    }

    #[test]
    fn region_filter_location_ids_india_returns_single_id() {
        let loc = RegionFilter::India.location_ids().unwrap();
        assert_eq!(loc, "14");
    }

    /// Every region must resolve — a name typo in `location_ids()` would
    /// silently yield `None`, disabling that region's filter entirely.
    #[test]
    fn every_region_variant_resolves_to_ids() {
        for &region in RegionFilter::all() {
            if region.is_default() {
                continue;
            }
            assert!(
                region.location_ids().is_some(),
                "{} did not resolve to location IDs",
                region.label()
            );
        }
    }

    #[test]
    fn crewed_filter_api_values() {
        assert_eq!(CrewedFilter::All.is_crewed(), None);
        assert_eq!(CrewedFilter::CrewedOnly.is_crewed(), Some(true));
        assert_eq!(CrewedFilter::UncrewedOnly.is_crewed(), Some(false));
    }

    #[test]
    fn date_range_net_lt_all_returns_none() {
        let now = Utc::now();
        assert_eq!(DateRangeFilter::All.net_lt(now), None);
    }

    #[test]
    fn date_range_net_lt_7_days_returns_future_date() {
        let now = Utc.with_ymd_and_hms(2026, 3, 1, 12, 0, 0).unwrap();
        let result = DateRangeFilter::Next7Days.net_lt(now).unwrap();
        assert!(result.starts_with("2026-03-08"));
    }

    // --- FilterState: snapshot round-trip ---

    #[test]
    fn to_active_filters_captures_all_selections() {
        let mut f = FilterState::default();
        f.status = StatusFilter::GoForLaunch;
        f.region = RegionFilter::Europe;
        f.is_crewed = CrewedFilter::CrewedOnly;
        f.date_range = DateRangeFilter::Next30Days;
        f.active_category = 2;

        let snap = f.to_active_filters();
        assert_eq!(snap.status, StatusFilter::GoForLaunch);
        assert_eq!(snap.region, RegionFilter::Europe);
        assert_eq!(snap.is_crewed, CrewedFilter::CrewedOnly);
        assert_eq!(snap.date_range, DateRangeFilter::Next30Days);
    }

    #[test]
    fn from_active_filters_restores_selections_and_resets_category() {
        let snap = ActiveFilters {
            status: StatusFilter::Tbd,
            region: RegionFilter::Japan,
            is_crewed: CrewedFilter::UncrewedOnly,
            date_range: DateRangeFilter::Next7Days,
        };

        let f = FilterState::from_active_filters(&snap);
        assert_eq!(f.active_category, 0);
        assert_eq!(f.status, StatusFilter::Tbd);
        assert_eq!(f.region, RegionFilter::Japan);
        assert_eq!(f.is_crewed, CrewedFilter::UncrewedOnly);
        assert_eq!(f.date_range, DateRangeFilter::Next7Days);
    }

    #[test]
    fn from_active_filters_default_snapshot_gives_default_state() {
        let f = FilterState::from_active_filters(&ActiveFilters::default());
        assert!(!f.has_active_filters());
        assert_eq!(f.active_category, 0);
    }

    // --- FilterState ---

    #[test]
    fn clear_all_resets_every_category_to_default() {
        let mut f = FilterState {
            active_category: 2,
            status: StatusFilter::GoForLaunch,
            region: RegionFilter::Europe,
            is_crewed: CrewedFilter::CrewedOnly,
            date_range: DateRangeFilter::Next30Days,
        };
        f.clear_all();
        assert!(f.status.is_default());
        assert!(f.region.is_default());
        assert!(f.is_crewed.is_default());
        assert!(f.date_range.is_default());
        // active_category is preserved — only values are cleared.
        assert_eq!(f.active_category, 2);
    }

    #[test]
    fn clear_all_on_default_state_is_noop() {
        let mut f = FilterState::default();
        f.clear_all();
        assert!(!f.has_active_filters());
    }

    #[test]
    fn filter_state_default_has_no_active_filters() {
        let f = FilterState::default();
        assert!(!f.has_active_filters());
    }

    #[test]
    fn filter_state_detects_active_filters() {
        let mut f = FilterState::default();
        f.status = StatusFilter::GoForLaunch;
        assert!(f.has_active_filters());
    }

    #[test]
    fn filter_state_next_category_wraps() {
        let mut f = FilterState::default();
        assert_eq!(f.active_category, 0);
        f.next_category();
        assert_eq!(f.active_category, 1);
        f.next_category();
        assert_eq!(f.active_category, 2);
        f.next_category();
        assert_eq!(f.active_category, 3);
        f.next_category();
        assert_eq!(f.active_category, 0);
    }

    #[test]
    fn filter_state_cycle_next_changes_active_category_value() {
        let mut f = FilterState::default();
        f.active_category = 0;
        f.cycle_next();
        assert_eq!(f.status, StatusFilter::GoForLaunch);

        f.active_category = 2;
        f.cycle_next();
        assert_eq!(f.is_crewed, CrewedFilter::CrewedOnly);
    }

    #[test]
    fn filter_state_cycle_prev_changes_active_category_value() {
        let mut f = FilterState::default();
        f.active_category = 0;
        f.cycle_prev();
        assert_eq!(f.status, StatusFilter::InFlight);
    }

    #[test]
    fn filter_state_apply_to_params_default() {
        let f = FilterState::default();
        let mut params = ListParams::default();
        let now = Utc::now();
        f.apply_to_params(&mut params, now);
        assert!(params.status_ids.is_none());
        assert!(params.location_ids.is_none());
        assert!(params.is_crewed.is_none());
        assert!(params.net_lt.is_none());
    }

    #[test]
    fn filter_state_apply_to_params_with_filters() {
        let f = FilterState {
            active_category: 0,
            status: StatusFilter::GoForLaunch,
            region: RegionFilter::US,
            is_crewed: CrewedFilter::CrewedOnly,
            date_range: DateRangeFilter::Next30Days,
        };
        let mut params = ListParams::default();
        let now = Utc.with_ymd_and_hms(2026, 3, 1, 12, 0, 0).unwrap();
        f.apply_to_params(&mut params, now);
        assert_eq!(params.status_ids, Some("1".into()));
        assert!(params.location_ids.is_some());
        assert_eq!(params.is_crewed, Some(true));
        assert!(params.net_lt.is_some());
    }

    #[test]
    fn filter_state_category_labels_matches_category_names() {
        let f = FilterState {
            active_category: 0,
            status: StatusFilter::GoForLaunch,
            region: RegionFilter::Europe,
            is_crewed: CrewedFilter::UncrewedOnly,
            date_range: DateRangeFilter::Next90Days,
        };
        let labels = f.category_labels();
        assert_eq!(labels[0], ("Status", "Go for Launch"));
        assert_eq!(labels[1], ("Region", "Europe"));
        assert_eq!(labels[2], ("Crewed", "Uncrewed"));
        assert_eq!(labels[3], ("Date", "90 days"));
    }
}
