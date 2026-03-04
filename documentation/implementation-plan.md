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

## Break 1: Reflection

Use plan mode or any brainstorming and code review skills to reflect on the design decisions and implementation so far. It is important we do this to fix issues early and stop us fighting an uphill battle later when the logic becomes larger and more complex to change.

**Discuss:**
- Are there still any open questions or design gaps for the steps we have implemented so far? Think about the design logic - are there any decisions we've made that don't make sense (overcomplicated, or too many assumptions)?
- Do current tests adequately appraise the expected behaviour of configuration, logging, and caching? Can we trust that all tests passing means these functions of the app are working?
- Looking ahead to the next phases, can you forsee any issues with how the new logic we will implement will interface with the logic we have already completed?
- Anything else you need to be explained, or want to discuss?

## Phase 3: API Client

### Step 3.1 — API client trait and LL2 implementation

Build the HTTP client with the vendor-agnostic trait and LL2-specific
implementation.

**Produce:**
- `src/api/client.rs`: trait with methods for fetching launch list, launch
  detail, and throttle status (design doc §Source Code Organisation —
  `api::client` defines traits)
- `src/vendor/launch_library_2/endpoints.rs`: base URL constants, endpoint
  paths, query parameter building (design doc §API Integration)
- LL2 implementation of the API client trait using `reqwest`
- Rate limiter integration: check before each request, record after
- API key header injection when configured
- Dev vs production base URL from config

**Acceptance criteria:** `cargo check` passes. Integration test with a mock
HTTP server (e.g., `wiremock` or `mockito`) verifies: successful list fetch
parses correctly, successful detail fetch parses correctly, rate limiter
blocks requests when exhausted.

---

### Step 3.2 — Error handling and retry logic

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

---

## Phase 4: TUI — Core Loop & List View

### Step 4.1 — Application state and event loop

Set up the ratatui terminal, app state machine, and async event loop.

**Produce:**
- `src/tui/app.rs`: `App` struct (with `error_state: Option<ErrorState>`),
  `AppScreen` enum, `ErrorState` enum (design doc §Application State Machine)
- `src/tui/event.rs`: async event loop with `tokio::select!` (design doc
  §Async Event Loop Architecture)
- `TerminalGuard` RAII struct for terminal restoration on normal exit and
  panic (design doc §Graceful Shutdown). Panic hook registered as secondary
  safety net.
- `main.rs`: startup sequence wiring — config → logging → cache → rate
  limiter → API sync → TUI (design doc §Startup Sequence)
- Minimum terminal size check with warning render (design doc §Minimum
  Terminal Size)
- Resize event handling

**Acceptance criteria:** `cargo run` enters alternate screen, shows a
placeholder UI, responds to `q` to quit, and restores terminal on exit
(including on panic — verify `TerminalGuard::drop` runs). Resize below
80x24 shows warning.

---

### Step 4.2 — List view rendering

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

---

### Step 4.3 — Time display and net precision

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

---

### Step 4.4 — Status bar

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

**Acceptance criteria:** Selecting a launch and pressing Enter shows the
detail view. Content matches the design doc mockup. Scrolling works.
Esc returns to list. Responsive layout switches at 100 columns.

---

### Step 5.2 — Data fetching integration

Wire up API calls to user actions with loading/error states.

**Produce:**
- Startup fetch: load list from cache or API (design doc §Startup Sequence
  step 4)
- Enter on launch: fetch detail if not cached/stale (design doc §Viewing
  Launch Details)
- Refresh (`r` key): only when stale (design doc §Manual Refresh Logic)
- Loading state: "Fetching launches..." / "Refreshing..." / "Retrying..."
  indicators
- Error display driven by `ErrorState` variants (design doc §Application
  State Machine — ErrorState):
  - `Transient`: inline error message, auto-dismissed after 10 seconds
  - `RateLimited`: disables refresh key, shows countdown to reset time
  - `Offline`: `[OFFLINE]` indicator in status bar, cleared on success
- First-run experience (design doc §First-Run Experience)

**Acceptance criteria:** App fetches on startup when cache is missing.
Detail fetch triggers on Enter. Refresh only works when stale.
`ErrorState::Transient` displays and auto-dismisses. `ErrorState::RateLimited`
disables refresh and shows reset time. `ErrorState::Offline` shows indicator.

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

Items identified during design review. Resolve as they come up during
implementation — none are blockers for starting.

1. **Pagination UX** (affects Step 4.2, 5.2)
   Keybindings include no page navigation (Page Up/Down, "load more").
   `launches_per_page` config exists and the API supports `limit`/`offset`,
   but the interaction model for moving between pages is unspecified.
   Decide: infinite scroll, explicit page buttons, or fixed single page?

2. **Detail view data model** (affects Step 1.2, 5.1)
   `LaunchDetailCache.data` is `serde_json::Value`. This works but means
   manual JSON field access in the detail renderer. Consider adding a typed
   `LaunchDetail` struct — can start with `Value` and type it later.

3. **crossterm `event-stream` feature** (affects Step 1.1)
   The async event loop uses `EventStream` which requires the
   `event-stream` feature on the `crossterm` crate. Add it to `Cargo.toml`.

4. **Config default discrepancy** (affects Step 1.3)
   The cache expiry section says `ttl_near_future` = 180 min and
   `ttl_imminent` = 30 min. The `config.toml` example says 60 and 5
   respectively. Pick one set of values and update the other.

5. **`unicode-width` dependency** (affects Step 1.1, 4.2)
   Mentioned in edge case tests for CJK display width but not in the
   dependency list. Add if CJK launch site names are expected.

6. **Region map location IDs** (affects Step 1.2)
   The design references a region-to-`pad__location` mapping but does not
   list the actual LL2 location IDs. These need to be sourced from the API
   (or its docs) when building `region_map.rs`.

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
                                5.1 → 5.2 → 6.1 → 6.2
                                                    ↓
                                            7.1 → 8.1 → 8.2
```
