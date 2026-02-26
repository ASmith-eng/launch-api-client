# Space Missions TUI - Design Document

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
- **Additional Crates**:
  - `directories` - Cross-platform config paths
  - `chrono` - Date/time handling
  - Rate limiting crate (e.g., `governor` or custom implementation)

---

## API Integration

### API Provider
**Launch Library 2** (TheSpaceDevs) - https://lldev.thespacedevs.com/

### Primary Endpoints

1. **`/launch/upcoming/`**
   - Gets upcoming launches with filtering and pagination
   - Returns launches sorted by launch date
   - Includes mission details, launch service provider, rocket info, mission patches
   - Supports filtering: `?search=NASA` for NASA missions only
   - Supports pagination: `?limit=` and `?offset=`

2. **`/launch/{id}/`**
   - Gets detailed info for a specific launch
   - Contains status updates, webcast links, and mission description
   - Used for detail view

3. **`/api-throttle`**
   - Returns current rate limit status
   - **Does not count against API rate limit**
   - Used for syncing local rate limit tracking with server

### Future Endpoints (Not in MVP)
- `/event/upcoming/` - Space events (EVAs, dockings, etc.)

---

## API Rate Limiting

### Rate Limit Constraints
- **Unauthenticated**: 15 requests per hour per IP
- **Authenticated** (with free API key): 30 requests per hour per key
- **Global limit**: Applies across all endpoints (not per-endpoint)

### Rate Limiting Strategy

#### Client-Side Tracking
```rust
struct RateLimiter {
    limit: usize,              // 15 or 30 based on authentication
    requests: VecDeque<Instant>, // Timestamps of requests in last hour
    last_sync: Instant,        // Last time we synced with server
}
```

- Track all API requests made in the last rolling hour
- Remove requests older than 1 hour from tracking
- Block requests that would exceed the limit
- Persist request history to disk (in `app_state.json`)

#### Rate Limit Calibration Strategy

Sync with server via `/api-throttle` endpoint:

1. **Always on startup** - Get accurate initial state
2. **When close to limit** - Sync if remaining requests ≤ 2
3. **After 429 errors** - Sync if we receive a rate limit error from server
4. **Long-running sessions** - Sync if app has been running >1 hour without sync
5. **Graceful degradation** - If `/api-throttle` fails, continue with local tracking

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

```
~/.config/space-missions/
├── app_state.json       # Rate limits, app metadata, startup count
├── cache.json           # Cached list of launches
├── details/             # Individual launch detail caches
│   ├── launch_123.json
│   ├── launch_456.json
│   └── ...
└── config.toml          # User configuration
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
  "launches": [
    {
      "id": "launch_123",
      "name": "Starship IFT-7",
      "net": "2026-02-28T09:00:00Z",
      "status": {
        "id": 1,
        "name": "Go for Launch"
      },
      "launch_service_provider": {
        "name": "SpaceX",
        "type": "Commercial"
      },
      "pad": {
        "location": {
          "name": "Starbase, Texas"
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

**Purpose**: Minimal data for list view, loaded on app startup

#### 3. `details/launch_*.json` - Individual Launch Details
```json
{
  "version": 1,
  "launch_id": "launch_123",
  "fetched_at": "2026-02-22T10:20:00Z",
  "expires_at": "2026-02-22T10:25:00Z",
  "ttl_strategy": "imminent",
  "data": {
    // Full API response for this launch
  }
}
```

**Purpose**: Full launch details, lazy-loaded only when user views a specific launch

### Request Strategy: Lazy Loading + Smart Expiry

**Core Principle**: Only make API requests when explicitly needed by user action and when cached data is stale.

#### When API Requests Are Made

1. **App startup**: Fetch `/launch/upcoming/` if no cache exists or cache is expired
2. **User selects launch**: Fetch `/launch/{id}/` if no detail cache exists or cache is expired
3. **User manually refreshes**: Only allowed when cache is stale (past expiry time)

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
    MediumTerm,         // Launches 1-7 days away - 1 hour TTL
    ShortTerm,          // Launches <24 hours away - 5 minute TTL
    RealTime,           // Active launches (in-progress) - 1 minute TTL
}
```

**Default TTL Values** (minutes):
- `ttl_past_launches`: 0 (never expires)
- `ttl_far_future`: 1440 (24 hours)
- `ttl_near_future`: 60 (1 hour)
- `ttl_imminent`: 5 (5 minutes)
- `ttl_active`: 1 (1 minute)
- `ttl_launch_list`: 30 (30 minutes)

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

### Layout

#### List View (Main Screen)
```
┌─ Upcoming Launches ──────────────────────────────────────── 12/15 ─┐
│                                                                     │
│  ▸ Starship IFT-7              Feb 28, 2026 09:00 UTC   [Go]      │
│    SpaceX | Starbase, TX                                           │
│                                                                     │
│  ▸ Crew-10                     Mar 2, 2026 14:30 UTC    [Go]       │
│    SpaceX | Kennedy Space Center                                   │
│                                                                     │
│  ▸ Artemis III                 Sep 2026                 [Planned]  │
│    NASA | Kennedy Space Center                                     │
│                                                                     │
├────────────────────────────────────────────────────────────────────┤
│ Updated 5m ago • Next refresh after 10:45 AM                       │
│ ↑/↓: Navigate | Enter: Details | r: Refresh (when stale) | q: Quit│
└────────────────────────────────────────────────────────────────────┘
```

#### Detail View
```
┌─ Starship IFT-7 Details ───────────────────────────────────────────┐
│                                                                     │
│ Launch: Feb 28, 2026 09:00:00 UTC                                  │
│ T-minus: 5 days, 14 hours, 23 minutes                              │
│ Status: Go for Launch ✓                                            │
│                                                                     │
│ Vehicle: Starship                                                   │
│ Location: Starbase, Texas                                           │
│                                                                     │
│ Mission: Orbital flight test demonstrating booster catch and       │
│ ship deorbit capabilities...                                       │
│                                                                     │
├────────────────────────────────────────────────────────────────────┤
│ Updated 2m ago • Next refresh after 10:07 AM          │ 11/15 reqs │
│ Esc: Back | r: Refresh (when stale) | ?: Help                     │
└────────────────────────────────────────────────────────────────────┘
```

### UI Components

#### Status Bar - Bottom of Screen

**Format**: Two-part status bar

**Left side** - Cache staleness info:
- When fresh: `Updated 5m ago • Next refresh after 10:45 AM`
- When stale: `Updated 2h ago (stale) • Press 'r' to refresh`

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

### Color Coding

**Launch Status Colors**:
- Green: Success, Go for Launch
- Yellow: Upcoming, TBD, To Be Confirmed
- Red: Failure, Scrubbed
- Gray: Planned (far future)

### Key Bindings

```
q         - Quit application
↑/↓       - Navigate list
Enter     - View launch details
Esc       - Back to list view
r         - Refresh current view (only when cache is stale)
f         - Toggle filters (future feature)
/         - Search (future feature)
?         - Show help overlay
```

### Features (MVP)

**Core Features**:
- List view of upcoming launches
- Detail view for selected launch
- Manual refresh (only when cache stale)
- Offline mode (works from cache)
- Status indicators for launch status
- Countdown timers (T-minus) for upcoming launches
- Filtering by agency and date range
- Smart caching with expiry

**Not in MVP** (future enhancements):
- Notifications
- Auto-refresh
- Favorites/watch list
- Search functionality
- Export to file
- Webcast link opening
- Event tracking (EVAs, dockings)

---

## User Configuration

### Configuration File: `config.toml`

Located at: `~/.config/space-missions/config.toml`

```toml
[api]
# Optional: Add your free API key from https://lldev.thespacedevs.com/
# This increases rate limit from 15 to 30 requests/hour
# Leave empty for unauthenticated usage
api_key = ""

[cache]
# Time-to-live based on launch proximity (in minutes)
ttl_past_launches = 0      # Never expires (data is final)
ttl_far_future = 1440      # 24 hours for launches >7 days away
ttl_near_future = 60       # 1 hour for launches 1-7 days away
ttl_imminent = 5           # 5 minutes for launches <24 hours away
ttl_active = 1             # 1 minute for in-progress launches
ttl_launch_list = 30       # 30 minutes for launch list

# Cache pruning settings
max_detail_age_days = 30   # Delete detail files older than this
max_detail_files = 100     # Keep at most this many detail files
prune_every_n_startups = 5 # Run pruning every N app startups

[ui]
# Timestamp format for "next refresh" time
time_format = "12h"        # Options: "12h" (10:45 AM) or "24h" (10:45)

# Show relative or absolute time first in staleness display
staleness_style = "relative"  # Options: "relative" or "absolute"
# "relative" = "Updated 5m ago • ..."
# "absolute" = "Last updated 10:40 AM • ..."

[filters]
# Default agencies to show (empty = show all)
default_agencies = []      # Example: ["NASA", "SpaceX"]
```

### Configuration Behavior

- Configuration file is **optional** - app works with sensible defaults
- If config file doesn't exist, it is **not** automatically created
- Users can create config file manually to customize behavior
- Invalid config values fall back to defaults with warning
- Rate limit settings are **not** configurable (enforced by API provider)

---

## Application Flow

### Startup Sequence

1. **Initialize cache manager**
   - Create config directory if it doesn't exist
   - Load or create `app_state.json`
   - Load `cache.json` if it exists
   - Load `config.toml` if it exists

2. **Maybe prune cache**
   - Increment startup count
   - If `startup_count % prune_every_n_startups == 0`, run pruning

3. **Sync rate limit**
   - Call `/api-throttle` endpoint
   - Update local rate limit state
   - Adjust limit based on API key presence

4. **Load launch list**
   - Check if `cache.json` exists and is fresh
   - If stale or missing, fetch `/launch/upcoming/`
   - Display list view

5. **Start TUI**
   - Render UI with cached/fetched data
   - Begin event loop

### User Interaction Flow

#### Viewing Launch List
1. User starts app → Shows cached list (or fetches if stale/missing)
2. User navigates with ↑/↓
3. If cache is stale, status bar shows "Press 'r' to refresh"
4. User presses `r` (if stale) → Fetches new list → Updates cache → Re-renders

#### Viewing Launch Details
1. User presses Enter on a launch
2. Check if detail cache exists and is fresh
3. If missing or stale → Fetch `/launch/{id}` → Cache result
4. Display detail view
5. If cache becomes stale, show "Press 'r' to refresh" in status bar

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
    base_path: PathBuf,
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
    launches: Vec<LaunchSummary>,
}

struct LaunchDetailCache {
    version: u32,
    launch_id: String,
    fetched_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    ttl_strategy: String,
    data: serde_json::Value,  // Full API response
}

struct LaunchSummary {
    id: String,
    name: String,
    net: DateTime<Utc>,
    status: LaunchStatus,
    launch_service_provider: Provider,
    pad: Location,
    mission: MissionSummary,
}
```

---

## Open Design Questions

### Questions Still To Be Decided & Feedback

1. **Filtering Implementation**
   - How should the filter UI work? Modal? Inline?
   - Should filters persist between sessions? Answer: No, the filter options are only simple so this is not necessary.
   - What filter options beyond agency? (Status, date range, rocket type?) Answer: date range, status, geographical region, crewed/uncrewed.

2. **Error Handling UX**
   - How to display API errors vs network errors vs cache errors? Answer: Failure to fetch from cache -> refetch from source, for other errors (timeout, 500 responses) we display a generic "Something went wrong fetching this data, please try again in a few minutes", for rate limits (client monitored or from API response) we should display a message that explains when the limit will reset.
   - Should we have a dedicated error view or inline messages? Answer: Unsure at this stage, but we should try some mock-ups once we have the basis of the UI design finalised to help decision make.
   - Retry logic for failed requests? Answer: Perhaps single retry for 500 status API request errors before showing error message to user.

3. **Detail View Content**
   - What specific fields from the API response should be displayed? Answer: We should brainstorm this based on the available data from the Launch_Library_2_API_Docs.md documentation and the available space we have in the UI.
   - Layout and organization of mission details?
   - Should we show mission patches/images if available? Answer: I think it would be awesome to load images if they are available!

4. **List View Pagination**
   - How many launches to fetch initially? (20, 50, 100?) Answer: This is a very important one - we should impose a hard cap of 50 launches limited in the fetch, but create a UI option to set this to 10, 25, or 50.
   - Should we support "load more" or fetch all upcoming at once? Answer: Perhaps we can paginate and cache viewed pages so they are not re-fetched on quick navigation?
   - How to handle very long lists (100+ launches)? Answer: as above, we will impose a strict max limit of 50.

5. **Help Overlay**
   - Content and layout for `?` help screen?
   - Should it be context-aware (different help for list vs detail view)? Answer: This is a great idea!

6. **Launch Status Mapping**
   - Need to map Launch Library 2 status IDs to display text and colors
   - What are all the possible status values from the API?

7. **Time Display**
   - How to display launch times in different timezones? Answer: We should always try to show launch times in "LAUNCH_TIME_IN_USER_TZ / LAUNCH_TIME_IN_LOCATION_TZ LAUNCH_LOCATION_TZ_STRING". If the launch time is not given in the location's time zone, we should just display UTC for the second part.
   - Format for countdown timers (days/hours/minutes/seconds)? Answer: "dd days, hh:mm:ss" please

8. **Offline Behavior**
   - What message to show when offline and no cache available? Answer: We should add an "offline" status somewhere on the UI outer frame even if there is cached data. But if there is nothing to display, we should also show "It looks like you're offline at the moment, we couldn't fetch any launch data" where the content would be.
   - Should we detect offline state proactively or just handle errors? Answer: Just handle errors at the moment, to keep the app lean.

9. **Binary Distribution**
   - Multi-platform builds, distribution and versioning should be later challenges for after we have finished a proof-of-concept.

10. **Logging**
    - What to log and at what levels?
    - Log file location and rotation?
    - Should logging be configurable?

---

## Success Criteria

### MVP is Complete When:

- ✅ App can fetch and display upcoming launches
- ✅ User can view detailed information for any launch
- ✅ Caching works correctly with TTL-based expiry
- ✅ Rate limiting prevents exceeding API limits
- ✅ Manual refresh works only when cache is stale
- ✅ Cache pruning prevents unbounded disk usage
- ✅ App works offline from cached data
- ✅ Status bar shows accurate staleness and rate limit info
- ✅ Configuration file works with sensible defaults
- ✅ Cross-platform support (Linux, macOS, Windows)
- ✅ Single binary executable

### Quality Criteria:

- Clean, maintainable Rust code
- Comprehensive error handling
- No panics in normal operation
- Responsive UI (no blocking operations)
- Clear user feedback for all actions
- Intuitive key bindings
- Professional terminal aesthetics

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
- Past launch history view

---

## Implementation Notes

### Development Phases

**Phase 1**: Core Infrastructure
- Project setup and dependencies
- Cache manager implementation
- Rate limiter implementation
- Configuration loading

**Phase 2**: API Integration
- HTTP client setup
- API client with rate limiting
- Response parsing and error handling
- Cache integration

**Phase 3**: TUI Implementation
- Basic UI layout with ratatui
- List view rendering
- Detail view rendering
- Navigation and key handling

**Phase 4**: Polish & Testing
- Status bar implementation
- Staleness indicators
- Error handling UX
- Cross-platform testing
- Documentation

**Phase 5**: Distribution
- Build automation
- Release process
- Package management
- User documentation

### Testing Strategy

- Unit tests for cache logic
- Unit tests for rate limiting
- Integration tests for API client
- Manual testing on Linux, macOS, Windows
- Testing with and without API key
- Testing offline behavior
- Testing cache expiry edge cases
- Testing rate limit edge cases

---

## References

- **Launch Library 2 API Documentation**: https://ll.thespacedevs.com/2.2.0/swagger
- **Ratatui Documentation**: https://ratatui.rs/
- **Tokio Documentation**: https://tokio.rs/
- **The Rust Programming Language**: https://doc.rust-lang.org/book/
