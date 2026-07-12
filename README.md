# Launch Client

[![CI](https://github.com/ASmith-eng/launch-api-client/actions/workflows/ci.yml/badge.svg)](https://github.com/ASmith-eng/launch-api-client/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](#license)

A fast, keyboard-driven terminal UI for browsing upcoming and in-progress
space launches, powered by the [Launch Library 2](https://ll.thespacedevs.com/docs/)
API from [The Space Devs](https://thespacedevs.com/).

Launch Client is built to be respectful of the API's tight rate limits. Rather
than polling in the background, it fetches data only when you ask for it and
caches everything intelligently based on how soon each launch is happening - so
the app stays snappy, works offline from its cache, and makes requests only when you
absolutely need to.

> The published binary is named `launch-client` (the repository is
> `launch-api-client`).

---

## Contents

- [Features](#features)
- [Screenshots](#screenshots)
- [Installation](#installation)
- [Updating](#updating)
- [Uninstalling](#uninstalling)
- [Configuration](#configuration)
- [Features in depth](#features-in-depth)
- [Keybindings](#keybindings)
- [Technical details](#technical-details)
  - [Extensibility: adding an API provider](#extensibility-adding-an-api-provider)
  - [Caching strategy](#caching-strategy)
- [Building & development](#building--development)
- [License](#license)

---

## Features

- **List view** of upcoming and in-progress launches with status badges,
  live countdowns, and paginated browsing.
- **Rich detail view** with a hero countdown, a reverse-chronological updates
  timeline, mission info, crew rosters, booster landing details, full vehicle
  specifications, reliability record bars, provider/location details, and links.
- **Mission filtering** by launch status, geographical region,
  crewed/uncrewed, and date range.
- **Dual-timezone, precision-aware time display** - every launch time is shown
  in both your local timezone and the launch site's timezone, and the format
  adapts to how precisely the launch is scheduled (exact time vs. "Sep 2026").
- **Smart proximity-based caching** - cache lifetimes scale with how soon a
  launch is happening, from 24 hours for distant launches down to 1 minute for
  ones in flight.
- **Offline mode** - the app runs entirely from its cache when the network is
  unavailable, with a visual indicator to let you know you're offline.
- **Rate-limit awareness** - client-side tracking synced with the API keeps you
  from ever exceeding your hourly request budget, with remaining requests shown
  in the title bar.
- Context-aware help overlay and graceful terminal-resize handling.
- **Single self-contained binary** - no runtime dependencies.

---

## Screenshots

**List view**

![Launch Client list view](documentation/screenshots/list-view.png)

**Detail view** (a crewed mission, showing crew, landing, vehicle specs, and record bars)

![Launch Client detail view](documentation/screenshots/detail-view.png)

**Filter panel**

![Launch Client filter panel](documentation/screenshots/filter-panel.png)

---

## Installation

Launch Client is a single executable with no runtime dependencies. Pick
whichever method suits you.

### Option 1 - Download a prebuilt binary (recommended)

1. Go to the [**Releases**](https://github.com/ASmith-eng/launch-api-client/releases)
   page and download the archive for your platform:

   | Platform | File |
   |----------|------|
   | Linux (x86_64) | `launch-client-<version>-x86_64-unknown-linux-gnu.tar.gz` |
   | macOS (Apple Silicon) | `launch-client-<version>-aarch64-apple-darwin.tar.gz` |
   | macOS (Intel) | `launch-client-<version>-x86_64-apple-darwin.tar.gz` |
   | Windows (x86_64) | `launch-client-<version>-x86_64-pc-windows-msvc.zip` |

2. Extract the archive.
3. Move the `launch-client` binary somewhere on your `PATH` (e.g.
   `/usr/local/bin` on macOS/Linux), then run it:

   ```sh
   launch-client
   ```

> **macOS Gatekeeper:** because the binary isn't notarized, the first launch may
> be blocked. Allow it under **System Settings → Privacy & Security**, or clear
> the quarantine attribute with `xattr -d com.apple.quarantine ./launch-client`.

### Option 2 - Install with Cargo

If you have a [Rust toolchain](https://rustup.rs/) installed, you can build and
install straight from the repository:

```sh
cargo install --git https://github.com/ASmith-eng/launch-api-client
```

This compiles the release binary and places `launch-client` in
`~/.cargo/bin/` (make sure that's on your `PATH`). It builds from the latest
commit on `main`, which always tracks the most recent release-ready code.

To pin a specific released version instead, add its tag:

```sh
cargo install --git https://github.com/ASmith-eng/launch-api-client --tag v1.2.1 --locked
```

`--locked` builds against the committed `Cargo.lock` for a reproducible set of
dependency versions.

### Option 3 - Build from source

```sh
git clone https://github.com/ASmith-eng/launch-api-client
cd launch-api-client
cargo build --release
```

The compiled binary is at `target/release/launch-client`.

---

## Updating

- **Prebuilt binary:** download the latest archive from the
  [Releases](https://github.com/ASmith-eng/launch-api-client/releases) page and
  replace your existing binary.
- **Cargo install:** re-run the install command with `--force`:

  ```sh
  cargo install --git https://github.com/ASmith-eng/launch-api-client --force
  ```

- **From source:** pull and rebuild:

  ```sh
  git pull
  cargo build --release
  ```

Your configuration and cache are stored separately from the binary (see
[Configuration](#configuration)), so updating never touches your settings. Cache
files carry a version number and are silently rebuilt from the API if the format
changes between releases - no manual migration needed.

---

## Uninstalling

1. **Remove the binary.**
   - If you installed with Cargo: `cargo uninstall launch-client`
   - Otherwise, delete the binary you placed on your `PATH`.

2. **Remove configuration and cached data** (optional). Launch Client keeps its
   data in the standard per-user directories for your platform:

   | Platform | Configuration | Cache |
   |----------|---------------|-------|
   | Linux | `~/.config/launch-client/` | `~/.cache/launch-client/` |
   | macOS | `~/Library/Application Support/launch-client/` | `~/Library/Caches/launch-client/` |
   | Windows | `%APPDATA%\launch-client\config\` | `%LOCALAPPDATA%\launch-client\cache\` |

   For example, on Linux:

   ```sh
   rm -rf ~/.config/launch-client ~/.cache/launch-client
   ```

---

## Configuration

Launch Client works out of the box with sensible defaults - no config file is
required, and one is never created automatically. To customize behaviour,
create a file named `config.toml` in the configuration directory for your
platform (see the table above).

All settings are optional; any key you omit keeps its default. Values that are
out of range or malformed are replaced with the default (and a warning is
written to the log) rather than crashing the app.

### Example `config.toml`

```toml
[api]
# Optional free API key from https://thespacedevs.com/ (via the Launch Library 2
# portal). Providing one raises your rate limit from 15 to 30 requests/hour.
# Leave empty for unauthenticated usage.
api_key = ""

[cache]
# These options allow you to adjust the cache lifetime boundaries.
# It is recommended to leave the default values unless you really need to
# fine-tune how often pages should be eligible to refresh over API.
# 
# Cache lifetimes, in minutes, chosen by how soon each launch is happening.
# Valid range for every TTL: 1–43200 (30 days).
ttl_far_future  = 1440   # launches more than 7 days away  (24 hours)
ttl_near_future = 180    # launches 1–7 days away          (3 hours)
ttl_imminent    = 30     # launches within 24 hours        (30 minutes)
ttl_active      = 1      # launches currently in progress  (1 minute)
ttl_launch_list = 30     # the upcoming-launches list      (30 minutes)

# Detail-cache pruning.
max_detail_age_days    = 30   # delete cached detail files older than this (min: 1)
max_detail_files       = 100  # keep at most this many detail files (stops unbounded disk usage)        (min: 1)
prune_every_n_startups = 5    # run pruning every N app startups        (range: 1–50)

[ui]
time_format       = "12h"       # "12h" (10:45 AM) or "24h" (10:45)
staleness_style   = "relative"  # "relative" ("Updated 5m ago") or "absolute" ("Last updated 10:40 AM")
launches_per_page = 25          # any value 1–100

[log]
level = "warn"      # "error", "warn", "info", "debug", or "trace"
file  = "app.log"   # allows you to choose the logfile name, stored in the config directory
```

### Options reference

| Section | Key | Default | Valid values | Description |
|---------|-----|---------|--------------|-------------|
| `[api]` | `api_key` | `""` | any string | Free LL2 API key. Present ⇒ 30 req/hr, absent ⇒ 15 req/hr. |
| `[cache]` | `ttl_far_future` | `1440` | 1–43200 (min) | TTL for launches more than 7 days away. |
| `[cache]` | `ttl_near_future` | `180` | 1–43200 (min) | TTL for launches 1–7 days away. |
| `[cache]` | `ttl_imminent` | `30` | 1–43200 (min) | TTL for launches within 24 hours. |
| `[cache]` | `ttl_active` | `1` | 1–43200 (min) | TTL for launches currently in progress. |
| `[cache]` | `ttl_launch_list` | `30` | 1–43200 (min) | TTL for the upcoming-launches list. |
| `[cache]` | `max_detail_age_days` | `30` | ≥ 1 | Delete cached detail files older than this many days. |
| `[cache]` | `max_detail_files` | `100` | ≥ 1 | Maximum number of cached detail files to retain. |
| `[cache]` | `prune_every_n_startups` | `5` | 1–50 | How often (in app startups) to run cache pruning. |
| `[ui]` | `time_format` | `"12h"` | `"12h"`, `"24h"` | Clock format used throughout the UI. |
| `[ui]` | `staleness_style` | `"relative"` | `"relative"`, `"absolute"` | How the "last updated" indicator is shown. |
| `[ui]` | `launches_per_page` | `25` | 1–100 | Number of launches per page in the list view. |
| `[log]` | `level` | `"warn"` | `error`/`warn`/`info`/`debug`/`trace` | Logging verbosity. |
| `[log]` | `file` | `"app.log"` | plain filename | Log filename (stored in the config dir; path separators and `..` are rejected). |

> **Note:** Past launches are cached permanently - their data is final and never
> re-fetched - so there is no TTL setting for them. Rate limits themselves are
> enforced by the API provider and are not configurable.

---

## Features in depth

### Smart proximity-based caching & offline mode

Launch Client never polls in the background. It fetches data only in response to
your actions - starting the app, opening a launch, applying filters, or pressing
`r` to refresh - and only when the cached copy has actually expired. How long
data stays fresh depends on how imminent the launch is: a launch a month away is
cached for a full day, while one that's in flight is cached for a minute. See
[Caching strategy](#caching-strategy) for the full breakdown.

The status bar always tells you how fresh the current view is
(`Updated 5m ago • Next refresh after 10:45 AM`). The manual-refresh key (`r`) is
only active once the cache is stale - you can't waste requests re-fetching data
that hasn't expired. If the network is unavailable, the app keeps working from
its cache and shows an `[OFFLINE]` marker.

### Rate-limit awareness

The Launch Library 2 API allows 15 requests per hour unauthenticated, or
30 per hour with a free API key. Launch Client tracks every request it makes
in a rolling one-hour window, persists that history between sessions, and
re-syncs with the server's `/api-throttle/` endpoint on startup (a call that
doesn't itself count against your limit). The remaining budget is shown in the
top-right of the title bar (e.g. `12/15 reqs`). If you ever hit the limit, the
app tells you exactly when requests will be available again instead of failing
silently.

To raise your limit, add a free API key to `config.toml` under `[api]`.

### The detail view

Pressing `Enter` on a launch opens a detailed view that adapts to what data the
launch actually has. Sections that don't apply are hidden entirely, so a simple
uncrewed launch stays compact while a complex crewed mission expands to show
everything:

- **Hero header** - a large T-minus countdown (color-coded by proximity), the
  launch status as a colored badge, the exact date/time, the launch window,
  weather, and probability. Failed launches show the failure reason.
- **Updates** - a reverse-chronological timeline of the most recent status
  updates (delays, pad assignments, go/no-go calls).
- **Mission** - name, type, orbit, and description, plus a crew roster for
  crewed flights and booster landing details (landing type and location per
  recoverable stage) where applicable.
- **Vehicle** - rocket name and variant, maiden-flight date, physical specs
  (length, diameter, mass, payload capacities, thrust), and a reliability
  **Record bar** - the rocket's previous launch success/failure record.
- **Provider & Location** - agency name, founding year, country, and record bar
  alongside the pad name, location, and how many launches have flown from it.
  These render side-by-side on terminals ≥ 100 columns wide and stack vertically
  below that.
- **Links** - webcasts, info pages, program names, and imagery.

Scroll the detail view with `↑`/`↓` or `j`/`k`.

### Filtering

Press `f` in the list view to open the filter panel. Cycle categories with
`Tab`, change a value with `←`/`→`, clear everything with `x`, then press `Enter`
to apply (or `Esc` to cancel). Filtering happens server-side - the app sends
your criteria to the API when you have confirmed your selection - so results
are complete rather than limited to the current page.
Available filters:

- **Status** - Go for Launch, TBD, TBC, On Hold, In Flight, …
- **Region** - US, Europe, Russia/Kazakhstan, China, India, Japan, Other
- **Crewed** - crewed only / uncrewed only
- **Date range** - next 7 / 30 / 90 days

Filters are not persisted between sessions.

### Precision-aware, dual-timezone times

Every launch time is displayed in both your local timezone and the launch
site's timezone, e.g. `Feb 28, 2026 03:00 AM CST / 09:00 AM UTC`. The display
also respects the API's precision for each launch: an exact time shows the full
timestamp, while a launch only pinned down to a month renders as `Sep 2026`, and
the live countdown is hidden when the time is too coarse to count down to.

---

## Keybindings

Press `?` at any time for a context-aware help overlay.

### List view

| Key | Action |
|-----|--------|
| `↑` / `↓`, `k` / `j` | Move selection up / down |
| `Home` / `End` | Jump to first / last launch |
| `n` / `p` | Next / previous page |
| `Enter` | Open launch details |
| `f` | Open the filter panel |
| `r` | Refresh (only when the cache is stale) |
| `?` | Toggle help |
| `q` / `Ctrl+C` | Quit |

### Detail view

| Key | Action |
|-----|--------|
| `↑` / `↓`, `k` / `j` | Scroll content |
| `Esc` | Back to the list |
| `r` | Refresh (only when the cache is stale) |
| `?` | Toggle help |
| `q` / `Ctrl+C` | Quit |

### Filter panel

| Key | Action |
|-----|--------|
| `Tab` | Next filter category |
| `←` / `→`, `h` / `l` | Change the selected value |
| `x` | Clear all filters |
| `Enter` | Apply filters (triggers a request) |
| `Esc` | Cancel and return to the list |

---

## Technical details

Launch Client is written in Rust, using [`ratatui`](https://ratatui.rs/) +
[`crossterm`](https://docs.rs/crossterm/) for the terminal UI,
[`tokio`](https://tokio.rs/) for async I/O, and
[`reqwest`](https://docs.rs/reqwest/) for HTTP. The event loop is single-threaded
and driven by `tokio::select!`, multiplexing terminal input against in-flight API
calls so the UI never blocks.

### Extensibility: adding an API provider

The application was designed so that different launch-data providers could be
slotted in without touching the UI, cache, or filtering code. This is achieved by
keeping all provider-specific knowledge behind a trait boundary.

**The layers:**

```
src/
├── api/
│   └── client.rs          # LaunchApi trait (vendor-agnostic) + Ll2Client impl
├── models.rs              # Core domain types the whole app speaks
└── vendor/
    └── launch_library_2/  # Everything specific to Launch Library 2
        ├── endpoints.rs         # Base URL, paths, query-string building
        ├── response_models.rs   # Serde structs matching LL2's JSON shapes
        ├── convert.rs           # From<Ll2…> for domain types
        ├── status_map.rs        # LL2 status IDs → name / color / style
        └── region_map.rs        # Region names → LL2 pad-location IDs
```

The rest of the app depends only on the [`LaunchApi`] trait and the domain types
in `src/models.rs` - never on any LL2-specific struct:

```rust
pub trait LaunchApi {
    fn fetch_launch_list(&self, params: &ListParams)
        -> impl Future<Output = Result<LaunchListResponse, AppError>> + Send;

    fn fetch_launch_detail(&self, id: &str)
        -> impl Future<Output = Result<LaunchDetail, AppError>> + Send;
}
```

The vendor module's `convert.rs` is where the provider's response shapes are
mapped onto the app's domain model via ordinary `From` impls
(`impl From<Ll2Launch> for LaunchSummary`, etc.). Because the TUI, cache, and
filter logic only ever see `LaunchSummary` / `LaunchDetail`, swapping providers
is a self-contained job:

1. Add a new `src/vendor/<provider>/` module with its `response_models`,
   `endpoints`, and `convert` (the `From` impls onto the domain types).
2. Provide the provider's status and region lookups (analogous to
   `status_map.rs` / `region_map.rs`).
3. Implement `LaunchApi` for a new client type (or generalize `Ll2Client`).
4. Construct that client in `main.rs`.

No changes are needed in `tui/`, `cache/`, or the filtering layer.

### Caching strategy

Caching is what makes Launch Client pleasant to use under the API's tight rate
limits. It rests on three ideas: **lazy fetching**, **proximity-based
expiry**, and **durable local state**.

**Where data lives.** Following each platform's conventions (via the
[`directories`](https://docs.rs/directories/) crate), configuration and cache are
kept separate:

```
<config_dir>/
├── config.toml            # your settings (optional)
└── app.log                # rotating log file

<cache_dir>/
├── app_state.json         # rate-limit history, startup count, prune bookkeeping
├── cache.json             # the current launch-list page (+ its filters & page number)
└── details/
    ├── <uuid>.json        # one cached detail response per launch you've opened
    └── …
```

**Lazy fetching.** The app makes a network request in only four situations:
on startup (if the list cache is missing or stale), when you open a launch whose
detail cache is missing or stale, when you apply filters, and when you explicitly
press `r` on stale data. There is no background polling and no auto-refresh -
you are always in control of when requests happen.

**Proximity-based expiry.** Instead of a single fixed TTL, each cached item
expires on a timescale matched to how soon its launch is happening. A launch far
in the future barely changes, so its data is held for a day; a launch that's
actively lifting off can change second to second, so it's held for only a minute
(under default settings):

| Tier | When it applies | Default TTL |
|------|-----------------|-------------|
| Permanent | Past launches (data is final) | never re-fetched |
| Far future | More than 7 days away | 24 hours |
| Near future | 1–7 days away | 3 hours |
| Imminent | Within 24 hours | 30 minutes |
| Active | Currently in progress | 1 minute |

(All but the permanent tier are tunable in `config.toml`.)

**Detail caches are lazy and self-limiting.** A launch's full detail is fetched
and cached only the first time you open it. To keep disk usage bounded, the
`details/` directory is pruned every few startups - files past a maximum age are
deleted, and if too many remain, the oldest are removed until the count is back
under the limit.

**Durable rate-limit state.** The rolling one-hour request history lives in
`app_state.json` and is restored on the next launch, so the app's view of your
remaining budget survives restarts. On startup it's reconciled with the server
via `/api-throttle/` (a call that doesn't count against the limit). The list
cache also records which page and filters it represents, so re-opening the app
drops you back exactly where you left off - without a network request if the data
is still fresh.

**Resilience.** Cache files are written atomically (temp file + rename) so a
crash mid-write can't corrupt them, and each carries a version number: if the
format changes in a future release, the old file is treated as a miss and
transparently rebuilt from the API rather than failing to parse.

---

## Building & development

Requires a stable Rust toolchain (see [rustup.rs](https://rustup.rs/)).

```sh
cargo build            # debug build
cargo build --release  # optimized build → target/release/launch-client
cargo test             # run the test suite
cargo clippy           # lint
```

Continuous integration runs `cargo check`, `cargo test`, and
`cargo clippy -D warnings` on every push and pull request. Tagged releases
(`v*`) trigger the release workflow, which builds and publishes the
cross-platform binaries.

---

## License

Released under the [MIT License](https://opensource.org/licenses/MIT).
