# Launch Client TUI - Design Document

## Project Overview

A Rust-based terminal UI application for browsing upcoming and ongoing space missions using the Launch Library 2 API. The application emphasizes efficient API usage through intelligent caching and provides users with real-time mission information in an easy-to-use terminal interface.

---

## Technology Stack

- **Language**: Rust
- **TUI Framework**: `ratatui` (formerly tui-rs)
- **Terminal Backend**: `crossterm`
- **Async Runtime**: `tokio`
- **HTTP Client**: `reqwest`
- **Serialization**: `serde` and `serde_json`
- **Logging**: `tracing` and `tracing-subscriber`
- **Error Handling**: `thiserror`
- **Additional Crates**:
  - `directories` - Cross-platform config paths
  - `chrono` and `chrono-tz` - Date/time handling with IANA timezone support
  - `toml` - Configuration file parsing
  - Rate limiting crate (e.g., `governor` or custom implementation)
- **Dev Dependencies**:
  - `tempfile` - Temporary directories for cache and file I/O tests
  - `wiremock` - Mock HTTP server for API client integration tests
  - `tokio` (with `test-util` feature) - Time control in async tests

---

## Source Code Organisation

### Vendor Configuration

API-provider-specific data (status ID mappings, geographical region lookups, endpoint paths) is isolated into a vendor configuration directory within the source code. This keeps third-party API coupling contained and makes it straightforward to swap providers in the future.

```
src/
├── vendor/
│   └── launch_library_2/
│       ├── mod.rs              # Re-exports
│       ├── endpoints.rs        # Base URL, endpoint paths, query building
│       ├── status_map.rs       # Status ID → name/color mappings
│       ├── region_map.rs       # pad__location ID → geographical region mappings
│       └── response_models.rs  # Serde models matching LL2 API response shapes
├── api/
│   ├── mod.rs
│   └── client.rs              # Generic API client traits; vendor-agnostic interface
├── cache/
├── tui/
├── config/
└── main.rs
```

The `api::client` module defines traits for fetching launch lists and details. The `vendor::launch_library_2` module provides the concrete implementation. Application logic (caching, TUI, filtering) depends only on the traits, not on LL2-specific types.

---

## API Integration

### API Provider
**Launch Library 2** (TheSpaceDevs)

- **Development/testing**: `https://lldev.thespacedevs.com/2.3.0/` (less restrictive rate limits, potentially stale data)
- **Production**: `https://ll.thespacedevs.com/2.3.0/`

The base URL should be configurable (see User Configuration section) to allow switching between dev and production servers.

### Primary Endpoints

1. **`/launches/upcoming/`**
   - Gets upcoming and in-progress launches with filtering and pagination
   - Returns launches sorted by launch date
   - Equivalent to `/launches/` but pre-filtered to current/future launches only
   - Request using `?mode=normal` for list view data (includes LSP, rocket config, mission, pad)
   - Supports filtering: `?status__ids=`, `?is_crewed=`, `?pad__location=`, `?net__gt=`, `?net__lt=`, `?search=`
   - Supports pagination: `?limit=` and `?offset=`
   - Supports ordering: `?ordering=net` (default for upcoming)
   - Maximum `limit` value: 100 (API-enforced)

2. **`/launch/{id}/`**
   - Gets detailed info for a specific launch (returns `detailed` response mode)
   - Launch IDs are UUIDs (e.g., `e3df2ecd-c239-472f-95e4-2b89b4f75800`)
   - Contains full mission data, webcast links, mission description, payload info, updates
   - Returns 404 if launch ID not found
   - Used for detail view

3. **`/api-throttle/`**
   - Returns current rate limit status
   - **Does not count against API rate limit**
   - Used for syncing local rate limit tracking with server

### Future Endpoints (Not in MVP)
- `/launches/previous/` - Past launches (same filters/response as `/launches/upcoming/`)
- `/event/upcoming/` - Space events (EVAs, dockings, etc.)

---

## API Rate Limiting

### Rate Limit Constraints
- **Unauthenticated**: 15 requests per hour per IP
- **Authenticated** (with free API key): 30 requests per hour per key
- **Global limit**: Applies across all endpoints (not per-endpoint)

### Rate Limiting Strategy

#### Clock Abstraction for Testability

Time-dependent components (rate limiter, cache TTL) accept an injected clock rather than calling `Utc::now()` or `Instant::now()` directly. This enables deterministic unit tests without wall-clock delays or flaky timing assertions.

```rust
trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> { Utc::now() }
}

#[cfg(test)]
struct FakeClock {
    now: std::sync::Mutex<DateTime<Utc>>,
}

#[cfg(test)]
impl FakeClock {
    fn advance(&self, duration: chrono::Duration) {
        let mut now = self.now.lock().unwrap();
        *now = *now + duration;
    }
}
```

Components that need the current time (e.g., `RateLimiter`, `CacheManager`) take a `&dyn Clock` or generic `C: Clock` parameter. In production, `SystemClock` is used. In tests, `FakeClock` allows advancing time explicitly to test expiry, rolling windows, and edge cases without `sleep`.

#### Client-Side Tracking
```rust
struct RateLimiter<C: Clock = SystemClock> {
    clock: C,
    limit: usize,              // 15 or 30 based on authentication
    requests: VecDeque<DateTime<Utc>>, // Timestamps of requests in last hour
    last_sync: DateTime<Utc>,  // Last time we synced with server
}
```

- Track all API requests made in the last rolling hour
- Remove requests older than 1 hour from tracking
- Block requests that would exceed the limit
- Persist request history to disk (in `app_state.json`)

#### Rate Limit Calibration Strategy

Sync with server via `/api-throttle/` endpoint:

1. **Always on startup** - Get accurate initial state
2. **When close to limit** - Sync if remaining requests <= 2
3. **After 429 errors** - Sync if we receive a rate limit error from server
4. **Long-running sessions** - Sync if app has been running >1 hour without sync
5. **Graceful degradation** - If `/api-throttle/` fails, perform one retry (same retry logic as other API calls), then continue with local tracking

```rust
async fn should_sync(&self) -> bool {
    self.last_sync.elapsed() > Duration::MAX ||        // Never synced
    self.remaining() <= 2 ||                           // Close to limit
    self.last_sync.elapsed() > Duration::from_secs(3600) // >1 hour
}
```

#### Error Handling
- Display clear error message when rate limit is reached
- Show time until requests are available again
- Never make requests that would violate rate limits

---

## Caching Strategy

### File Structure

The application follows XDG Base Directory conventions, separating user
configuration from cache data. The `directories` crate's `ProjectDirs`
provides platform-appropriate paths:

| Platform | Config (`config_dir`) | Cache (`cache_dir`) |
|----------|----------------------|---------------------|
| Linux | `~/.config/launch-client/` | `~/.cache/launch-client/` |
| macOS | `~/Library/Application Support/launch-client/` | `~/Library/Caches/launch-client/` |
| Windows | `{FOLDERID_RoamingAppData}\launch-client\` | `{FOLDERID_LocalAppData}\launch-client\cache\` |

```
~/.config/launch-client/       (config_dir)
├── config.toml                # User configuration
└── app.log                    # Application log file

~/.cache/launch-client/        (cache_dir)
├── app_state.json             # Rate limits, app metadata, startup count
├── cache.json                 # Cached list of launches
└── details/                   # Individual launch detail caches
    ├── e3df2ecd-c239-472f-95e4-2b89b4f75800.json
    ├── a1b2c3d4-e5f6-7890-abcd-ef1234567890.json
    └── ...
```

### Cache Files

#### 1. `app_state.json` - Application State
```json
{
  "version": 1,
  "rate_limit": {
    "limit": 15,
    "requests": [
      "2026-02-22T10:15:00Z",
      "2026-02-22T10:20:00Z"
    ],
    "last_sync": "2026-02-22T10:25:00Z",
    "authenticated": false
  },
  "last_startup": "2026-02-22T10:00:00Z",
  "startup_count": 42,
  "last_prune": "2026-02-20T08:00:00Z"
}
```

**Purpose**: Persists rate limit tracking and app metadata across sessions

#### 2. `cache.json` - Launch List Cache
```json
{
  "version": 1,
  "fetched_at": "2026-02-22T10:15:00Z",
  "expires_at": "2026-02-22T10:45:00Z",
  "total_count": 147,
  "launches": [
    {
      "id": "e3df2ecd-c239-472f-95e4-2b89b4f75800",
      "name": "Starship IFT-7",
      "net": "2026-02-28T09:00:00Z",
      "net_precision": {
        "id": 1,
        "name": "Day",
        "abbrev": "Day"
      },
      "window_start": "2026-02-28T09:00:00Z",
      "window_end": "2026-02-28T12:00:00Z",
      "status": {
        "id": 1,
        "name": "Go for Launch",
        "abbrev": "Go"
      },
      "launch_service_provider": {
        "name": "SpaceX",
        "type": { "name": "Commercial" }
      },
      "pad": {
        "location": {
          "name": "Starbase, Texas",
          "timezone_name": "America/Chicago"
        }
      },
      "mission": {
        "name": "Starship IFT-7",
        "type": "Test Flight"
      }
    }
  ]
}
```

**Purpose**: Data for list view, loaded on app startup. Includes `net_precision` and `window_start`/`window_end` for correct time display, `timezone_name` for location-aware time rendering, and `total_count` from the API response to show "Showing N of M".

#### 3. `details/{uuid}.json` - Individual Launch Details
```json
{
  "version": 1,
  "launch_id": "e3df2ecd-c239-472f-95e4-2b89b4f75800",
  "fetched_at": "2026-02-22T10:20:00Z",
  "expires_at": "2026-02-22T10:25:00Z",
  "ttl_strategy": "imminent",
  "data": {
    // Full API response for this launch (detailed mode)
  }
}
```

**Purpose**: Full launch details, lazy-loaded only when user views a specific launch

### Request Strategy: Lazy Loading + Smart Expiry

**Core Principle**: Only make API requests when explicitly needed by user action and when cached data is stale.

#### When API Requests Are Made

1. **App startup**: Fetch `/launches/upcoming/` if no cache exists or cache is expired
2. **User selects launch**: Fetch `/launch/{id}/` if no detail cache exists or cache is expired
3. **User manually refreshes**: Only allowed when cache is stale (past expiry time)
4. **User applies filters**: Fetch `/launches/upcoming/` with filter parameters after user confirms filter selection

#### Never Auto-Fetch
- No background polling
- No auto-refresh while app is open
- User controls all non-startup API requests

### Cache Expiry (Time-To-Live)

Cache expiry based on launch proximity:

```rust
enum CacheStrategy {
    Permanent,           // Past launches - never expires
    LongTerm,           // Launches >7 days away - 24 hour TTL
    MediumTerm,         // Launches 1-7 days away - 3 hour TTL
    ShortTerm,          // Launches <24 hours away - 30 minute TTL
    RealTime,           // Active launches (in-progress) - 1 minute TTL
}
```

**Default TTL Values** (minutes, configurable range: 1–43 200):
- `ttl_far_future`: 1440 (24 hours)
- `ttl_near_future`: 180 (3 hours)
- `ttl_imminent`: 30 (30 minutes)
- `ttl_active`: 1 (1 minute)
- `ttl_launch_list`: 30 (30 minutes)

> **Note:** Past launches use the `Permanent` strategy internally — their data is
> final and never re-fetched. No TTL configuration is exposed for this tier.

### Cache Pruning

**Purpose**: Prevent unbounded disk usage from accumulated detail cache files

**Pruning Rules**:
1. **Age-based**: Delete detail files older than N days (default: 30)
2. **Count-based**: Keep at most N detail files (default: 100)
3. **Schedule**: Run pruning every N app startups (default: 5)

**Default Configuration**:
- `max_detail_age_days`: 30
- `max_detail_files`: 100
- `prune_every_n_startups`: 5

**Pruning Algorithm**:
1. Delete all files older than `max_detail_age_days`
2. If remaining files > `max_detail_files`, delete oldest files until count is within limit
3. Log number of files deleted and space freed

---

## User Interface Design

### Minimum Terminal Size

The application requires a minimum terminal size of **80 columns x 24 rows**. If the terminal is smaller than this, a centered warning message is rendered over the content area:

```
╭────────────────────────────────────────╮
│                                        │
│   Terminal window too small.           │
│                                        │
│   Please resize to at least 80x24      │
│   to start seeing rockets!             │
│                                        │
╰────────────────────────────────────────╯
```

The application listens for terminal resize events and re-renders immediately when the terminal is resized. If the terminal grows back above the minimum, normal rendering resumes automatically.

### Border Style

All bordered UI components use **rounded corners** via ratatui's `BorderType::Rounded`, which renders Unicode rounded box-drawing characters (`╭ ╮ ╰ ╯`). These are well-supported across modern terminal emulators (Windows Terminal, iTerm2, kitty, Alacritty, foot, GNOME Terminal).

This applies globally to:
- The outer application frame
- Inner panels in the detail view
- The help overlay popup
- The terminal-too-small warning box

A shared helper ensures consistency:

```rust
fn styled_block(title: &str) -> Block {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(title)
}
```

### Layout

#### List View (Main Screen)
```
╭─ Upcoming Launches ─── Showing 25 of 147 ───────────────── 12/15 ─╮
│ [Status: All ▾] [Region: All ▾] [Crewed: All ▾] [Date: All ▾]    │
│                                                                    │
│  ▸ Starship IFT-7              Feb 28 03:00 CST / 09:00 UTC [Go]   │
│    SpaceX | Starbase, TX                                           │
│                                                                    │
│    Crew-10                     Mar 2 08:30 CST / 14:30 UTC  [Go]   │
│    SpaceX | Kennedy Space Center                                   │
│                                                                    │
│    Artemis III                 Sep 2026                  [Planned] │
│    NASA | Kennedy Space Center                                     │
│                                                                    │
├────────────────────────────────────────────────────────────────────┤
│ Updated 5m ago • Next refresh after 10:45 AM                       │
│ ↑/↓: Navigate · Enter: Details · f: Filters · r: Refresh · q: Quit │
╰────────────────────────────────────────────────────────────────────╯
```

Note: Launches with only month or year precision (from `net_precision`) display accordingly - "Sep 2026" instead of a full datetime.

#### Detail View

The detail view uses a **hero header + two-column grid** layout. The hero header prominently displays the countdown, status, and timing — the most time-sensitive information. Below the header, a two-column grid presents vehicle/provider alongside location details, followed by full-width sections for the mission description and links.

On terminals narrower than 100 columns, the two-column grid falls back to a single-column stacked layout.

```
╭─ Starship IFT-7 ─────────────────────────────────────── 11/15 reqs ─╮
│                                                                       │
│   ██  T-01:14:23:45  ██     Go for Launch                            │
│   Feb 28, 2026 03:00 AM CST / 09:00 AM UTC                          │
│   Window: 09:00 - 12:00 UTC  ·  Probability: 90%                    │
│   Weather: No concerns                                                │
│                                                                       │
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │
│                                                                       │
│   VEHICLE                      │  LOCATION                           │
│   Starship (Super Heavy +      │  Orbital Launch Mount A             │
│   Starship)                    │  Starbase, Texas, United States     │
│                                │                                     │
│   PROVIDER                     │                                     │
│   SpaceX (Commercial)          │                                     │
│   ████████████████████░ 98%    │                                     │
│   301 launches (295 ok, 6 fail)│                                     │
│                                                                       │
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │
│                                                                       │
│   MISSION                                                             │
│   Starship IFT-7 (Test Flight)                                       │
│   Orbit: Low Earth Orbit (LEO)  ·  Payload: 0 kg                    │
│                                                                       │
│   Orbital flight test demonstrating booster catch and ship deorbit   │
│   capabilities. Seventh integrated flight test of the Starship       │
│   launch system.                                                      │
│                                                                       │
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │
│                                                                       │
│   LINKS                                                               │
│   Webcast   https://www.youtube.com/watch?v=...                      │
│   Info      https://www.spacex.com/launches/mission-7/               │
│   Program   Starship Development                                     │
│   Image     https://thespacedevs-prod.nyc3.digitalocean...          │
│                                                                       │
├───────────────────────────────────────────────────────────────────────┤
│ Updated 2m ago • Next refresh after 10:07 AM                          │
│ Esc: Back · ↑/↓: Scroll · r: Refresh (when stale) · ?: Help         │
╰───────────────────────────────────────────────────────────────────────╯
```

**Hero header**: The `██` block characters flanking the countdown are colored to match the launch status, acting as a colored badge. The countdown and status text are bold and prominently styled (see Visual Hierarchy section below).

**Provider record bar**: The provider's launch success rate is visualized as a compact inline bar using `█` (green, successes) and `░` (red/dim, failures) characters, scaled to a fixed width of 20 characters. This gives an instant visual read on reliability.

**Section separators**: The `━━━━` horizontal rules are rendered in dark gray dim — visible enough to create structure without competing with content.

The detail view supports vertical scrolling with `up`/`down` arrow keys (or `j`/`k`) when content exceeds the visible area. A scroll indicator is shown on the right edge when content overflows.

#### Help Overlay

The help overlay is context-aware, showing different content depending on the current view. Rendered as a centered popup box, dismissible with `?` or `Esc`.

**List View Help:**
```
╭─ Help ─────────────────────────────╮
│                                     │
│  Navigation                        │
│  ↑/↓       Navigate launch list   │
│  Enter     View launch details     │
│  Home/End  Jump to first/last      │
│                                     │
│  Actions                           │
│  r         Refresh (when stale)    │
│  f         Open filter panel       │
│                                     │
│  General                           │
│  ?         Toggle this help        │
│  q         Quit application        │
│                                     │
│  Tip: Create config.toml in your   │
│  config directory to customize.    │
│                                     │
│  Press ? or Esc to close           │
╰─────────────────────────────────────╯
```

**Detail View Help:**
```
╭─ Help ─────────────────────────────╮
│                                     │
│  Navigation                        │
│  ↑/↓/j/k   Scroll content         │
│  Esc        Back to list           │
│                                     │
│  Actions                           │
│  r          Refresh (when stale)   │
│                                     │
│  General                           │
│  ?          Toggle this help       │
│  q          Quit application       │
│                                     │
│  Press ? or Esc to close           │
╰─────────────────────────────────────╯
```

### UI Components

#### Status Bar - Bottom of Screen

**Format**: Two-part status bar

**Left side** - Cache staleness info:
- When fresh: `Updated 5m ago • Next refresh after 10:45 AM`
- When stale: `Updated 2h ago (stale) • Press 'r' to refresh`
- When offline with cache: `Updated 2h ago (stale) • [OFFLINE]`
- When refreshing: `Refreshing...`

**Right side** - Rate limit info:
- Format: `12/15 reqs` (requests remaining / total limit)
- Shown in top-right of title bar

#### Staleness Indicators

**Fresh Cache** (within TTL):
- Show relative time: "Updated 5m ago"
- Show next refresh time: "Next refresh after 10:45 AM"
- Refresh key (`r`) is disabled

**Stale Cache** (past TTL):
- Show relative time: "Updated 2h ago"
- Show "(stale)" indicator
- Show action: "Press 'r' to refresh"
- Refresh key (`r`) is enabled

**During Refresh**:
- Show loading indicator: "Refreshing..."
- Disable navigation during fetch

#### Filter Panel

An inline filter bar displayed below the title bar. Users press `f` to focus the filter panel, then use `Tab` to cycle between filter categories and `left`/`right` to change values within a category. Pressing `Enter` applies the selected filters and triggers an API request. Pressing `Esc` cancels filter changes.

**Filter categories**:
- **Status**: All, Go for Launch, TBD, TBC, On Hold, In Flight (maps to `status__ids` API parameter)
- **Region**: All, US, Europe, Russia/Kazakhstan, China, India, Japan, Other (maps to `pad__location` API parameter via vendor region lookup)
- **Crewed**: All, Crewed Only, Uncrewed Only (maps to `is_crewed` API parameter)
- **Date Range**: All, Next 7 days, Next 30 days, Next 90 days (maps to `net__lt` API parameter)

Filters are **not** persisted between sessions. Each filter change triggers a server-side API request (when the user confirms with `Enter`), because client-side filtering on paginated data would produce incomplete results.

The region-to-`pad__location` ID mapping is maintained in the vendor configuration directory (`src/vendor/launch_library_2/region_map.rs`).

### Color and Styling

The UI uses a restrained palette of **5 semantic colors** beyond the default white and gray, ensuring readability on both dark and light terminal themes. All meaning conveyed by color is also conveyed by text, so the interface remains accessible without color support.

| Color | Purpose |
|---|---|
| **Green** | Positive status (Go, Success), high probability, success portion of provider record bar |
| **Yellow** | Uncertain status (TBD, TBC, Hold), medium probability, imminent countdown |
| **Red** | Negative status (Failure), low probability, very imminent countdown, failure portion of record bar |
| **Cyan** | In Flight status, link labels in detail view |
| **Dark Gray** | Labels, separators, structural chrome |

#### Launch Status Colors and Styles

Vendor-specific, defined in `src/vendor/launch_library_2/status_map.rs`:

| Status ID | Name             | Abbreviation | Color  | Style |
|-----------|------------------|-------------|--------|-------|
| 1         | Go for Launch    | Go          | Green  | Bold |
| 2         | To Be Determined | TBD         | Yellow | Normal |
| 3         | Launch Successful| Success     | Green  | Normal |
| 4         | Launch Failure   | Failure     | Red    | Bold |
| 5         | On Hold          | Hold        | Yellow | Bold |
| 6         | In Flight        | In Flight   | Cyan   | Bold |
| 7         | Partial Failure  | P. Failure  | Red    | Normal |
| 8         | To Be Confirmed  | TBC         | Yellow | Normal |

Any unknown status ID falls back to the default terminal color (Gray) with normal weight so the app does not break if new statuses are added to the API.

Note: ratatui's `Modifier::RapidBlink` could be applied to "In Flight" as a progressive enhancement, but terminal support varies widely. The cyan bold text is sufficient on its own.

#### Visual Hierarchy

Three tiers of text styling create depth and scannability:

**Tier 1 — Primary** (values the user is scanning for):
- Style: Bold, White (or terminal default bright)
- Used for: Countdown, launch name, status text, NET datetime

**Tier 2 — Secondary** (supporting context):
- Style: Normal weight, White/default
- Used for: Provider name, rocket name, mission description, pad name, location, link URLs

**Tier 3 — Labels & Chrome** (structural, not content):
- Style: Dim, Gray
- Used for: Section headings (`VEHICLE`, `LOCATION`, etc.), field labels (`Window:`, `Orbit:`), separator lines, `·` delimiters, status bar text

#### Countdown Styling

The countdown timer changes color based on proximity to launch, matching the cache TTL tiers:

| Time to Launch | Style |
|---|---|
| > 7 days | Dim white |
| 1–7 days | Normal white |
| < 24 hours | Yellow bold |
| < 1 hour | Red bold |
| In Flight / T+ | Cyan bold |

#### Probability Coloring

The launch probability percentage is colored inline in the detail view:

| Range | Color |
|---|---|
| 80–100% | Green |
| 50–79% | Yellow |
| < 50% | Red |
| -1 / null | Dim gray, displayed as "N/A" |

#### Link Styling

In the detail view links section, labels are rendered in **Cyan** (the conventional terminal color for hyperlinks) and URLs in **Dim** (with underline where supported). Non-URL values (e.g., program names) use normal Tier 2 styling.

Note: Some modern terminals support OSC 8 hyperlink escape sequences for clickable URLs. Ratatui does not have native support for this, but it is noted as a future enhancement opportunity.

### Time Display

#### Dual Timezone Format
Launch times are displayed as: `LAUNCH_TIME_IN_USER_TZ / LAUNCH_TIME_IN_LOCATION_TZ LOCATION_TZ_ABBREV`

Example: `Feb 28, 2026 03:00 AM CST / 09:00 AM UTC`

The location timezone is derived from `pad.location.timezone_name` (an IANA timezone name, e.g., `"America/Chicago"`), converted using the `chrono-tz` crate. If `timezone_name` is not available in the API response, UTC is used as the fallback for the location part.

#### Net Precision Awareness
The API provides a `net_precision` field indicating the accuracy of the launch time. Display format adapts accordingly:
- **Hour/Minute precision**: Full datetime (e.g., `Feb 28, 2026 03:00 AM CST / 09:00 AM UTC`)
- **Day precision**: Date only (e.g., `Feb 28, 2026`)
- **Month precision**: Month and year (e.g., `Mar 2026`)
- **Year precision**: Year only (e.g., `2026`)

#### Launch Window
The detail view shows the launch window (`window_start` to `window_end`) when available, in addition to the NET (No Earlier Than) time.

#### Countdown Timer
Format: `dd days, hh:mm:ss` (e.g., `01 days, 14:23:45`)

The countdown is calculated from `net` and updates in the detail view. When `net_precision` is too coarse (month/year level), the countdown is not shown.

### Key Bindings

```
q         - Quit application
↑/↓       - Navigate list / Scroll detail view
j/k       - Scroll detail view (vim-style)
Enter     - View launch details / Confirm filter selection
Esc       - Back to list view / Cancel filter editing / Close help
r         - Refresh current view (only when cache is stale)
f         - Focus filter panel (list view only)
Tab       - Cycle filter categories (when filter panel focused)
←/→       - Change filter value (when filter panel focused)
Home/End  - Jump to first/last item in list
?         - Show help overlay (context-aware)
```

### Features (MVP)

**Core Features**:
- List view of upcoming launches with pagination (10/25/50 items)
- Detail view for selected launch with scrollable content
- Manual refresh (only when cache stale)
- Offline mode (works from cache, shows `[OFFLINE]` indicator)
- Status indicators for launch status with color coding
- Countdown timers (T-minus) for upcoming launches
- Filtering by status, geographical region, crewed/uncrewed, and date range (server-side)
- Smart caching with proximity-based TTL expiry
- Dual timezone time display with net precision awareness
- Context-aware help overlay
- Terminal resize handling with minimum size enforcement

**Not in MVP** (future enhancements):
- Notifications
- Auto-refresh
- Favorites/watch list
- Search functionality
- Export to file
- Webcast link opening
- Event tracking (EVAs, dockings)
- Terminal image rendering (sixel/kitty protocol via `ratatui-image`)

---

## User Configuration

### Configuration File: `config.toml`

Located at: `<config_dir>/config.toml` (see §File Structure for platform paths)

```toml
[api]
# Optional: Add your free API key from https://lldev.thespacedevs.com/
# This increases rate limit from 15 to 30 requests/hour
# Leave empty for unauthenticated usage
api_key = ""

# Base URL for the Launch Library 2 API
# Use "https://lldev.thespacedevs.com/2.3.0" for development/testing
# Use "https://ll.thespacedevs.com/2.3.0" for production
base_url = "https://lldev.thespacedevs.com/2.3.0"

[cache]
# Time-to-live based on launch proximity (in minutes, range: 1–43200)
# Past launches are cached permanently (data is final) — no config needed.
ttl_far_future = 1440      # 24 hours for launches >7 days away
ttl_near_future = 180       # 3 hours for launches 1-7 days away
ttl_imminent = 30           # 30 minutes for launches <24 hours away
ttl_active = 1             # 1 minute for in-progress launches
ttl_launch_list = 30       # 30 minutes for launch list

# Cache pruning settings
max_detail_age_days = 30   # Delete detail files older than this (min: 1)
max_detail_files = 100     # Keep at most this many detail files (min: 1)
prune_every_n_startups = 5 # Run pruning every N app startups (range: 1–50)

[ui]
# Timestamp format for "next refresh" time
time_format = "12h"        # Options: "12h" (10:45 AM) or "24h" (10:45)

# Show relative or absolute time first in staleness display
staleness_style = "relative"  # Options: "relative" or "absolute"
# "relative" = "Updated 5m ago • ..."
# "absolute" = "Last updated 10:40 AM • ..."

# Number of launches to display per page
launches_per_page = 25     # Options: 10, 25, 50

[log]
# Log level: "error", "warn", "info", "debug", "trace"
level = "warn"
# Log file name (stored in config_dir)
file = "app.log"
```

### Configuration Behavior

- Configuration file is **optional** - app works with sensible defaults
- If config file doesn't exist, it is **not** automatically created
- Users can create config file manually to customize behavior
- Missing keys keep their defaults (via `#[serde(default)]` on all structs)
- Unknown keys are silently ignored (forward-compatible with newer config files)
- Rate limit settings are **not** configurable (enforced by API provider)
- The `toml` crate with `serde` is used for parsing (TOML is the idiomatic config format in the Rust ecosystem and supports comments, unlike JSON)

#### Defensive Sanitisation

After deserialization, `Config::sanitize()` validates every field and replaces out-of-range or dangerous values with safe defaults (logging a warning for each):

| Field | Valid range | Notes |
|---|---|---|
| `ttl_*` (all TTL fields) | 1–43 200 minutes | 30 day upper bound prevents stale-forever data |
| `prune_every_n_startups` | 1–50 | 0 would disable pruning; >50 is unreasonable |
| `max_detail_age_days` | ≥ 1 | 0 would prune everything immediately |
| `max_detail_files` | ≥ 1 | 0 would prune everything immediately |
| `base_url` | Must start with `http://` or `https://` | Prevents non-HTTP schemes |
| `time_format` | `"12h"` or `"24h"` | — |
| `staleness_style` | `"relative"` or `"absolute"` | — |
| `launches_per_page` | 1–100 | — |
| `log.level` | `"error"`, `"warn"`, `"info"`, `"debug"`, `"trace"` | — |
| `log.file` | Plain filename (no `/`, `\`, `..`, or empty) | Prevents path traversal |

---

## Error Handling

### Error Type Architecture

The application uses `thiserror` to define a structured error enum. All fallible operations return `Result<T, AppError>` (or a module-specific error that converts into `AppError`). This enables matching on error variants to drive retry logic, UI state transitions, and user-facing messages.

```rust
#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("API request failed: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Rate limit exceeded, next available at {0}")]
    RateLimited(DateTime<Utc>),

    #[error("API returned error {status}: {message}")]
    ApiError { status: u16, message: String },

    #[error("Cache I/O error: {0}")]
    CacheIo(#[from] std::io::Error),

    #[error("Cache deserialization failed: {0}")]
    CacheParse(#[from] serde_json::Error),

    #[error("Configuration error: {0}")]
    Config(String),
}
```

#### Retryable vs Non-Retryable Errors

Errors are classified to determine whether an automatic retry is appropriate:

| Retryable | Non-Retryable |
|---|---|
| Connection timeout / reset | HTTP 4xx (except 429) |
| DNS resolution failure | Deserialization failure (malformed JSON) |
| HTTP 5xx (500, 502, 503, 504) | Rate limit exceeded (429 — handled separately) |
| HTTP 429 (after throttle sync) | Cache I/O errors |

A helper method on `AppError` encodes this classification:

```rust
impl AppError {
    fn is_retryable(&self) -> bool {
        match self {
            AppError::Network(e) => {
                e.is_timeout() || e.is_connect() ||
                e.status().map_or(false, |s| s.is_server_error())
            }
            _ => false,
        }
    }
}
```

### Error Categories and UI Behaviour

#### Cache Read Errors
If cached data fails to load (corrupt file, I/O error), the app logs a `WARN` and falls back to re-fetching from the API. The corrupt cache file is overwritten on the next successful fetch. If the re-fetch also fails (offline, rate limited), the user is shown the appropriate error message rather than a generic "offline" screen — the log entry provides diagnostic context.

#### Cache Version Mismatches
The `version` field in each cache file (`app_state.json`, `cache.json`, `details/*.json`) is checked on load. If the version does not match the current expected version, the file is treated as a cache miss (not an error): the data is discarded, a `DEBUG`-level log is emitted, and the app proceeds as if the cache file did not exist. There is no migration between cache versions — the cache is ephemeral and will be repopulated from the API.

#### Network/API Errors (Timeouts, 5xx responses)
- Check `AppError::is_retryable()` — only retry on transient network errors and 5xx responses (see §Error Type Architecture for the full classification)
- Perform a **single automatic retry** after a 1-second `tokio::time::sleep` (non-blocking — the event loop continues rendering during the pause)
- If the retry also fails, display an inline error message where content would appear: `"Something went wrong fetching this data, please try again in a few minutes"`
- Non-retryable errors (4xx, deserialization failures) skip the retry and display the error immediately
- Show a brief "Retrying..." indicator during the retry attempt
- If cached data exists, continue displaying it with a staleness indicator

#### Rate Limit Errors (Client-detected or 429 response)
- Display a specific message explaining the rate limit: `"Rate limit reached. Requests will be available again at HH:MM"`
- Calculate and show the time when the oldest tracked request will expire from the rolling hour window
- Disable the refresh key until requests are available

#### Offline / No Cache (Cold Start Failure)
- Display a centered message in the content area: `"It looks like you're offline at the moment, we couldn't fetch any launch data"`
- If the app detects it cannot reach the API at any point during the session, add an `[OFFLINE]` indicator to the status bar
- Offline state is detected reactively by handling network errors (not proactively probed)

#### Error Display
Errors are displayed **inline within the existing view layout**, positioned where content would normally appear. This avoids jarring context switches. The status bar continues to function normally during error states.

---

## Application Architecture

### Application State Machine

The TUI is driven by a state machine that tracks the current screen and application state:

```rust
enum AppScreen {
    List,
    Detail(String),     // launch UUID
    Help(Box<AppScreen>), // wraps the screen that was active when help opened
    FilterPanel,
}

struct App {
    screen: AppScreen,
    launches: Vec<LaunchSummary>,
    selected_index: usize,
    list_scroll_offset: usize,
    detail_scroll_offset: usize,
    detail_cache: HashMap<String, LaunchDetailCache>,
    cache_manager: CacheManager,
    rate_limiter: RateLimiter,
    config: Config,
    loading: bool,
    error_state: Option<ErrorState>,
    is_offline: bool,
    filter_state: FilterState,
    terminal_size: (u16, u16),  // (columns, rows)
}

struct FilterState {
    active_category: usize,     // which filter is focused
    status: Option<Vec<u32>>,   // status IDs
    region: Option<String>,     // pad__location value
    is_crewed: Option<bool>,
    date_range: Option<DateRange>,
}

enum ErrorState {
    /// Transient error — auto-dismiss after a timeout, user can retry
    Transient {
        message: String,
        dismiss_at: Instant,
    },
    /// Rate limit hit — persists until requests are available again
    RateLimited {
        available_at: DateTime<Utc>,
    },
    /// Network is unreachable — persists until a successful request
    Offline,
}
```

`ErrorState` replaces a plain string + timestamp. The enum variants drive different UI behaviors:
- `Transient`: shown inline where content would appear, auto-dismissed after 10 seconds. The refresh key remains available.
- `RateLimited`: disables the refresh key and shows a countdown to when requests become available. Persists until `available_at` is in the past.
- `Offline`: sets the `[OFFLINE]` indicator in the status bar. Cleared when any API request succeeds.

### Async Event Loop Architecture

The application uses a single-threaded async model with `tokio::select!` to multiplex between terminal input events and pending API calls. This is simpler than a multi-threaded approach and sufficient for this application's needs.

```rust
loop {
    // Check terminal size on each iteration
    if terminal_too_small() {
        render_size_warning(&mut terminal, &app)?;
        // Still process events (resize, quit) but skip normal rendering
    }

    tokio::select! {
        // Handle terminal input events (keys, resize)
        event = crossterm_event_stream.next() => {
            match event {
                Key(key_event) => handle_input(&mut app, key_event),
                Resize(cols, rows) => app.terminal_size = (cols, rows),
                _ => {}
            }
        }
        // Handle completed API responses (only when a request is in-flight)
        result = &mut pending_api_call, if app.loading => {
            handle_api_result(&mut app, result);
        }
    }

    render(&mut terminal, &app)?;
}
```

### Graceful Shutdown

When the application exits (via `q` key or terminal signal):

1. Save `app_state.json` with current rate limit tracking data
2. Flush any pending cache writes
3. Restore the terminal to its original state (crossterm `disable_raw_mode`, `LeaveAlternateScreen`)

This ensures rate limit tracking data is not lost mid-session. Terminal restoration is handled via a RAII guard struct whose `Drop` implementation calls `disable_raw_mode` and `LeaveAlternateScreen`. This guarantees restoration on both normal exits (`main` returning `Ok` or `Err`) and panics, without relying solely on a panic hook.

```rust
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), LeaveAlternateScreen);
    }
}
```

A panic hook is still registered as a belt-and-suspenders measure (the guard's `Drop` may not run if the panic triggers an abort), but the guard is the primary mechanism.

### First-Run Experience

On first launch with no cache and no config file:

1. Display a "Fetching launches..." message in the content area while the initial API call is in progress
2. If the fetch succeeds, render the list view normally
3. If the fetch fails (offline, rate limited), display the appropriate error message (see Error Handling section)

The config directory (`config_dir`) and cache directory (`cache_dir`) are created automatically on first run. The `config.toml` is not auto-created.

---

## Application Flow

### Startup Sequence

1. **Initialize cache manager**
   - Create config and cache directories if they don't exist
   - Load or create `app_state.json`
   - Load `cache.json` if it exists
   - Load `config.toml` if it exists

2. **Maybe prune cache**
   - Increment startup count
   - If `startup_count % prune_every_n_startups == 0`, run pruning

3. **Sync rate limit**
   - Call `/api-throttle/` endpoint
   - Update local rate limit state
   - Adjust limit based on API key presence

4. **Load launch list**
   - Check if `cache.json` exists and is fresh
   - If fresh, display cached data immediately
   - If stale or missing, show "Fetching launches..." and fetch `/launches/upcoming/?mode=normal&limit=N`
   - Display list view

5. **Start TUI**
   - Enter alternate screen, enable raw mode
   - Register panic hook for terminal restoration
   - Render UI with cached/fetched data
   - Begin async event loop

### User Interaction Flow

#### Viewing Launch List
1. User starts app -> Shows cached list (or fetches if stale/missing)
2. User navigates with up/down arrow keys
3. If cache is stale, status bar shows "Press 'r' to refresh"
4. User presses `r` (if stale) -> Fetches new list -> Updates cache -> Re-renders

#### Viewing Launch Details
1. User presses Enter on a launch
2. Check if detail cache exists and is fresh
3. If missing or stale -> Fetch `/launch/{uuid}/` -> Cache result
4. Display detail view with scrolling support
5. If cache becomes stale, show "Press 'r' to refresh" in status bar

#### Filtering Launches
1. User presses `f` to focus filter panel
2. Use `Tab` to cycle between filter categories
3. Use left/right arrows to change values within a category
4. Press `Enter` to apply filters -> Triggers API request with filter parameters
5. Press `Esc` to cancel and revert to previous filter state

#### Manual Refresh Logic
```rust
fn can_refresh(&self, cache_type: CacheType) -> bool {
    match cache_type {
        CacheType::List => self.is_launch_list_stale(),
        CacheType::Detail(id) => self.is_launch_detail_stale(id),
    }
}

// Only allow refresh if cache is stale
if user_pressed_refresh && self.can_refresh(current_view) {
    fetch_and_update_cache();
}
```

---

## Data Models

### Core Structs

```rust
struct CacheManager {
    config_dir: PathBuf,
    cache_dir: PathBuf,
    app_state: AppState,
    launch_list: Option<LaunchListCache>,
    config: Config,
}

struct AppState {
    version: u32,
    rate_limit: RateLimitState,
    last_startup: DateTime<Utc>,
    startup_count: u32,
    last_prune: Option<DateTime<Utc>>,
}

struct RateLimitState {
    limit: usize,
    requests: Vec<DateTime<Utc>>,
    last_sync: DateTime<Utc>,
    authenticated: bool,
}

struct LaunchListCache {
    version: u32,
    fetched_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    total_count: u32,
    launches: Vec<LaunchSummary>,
}

struct LaunchDetailCache {
    version: u32,
    launch_id: String,        // UUID string
    fetched_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    ttl_strategy: String,
    data: serde_json::Value,  // Full API response
}

struct LaunchSummary {
    id: String,                // UUID from API
    name: String,
    net: DateTime<Utc>,
    net_precision: Option<NetPrecision>,
    window_start: Option<DateTime<Utc>>,
    window_end: Option<DateTime<Utc>>,
    status: LaunchStatus,
    launch_service_provider: Provider,
    pad: PadInfo,
    mission: Option<MissionSummary>,
}

struct NetPrecision {
    id: u32,
    name: String,
    abbrev: String,
}

struct LaunchStatus {
    id: u32,
    name: String,
    abbrev: String,
}

struct Provider {
    name: String,
    provider_type: ProviderType, // { name: String }
}

struct PadInfo {
    name: Option<String>,
    location: LocationInfo,
}

struct LocationInfo {
    name: String,
    timezone_name: Option<String>,  // IANA timezone e.g. "America/Chicago"
    country: Option<CountryInfo>,
}

struct MissionSummary {
    name: String,
    mission_type: String,
    description: Option<String>,
    orbit: Option<OrbitInfo>,
}
```

---

## Logging

### Configuration

Logging uses the `tracing` crate (the Rust ecosystem standard, integrates well with tokio's async runtime).

- **Log file**: `<config_dir>/app.log`
- **Default level**: `WARN` (configurable in `config.toml`)
- **Rotation**: Truncate log file at 1MB on app startup

### What Gets Logged

| Level | Events |
|-------|--------|
| ERROR | Unrecoverable errors, panic information, file I/O failures |
| WARN  | API errors, cache corruption detected, config parse failures with fallback |
| INFO  | Rate limit events (approaching limit, limit reset), cache pruning results |
| DEBUG | API requests/responses, cache hits/misses, config values loaded |
| TRACE | Event loop iterations, render timing, key events |

---

## Success Criteria

### MVP is Complete When:

- App can fetch and display upcoming launches
- User can view detailed information for any launch
- Caching works correctly with TTL-based expiry
- Rate limiting prevents exceeding API limits
- Manual refresh works only when cache is stale
- Cache pruning prevents unbounded disk usage
- App works offline from cached data
- Status bar shows accurate staleness and rate limit info
- Configuration file works with sensible defaults
- Filtering by status, region, crewed/uncrewed, and date range works
- Detail view shows comprehensive launch information with scrolling
- Time display respects net precision and shows dual timezones
- Help overlay is context-aware
- Terminal resize is handled gracefully
- Cross-platform support (Linux, macOS, Windows)
- Single binary executable

### Quality Criteria:

- Clean, maintainable Rust code with vendor-specific logic isolated
- Comprehensive error handling with user-friendly messages
- No panics in normal operation (panic hook restores terminal)
- Responsive UI (no blocking operations in the event loop)
- Clear user feedback for all actions (loading, errors, staleness)
- Intuitive key bindings with discoverable help
- Professional terminal aesthetics
- Graceful shutdown preserving application state

---

## Future Enhancements (Post-MVP)

- System notifications for followed launches
- Favorites/watch list with persistence
- Search functionality
- Export launch details to markdown/text
- Open webcast links in browser
- Event tracking (EVAs, dockings, etc.)
- Mission timeline visualization
- Multi-language support
- Custom themes/color schemes
- More sophisticated filtering (by rocket, orbit, mission type)
- Past launch history view (using `/launches/previous/`)
- Terminal image rendering for mission patches and rocket images (sixel/kitty protocol via `ratatui-image`)
- First-run ASCII art splash screen

---

## Implementation Notes

### Development Phases

**Phase 1**: Core Infrastructure
- Project setup and dependencies
- Vendor configuration structure (`src/vendor/launch_library_2/`)
- Cache manager implementation
- Rate limiter implementation
- Configuration loading (`toml` + `serde`)
- Logging setup (`tracing`)

**Phase 2**: API Integration
- HTTP client setup with generic traits
- LL2 vendor implementation of API client
- Response parsing and error handling (including retry logic)
- Cache integration

**Phase 3**: TUI Implementation
- Application state machine
- Async event loop with `tokio::select!`
- Basic UI layout with ratatui
- List view rendering with pagination
- Detail view rendering with scrolling
- Navigation and key handling
- Terminal resize handling

**Phase 4**: Filtering & Polish
- Filter panel UI and server-side filter queries
- Status bar implementation with staleness indicators
- Help overlay (context-aware)
- Error handling UX (inline messages)
- Offline detection and indicators
- First-run experience
- Graceful shutdown

**Phase 5**: Testing & Distribution
- Unit tests for cache logic, rate limiting, time display, net precision
- Integration tests for API client
- Manual testing on Linux, macOS, Windows
- Testing with and without API key
- Testing offline behavior and error scenarios
- Build automation and release process

### Testing Strategy

#### Test Infrastructure

- **Filesystem isolation**: All tests that read/write files use `tempfile::TempDir` to create isolated temporary directories. This prevents tests from interfering with each other or the user's real config. `tempfile` is a dev dependency.
- **HTTP mocking**: API client integration tests use `wiremock` to spin up a local HTTP server with canned responses. This tests the full reqwest stack (serialization, headers, query parameters) rather than mocking at the trait level. The `api::client` trait boundary remains available for unit-testing higher-level logic (e.g., TUI data flow) without network calls.
- **Time control**: All time-dependent tests use `FakeClock` (see §Clock Abstraction for Testability) to advance time deterministically. No `thread::sleep` or wall-clock assertions in unit tests.
- **UI rendering**: Rendering functions are structured to accept `&App` state and `&mut Frame` so they can be called in test contexts. `ratatui::backend::TestBackend` is used to render to an in-memory buffer and assert on output content (e.g., verifying that a status badge renders with the correct text, or that the "stale" indicator appears).

#### Unit Tests

- **Cache logic**: TTL calculation for all five tiers, pruning (age-based and count-based), read/write round-trips, corrupt file handling, version mismatch treated as cache miss
- **Rate limiting**: Rolling window tracking, `remaining()` count, `can_make_request()` blocks at limit, requests older than 1 hour are pruned, `should_sync()` triggers under specified conditions, `next_available_at()` returns correct time
- **Time display**: Dual timezone formatting, all net precision levels (hour, day, month, year), countdown formatting for various deltas, countdown suppression for coarse precision, relative time formatting ("5m ago")
- **Vendor mappings**: Status ID → color/style for all 8 statuses, unknown status ID falls back to gray, region name → `pad__location` ID mapping
- **Config parsing**: Missing file → defaults, valid file → parsed values, partial file → defaults for missing keys, invalid values → defaults with warning logged
- **Error classification**: `is_retryable()` returns correct results for each error variant

#### Integration Tests

- **API client**: Successful list fetch parses correctly, successful detail fetch parses correctly, rate limiter blocks requests when exhausted, 5xx triggers retry then returns error, 429 triggers throttle sync, timeout returns offline-compatible error, API key is sent in header when configured, query parameters are constructed correctly for each filter combination
- **Vendor trait contract**: LL2 implementation correctly implements the `api::client` trait — response parsing doesn't silently drop fields, pagination parameters are constructed correctly, filter parameters map to correct API query strings

#### Edge Case Tests

- **Empty API responses**: `/launches/upcoming/` returns `{"count": 0, "results": []}` — list view handles empty state gracefully
- **Clock skew**: System clock jumps backward (simulated with `FakeClock`) — rate limiter rolling window doesn't block forever, expired cache entries are handled correctly
- **Large content**: Mission description of 10KB+ — detail view scrolling handles it without layout corruption
- **Unicode in data**: Launch names and locations with multi-byte UTF-8 characters — TUI layout calculates display width correctly (note: `unicode-width` crate may be needed for CJK characters)
- **Concurrent app instances**: Two app instances running simultaneously — last-write-wins is acceptable for cache files (atomic writes via temp-file-then-rename prevent corruption, but no file locking)
- **Cache expiry boundaries**: Launch exactly 7 days away, exactly 24 hours away, in-flight status transitions

#### Manual Testing

- Linux, macOS, Windows
- With and without API key
- Offline behavior (no network, partial network)
- Terminal resize (below minimum, resize during use, responsive layout threshold at 100 columns)
- First run with no config/cache directories

---

## References

- **Launch Library 2 API Documentation**: https://ll.thespacedevs.com/docs/
- **Ratatui Documentation**: https://ratatui.rs/
- **Tokio Documentation**: https://tokio.rs/
- **The Rust Programming Language**: https://doc.rust-lang.org/book/
- **Crossterm Documentation**: https://docs.rs/crossterm/
- **Tracing Documentation**: https://docs.rs/tracing/
