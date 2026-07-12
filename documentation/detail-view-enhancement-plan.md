# Detail View Enhancement Plan

This document describes the planned expansion of the launch detail view. It
covers new data fields to extract from the Launch Library 2 API detailed
response, how they are organised into UI sections, and the target layout with
ASCII mockups.

Companion references:
- [Design document](./launch-api-tui-design.md) -- current detail view design
- [Implementation plan](./detail-view-implementation-plan.md) -- ordered steps for building these changes
- [LL2 sample detail response](./reference/ll2-sample-detail-response.json) -- Falcon 9 / Starlink (landing, vehicle specs, updates)
- [LL2 sample crewed detail response](./reference/ll2-sample-detail-response-crewed.json) -- Soyuz MS-29 (crew, spacecraft stage)
- [LL2 API docs](./reference/launch-library-2-api-docs.md)

---

## Contents

| Lines | Section |
|------:|---------|
| 29    | [Design Goals](#design-goals) |
| 40    | [Section Overview](#section-overview) |
| 57    | [New API Fields](#new-api-fields) |
| 165   | [Section Details and Mockups](#section-details-and-mockups) |
| 360   | [Narrow Terminal Fallback](#narrow-terminal-fallback) |
| 395   | [Conditional Rendering Rules](#conditional-rendering-rules) |

---

## Design Goals

1. **Maximise horizontal space** -- most users will view the TUI on a
   landscape terminal. Pack information across the width so users scroll less.
2. **Group by domain, not by API shape** -- sections map to what the user cares
   about (this launch, this vehicle, this provider) rather than how the JSON
   nests.
3. **Progressive disclosure** -- sections that only apply to certain launches
   (crew, landing, fail reason, updates) are hidden when empty, keeping simple
   launches clean.
4. **Preserve the existing visual language** -- status-coloured badges, `█`/`░`
   record bars, section headings in `UPPER CASE`, `━━━` dark-gray separators,
   and the three-tier text hierarchy (primary / secondary / label) all carry
   over unchanged.

---

## Section Overview

The detail view is divided into seven sections rendered top-to-bottom,
separated by `━━━` horizontal rules. Sections 2 and 3 are conditionally
rendered.

| # | Section | Content | Conditional? |
|---|---------|---------|:------------:|
| 1 | **Hero** | Countdown, status, timing, probability, weather, fail reason | No |
| 2 | **Updates** | Reverse-chronological timeline of status updates | Yes -- only when updates exist |
| 3 | **Mission** | Mission name/type, orbit, description, crew, landing | No (falls back to "no mission info") |
| 4 | **Vehicle** | Rocket name/variant, physical specs, vehicle record bar | No |
| 5 | **Provider** | Name, type, founding year, country, provider record bar | No (two-column with Location) |
| 6 | **Location** | Pad name, location, country, pad launch count | No (two-column with Provider) |
| 7 | **Links** | Webcasts, info URLs, programs, image URL | Yes -- only when any links exist |

Provider and Location are rendered side-by-side in a two-column layout when
the terminal is >= 100 columns wide, and stacked vertically below that
threshold. All other sections are always full-width.

---

## New API Fields

Below are the new fields to extract from the LL2 `/launch/{id}/` detailed
response. Fields already in use by the current detail view are not listed.

### Hero -- fail reason

| Domain field | API source path | Type | Notes |
|---|---|---|---|
| `failreason` | `failreason` | `Option<String>` | Empty string in API when absent; treat `""` as `None` |

### Updates -- status timeline

Source: `updates[]` array on the launch root object.

| Domain field | API source path | Type | Notes |
|---|---|---|---|
| `updates` | `updates` | `Vec<LaunchUpdate>` | Reverse-chronological; cap display at 5 most recent |

Each `LaunchUpdate`:

| Field | API source path | Type |
|---|---|---|
| `comment` | `updates[].comment` | `String` |
| `created_on` | `updates[].created_on` | `DateTime<Utc>` |
| `info_url` | `updates[].info_url` | `Option<String>` |

We intentionally skip `profile_image` and `created_by` -- the author avatar
is not renderable in a TUI and the author name adds little value for the end
user. The `info_url` is stored but not displayed initially (it is a URL and
not easily digestible in a TUI); it is retained in the domain model in case a
future enhancement wants to surface it.

### Mission -- crew

Source: `rocket.spacecraft_stage[].launch_crew[]` (only present on crewed
flights). The crew lists live on the `spacecraft_stage` object itself, **not**
nested under `spacecraft`. Three separate arrays exist -- `launch_crew`,
`onboard_crew`, and `landing_crew` -- we use `launch_crew` as it represents
the crew assigned to the launch.

| Domain field | API source path | Type | Notes |
|---|---|---|---|
| `crew` | `rocket.spacecraft_stage[0].launch_crew` | `Vec<CrewMember>` | Empty vec for uncrewed flights |

Each `CrewMember`:

| Field | API source path | Type | Notes |
|---|---|---|---|
| `name` | `.astronaut.name` | `String` | |
| `role` | `.role.role` | `String` | Note: the field is `role.role`, not `role.name` |
| `agency` | `.astronaut.agency.name` | `String` | Full name can be long (e.g. "National Aeronautics and Space Administration"); consider using `.astronaut.agency.abbrev` (e.g. "NASA", "RFSA") for the TUI display column |

**Verified** against live Soyuz MS-29 response (`a556b963`). Key
observations:
- `spacecraft_stage` is an array (empty `[]` for uncrewed flights, not
  `null`). For crewed flights it typically has one entry.
- Crew is on the stage object, not under `spacecraft`. The `spacecraft`
  object has no `crew` key at all.
- `role` is an object `{ id, role, priority }` -- the display string is
  in the `role` field (not `name`).
- Agency names can be verbose; `.abbrev` is available as a compact
  alternative for the three-column crew table.

### Mission -- landing

Source: `rocket.launcher_stage[].landing` (one entry per recoverable stage).

| Domain field | API source path | Type | Notes |
|---|---|---|---|
| `landings` | `rocket.launcher_stage` | `Vec<StageLanding>` | Empty vec when no recovery is attempted |

Each `StageLanding`:

| Field | API source path | Type | Notes |
|---|---|---|---|
| `stage_type` | `.type` | `Option<String>` | e.g. "Core", "Side Booster" |
| `landing_attempt` | `.landing.attempt` | `bool` | |
| `landing_success` | `.landing.success` | `Option<bool>` | `None` before landing occurs |
| `landing_type` | `.landing.type.abbrev` | `Option<String>` | e.g. "RTLS", "ASDS", "Parachute" |
| `landing_location` | `.landing.landing_location.name` | `Option<String>` | e.g. "LZ-1", "OCISLY"; can be `null` even when attempt is true |

**Verified** against live Falcon 9 response (`f6c9396f`). The
`launcher_stage` array, `landing` object, `landing.type`, and
`landing.landing_location` shapes are confirmed. Note: `landing_location`
can be `null` for some landings (e.g. new or unassigned drone ships), and
`landing.type` can also be `null` before landing details are finalised.
Both must be handled gracefully in the landing row display.

Both crew and landing structures have now been verified against live API
responses (Soyuz MS-29 for crew, Falcon 9 Starlink for landing).

### Vehicle -- expanded rocket configuration

Source: `rocket.configuration` object.

| Domain field | API source path | Type | Notes |
|---|---|---|---|
| `rocket_variant` | `rocket.configuration.variant` | `Option<String>` | e.g. "Block 5", "8K74PS" |
| `rocket_description` | `rocket.configuration.description` | `Option<String>` | Not displayed; stored for potential future use |
| `rocket_length` | `rocket.configuration.length` | `Option<f64>` | Metres |
| `rocket_diameter` | `rocket.configuration.diameter` | `Option<f64>` | Metres |
| `rocket_launch_mass` | `rocket.configuration.launch_mass` | `Option<f64>` | Tonnes (t); API returns float -- truncate to integer for display |
| `rocket_leo_capacity` | `rocket.configuration.leo_capacity` | `Option<f64>` | Kilograms; API returns float -- truncate to integer for display |
| `rocket_gto_capacity` | `rocket.configuration.gto_capacity` | `Option<f64>` | Kilograms; API returns float -- truncate to integer for display |
| `rocket_thrust` | `rocket.configuration.to_thrust` | `Option<f64>` | Kilonewtons (kN); API returns float -- truncate to integer for display |
| `rocket_maiden_flight` | `rocket.configuration.maiden_flight` | `Option<String>` | Date string `"YYYY-MM-DD"` |
| `rocket_total_launches` | `rocket.configuration.total_launch_count` | `Option<u32>` | |
| `rocket_successful_launches` | `rocket.configuration.successful_launches` | `Option<u32>` | |
| `rocket_failed_launches` | `rocket.configuration.failed_launches` | `Option<u32>` | |
| `rocket_consecutive_successes` | `rocket.configuration.consecutive_successful_launches` | `Option<u32>` | |

### Provider -- expanded agency details

Source: `launch_service_provider` object.

| Domain field | API source path | Type | Notes |
|---|---|---|---|
| `provider_country_code` | `launch_service_provider.country[0].alpha_3_code` | `Option<String>` | e.g. "USA", "NZL"; `country` is an array of objects -- take first entry's `alpha_3_code`. `None` when array is empty |
| `provider_founding_year` | `launch_service_provider.founding_year` | `Option<u32>` | API returns integer (e.g. `2002`), not string |
| `provider_consecutive_successes` | `launch_service_provider.consecutive_successful_launches` | `Option<u32>` | |

### Location -- pad launch count

Source: `pad` object.

| Domain field | API source path | Type | Notes |
|---|---|---|---|
| `pad_total_launch_count` | `pad.total_launch_count` | `Option<u32>` | |

---

## Section Details and Mockups

All mockups below assume a terminal width of ~100 columns.

### 1. Hero

Unchanged from the current design, with one addition: **fail reason** is
displayed below weather concerns when the launch has failed.

```
╭─ Starship IFT-7 ─────────────────────────────────────── 11/15 reqs ─╮
│                                                                       │
│   ██  T-01:14:23:45  ██     Go for Launch                            │
│   Feb 28, 2026 03:00 AM CST / 09:00 AM UTC                          │
│   Window: 09:00 - 12:00 UTC  ·  Probability: 90%                    │
│   Weather: No concerns                                                │
│                                                                       │
```

When a launch has failed, the fail reason appears styled as a warning:

```
│   ██  T+00:00:02:33  ██     Launch Failure                           │
│   Mar 15, 2026 14:00 UTC                                             │
│   Failure: Second stage engine failed to ignite after stage           │
│   separation.                                                         │
│                                                                       │
```

### 2. Updates (conditional)

Shown only when the API returns a non-empty `updates` array. Displays the
5 most recent updates in reverse-chronological order. Each entry shows the
timestamp as a styled label followed by the comment text, word-wrapped to
the available width.

```
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │
│                                                                       │
│   UPDATES                                                             │
│                                                                       │
│   Mar 21, 02:31 UTC                                                  │
│   Delayed to March 25.                                                │
│                                                                       │
│   Mar 20, 19:08 UTC                                                  │
│   Launch pad assigned.                                                │
│                                                                       │
│   Mar 19, 10:41 UTC                                                  │
│   Added daily launch window.                                          │
│                                                                       │
│   Feb 19, 14:31 UTC                                                  │
│   NET March 24.                                                       │
│                                                                       │
│   Jan 15, 21:47 UTC                                                  │
│   Added mission name.                                                 │
│                                                                       │
```

Timestamps are styled as Tier 3 (label). Comment text is Tier 2 (secondary).
A blank line separates each entry.

### 3. Mission

Contains the core mission information (unchanged from current) plus two new
conditional sub-sections: crew and landing.

```
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │
│                                                                       │
│   MISSION                                                             │
│   Crew-10 (Crew Rotation)                                             │
│   Orbit: Low Earth Orbit (LEO)                                        │
│                                                                       │
│   Fourth operational mission of the SpaceX Crew Dragon spacecraft     │
│   to the International Space Station as part of NASA's Commercial     │
│   Crew Program.                                                       │
│                                                                       │
│   CREW                                                                │
│   Anne McClain        Commander         NASA                          │
│   Akihiko Hoshide     Pilot             JAXA                          │
│   Thomas Pesquet      Mission Spc.      ESA                           │
│   Megan McArthur      Mission Spc.      NASA                          │
│                                                                       │
│   LANDING                                                             │
│   Booster   RTLS   Landing Zone 1                                     │
│                                                                       │
```

For missions with multiple recoverable stages (e.g. Falcon Heavy):

```
│   LANDING                                                             │
│   Core        ASDS   Of Course I Still Love You                       │
│   Side Booster RTLS   Landing Zone 1                                  │
│   Side Booster RTLS   Landing Zone 2                                  │
│                                                                       │
```

The CREW sub-section is only rendered when the crew list is non-empty. The
LANDING sub-section is only rendered when at least one stage has
`landing_attempt: true`. Within each landing row, the columns are:
stage type, landing type abbreviation, and landing location name.

### 4. Vehicle

Expanded from the current view (which shows only the rocket full name) to
include physical specifications and a vehicle-specific record bar.

The specs use an internal two-column key-value layout to maximise horizontal
space usage. Specs keys on the left, payload capacities on the right.

```
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │
│                                                                       │
│   VEHICLE                                                             │
│   Falcon 9 Block 5                                                    │
│   Maiden flight: Jun 4, 2010                                          │
│                                                                       │
│   Length: 70 m                  LEO capacity: 22,800 kg               │
│   Diameter: 3.7 m              GTO capacity: 8,300 kg                │
│   Launch mass: 549 t           Thrust: 7,607 kN                      │
│                                                                       │
│   ████████████████████ 98%                                            │
│   301 launches (295 ok, 6 fail) · 42 consecutive                     │
│                                                                       │
```

The rocket name line combines `rocket_full_name` and `rocket_variant`. If
variant is already part of the full name (as it often is) it is not
duplicated. Format: `"{full_name}"` or `"{full_name} {variant}"` when variant
adds information.

The maiden flight date is formatted as `"Mon DD, YYYY"`.

The specs grid only renders rows where the API returned data. If all spec
fields are null (as with some historical or obscure rockets), the specs block
is omitted entirely and the section shows just the name + record bar.

The record bar is identical in style to the existing provider record bar.
The `consecutive` count is appended after the stats line when available.

### 5 & 6. Provider | Location (two-column)

Provider and Location are rendered side-by-side when the terminal is >= 100
columns wide. Provider sits on the left, Location on the right.

Provider is expanded with founding year and country code. The provider record
bar (already implemented) stays here. A consecutive-successes count is
appended to the stats line.

Location gains the pad launch count.

```
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │
│                                                                       │
│   PROVIDER                      │  LOCATION                          │
│   Rocket Lab (Commercial)       │  Rocket Lab Launch Complex 1A      │
│   Founded 2006 · NZL            │  Mahia Peninsula, New Zealand      │
│                                 │  38 launches from this pad          │
│   ████████████████████░ 95%     │                                     │
│   84 launches (80 ok, 4 fail)   │                                     │
│   43 consecutive                │                                     │
│                                                                       │
```

The `"Founded YYYY · CC"` line uses the founding year and the country code
from the API. Both are styled as Tier 2 (secondary).

The `"N launches from this pad"` line under Location uses `pad_total_launch_count`
and is styled as Tier 3 (label).

The `"N consecutive"` line under the provider record bar is styled as Tier 3
(label) and is only shown when the consecutive count is > 0.

### 7. Links

Unchanged from the current design.

```
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │
│                                                                       │
│   LINKS                                                               │
│   Webcast    https://www.youtube.com/watch?v=...                      │
│   Info       https://www.spacex.com/launches/mission-7/               │
│   Program    Starship Development                                     │
│   Image      https://thespacedevs-prod.nyc3.digitalocean...           │
│                                                                       │
├───────────────────────────────────────────────────────────────────────┤
│ Updated 2m ago · Next refresh after 10:07 AM                          │
│ Esc: Back · ↑/↓: Scroll · r: Refresh (when stale) · ?: Help         │
╰───────────────────────────────────────────────────────────────────────╯
```

---

## Full Page Mockup

A complete detail view for a crewed mission with updates, demonstrating all
sections together:

```
╭─ Crew-10 ─────────────────────────────────────────────── 11/15 reqs ─╮
│                                                                       │
│   ██  T-01:14:23:45  ██     Go for Launch                            │
│   Mar 14, 2026 04:30 AM EST / 09:30 AM UTC                          │
│   Window: 09:30 - 13:30 UTC  ·  Probability: 95%                    │
│   Weather: No concerns                                                │
│                                                                       │
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │
│                                                                       │
│   UPDATES                                                             │
│                                                                       │
│   Mar 13, 18:00 UTC                                                  │
│   GO for launch. Crew walkout scheduled for 05:45 UTC.               │
│                                                                       │
│   Mar 12, 14:22 UTC                                                  │
│   Flight readiness review complete. All systems nominal.              │
│                                                                       │
│   Mar 10, 09:00 UTC                                                  │
│   Static fire test completed successfully.                            │
│                                                                       │
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │
│                                                                       │
│   MISSION                                                             │
│   Crew-10 (Crew Rotation)                                             │
│   Orbit: Low Earth Orbit (LEO)                                        │
│                                                                       │
│   Tenth operational crew rotation mission to the International        │
│   Space Station under NASA's Commercial Crew Program.                 │
│                                                                       │
│   CREW                                                                │
│   Anne McClain        Commander         NASA                          │
│   Akihiko Hoshide     Pilot             JAXA                          │
│   Thomas Pesquet      Mission Spc.      ESA                           │
│   Megan McArthur      Mission Spc.      NASA                          │
│                                                                       │
│   LANDING                                                             │
│   Booster   RTLS   Landing Zone 1                                     │
│                                                                       │
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │
│                                                                       │
│   VEHICLE                                                             │
│   Falcon 9 Block 5                                                    │
│   Maiden flight: Jun 4, 2010                                          │
│                                                                       │
│   Length: 70 m                  LEO capacity: 22,800 kg               │
│   Diameter: 3.7 m              GTO capacity: 8,300 kg                │
│   Launch mass: 549 t           Thrust: 7,607 kN                      │
│                                                                       │
│   ████████████████████ 98%                                            │
│   301 launches (295 ok, 6 fail) · 42 consecutive                     │
│                                                                       │
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │
│                                                                       │
│   PROVIDER                      │  LOCATION                          │
│   SpaceX (Commercial)           │  LC-39A                             │
│   Founded 2002 · USA            │  KSC, Florida, United States       │
│                                 │  223 launches from this pad         │
│   ████████████████████ 98%      │                                     │
│   301 launches (295 ok, 6 fail) │                                     │
│   42 consecutive                │                                     │
│                                                                       │
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │
│                                                                       │
│   LINKS                                                               │
│   Webcast    https://www.youtube.com/watch?v=...                      │
│   Info       https://www.nasa.gov/crew-10/                            ��
│   Program    Commercial Crew Program                                  │
│   Image      https://thespacedevs-prod.nyc3.digitalocean...           │
│                                                                       │
├───────────────────────────────────────────────────────────────────────┤
│ Updated 2m ago · Next refresh after 10:07 AM                          │
│ Esc: Back · ↑/↓: Scroll · r: Refresh (when stale) · ?: Help         │
╰───────────────────────────────────────────────────────────────────────╯
```

---

## Narrow Terminal Fallback

Below 100 columns, the two-column Provider | Location section falls back to
single-column stacked layout (Provider first, then Location), consistent with
the existing narrow-terminal behaviour.

The vehicle specs grid also falls back to a single column:

```
│   Length: 70 m                                                        │
│   Diameter: 3.7 m                                                    │
│   Launch mass: 549 t                                                 │
│   LEO capacity: 22,800 kg                                            │
│   GTO capacity: 8,300 kg                                             │
│   Thrust: 7,607 kN                                                   │
```

The crew table retains its three-column alignment but columns may be
truncated on very narrow terminals. Minimum viable width remains 80 columns
per the existing design.

All other sections are already full-width and need no fallback changes.

---

## Conditional Rendering Rules

| Element | Condition to render |
|---|---|
| Fail reason (hero) | `failreason` is `Some` and non-empty |
| Updates section | `updates` vec is non-empty |
| Crew sub-section | `crew` vec is non-empty |
| Landing sub-section | At least one entry in `landings` has `landing_attempt == true` |
| Landing row columns | `landing_type` and `landing_location` are each independently optional; omit the column value (don't show "Unknown") when `None` |
| Vehicle specs grid | At least one spec field (`length`, `diameter`, `launch_mass`, `leo_capacity`, `gto_capacity`, `thrust`) is `Some` |
| Vehicle maiden flight | `rocket_maiden_flight` is `Some` |
| Vehicle record bar | `rocket_total_launches` is `Some` and > 0 |
| Vehicle consecutive | `rocket_consecutive_successes` is `Some` and > 0 |
| Provider founded line | Either `provider_founding_year` or `provider_country_code` is `Some` |
| Provider consecutive | `provider_consecutive_successes` is `Some` and > 0 |
| Pad launch count | `pad_total_launch_count` is `Some` and > 0 |
| Links section | At least one of: `vid_urls`, `info_urls`, `programs`, or `image_url` is non-empty/present |

When a conditional element is hidden, its space is reclaimed -- no blank
placeholder is left behind. This means a simple uncrewed expendable launch
with no updates will render a compact detail view (Hero, Mission, Vehicle,
Provider | Location, Links) while a complex crewed mission will expand to
show everything.

### General nullability notes

Most new fields are wrapped in `Option` because the LL2 API can return
`null` for any of them depending on how complete the launch record is.
Launches that are far in the future or from less-tracked providers are
especially likely to have sparse data. The conditional rendering rules
above already cover section- and element-level visibility, but within a
rendered section, individual missing values should simply be omitted from
the display (no "N/A" or "Unknown" placeholders) so the layout stays
clean. The one exception is the vehicle name line, which always renders
`rocket_full_name` (a required field) even when `variant` is `None`.
