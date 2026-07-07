# Detail View Enhancement — Implementation Plan

This plan breaks the detail view enhancement into ordered, self-contained
steps. Each step produces compilable, testable code and builds on the
previous one. Steps are sized for a single focused session and follow
test-driven development: write or update tests first, then make them pass.

Companion references:
- [Design plan](./detail-view-enhancement-plan.md) — field definitions,
  mockups, and conditional rendering rules
- [LL2 sample detail response](./reference/ll2-sample-detail-response.json)
  — Falcon 9 / Starlink (landing, vehicle specs, updates)
- [LL2 sample crewed detail response](./reference/ll2-sample-detail-response-crewed.json)
  — Soyuz MS-29 (crew, spacecraft stage)
- [Original implementation plan](./implementation-plan.md) — for context on
  established patterns

---

## Conventions

These conventions apply to all steps and are not repeated in each one.

1. **Test-first.** Every new struct, `From` impl, and rendering function gets
   tests written before the implementation. Tests live in inline
   `#[cfg(test)]` modules in the same file, matching the existing pattern.

2. **Rust best practices.** Follow the
   [Microsoft Pragmatic Rust Guidelines](./reference/microsoft-guidelines.md)
   and [Rust API Guidelines](./reference/rust-api-guidelines.md) in the
   reference directory. Key points: derive common traits on public types,
   prefer `Option` over sentinel values, use `#[serde(default)]` on all
   optional fields for forward compatibility.

3. **`cargo check` + `cargo test` must pass after every step.** If a step
   introduces a new field on `LaunchDetail`, the `dummy_launch_detail()`
   helper in `models.rs` must be updated in the same step (otherwise
   compilation will fail across dozens of test sites).

4. **No dead code.** New domain model fields and LL2 response model fields
   should be added in the same step as the conversion logic or rendering
   code that uses them. If a field is intentionally stored but not yet
   displayed (e.g. `rocket_description`), annotate it with
   `#[allow(dead_code)]` and a `// Stored for future use` comment.

5. **Incremental commits.** Each step (or logical sub-section within a step)
   should be committed separately so the change is reviewable and
   bisectable.

---

## Contents

| Step | Section | Description |
|------|---------|-------------|
| 0    | [Prerequisite fix](#step-0--prerequisite-fix-top-level-url-field-names) | Fix top-level `vid_urls`/`info_urls` serde rename for v2.3.0 API |
| 1    | [Domain model](#step-1--domain-model-expansion) | New types + fields on `LaunchDetail`, cache version bump |
| 2    | [LL2 response models](#step-2--ll2-response-models) | New serde structs for API deserialization |
| 3    | [Conversion layer](#step-3--conversion-layer) | `From` impls: LL2 response → domain types |
| 4    | [Hero: fail reason](#step-4--hero-section-fail-reason) | Add fail reason to hero section |
| 5    | [Updates section](#step-5--updates-section) | New conditional section |
| 6    | [Mission: crew + landing](#step-6--mission-section-crew--landing) | New conditional sub-sections |
| 7    | [Vehicle expansion](#step-7--vehicle-section-expansion) | Specs grid, maiden flight, record bar |
| 8    | [Provider + Location](#step-8--provider--location-expansion) | Founded line, country, consecutive, pad count |
| 9    | [Layout restructure](#step-9--layout-restructure) | Reorder sections to match design plan |

---

## Step 0 — Prerequisite fix: top-level URL field names ✅ Complete

**Implementation notes:**
- `Ll2LaunchDetail::vid_urls` / `info_urls` switched from `#[serde(rename = "...URLs")]` to `#[serde(alias = "...URLs", default)]`. Canonical key is now snake_case (matches v2.3.0); legacy camelCase still deserializes.
- `SAMPLE_DETAIL_JSON` updated to v2.3.0 snake_case.
- New test `deserialize_launch_detail_accepts_legacy_camelcase_url_fields` covers backward compatibility with v2.2.0 / older cached payloads.
- `cargo test` passes (434 tests).

During API verification we discovered that the LL2 v2.3.0 API returns
top-level URL arrays as `vid_urls` and `info_urls` (snake_case), but the
current `Ll2LaunchDetail` struct expects `vidURLs` / `infoURLs` (camelCase
from the older v2.2.0 API). The `#[serde(default)]` on these fields means
deserialization silently succeeds with empty vecs, and the fallback to
mission-level URLs may mask the issue — but top-level URLs (which are the
preferred source) are being silently dropped.

### Changes

**`src/vendor/launch_library_2/response_models.rs`**

- Update the `#[serde]` attributes on `Ll2LaunchDetail::vid_urls` and
  `Ll2LaunchDetail::info_urls` to accept both the old and new field names.
  Use `#[serde(alias = "vidURLs", alias = "vid_urls", default)]` so both
  API versions work.

**`src/vendor/launch_library_2/response_models.rs` — tests**

- Update `SAMPLE_DETAIL_JSON` to use the v2.3.0 snake_case field names.
- Add a test that verifies deserialization works with the old camelCase names
  too (backward compatibility with any cached responses).

### Acceptance criteria

- `cargo test` passes.
- The `deserialize_launch_detail` test confirms top-level `vid_urls` /
  `info_urls` are parsed (not empty) with v2.3.0-style field names.

---

## Step 1 — Domain model expansion ✅ Complete

Add all new domain types and fields to `models.rs`. This step touches no
rendering or API code — it only defines the shapes and updates the test
helper.

**Implementation notes:**
- `LaunchUpdate`, `CrewMember`, `StageLanding` added with `#[derive(Debug, Clone, Serialize, Deserialize)]`.
- All 21 new fields added to `LaunchDetail` with `#[serde(default)]`.
- `rocket_description` annotated `#[allow(dead_code)]`.
- `CACHE_VERSION` bumped to `2`.
- `dummy_launch_detail()` updated; `From<Ll2LaunchDetail>` impl in `convert.rs` set new fields to `None`/empty defaults (Step 3 will populate them).
- New test `launch_detail_deserializes_with_missing_new_fields` covers the old-cache scenario.
- `cargo check` clean; all 433 tests pass.

### New types

```rust
/// A status update posted to a launch.
pub struct LaunchUpdate {
    pub comment: String,
    pub created_on: DateTime<Utc>,
    pub info_url: Option<String>,
}

/// A crew member assigned to a launch.
pub struct CrewMember {
    pub name: String,
    pub role: String,
    pub agency: String,
}

/// Landing information for a single recoverable stage.
pub struct StageLanding {
    pub stage_type: Option<String>,
    pub landing_attempt: bool,
    pub landing_success: Option<bool>,
    pub landing_type: Option<String>,
    pub landing_location: Option<String>,
}
```

All three types derive `Debug, Clone, Serialize, Deserialize`.

### New fields on `LaunchDetail`

| Field | Type | Default for `dummy_launch_detail()` |
|-------|------|-------------------------------------|
| `failreason` | `Option<String>` | `None` |
| `updates` | `Vec<LaunchUpdate>` | `vec![]` |
| `crew` | `Vec<CrewMember>` | `vec![]` |
| `landings` | `Vec<StageLanding>` | `vec![]` |
| `rocket_variant` | `Option<String>` | `None` |
| `rocket_description` | `Option<String>` | `None` |
| `rocket_length` | `Option<f64>` | `None` |
| `rocket_diameter` | `Option<f64>` | `None` |
| `rocket_launch_mass` | `Option<f64>` | `None` |
| `rocket_leo_capacity` | `Option<f64>` | `None` |
| `rocket_gto_capacity` | `Option<f64>` | `None` |
| `rocket_thrust` | `Option<f64>` | `None` |
| `rocket_maiden_flight` | `Option<String>` | `None` |
| `rocket_total_launches` | `Option<u32>` | `None` |
| `rocket_successful_launches` | `Option<u32>` | `None` |
| `rocket_failed_launches` | `Option<u32>` | `None` |
| `rocket_consecutive_successes` | `Option<u32>` | `None` |
| `provider_country_code` | `Option<String>` | `None` |
| `provider_founding_year` | `Option<u32>` | `None` |
| `provider_consecutive_successes` | `Option<u32>` | `None` |
| `pad_total_launch_count` | `Option<u32>` | `None` |

All new fields must use `#[serde(default)]` so that cache files written
before this change deserialize cleanly (they will get `None`/empty defaults)
before the version check rejects them.

### Cache version bump

Change `CACHE_VERSION` from `1` to `2` in `models.rs:241`. This ensures
old detail caches are treated as misses and re-fetched with the full field
set.

### Annotate unused fields

`rocket_description` is intentionally stored but not displayed. Annotate
with `#[allow(dead_code)]`.

### Tests

- Verify that `dummy_launch_detail()` compiles and round-trips through
  serde (existing `launch_detail_cache_serde_round_trip` test covers this
  once the helper is updated).
- Add a test that deserializes a `LaunchDetail` JSON blob that is missing
  the new fields (simulating old cache) — all new fields should get
  defaults.

### Acceptance criteria

- `cargo check` passes (all call sites of `dummy_launch_detail()` compile).
- `cargo test` passes.
- New types are defined but not yet populated from the API.

---

## Step 2 — LL2 response models ✅ Complete

**Implementation notes:**
- 11 new structs added: `Ll2LaunchUpdate`, `Ll2LauncherStage`, `Ll2Landing`, `Ll2LandingType`, `Ll2LandingLocation`, `Ll2SpacecraftStage`, `Ll2CrewEntry`, `Ll2Role`, `Ll2Astronaut`, `Ll2AstronautAgency`. The plan listed `Ll2Country` separately but the existing struct was reused — `alpha_3_code: Option<String>` was added with `#[serde(default)]` so pad-level country (no alpha_3) and provider-level country (alpha_3 present) both deserialize.
- `Ll2LauncherStage::stage_type` and `Ll2Landing::landing_type` use `#[serde(rename = "type")]` to dodge the keyword.
- `Ll2LaunchDetail` extended with `failreason` + `updates`; `Ll2ProviderDetail` with `country`/`founding_year`/`consecutive_successful_launches`; `Ll2Rocket` with `launcher_stage`/`spacecraft_stage`; `Ll2RocketConfiguration` with all 13 spec fields; `Ll2Pad` with `total_launch_count`. All new fields use `#[serde(default)]`.
- All five `Ll2Pad`/`Ll2LaunchDetail` test fixtures in `convert.rs` updated for the new fields (Step 3 will populate them from the conversion logic).
- Three new tests against fixture samples: `deserialize_falcon9_sample_detail`, `deserialize_soyuz_crewed_sample_detail`, `deserialize_launch_detail_tolerates_missing_step2_fields`.
218 +- **Test fixtures convention:** sample API JSON for this vendor lives in `src/vendor/launch_library_2/fixtures/` and is exposed as named `&'static str` constants by `fixtures.rs` (e.g. `fixtures::FALCON9_DETAIL`)
+. The previous inline `SAMPLE_LIST_JSON` / `SAMPLE_DETAIL_JSON` constants were migrated to this layout, so all LL2 sample payloads are centralised in one place. When the API version bumps, drop a fresh capture in
+to `fixtures/` and tests pick up the new shape automatically.
- **Test fixtures convention:** sample API JSON for this vendor lives in `src/vendor/launch_library_2/fixtures/` and is exposed as named `&'static str` constants by `fixtures.rs` (e.g. `fixtures::FALCON9_DETAIL`). The previous inline `SAMPLE_LIST_JSON` / `SAMPLE_DETAIL_JSON` constants were migrated to this layout, so all LL2 sample payloads are centralised in one place. When the API version bumps, drop a fresh capture into `fixtures/` and tests pick up the new shape automatically.
- 437 tests pass (+3 new). Build emits dead-code warnings for the new fields — these will resolve in Step 3 when the conversion layer reads them.

### New structs

| Struct | Purpose | Key fields |
|--------|---------|------------|
| `Ll2LaunchUpdate` | `updates[]` entries | `comment`, `created_on`, `info_url` |
| `Ll2LauncherStage` | `rocket.launcher_stage[]` | `type` (renamed to avoid keyword), `landing` |
| `Ll2Landing` | `.landing` on a launcher stage | `attempt`, `success`, `type`, `landing_location` |
| `Ll2LandingType` | `.landing.type` | `abbrev` |
| `Ll2LandingLocation` | `.landing.landing_location` | `name` |
| `Ll2SpacecraftStage` | `rocket.spacecraft_stage[]` | `launch_crew` |
| `Ll2CrewEntry` | `.launch_crew[]` | `role`, `astronaut` |
| `Ll2Role` | `.role` on crew entry | `role` (the display string — not `name`) |
| `Ll2Astronaut` | `.astronaut` | `name`, `agency` |
| `Ll2AstronautAgency` | `.astronaut.agency` | `name`, `abbrev` |
| `Ll2Country` | Country object in `country[]` arrays | `alpha_3_code` |

### Expanded existing structs

**`Ll2LaunchDetail`** — add:
- `failreason: Option<String>` with `#[serde(default)]`
- `updates: Vec<Ll2LaunchUpdate>` with `#[serde(default)]`

**`Ll2Rocket`** — add:
- `launcher_stage: Vec<Ll2LauncherStage>` with `#[serde(default)]`
- `spacecraft_stage: Vec<Ll2SpacecraftStage>` with `#[serde(default)]`

**`Ll2RocketConfiguration`** — add all spec fields:
- `variant`, `description`, `length`, `diameter`, `launch_mass`,
  `leo_capacity`, `gto_capacity`, `to_thrust`, `maiden_flight`,
  `total_launch_count`, `successful_launches`, `failed_launches`,
  `consecutive_successful_launches` — all `Option` with `#[serde(default)]`

**`Ll2ProviderDetail`** — add:
- `country: Vec<Ll2Country>` with `#[serde(default)]`
- `founding_year: Option<u32>` with `#[serde(default)]`
- `consecutive_successful_launches: Option<u32>` with `#[serde(default)]`

**`Ll2Pad`** — add:
- `total_launch_count: Option<u32>` with `#[serde(default)]`

### Tests

Write deserialization tests using the reference sample JSON files to
validate the new struct shapes against real API data:

1. **Falcon 9 sample** — load `ll2-sample-detail-response.json`, deserialize
   into `Ll2LaunchDetail`, and assert:
   - `failreason` is `Some("")`
   - `updates` has 15 entries; first entry's `comment` and `created_on`
     match expected values
   - `rocket.launcher_stage[0].type` is `"Core"`
   - `rocket.launcher_stage[0].landing.attempt` is `true`
   - `rocket.launcher_stage[0].landing.type.abbrev` is `"ASDS"`
   - `rocket.launcher_stage[0].landing.landing_location.name` is
     `"Of Course I Still Love You"`
   - `rocket.configuration.variant` is `Some("Block 5")`
   - `rocket.configuration.length` is `Some(70.0)`
   - `rocket.configuration.to_thrust` is `Some(7607.0)`
   - `rocket.spacecraft_stage` is empty vec
   - `launch_service_provider.country[0].alpha_3_code` is `"USA"`
   - `launch_service_provider.founding_year` is `Some(2002)`
   - `pad.total_launch_count` is `Some(259)`

2. **Soyuz crewed sample** — load `ll2-sample-detail-response-crewed.json`,
   deserialize into `Ll2LaunchDetail`, and assert:
   - `rocket.spacecraft_stage[0].launch_crew` has 3 entries
   - First crew member: `astronaut.name` is `"Pyotr Dubrov"`,
     `role.role` is `"Commander"`,
     `astronaut.agency.abbrev` is `"RFSA"`
   - `rocket.launcher_stage` is empty vec

3. **Null tolerance** — a minimal JSON blob with all new fields absent
   should deserialize without error (all default to `None`/empty).

### Loading reference JSON in tests

Use `include_str!` to embed the reference samples at compile time:

```rust
const SAMPLE_FALCON9_JSON: &str =
    include_str!("../../../documentation/reference/ll2-sample-detail-response.json");
const SAMPLE_SOYUZ_JSON: &str =
    include_str!("../../../documentation/reference/ll2-sample-detail-response-crewed.json");
```

This avoids fragile runtime path resolution and ensures tests break if
the reference files are moved or deleted.

### Acceptance criteria

- All deserialization tests pass against both reference samples.
- Existing `deserialize_launch_detail` test still passes.
- `cargo test` passes.

---

## Step 3 — Conversion layer ✅ Complete

**Implementation notes:**
- `From<Ll2LaunchDetail>` now populates all 21 new fields (previously hardcoded to `None`/`vec![]` placeholders from Step 1).
- Added a module-private `non_empty()` helper (`""` → `None`), applied to `failreason` and `rocket_variant`.
- Three element-level `From` impls added: `Ll2LaunchUpdate → LaunchUpdate`, `Ll2CrewEntry → CrewMember` (agency `abbrev` preferred, falls back to `name`; graceful empty-string defaults when astronaut/role absent), `Ll2LauncherStage → StageLanding`.
- The rocket is unpacked once into `(config, launcher_stages, spacecraft_stages)`; crew is taken from the first `spacecraft_stage`, `provider_country_code` from `country[0].alpha_3_code`, and `pad_total_launch_count` is read before `pad` is moved into `Self`.
- **Deviation:** added `#[derive(Default)]` to `Ll2RocketConfiguration` (all fields are `Option`) to enable a clean `unwrap_or_default()` unpack in the conversion.
- **Deviation:** the full/crew conversion tests exercise the real `FALCON9_DETAIL` / `SOYUZ_CREWED_DETAIL` fixtures rather than a hand-built `Ll2LaunchDetail` — stronger coverage of the same intent the plan described.
- `cargo test` passes (441 tests: +4 new, +1 extended). Clippy clean on the changed files (`convert.rs`, `response_models.rs`).

Extend the `From<Ll2LaunchDetail> for LaunchDetail` impl in `convert.rs`
to populate the new domain model fields from the LL2 response models.

### Conversion logic

| Domain field | Source | Logic |
|---|---|---|
| `failreason` | `ll2.failreason` | `Some` only if non-empty string; treat `""` as `None` |
| `updates` | `ll2.updates` | Map each `Ll2LaunchUpdate` → `LaunchUpdate` |
| `crew` | `ll2.rocket.spacecraft_stage[0].launch_crew` | Map each entry; use `agency.abbrev` for the `agency` field (compact for TUI). Fall back to `agency.name` if `abbrev` is absent. Default to empty vec if no spacecraft stage. |
| `landings` | `ll2.rocket.launcher_stage` | Map each entry → `StageLanding` |
| `rocket_variant` | `ll2.rocket.configuration.variant` | `Some` only if non-empty |
| `rocket_description` | `ll2.rocket.configuration.description` | Direct mapping |
| `rocket_length` | `ll2.rocket.configuration.length` | Direct |
| `rocket_diameter` | `ll2.rocket.configuration.diameter` | Direct |
| `rocket_launch_mass` | `ll2.rocket.configuration.launch_mass` | Direct |
| `rocket_leo_capacity` | `ll2.rocket.configuration.leo_capacity` | Direct |
| `rocket_gto_capacity` | `ll2.rocket.configuration.gto_capacity` | Direct |
| `rocket_thrust` | `ll2.rocket.configuration.to_thrust` | Direct |
| `rocket_maiden_flight` | `ll2.rocket.configuration.maiden_flight` | Direct |
| `rocket_total_launches` | `ll2.rocket.configuration.total_launch_count` | Direct |
| `rocket_successful_launches` | `ll2.rocket.configuration.successful_launches` | Direct |
| `rocket_failed_launches` | `ll2.rocket.configuration.failed_launches` | Direct |
| `rocket_consecutive_successes` | `ll2.rocket.configuration.consecutive_successful_launches` | Direct |
| `provider_country_code` | `ll2.launch_service_provider.country` | First entry's `alpha_3_code`, or `None` if array empty |
| `provider_founding_year` | `ll2.launch_service_provider.founding_year` | Direct |
| `provider_consecutive_successes` | `ll2.launch_service_provider.consecutive_successful_launches` | Direct |
| `pad_total_launch_count` | `ll2.pad.total_launch_count` | Direct |

### Empty-string normalisation

Several API fields use `""` rather than `null` for absent values. Apply a
shared helper:

```rust
/// Convert an empty string to `None`.
fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.is_empty())
}
```

Use this for `failreason` and `rocket_variant`.

### Tests

1. **Full conversion test** — construct an `Ll2LaunchDetail` with all new
   fields populated (similar to the existing `sample_ll2_launch_detail()`
   helper, extended with new fields). Convert to `LaunchDetail` and assert
   all new fields carry over correctly.

2. **Empty / null conversion test** — construct an `Ll2LaunchDetail` where
   all new fields are `None`/empty/`""`. Verify that:
   - `failreason` is `None` (not `Some("")`)
   - `rocket_variant` is `None` (not `Some("")`)
   - `crew`, `landings`, `updates` are empty vecs
   - All spec fields are `None`

3. **Crew agency abbreviation fallback** — verify that when `agency.abbrev`
   is `None`, the conversion falls back to `agency.name`.

4. **Country code extraction** — verify that `provider_country_code` is
   `None` when the `country` array is empty.

### Acceptance criteria

- All conversion tests pass.
- `cargo test` passes.

---

## Step 4 — Hero section: fail reason ✅ Complete

**Implementation notes:**
- Added a fail-reason block to `build_hero_section()` in `detail.rs`, directly below the weather-concerns block and mirroring its structure: a styled `"Failure: "` prefix (`style::label()`) followed by word-wrapped, centered reason text.
- Added a semantic `style::warning()` helper (Red — the palette's documented "Failure" colour) rather than inlining a colour, consistent with the style module's stated purpose. The reason text uses it.
- Kept an `!reason.is_empty()` guard mirroring the weather block for safety, even though Step 3 conversion already normalises `""` → `None`.
- **Renders live immediately** — unlike Steps 5–8, no Step 9 wiring is needed because the hero section is always part of `build_content_lines()`. Any failed launch's detail view now shows the reason.
- Tests: `hero_shows_fail_reason_when_present` (text + red styling), `hero_omits_fail_reason_when_absent`, `hero_wraps_long_fail_reason` (multi-line wrap), plus `style::warning_is_red`.
- `cargo test` passes (445 tests, +4). Clippy clean on `detail.rs` / `style.rs`.

Add the fail reason display to the hero section of the detail view.

### Changes

**`src/tui/views/detail.rs` — `build_hero_section()`**

After the weather concerns block (around line 228), add:

```
if let Some(reason) = &detail.failreason {
    // "Failure: {reason}" styled as warning (yellow/red)
}
```

Word-wrap the fail reason text like weather concerns. Use `style::label()`
for the "Failure: " prefix and a warning colour for the reason text. The
design plan shows this as a distinct visual element below weather.

### Tests

1. Test that a `LaunchDetail` with `failreason: Some("Engine failed")`
   produces content lines containing `"Failure:"` and `"Engine failed"`.
2. Test that `failreason: None` adds no extra lines.
3. Test with a long fail reason string — verify word-wrapping.

### Acceptance criteria

- `cargo test` passes.
- Fail reason renders below weather when present, absent when `None`.

---

## Step 5 — Updates section

Add the new conditional Updates section.

### Changes

**`src/tui/views/detail.rs`**

Add a new `build_updates_section()` function:

- Skip entirely if `detail.updates` is empty (conditional rendering).
- Show at most 5 entries (take the first 5; they arrive
  reverse-chronological from the API).
- Each entry: timestamp as Tier 3 label, comment as Tier 2 secondary.
- Blank line between entries.
- Timestamp format: `"Mon DD, HH:MM UTC"` (e.g. `"Mar 21, 02:31 UTC"`).

This function is not yet wired into `build_content_lines()` — that happens
in Step 9 (layout restructure). For now, just implement and test it in
isolation.

### Tests

1. Empty updates vec → function returns no lines.
2. 3 updates → all 3 render with correct timestamp and comment.
3. 8 updates → only first 5 render.
4. Long comment text → verify word-wrapping.

### Acceptance criteria

- `cargo test` passes.
- Function exists and is tested but not yet called from the main renderer.

---

## Step 6 — Mission section: crew + landing

Add the CREW and LANDING conditional sub-sections to the mission section.

### Changes

**`src/tui/views/detail.rs` — `build_mission_section()`**

After the existing mission description block, add:

1. **CREW sub-section** (only when `detail.crew` is non-empty):
   - Sub-heading: `"   CREW"` styled as `section_heading()`.
   - Three-column table: name, role, agency — aligned with fixed column
     widths.

2. **LANDING sub-section** (only when any entry in `detail.landings` has
   `landing_attempt == true`):
   - Sub-heading: `"   LANDING"` styled as `section_heading()`.
   - Each row: stage type, landing type abbreviation, landing location name.
   - Omit columns that are `None` (don't print "Unknown").

### Tests

1. No crew, no landings → mission section unchanged from existing behaviour.
2. 4 crew members → CREW sub-heading appears, all 4 render in table format.
3. 1 landing with `landing_attempt: true` → LANDING sub-heading + row.
4. 1 landing with `landing_attempt: false` → no LANDING sub-section.
5. 3 landings (Falcon Heavy style: 1 core ASDS + 2 side boosters RTLS) →
   3 rows render.
6. Landing with `landing_type: None` and `landing_location: None` → row
   renders with just the stage type.

### Acceptance criteria

- `cargo test` passes.
- Crew and landing sub-sections render correctly within the mission section.

---

## Step 7 — Vehicle section expansion

Expand the vehicle section from just a rocket name to include specs,
maiden flight, and a vehicle-specific record bar.

### Changes

**`src/tui/views/detail.rs` — `build_vehicle_lines()`**

Rewrite to include:

1. **Rocket name line** — combine `rocket_full_name` and `rocket_variant`.
   If `variant` is already contained in `full_name`, do not duplicate it.
2. **Maiden flight** — `"Maiden flight: Jun 4, 2010"` styled as
   `style::label()`. Only shown when `rocket_maiden_flight` is `Some`.
   Parse the `"YYYY-MM-DD"` string and format as `"Mon DD, YYYY"`.
3. **Specs grid** — two-column key-value layout when terminal >= 100 cols,
   single-column below that. Left column: length, diameter, launch mass.
   Right column: LEO capacity, GTO capacity, thrust. Only render rows where
   the value is `Some`. Omit the entire grid if all six are `None`.
   Format floats as integers for display (e.g. `549.0` → `"549 t"`).
4. **Vehicle record bar** — same `build_record_bar_line()` helper used by
   provider. Show when `rocket_total_launches` is `Some` and > 0.
   Stats line: `"N launches (X ok, Y fail)"`.
   Consecutive line: `"N consecutive"` when > 0, styled as Tier 3.

### Tests

1. All vehicle fields `None` → only the existing rocket name renders (no
   specs, no bar).
2. Full specs → two-column grid renders all 6 values with units.
3. Partial specs (only length + thrust) → grid renders only those 2 rows.
4. Maiden flight formatting: `"2018-05-11"` → `"May 11, 2018"`.
5. Vehicle record bar: 569 ok, 1 fail, 570 total → bar + stats line.
6. Consecutive successes: 272 → `"272 consecutive"` line appears.
7. Variant deduplication: `full_name = "Falcon 9 Block 5"`,
   `variant = "Block 5"` → renders `"Falcon 9 Block 5"` (not
   `"Falcon 9 Block 5 Block 5"`).
8. Variant adds info: `full_name = "Falcon 9"`,
   `variant = "Block 5"` → renders `"Falcon 9 Block 5"`.
9. Narrow terminal (< 100 cols) → specs grid renders single-column.

### Acceptance criteria

- `cargo test` passes.
- Vehicle section matches the design plan mockup.

---

## Step 8 — Provider + Location expansion

Add the new fields to the provider and location sections.

### Changes

**`src/tui/views/detail.rs` — `build_provider_lines()`**

After the provider name line, add:

- **Founded line**: `"Founded {year} · {country_code}"` styled as Tier 2.
  Only shown when either `provider_founding_year` or
  `provider_country_code` is `Some`. Uses `·` separator only if both
  present; shows whichever is available if only one is.
- **Consecutive successes**: `"N consecutive"` line below the existing stats
  line, styled as Tier 3. Only shown when > 0.

**`src/tui/views/detail.rs` — `build_location_lines()`**

After the location name line, add:

- **Pad launch count**: `"N launches from this pad"` styled as Tier 3.
  Only shown when `pad_total_launch_count` is `Some` and > 0.

### Tests

1. No founding year, no country code → no founded line.
2. Both present → `"Founded 2002 · USA"`.
3. Only year → `"Founded 2002"`.
4. Only country → `"NZL"`.
5. Consecutive successes 148 → `"148 consecutive"` line appears.
6. Consecutive successes 0 → line omitted.
7. Pad launch count 259 → `"259 launches from this pad"` line.
8. Pad launch count `None` → line omitted.

### Acceptance criteria

- `cargo test` passes.
- Provider and location sections match the design plan mockup.

---

## Step 9 — Layout restructure

Reorder the sections in `build_content_lines()` to match the design plan.
This step also moves provider/location into a two-column pair and vehicle
to its own full-width section.

### Current section order

```
Hero → Vehicle+Location (two-col) / Provider (full-width) → Mission → Links
```

### New section order (from design plan)

```
Hero → Updates (conditional) → Mission → Vehicle (full-width) → Provider|Location (two-col) → Links (conditional)
```

### Changes

**`src/tui/views/detail.rs` — `build_content_lines()`**

Rewrite the section sequence:

```rust
fn build_content_lines(detail: &LaunchDetail, width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(128);

    // 1. Hero
    build_hero_section(&mut lines, detail, width);

    // 2. Updates (conditional)
    if !detail.updates.is_empty() {
        push_separator(&mut lines, width);
        build_updates_section(&mut lines, detail, width);
    }

    // 3. Mission
    push_separator(&mut lines, width);
    build_mission_section(&mut lines, detail);

    // 4. Vehicle (full-width)
    push_separator(&mut lines, width);
    build_vehicle_section(&mut lines, detail, width);

    // 5. Provider | Location (two-col or stacked)
    push_separator(&mut lines, width);
    if width >= TWO_COL_MIN_WIDTH {
        build_provider_location_two_col(&mut lines, detail, width);
    } else {
        build_provider_location_stacked(&mut lines, detail, width);
    }

    // 6. Links (conditional)
    if has_any_links(detail) {
        push_separator(&mut lines, width);
        build_links_section(&mut lines, detail);
    }

    lines.push(Line::raw(""));
    lines
}
```

**`build_two_column_section()`** → rename to
`build_provider_location_two_col()`. This function now pairs Provider
(left) and Location (right) instead of Vehicle and Location. Vehicle
becomes its own `build_vehicle_section()` full-width function.

**`build_single_column_section()`** → rename to
`build_provider_location_stacked()`. Same change: provider + location
only, no vehicle.

**`build_vehicle_section()`** — new function that renders the VEHICLE
heading, the expanded vehicle content from Step 7, always full-width.

### Tests

1. **Section order** — verify content lines contain section headings in the
   correct order: any hero content, then `"UPDATES"` (if present),
   `"MISSION"`, `"VEHICLE"`, `"PROVIDER"`, `"LOCATION"`, `"LINKS"` (if
   present).
2. **No updates** — verify no `"UPDATES"` heading and no separator before
   mission.
3. **No links** — verify no `"LINKS"` heading and no trailing separator.
4. **Two-col at 100+ width** — provider and location appear side-by-side.
5. **Single-col below 100** — provider and location stack vertically.
6. **Update existing tests** — some existing content tests in `detail.rs`
   check for specific section ordering or content presence. Update these
   to reflect the new order (e.g. `content_lines_contain_vehicle` may need
   to look for vehicle-specific content in a different position).
7. **Full integration test** — use a `dummy_launch_detail()` with all new
   fields populated. Render into a `TestBackend` and verify the complete
   output matches expectations.

### Acceptance criteria

- `cargo test` passes (including updated existing tests).
- The detail view section order matches the design plan mockup.
- Two-column and single-column responsive layouts work correctly.

---

## Summary

| Step | Files modified | New types | Tests |
|------|---------------|-----------|-------|
| 0 | `response_models.rs` | — | 2 |
| 1 | `models.rs` | `LaunchUpdate`, `CrewMember`, `StageLanding` | 2 |
| 2 | `response_models.rs` | ~11 new LL2 structs | 3+ |
| 3 | `convert.rs` | — | 4 |
| 4 | `detail.rs` | — | 3 |
| 5 | `detail.rs` | — | 4 |
| 6 | `detail.rs` | — | 6 |
| 7 | `detail.rs` | — | 9 |
| 8 | `detail.rs` | — | 8 |
| 9 | `detail.rs` | — | 7+ |

Steps 0–3 form the data pipeline (model → deserialize → convert). Steps
4–8 add rendering for individual sections and can be worked in parallel
if desired since they touch different functions. Step 9 is the final
integration step that wires everything together.
