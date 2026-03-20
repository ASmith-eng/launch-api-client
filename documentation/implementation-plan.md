# Implementation Plan

This plan breaks the Launch Client TUI into self-contained steps designed for
AI agent sessions. Each step produces compilable, testable code and builds on
the previous one. Steps reference specific sections of the
[design document](./launch-api-tui-design.md).

---

## Phase 1: Project Scaffolding & Core Types

### Step 1.1 — Project initialisation ✅

Set up the Rust project skeleton with all dependencies and the module tree
from the design doc.

**Produce:**
- `Cargo.toml` with all dependencies (ratatui, crossterm, tokio, reqwest,
  serde, serde_json, chrono, chrono-tz, toml, tracing, tracing-subscriber,
  directories, governor, thiserror)
- Dev dependencies: `tempfile`, `wiremock`, `tokio` (with `test-util` feature)
  (design doc §Technology Stack — Dev Dependencies)
- Empty module files matching the source code organisation (design doc §Source
  Code Organisation): `src/vendor/launch_library_2/`, `src/api/`, `src/cache/`,
  `src/tui/`, `src/config/`, `src/main.rs`
- Each `mod.rs` re-exports its children; `main.rs` declares all top-level
  modules

**Acceptance criteria:** `cargo check` passes with no errors.

**Status:** Complete. `cargo check` passes. All modules wired up.

---

### Step 1.2 — Data models and vendor types ✅

Implement all core structs and the vendor-specific LL2 response models.

**Produce:**
- Core domain structs in appropriate modules (design doc §Data Models):
  `LaunchSummary`, `LaunchStatus`, `NetPrecision`, `Provider`, `PadInfo`,
  `LocationInfo`, `MissionSummary`, `LaunchListCache`, `LaunchDetailCache`,
  `AppState`, `RateLimitState`
- `AppError` enum with `thiserror` derive and `is_retryable()` method
  (design doc §Error Type Architecture)
- `ErrorState` enum (`Transient`, `RateLimited`, `Offline`) for UI error
  state management (design doc §Application State Machine)
- `Clock` trait with `SystemClock` and `FakeClock` (test-only)
  implementations (design doc §Clock Abstraction for Testability)
- Vendor response models in `src/vendor/launch_library_2/response_models.rs`
  with serde `Deserialize` matching the LL2 API JSON shapes (design doc
  §Caching Strategy for example payloads)
- Conversion logic from vendor response models → core domain types
- Status map in `src/vendor/launch_library_2/status_map.rs` (design doc
  §Launch Status Colors and Styles)
- Region map in `src/vendor/launch_library_2/region_map.rs` (design doc
  §Filter Panel — region-to-`pad__location` mapping)

**Acceptance criteria:** `cargo check` passes. Unit tests verify:
serde round-trip on example JSON payloads from the design doc,
`AppError::is_retryable()` returns correct results for each variant.

**Status:** Complete. 27 tests passing. All domain types, vendor models,
status map, region map, conversions, Clock trait, AppError, and ErrorState
implemented. clippy clean (only dead-code warnings from unused-yet pub items).

---

### Step 1.3 — Configuration loading ✅

Implement `config.toml` parsing with defaults and directory setup.

**Produce:**
- `src/config/mod.rs`: `Config` struct with serde `Deserialize`, matching
  all fields in design doc §User Configuration
- Default values for every field (design doc §Configuration Behavior —
  "app works with sensible defaults")
- `load_config()` function: reads `config.toml` from the platform config
  directory (`directories` crate), falls back to defaults on missing/invalid
  file, logs warnings on parse failures
- Directory creation logic for both `config_dir` and `cache_dir`
  (design doc §File Structure)

**Acceptance criteria:** `cargo test` passes. Tests cover: missing file →
defaults, valid file → parsed values, partial file → defaults for missing
keys, invalid values → defaults with warning.

**Status:** Complete. 8 new tests (35 total). Config struct with 4 sections
(api, cache, ui, log), all `#[serde(default)]`. `load_config()` handles
missing/invalid/partial files. `AppDirs` resolves platform paths and creates
directories including `details/` subdirectory.

---

### Step 1.4 — Logging setup ✅

Initialise `tracing` with file output.

**Produce:**
- `src/logging.rs` (or within `main.rs`): `init_logging()` function
- Writes to `<config_dir>/app.log` (design doc §File Structure)
- Default level WARN, configurable via `Config`
- Log file truncated at 1 MB on startup (design doc §Logging)

**Acceptance criteria:** `cargo run` creates log file. Setting log level to
`debug` in config produces debug output in the log file.

**Status:** Complete. 4 new tests (39 total). `src/logging.rs` with
`init_logging()`, 1 MB truncation on startup, configurable level via
`LogConfig`. `main.rs` wired up: resolves dirs, loads config, inits logging.

---

## Phase 2: Cache & Rate Limiting

### Step 2.1 — Cache manager (read/write) ✅

Implement the file-based cache system for app state, launch list, and
launch details.

**Produce:**
- `src/cache/mod.rs`: `CacheManager` struct (design doc §Data Models)
- Read/write for `app_state.json`, `cache.json`, `details/{uuid}.json`
- File structure as specified (design doc §Caching Strategy — File Structure)
- Atomic writes (write to temp file then rename) to prevent corruption
- Cache version checking on load: version mismatch is treated as a cache miss,
  not an error (design doc §Cache Version Mismatches)
- Cache read errors logged at `WARN` level before falling back to re-fetch
  (design doc §Cache Read Errors)

**Acceptance criteria:** Unit tests (using `tempfile::TempDir`) verify: write
then read round-trips for all three cache types, missing files return `None`,
corrupt files return `Err` or `None` gracefully, version mismatch returns
`None` (treated as cache miss).

**Status:** Complete. 22 new tests (57 total). `CacheManager` with 6 public
methods (load/save for app_state, launch_list, launch_detail). Generic
`load_versioned<T>()` via `HasVersion` trait. Atomic writes (`.tmp` + rename).
Version mismatch → `Ok(None)` with DEBUG log; corrupt files → `Ok(None)` with
WARN log. clippy clean (only pre-existing dead-code warnings).

---

### Step 2.2 — Cache TTL and expiry logic ✅

Add TTL calculation based on launch proximity and cache staleness checks.

**Produce:**
- `CacheStrategy` enum and TTL calculation (design doc §Cache Expiry)
- `is_stale()` methods on `LaunchListCache` and `LaunchDetailCache`
- TTL values driven by `Config` (design doc §User Configuration — cache
  section)
- `expires_at` computation when writing cache entries
- `CacheManager` accepts a `Clock` parameter for time queries (design doc
  §Clock Abstraction for Testability)

**Acceptance criteria:** Unit tests (using `FakeClock` and `tempfile::TempDir`)
cover all five TTL tiers. Tests for edge cases: launch exactly 7 days away,
exactly 24 hours away, in-flight status, past launches. Time is advanced
via `FakeClock` — no wall-clock sleeps.

**Status:** Complete. 34 new tests (91 total). `CacheStrategy` enum with 5
tiers (Permanent/LongTerm/MediumTerm/ShortTerm/RealTime), `for_launch()`
determination by status ID + proximity, `ttl_minutes()` driven by CacheConfig,
`compute_expires_at()` and `list_expires_at()` helpers, `is_stale(now)` on both
cache types, `CacheManager<C: Clock>` generic. `FakeClock` updated to
`Arc<Mutex>` for cloneable shared time. All boundary tests covered. clippy
clean.

---

### Step 2.3 — Cache pruning ✅

Implement detail cache cleanup on startup.

**Produce:**
- Pruning logic in `CacheManager` (design doc §Cache Pruning)
- Age-based and count-based deletion
- Runs every N startups based on config
- Startup count tracking in `app_state.json`

**Acceptance criteria:** Unit tests with a temp directory: create N+1 dummy
detail files, verify oldest are pruned to N. Verify age-based pruning
deletes old files.

**Status:** Complete. 28 new tests (123 total). `prune_details()` on
CacheManager: age-based deletion (files with `fetched_at` older than
`max_detail_age_days`), then count-based (oldest by `fetched_at` removed until
within `max_detail_files`). Corrupt files cleaned up during pruning.
`should_prune(startup_count, config)` helper for startup-count gating.
`read_fetched_at()` uses serde_json::Value for lightweight partial parsing.
Also added `Config::sanitize()` — defensive validation of all config fields
after deserialization (zero TTLs, invalid URLs, path traversal in log filename,
invalid enum-like strings, out-of-range numeric values). Removed
`ttl_past_launches` from config (past launches use internal `Permanent`
strategy — data is final, no re-fetch needed). Added max caps: TTL fields
capped at 43 200 min (30 days), `prune_every_n_startups` capped at 50.
Design document updated with sanitisation table and revised config example.
Tests cover age/count/combined pruning, boundary conditions, corrupt files,
missing dir, non-.json skipping, FakeClock advancement, max-cap validation.

---

### Step 2.4 — Rate limiter ✅

Implement client-side rate limit tracking.

**Produce:**
- `src/api/rate_limiter.rs`: `RateLimiter<C: Clock>` struct, generic over
  `Clock` (design doc §API Rate Limiting, §Clock Abstraction for Testability)
- Rolling-window tracking with `VecDeque<DateTime<Utc>>`
- `can_make_request()` and `record_request()` methods
- `remaining()` count and `next_available_at()` time
- Persistence: load/save request history from/to `app_state.json`
- `should_sync()` logic for `/api-throttle/` calibration (design doc
  §Rate Limit Calibration Strategy)

**Acceptance criteria:** Unit tests (using `FakeClock`): recording 15 requests
blocks the 16th, requests older than 1 hour are pruned (advance `FakeClock`
by 1 hour to verify), `remaining()` returns correct count,
`should_sync()` triggers under specified conditions, `next_available_at()`
returns correct time. No wall-clock sleeps.

**Status:** Complete. 21 new tests (143 total). `RateLimiter<C: Clock>` with
rolling-window `VecDeque<DateTime<Utc>>`, 1-hour window, 15/30 request limits.
Methods: `can_make_request()`, `record_request()`, `remaining()`, `limit()`,
`next_available_at()`, `should_sync()`, `record_sync()`, `set_authenticated()`.
Persistence via `from_state()`/`to_state()` converting to/from `RateLimitState`.
`record_sync()` reconciles local window with server-reported usage (drops excess
or adds synthetic entries). Expired requests pruned on load. Clock backward jump
handled correctly. No `governor` crate used — custom sliding window as designed.
clippy clean (only pre-existing dead-code warnings).

---

## Break 1: Reflection ✅

Use plan mode or any brainstorming and code review skills to reflect on the design decisions and implementation so far. It is important we do this to fix issues early and stop us fighting an uphill battle later when the logic becomes larger and more complex to change.

**Discuss:**
- Are there still any open questions or design gaps for the steps we have implemented so far? Think about the design logic - are there any decisions we've made that don't make sense (overcomplicated, or too many assumptions)?
- Do current tests adequately appraise the expected behaviour of configuration, logging, and caching? Can we trust that all tests passing means these functions of the app are working?
- Looking ahead to the next phases, can you forsee any issues with how the new logic we will implement will interface with the logic we have already completed?
- Anything else you need to be explained, or want to discuss?

**Status:** Complete. Full codebase review conducted. Findings:

**Design decisions — all sound:**
- Clock trait abstraction is consistent across CacheManager and RateLimiter.
- Vendor boundary (LL2 types → domain types via `From`) is clean.
- Config sanitization is thorough. `ErrorState::Transient` correctly uses
  `std::time::Instant` (not `Clock`) for UI dismiss timing.
- `governor` crate was already removed (custom rolling window is simpler).

**Open questions status:**
- #2 (detail view typed model): Add `LaunchDetail` struct in Step 3.1 when
  defining the detail endpoint response shape.
- #3 (crossterm event-stream): Already in Cargo.toml. Resolved.
- #4 (config defaults): Code uses 180/30 matching the expiry section. Config
  example in design doc is the outlier — doc-only fix needed. Resolved.
- #1 (pagination), #5 (unicode-width), #6 (region IDs): Deferred to later
  phases as noted in each question.

**Test adequacy:** 143 tests provide good coverage for Phase 1–2. Integration
tests for the API client and UI rendering tests will fill remaining gaps.

**Looking ahead — no blockers:**
- `main.rs` needs `#[tokio::main]` for async (Step 4.1).
- Single-threaded `tokio::select!` avoids `Arc<Mutex>` on shared state.

**Cleanup applied:**
- Fixed clippy: `.is_multiple_of()` in cache/mod.rs, `#[derive(Default)]` on
  Config.

---

## Phase 3: API Client

### Step 3.1 — API client trait and LL2 implementation ✅

Build the HTTP client with the vendor-agnostic trait and LL2-specific
implementation.

**Produce:**
- `src/api/client.rs`: async trait with methods for fetching launch list,
  launch detail, and throttle status (design doc §Source Code Organisation —
  `api::client` defines traits)
- `src/vendor/launch_library_2/endpoints.rs`: base URL constants, endpoint
  paths, query parameter building (design doc §API Integration)
- LL2 implementation of the API client trait using `reqwest`
- Rate limiter integration: check before each request, record after
- API key header injection when configured (`Authorization: Token <key>`)
- Dev vs production base URL from config
- Typed `LaunchDetail` struct for the `/launch/{id}/` response (resolves
  Open Question #2 — replaces raw `serde_json::Value` in detail cache)

**Acceptance criteria:** `cargo check` passes. Integration test with a mock
HTTP server (`wiremock`) verifies: successful list fetch parses correctly,
successful detail fetch parses correctly, rate limiter blocks requests when
exhausted.

**Status:** Complete. 23 new tests (166 total). `LaunchApi` trait with native
async fn (no `async-trait` crate needed on Rust 1.93). `Ll2Client<C: Clock>`
wraps `reqwest::Client` + `Mutex<RateLimiter<C>>`. Rate limiter checked before
each request; throttle endpoint does not count against limit. `LaunchDetail`
domain type added with fields for the detail view (probability, weather,
provider stats, rocket name, URLs, programs). `LaunchDetailCache.data` changed
from `serde_json::Value` to typed `LaunchDetail` (resolves Open Question #2).
Vendor types: `Ll2LaunchDetail`, `Ll2ThrottleResponse` (verified against live
`/api-throttle/` endpoint — fields: `your_request_limit`, `current_use`,
`limit_frequency_secs`, `next_use_secs`, `ident`). `endpoints.rs` with URL
builders for all three endpoints + `ListParams` for filter query parameters.
Conversions: `Ll2LaunchDetail → LaunchDetail`, `Ll2ThrottleResponse →
ThrottleStatus`. All existing cache tests updated for typed `LaunchDetail`.
wiremock integration tests cover: list fetch, detail fetch, 404 handling,
throttle sync, rate limiter blocking, API key header injection, server error
mapping. clippy clean (only pre-existing dead-code warnings).

---

### Step 3.2 — Error handling and retry logic ✅

Add network error handling, retry, and `/api-throttle/` sync.

**Produce:**
- Retry logic gated by `AppError::is_retryable()` — only transient network
  errors and 5xx responses trigger a retry; non-retryable errors (4xx,
  deserialization) skip retry entirely (design doc §Error Type Architecture,
  §Error Categories and UI Behaviour — Network/API Errors)
- Single automatic retry with 1-second `tokio::time::sleep` (non-blocking)
- 429 response handling: sync with `/api-throttle/`, update local rate limit
  state, set `ErrorState::RateLimited`
- Startup sync with `/api-throttle/` — also gets retry treatment on failure
  (design doc §Rate Limit Calibration Strategy, §Startup Sequence step 3)
- Offline detection (reactive, from network errors — sets
  `ErrorState::Offline`, design doc §Error Categories and UI Behaviour —
  Offline)

**Acceptance criteria:** Tests with mock server (`wiremock`): 5xx triggers one
retry then returns error, 4xx skips retry and returns error immediately, 429
triggers throttle sync, timeout returns offline-compatible error,
`/api-throttle/` failure retries once then degrades gracefully.
`cargo test` passes.

**Status:** Complete. 15 new tests (181 total). Changes:

- `src/error.rs` — `is_retryable()` extended to classify `ApiError` with 5xx
  status as retryable. New `is_offline_signal()` method for UI-layer offline
  detection (connect errors + timeouts).
- `src/api/client.rs` — `send_with_retry()`: wraps `send_and_check()` with
  single automatic retry (1-second `tokio::time::sleep`) for retryable errors.
  `handle_429()`: on 429 response, best-effort sync with `/api-throttle/`
  endpoint then returns `AppError::RateLimited(next_available_at)`. If throttle
  sync itself fails, falls back to local rate limiter state.
  `fetch_throttle_raw()`: extracted raw throttle HTTP call shared by
  `handle_429()`, `fetch_throttle_status()`, and `sync_throttle_with_retry()`.
  `sync_throttle_with_retry()`: public method for startup sequence — calls
  throttle endpoint with single retry on transient failure, degrades gracefully
  on persistent failure (logs warning, returns error for caller to handle).
  Trait impl methods now use `send_with_retry()` instead of `send_and_check()`.
- wiremock tests cover: 5xx triggers retry (2 requests made), 5xx retry
  succeeds on second attempt, 4xx skips retry (1 request), 404 detail skips
  retry, 429 → throttle sync + RateLimited error, 429 with throttle sync
  failure still returns RateLimited, 429 on retry attempt also triggers throttle
  sync, startup sync retries on 5xx and succeeds, startup sync degrades
  gracefully on persistent failure (2 requests), startup sync no retry on 4xx
  (1 request).
- `is_offline_signal()` tested via unit tests on error variants (Network errors
  signal offline; ApiError, CacheIo, Config, RateLimited do not).
- clippy clean (only pre-existing dead-code warnings from unused-yet pub items).

---

## Phase 4: TUI — Core Loop & List View

### Step 4.1 — Application state and event loop ✅

Set up the ratatui terminal, app state machine, and async event loop.

**Produce:**
- `src/tui/app.rs`: `App` struct (with `error_state: Option<ErrorState>`),
  `AppScreen` enum, `ErrorState` enum (design doc §Application State Machine)
- `src/tui/event.rs`: async event loop with `tokio::select!` (design doc
  §Async Event Loop Architecture)
- `TerminalGuard` RAII struct for terminal restoration on normal exit and
  panic (design doc §Graceful Shutdown). Panic hook registered as secondary
  safety net.
- `main.rs`: switch to `#[tokio::main] async fn main()`, wire up startup
  sequence — config → logging → cache → rate limiter → API sync → TUI
  (design doc §Startup Sequence)
- Minimum terminal size check with warning render (design doc §Minimum
  Terminal Size)
- Resize event handling

**Acceptance criteria:** `cargo run` enters alternate screen, shows a
placeholder UI, responds to `q` to quit, and restores terminal on exit
(including on panic — verify `TerminalGuard::drop` runs). Resize below
80x24 shows warning.

**Status:** Complete. 181 tests still passing (no new tests — this step is
primarily wiring). New files:

- `src/tui/app.rs` — `App` struct with all state fields from the design doc,
  `AppScreen` enum (List/Detail/Help/FilterPanel), `FilterState` struct,
  `MIN_COLS`/`MIN_ROWS` constants, `is_terminal_too_small()` helper.
- `src/tui/terminal.rs` — `TerminalGuard` RAII struct (Drop restores terminal),
  `setup_terminal()` (alternate screen + raw mode), `install_panic_hook()`
  (secondary safety net). Type alias `Tui` for the terminal backend.
- `src/tui/event.rs` — `run_event_loop()` with `tokio::select!` multiplexing
  crossterm `EventStream` and optional pending API future. `FetchResult` enum
  for async fetch outcomes. Key handlers for List/Detail/Help/FilterPanel
  screens (q/Esc/arrows/Enter/?/Ctrl+C). Placeholder renderers for all views.
  Minimum terminal size warning. Error state rendering (Transient auto-dismiss
  after 10s, RateLimited with reset time, Offline indicator).
- `src/main.rs` — Full startup sequence: `#[tokio::main]`, AppDirs → config →
  logging → CacheManager → app_state load/create → startup count increment →
  conditional cache pruning → Ll2Client with RateLimiter from persisted state →
  `/api-throttle/` sync → cache.json load with freshness check → terminal setup
  → initial fetch if needed → event loop → save app_state on exit.
- `Cargo.toml` — Added `futures = "0.3"` for `StreamExt` (used with crossterm
  `EventStream`).

clippy clean (only pre-existing dead-code warnings from unused-yet pub items).

---

### Step 4.2 — List view rendering ✅

Render the launch list with navigation.

**Produce:**
- `src/tui/views/list.rs`: list view renderer (design doc §List View layout)
- Launch items showing: name, time (dual timezone), status badge, provider,
  location (design doc §List View mockup)
- Selected item highlighting with `▸` marker
- Up/down/Home/End navigation updating `selected_index` and
  `list_scroll_offset`
- "Showing N of M" in title bar
- Rate limit display in title bar (`12/15 reqs`)
- `styled_block()` helper with rounded borders (design doc §Border Style)

**Acceptance criteria:** `cargo run` displays cached (or freshly fetched)
launches in a scrollable list. Arrow keys navigate. Visual output matches
the design doc mockup.

**Status:** Complete. 17 new tests (198 total). New/modified files:

- `src/tui/views/mod.rs` — `styled_block()` helper using `BorderType::Rounded`
  with `DarkGray` border. Re-exports `list` module.
- `src/tui/views/list.rs` — Full list view renderer: `render_list()` with
  scrollable launch items (2 lines per item + blank separator), status badges
  colored via `status_style()`, dual timezone time display (location tz / UTC)
  using `chrono-tz`, net precision awareness (Month → "Sep 2026", Year → "2026",
  Day → "Feb 28, 2026", Hour/Minute → full dual tz), name truncation with
  ellipsis, `▸` selection marker with bold white, provider | location line in
  gray. `compute_scroll_offset()` keeps selected item visible. Bottom hint bar
  with keybinding shortcuts. Loading/empty/offline states handled.
- `src/tui/app.rs` — Added `rate_limit_remaining: Option<u32>` and
  `rate_limit_total: Option<u32>` fields for title bar display.
- `src/tui/event.rs` — Replaced `render_list_placeholder()` with
  `list::render_list()`. Added `update_list_scroll()` called after every
  navigation key to keep scroll offset in sync.
- `src/main.rs` — Populates rate limit display fields from `Ll2Client`'s
  rate limiter state after throttle sync.
- Tests cover: scroll offset (empty, no-scroll, above/below window, clamped,
  all-visible), string truncation (short, exact, ellipsis), net time formatting
  (month, year, day, hour+timezone, no precision, invalid timezone), status
  badge rendering (known + unknown IDs). clippy clean.

---

### Step 4.3 — Time display and net precision ✅

Implement the time formatting logic used across views.

**Produce:**
- Dual timezone formatting: user local time + launch location timezone
  (design doc §Time Display — Dual Timezone Format)
- Net precision awareness: full datetime / date / month / year display
  depending on precision (design doc §Net Precision Awareness)
- Countdown calculation (`T-dd:hh:mm:ss`) with precision gating (design doc
  §Countdown Timer)
- Countdown color tiers (design doc §Countdown Styling)
- Relative time formatting for staleness ("5m ago", "2h ago")

**Acceptance criteria:** Unit tests for each precision level. Tests for
timezone conversion using known IANA zones. Countdown formats correctly for
various time deltas.

**Status:** Complete. 40 new tests (232 total, net +34 after moving 6 from
list.rs). New/modified files:

- `src/tui/time_fmt.rs` — Shared time formatting module with:
  `format_net_time()` — precision-aware display (Month → "Sep 2026",
  Year → "2026", Day → "Feb 28, 2026", Hour/Minute → dual timezone).
  `format_dual_timezone()` — `Feb 28 03:00 CST / 09:00 UTC` using chrono-tz,
  falls back to UTC on missing/invalid timezone.
  `has_precise_time()` — precision gating for countdown (Month/Year → false).
  `format_countdown()` — `T-dd days, hh:mm:ss` for future, `T+` for past;
  omits days component when < 1 day.
  `countdown_style()` — proximity-based styling: >7d dim white, 1–7d normal
  white, <24h yellow bold, <1h red bold, T+ cyan bold.
  `format_relative_time()` — "just now", "5m ago", "2h ago", "3d ago".
- `src/tui/mod.rs` — Added `pub mod time_fmt`.
- `src/tui/views/list.rs` — Refactored to use `time_fmt::format_net_time()`
  instead of inline formatting. Removed 6 duplicate time tests (now in
  time_fmt.rs with broader coverage).
- Tests cover: all 4 precision levels × multiple timezones (Chicago, New York,
  Tokyo, UTC), invalid timezone fallback, countdown (future with/without days,
  past/in-flight, exact zero, exactly 1 day, large values), all 5 countdown
  style tiers + boundary values (exactly 7d, 24h, 1h), relative time (just now,
  minutes, hours, days, future-as-just-now, boundary values 60s/1h/1d/59m/23h).
  clippy clean.

---

### Step 4.4 — Status bar ✅

Implement the bottom status bar.

**Produce:**
- Two-part status bar (design doc §Status Bar)
- Left: cache staleness info with all four states (fresh, stale, offline,
  refreshing)
- Cache staleness display respecting `staleness_style` config (relative vs
  absolute)
- Time format respecting `time_format` config (12h vs 24h)

**Acceptance criteria:** Status bar renders correctly in all four states.
Staleness text updates reflect cache age. Config toggles work.

**Status:** Complete. 20 new tests (252 total). New/modified files:

- `src/tui/views/status_bar.rs` — `build_status_line()` returns a styled
  `Line` for the status bar. Four states: refreshing (`"Refreshing..."`),
  offline (`"Updated Xm ago (stale) • [OFFLINE]"`), stale (`"Updated Xh ago
  (stale) • Press 'r' to refresh"`), fresh (`"Updated Xm ago • Next refresh
  after HH:MM UTC"`). Respects `staleness_style` config (`"relative"` →
  `"Updated 5m ago"`, `"absolute"` → `"Last updated 10:40 AM UTC"`) and
  `time_format` config (`"12h"` → `"10:45 AM"`, `"24h"` → `"10:45"`).
- `src/tui/time_fmt.rs` — Added `format_time_of_day()` for absolute time
  display (12h with AM/PM or 24h). 8 new tests covering morning/afternoon/
  midnight/noon for both formats plus unknown format fallback.
- `src/tui/app.rs` — Added `cache_fetched_at`, `cache_expires_at`
  (`Option<DateTime<Utc>>`) and `ui_config` (`UiConfig`) fields to `App`.
- `src/tui/views/list.rs` — Replaced placeholder staleness line in
  `render_hint_bar()` with `status_bar::build_status_line()`.
- `src/tui/views/mod.rs` — Added `pub mod status_bar`.
- `src/main.rs` — Populates `cache_fetched_at`, `cache_expires_at`, and
  `ui_config` from loaded cache and config.
- Tests cover: all four status states (refreshing, offline with/without cache,
  stale, fresh), relative vs absolute staleness style, 12h vs 24h time format,
  no cache metadata, state priority (loading overrides stale, offline overrides
  stale). clippy clean (only pre-existing dead-code warnings).

---

## Break 2: Reflection

Use plan mode or any brainstorming and code review skills to reflect on the design decisions and implementation for phases 3 and 4. It is important we do this to fix issues early and stop us fighting an uphill battle later when the logic becomes larger and more complex to change.

**Discuss:**
- Are there still any open questions or design gaps for the steps we have implemented so far? Think about the design logic - are there any decisions we've made that don't make sense (overcomplicated, or too many assumptions)?
- Do current tests adequately appraise the expected behaviour of the TUI, list view and api request handling? Can we trust that all tests passing means these functions of the app are working?
- Looking ahead to the next phases, can you forsee any issues with how the new logic we will implement will interface with the logic we have already completed?
- Anything else you need to be explained, or want to discuss?

**Status:** Complete. Full codebase review of Phase 3–4 conducted. Findings:

**Design decisions — all sound:**
- Rate limiter behind `Mutex` with `&self` trait methods; never held across await
  points.
- `send_with_retry()` layered on `send_and_check()` — clean separation of retry
  logic from core HTTP.
- 429 handling with best-effort `/api-throttle/` sync and local-state fallback.
- `ErrorState` uses `std::time::Instant` (monotonic) for transient auto-dismiss,
  not `Clock` trait — correct for UI timing.
- `TerminalGuard` RAII + panic hook — belt-and-suspenders terminal restoration.

**Issues identified (addressed in Break 2.2 and future steps):**
1. Terminal draw errors mapped to `AppError::CacheIo` — semantically wrong.
   → Fix in Break 2.2: add generic `Io` variant.
2. `api_client` moved into initial fetch future — can't be reused for detail
   fetches, refresh, or filter re-fetches. Blocks Step 5.2.
   → Fix in Break 2.2: wrap client in `Arc`.
3. `handle_fetch_result` updates `app.launches` in memory but never persists to
   cache. `CacheManager` not accessible from the event loop.
   → Address in Step 5.2: pass `CacheManager` into event loop or wrap in `Arc`.
4. `detail_scroll_offset` increments without upper bound — user can scroll past
   content. → Clamp in Step 5.1 when content height is known.
5. No `r` (refresh) key handler despite status bar showing "Press 'r' to refresh".
   → Wire in Step 5.2 alongside `Arc<Client>` + cache access.

**Test adequacy:**
- API client (Phase 3): Strong. wiremock integration tests cover all HTTP status
  codes, retry behaviour, 429 flow, API key injection, request counts.
- TUI rendering (Phase 4): Pure function tests (scroll offset, truncation, time
  formatting, status bar states) are thorough. No `TestBackend` rendering tests
  yet — defer to Step 8.2 per design doc §Testing Strategy.
- Event loop: No tests (async + terminal is hard to unit test), but
  `handle_fetch_result` is a pure state transition that could be tested. Consider
  in Step 8.2.

**Cleanup applied:**
- Fixed clippy: removed `.into()` on already-correct `std::io::Error` type in
  `event.rs`, then simplified to `AppError::CacheIo` constructor reference.

---

### Break 2.2 — Pre-Phase 5 refactoring

Address structural issues identified in Break 2 that would block or complicate
Phase 5 implementation.

**Produce:**
- Add generic `AppError::Io(std::io::Error)` variant to `src/error.rs` for
  non-cache I/O errors (terminal rendering, future general I/O). Update
  `event.rs` render error mapping to use the new variant. Existing `CacheIo`
  variant remains for cache-specific errors with its `#[from]` derive — the new
  `Io` variant is constructed explicitly (no `#[from]` to avoid conflict).
- Wrap `Ll2Client` in `Arc` so it can be shared between the startup fetch future
  and the event loop. Update `main.rs` to use `Arc<Ll2Client<C>>` and clone into
  the fetch closure. Update `fetch_launch_list()` to accept `Arc<Ll2Client<C>>`.
- Pass `CacheManager` (or a reference/`Arc`) into `run_event_loop()` so that
  fetch results can be persisted to disk. Update the event loop signature and
  `handle_fetch_result` to accept and use it.
- Update `FetchResult::LaunchList` to include cache metadata (`fetched_at`,
  `expires_at`) so the event loop can update `app.cache_fetched_at` and
  `app.cache_expires_at` after a successful fetch.

**Acceptance criteria:** `cargo test` passes. `cargo clippy` clean (only
pre-existing dead-code warnings). The API client is accessible from the event
loop after the initial fetch. Fetch results are persisted to cache. Terminal
render errors use `AppError::Io`, not `AppError::CacheIo`.

**Status:** Complete. 10 new tests (262 total). Changes:

- `src/error.rs` — Added `AppError::Io(std::io::Error)` variant (no `#[from]`,
  constructed explicitly to avoid conflict with `CacheIo`). 4 new tests: not
  retryable, not offline signal, display prefix is "I/O error" (distinct from
  "Cache I/O error"), distinct from `CacheIo`.
- `src/tui/event.rs` — Terminal render errors now use `AppError::Io` instead of
  `AppError::CacheIo`. `run_event_loop()` signature expanded: accepts
  `Arc<Ll2Client<C>>`, `&CacheManager<C>`, and `&CacheConfig`. Generic over
  `C: Clock + Send + Sync + 'static`. `handle_fetch_result()` now takes client,
  cache manager, and cache config; on successful list fetch: persists to disk via
  `cache_manager.save_launch_list()`, updates `app.cache_fetched_at` /
  `cache_expires_at`, updates `app.rate_limit_remaining` / `rate_limit_total`
  from the client's rate limiter. 6 new tests for `handle_fetch_result`:
  state update, cache persistence, selection reset, transient error, rate limited
  error, offline cleared on success.
- `src/main.rs` — `Ll2Client` wrapped in `Arc`. `fetch_launch_list()` accepts
  `Arc<Ll2Client<C>>`. Event loop receives `Arc::clone(&api_client)`,
  `&cache_manager`, `&config.cache`. On exit, rate limit state persisted from
  `api_client.rate_limiter().to_state()` into `app_state` (no longer lost on
  exit). Removed stale comment about Step 8.1 fixing client access.
- clippy clean (only pre-existing dead-code warnings).

---

## Phase 5: TUI — Detail View & Data Flow

### Step 5.1 — Detail view rendering

Render the full launch detail screen.

**Produce:**
- `src/tui/views/detail.rs`: detail view renderer (design doc §Detail View
  layout)
- Hero header: countdown with status-coloured `██` badges, NET time, launch
  window, probability, weather (design doc §Detail View mockup)
- Two-column grid: vehicle/provider + location (design doc §Detail View)
- Responsive fallback to single column below 100 columns
- Provider record bar: `█`/`░` success/failure visualization (design doc
  §Provider record bar)
- Mission section: type, orbit, payload, description
- Links section with cyan labels (design doc §Link Styling)
- Section separators in dark gray dim (design doc §Section separators)
- Vertical scrolling with scroll indicator
- Clamp `detail_scroll_offset` to content height so the user cannot scroll
  past the end of the detail content (Break 2 finding #4)

**Acceptance criteria:** Selecting a launch and pressing Enter shows the
detail view. Content matches the design doc mockup. Scrolling works and
is clamped to content bounds. Esc returns to list. Responsive layout
switches at 100 columns.

---

### Step 5.2 — Data fetching integration

Wire up API calls to user actions with loading/error states. Relies on the
`Arc<Client>` and `CacheManager` access established in Break 2.2.

**Produce:**
- Startup fetch: load list from cache or API (design doc §Startup Sequence
  step 4)
- Enter on launch: fetch detail if not cached/stale (design doc §Viewing
  Launch Details). Cache the `LaunchDetail` in `app.detail_cache` and persist
  via `CacheManager` (Break 2 finding #3).
- Refresh (`r` key): wire the handler in `handle_list_key` — only triggers
  when cache is stale and no fetch is in-flight (design doc §Manual Refresh
  Logic). Spawns a new list fetch using the `Arc<Client>` (Break 2 finding #5).
- Loading state: "Fetching launches..." / "Refreshing..." / "Retrying..."
  indicators
- Error display driven by `ErrorState` variants (design doc §Application
  State Machine — ErrorState):
  - `Transient`: inline error message, auto-dismissed after 10 seconds
  - `RateLimited`: disables refresh key, shows countdown to reset time
  - `Offline`: `[OFFLINE]` indicator in status bar, cleared on success
- First-run experience (design doc §First-Run Experience)
- Update `app.rate_limit_remaining` / `rate_limit_total` after each fetch
  completes (currently only set once at startup)

**Acceptance criteria:** App fetches on startup when cache is missing.
Detail fetch triggers on Enter. Refresh only works when stale.
`ErrorState::Transient` displays and auto-dismisses. `ErrorState::RateLimited`
disables refresh and shows reset time. `ErrorState::Offline` shows indicator.
Fetched data is persisted to cache via `CacheManager`.

---

## Phase 6: Filtering & Help

### Step 6.1 — Filter panel

Implement the inline filter UI and server-side filtering.

**Produce:**
- `src/tui/views/filter.rs` or within list view: filter panel renderer
  (design doc §Filter Panel)
- Four filter categories: status, region, crewed, date range
- `f` to focus, `Tab` to cycle, left/right to change values
- `Enter` to apply (triggers API request with filter params), `Esc` to cancel
- Filter state tracking (`FilterState` struct — design doc §Application
  State Machine)
- Region-to-`pad__location` mapping from vendor config
- Filters not persisted between sessions

**Acceptance criteria:** Filter panel appears on `f`. Tab cycles categories.
Applying a filter triggers a new API request with correct query parameters.
Esc reverts. Results update in the list view.

---

### Step 6.2 — Help overlay

Implement the context-aware help popup.

**Produce:**
- Centered popup rendering (design doc §Help Overlay)
- List view help content and detail view help content
- `?` toggles help, `Esc` closes it
- `AppScreen::Help(Box<AppScreen>)` wrapping (design doc §Application State
  Machine)

**Acceptance criteria:** `?` shows context-appropriate help. Pressing `?`
or `Esc` dismisses it. Underlying view is not interactive while help is
shown.

---

## Phase 7: Visual Polish & Colour

### Step 7.1 — Colour scheme and visual hierarchy

Apply the full styling system across all views.

**Produce:**
- 5-colour semantic palette applied consistently (design doc §Color and
  Styling)
- Three-tier text hierarchy: primary (bold white), secondary (normal),
  labels/chrome (dim gray) — (design doc §Visual Hierarchy)
- Launch status colours and styles from vendor status map (design doc
  §Launch Status Colors and Styles)
- Probability colouring (design doc §Probability Coloring)
- Countdown colour tiers (design doc §Countdown Styling)
- Unknown status fallback to gray (design doc §Launch Status Colors)

**Acceptance criteria:** Visual output matches the design doc styling
tables. All status IDs render with correct colour/style. Unknown status
IDs don't crash.

---

## Phase 8: Shutdown, Edge Cases & Testing

### Step 8.1 — Graceful shutdown and state persistence

Ensure clean exit and state saving.

**Produce:**
- Save `app_state.json` on quit (design doc §Graceful Shutdown)
- Flush pending cache writes
- Terminal restoration via `TerminalGuard` RAII (primary) and panic hook
  (secondary) — design doc §Graceful Shutdown
- Handle Ctrl+C / terminal signals

**Acceptance criteria:** Quitting with `q` saves state and restores terminal.
Killing with Ctrl+C restores terminal via `TerminalGuard::drop`. Panic
restores terminal. Rate limit data persists across sessions.

---

### Step 8.2 — Testing and hardening

Comprehensive test pass across all modules. Reference design doc §Testing
Strategy for full specification of test infrastructure, test categories, and
edge cases.

**Produce:**
- Unit tests for: cache TTL edge cases (all five tiers + boundary values),
  cache version mismatch handling, rate limiter rolling window (using
  `FakeClock`), time display (all precision levels, timezone edge cases),
  vendor mappings (all 8 statuses + unknown fallback), config parsing,
  `AppError::is_retryable()` classification
- Integration tests for: API client with `wiremock` mock server (success,
  retry on 5xx, no retry on 4xx, 429 throttle sync, timeout), vendor trait
  contract verification (response parsing, pagination params, filter query
  strings)
- UI rendering tests using `ratatui::backend::TestBackend`: verify status
  badge text, staleness indicator appearance, empty list state rendering
  (design doc §Testing Strategy — UI rendering)
- Edge case tests: empty API responses, clock skew (backward jump via
  `FakeClock`), large content (10KB+ description), unicode display width,
  cache expiry boundaries (design doc §Testing Strategy — Edge Case Tests)
- Manual testing checklist: Linux/macOS/Windows, with/without API key,
  offline behaviour, terminal resize (including responsive layout threshold
  at 100 columns), first run with no config/cache directories
  (design doc §Testing Strategy — Manual Testing)

**Acceptance criteria:** `cargo test` passes. All scenarios from the design
doc §Testing Strategy have been exercised. No `thread::sleep` or wall-clock
dependencies in unit tests.

---

## Open Questions

Items identified during design review. Resolved items marked with ✅.

1. **Pagination UX** (affects Step 4.2, 5.2)
   Keybindings include no page navigation (Page Up/Down, "load more").
   `launches_per_page` config exists and the API supports `limit`/`offset`,
   but the interaction model for moving between pages is unspecified.
   Decide: infinite scroll, explicit page buttons, or fixed single page?

2. ✅ **Detail view data model** — resolved in Break 1. A typed
   `LaunchDetail` struct will be added in Step 3.1 alongside the detail
   endpoint implementation, replacing raw `serde_json::Value` in the cache.

3. ✅ **crossterm `event-stream` feature** — already present in Cargo.toml
   since Step 1.1.

4. ✅ **Config default discrepancy** — code defaults (180/30) match the
   design doc's cache expiry section. The config.toml example in the design
   doc is the outlier and should be updated to match (doc-only fix).

5. **`unicode-width` dependency** (affects Step 4.2)
   Mentioned in edge case tests for CJK display width but not in the
   dependency list. Add if CJK launch site names are expected.

6. **Region map location IDs** (affects Step 6.1)
   `region_map.rs` has placeholder IDs. These must be manually verified against
   the live LL2 API before Step 6.1 — query `/location/` endpoint and confirm
   that each region's location IDs match what the API returns. This is a manual
   prerequisite; do not start Step 6.1 until this is done.

---

## Notes for AI Agent Sessions

**General guidelines:**
- Each step is scoped to be completable in a single agent session
- Always run `cargo check` (or `cargo test` where tests exist) before
  finishing a step
- Reference the design doc section cited in each step — don't re-derive
  architectural decisions
- Steps within a phase are sequential; phases are mostly sequential but
  Phase 7 (colour) can be partially done alongside Phase 5/6

**Dependency chain:**
```
1.1 → 1.2 → 1.3 → 1.4
                ↓
        2.1 → 2.2 → 2.3
                ↓
              2.4 → 3.1 → 3.2
                            ↓
                    4.1 → 4.2 → 4.3 → 4.4
                                        ↓
                              Break 2.2 → 5.1 → 5.2 → 6.1 → 6.2
                                                            ↓
                                                    7.1 → 8.1 → 8.2
```
