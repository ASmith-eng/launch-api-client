//! Centralised LL2 sample API responses for use across `#[cfg(test)]` code.
//!
//! All sample JSON used by tests in this vendor module lives in
//! `fixtures/*.json` and is exposed here as named `&'static str` constants.
//! When refreshing for a new LL2 API version, drop the new capture into
//! `fixtures/` and tests will exercise the new shape automatically.
//!
//! For backward-compatibility regressions (e.g. legacy field name aliases),
//! prefer a small, focused JSON snippet inlined in the test that asserts the
//! quirk — full historical captures should only be added under
//! `fixtures/legacy/` when there is a real regression to anchor.
#![cfg(test)]

/// Paginated list response (`/launches/upcoming/?mode=normal`) — single
/// upcoming Starship IFT-7 entry. Hand-crafted to exercise the list
/// deserialization path.
pub const STARSHIP_LIST: &str = include_str!("fixtures/starship-list.json");

/// Detailed launch response (`/launch/{id}/`) — Starship IFT-7. Hand-crafted
/// to exercise the detail deserialization path with both top-level URL
/// arrays and provider stats.
pub const STARSHIP_DETAIL: &str = include_str!("fixtures/starship-detail.json");

/// Real-world capture: Falcon 9 / Starlink launch. Exercises launcher-stage
/// landing, full rocket configuration specs, provider country/founding year,
/// pad launch count, and the `updates[]` array.
pub const FALCON9_DETAIL: &str = include_str!("fixtures/falcon9-detail.json");

/// Real-world capture: Soyuz MS-29 (crewed). Exercises the
/// `spacecraft_stage[].launch_crew[]` shape.
pub const SOYUZ_CREWED_DETAIL: &str = include_str!("fixtures/soyuz-crewed-detail.json");
