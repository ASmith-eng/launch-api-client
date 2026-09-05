# Region Filter Hardening — Design Plan

Status: **IMPLEMENTED — §5.1–5.3 and §5.5 shipped; §5.4 dropped; region set
revised in §9, which resolves §8.2**

The region filter silently returned unfiltered results for its entire life. The
immediate fix is in the working tree; this document covers what we do so that
the *next* change on LL2's side is loud instead of silent. Like the other plans
in this directory, it is the contract we agree on before writing code.

---

## 1. Goal & scope

- **In scope:** removing hand-transcribed LL2 location IDs from the codebase,
  giving a region with no registered sites an honest UI state, and adding the
  one test that can actually detect a filter silently becoming a no-op.
- **Out of scope:** any runtime location discovery, an in-app location browser,
  a multi-provider abstraction, or changes to the filter panel UI beyond a
  status-bar count (§5.5) and one empty-state message (§5.3).

No new dependencies, and no additional API calls in the normal run path.

---

## 2. What broke

Two independent defects, and keeping them separate is what drives the rest of
this plan.

**D1 — wrong parameter name.** `endpoints.rs` emitted `pad__location__in`,
which is not a filter LL2 recognises. Django REST Framework **silently ignores
unknown query parameters** rather than rejecting them, so the API returned all
193 upcoming launches on every request regardless of the selected region.

**D2 — wrong location IDs.** `region_map.rs` carried IDs that were largely
invalid: several didn't exist at all (the table only has 68 rows, and we
referenced `184`, `188`, `199`), and `25` — Pacific Spaceport Complex, Alaska —
was filed under China. The comments naming each site were shifted relative to
the IDs they annotated, so `12` was labelled Kennedy when it is Cape Canaveral.

D2 was invisible because D1 meant the IDs were never sent anywhere that would
evaluate them.

Both are fixed in the working tree: the parameter is now `location__ids`, the
`ListParams` field renamed to match, and the region table rebuilt from the live
`/locations/` endpoint filtered to `active` sites. Verified end-to-end — Europe
returns 23 launches against 193 unfiltered, including Andøya and Esrange, which
the old table could not have matched under any parameter name.

**The distinction that matters:** runtime ID discovery would have caught D2 and
been completely blind to D1. D1 is the one that broke the feature.

**The shape to remember:** D1's symptom was a filter that quietly became no
filter at all. §5.3 exists because there is a second, subtler path to that same
symptom still open in the code.

---

## 3. Probe results — the LL2 launch filterset

Two requests to the dev mirror settled every open question. The browsable API
at `/2.3.0/launches/upcoming/` renders the complete django-filter form, so a
single fetch enumerates every valid parameter rather than guessing one at a
time. Worth remembering as a technique — it is far cheaper than probing names
individually, and there is no OpenAPI schema at the usual paths.

| Parameter | Status | Notes |
|---|---|---|
| `location__ids` | valid | Helptext: *"Comma-separated location IDs."* Now shipped. |
| `status__ids` | valid | Comma-separated. The status filter was never broken. |
| `is_crewed` | valid | Three-state `unknown`/`true`/`false`; we send the latter two. |
| `net__gt` / `net__lt` | valid | Date-range filter is sound. |
| `pad__location` | valid | Single-value only — `pad__location=13,33` returns a validation error. Not usable for regions. |
| country / region | **absent** | No country filter exists in any form. |
| `include_suborbital` | unexamined | Three-state, previously unknown to us. See §6.1. |

The absent country filter is the finding that shapes §4. Locations carry ISO
3166 alpha-2 codes (`country.alpha_2_code`, modelled as
`Ll2Location.country: Option<Ll2Country>` in `response_models.rs`), and 23
distinct countries are represented — but launches cannot be filtered by them.
Numeric location IDs are the only geographic handle LL2 offers, so we cannot
delete the ID table. We have to own it properly instead.

Note the granularity: `location__ids` filters by *site*, not by pad. Cape
Canaveral SFS is a single ID covering all its pads. A region can therefore
never be narrower than a whole site, which is fine for every region we define.

---

## 4. Design principle — put the human on the right half

Today the code hardcodes the volatile external facts and buries the stable
editorial judgement. That is backwards.

- *"Is French Guiana part of Europe?"* is a product decision. It never changes
  unless we change it.
- *"What is location 25?"* is a row in someone else's database. It moved
  without telling us, and it will move again.

The seam belongs between those two:

| Input | Owner | Volatility |
|---|---|---|
| Region → ISO country codes | us, hand-written | stable indefinitely |
| Location ID, name, country, active flag | LL2, fetched | changes a few times a year |

**These are generator inputs, not runtime layers.** The join happens once,
offline, in `tools/`. The only thing that ships is a direct region → location
ID table — no country codes reach the binary, and the app performs exactly one
lookup, as it does today. A new Norwegian spaceport joins Europe on the next
regeneration without anyone having to notice it needed to.

The same line separates what is ours from what is a vendor's, which is the only
concession this plan makes to a possible second data provider: regions and
their country sets are provider-neutral and live outside `src/vendor/`;
location IDs are LL2's and live inside it. No trait, no abstraction — just not
letting `location__ids` leak upward into the TUI.

### Constraints

**R1 — no additional requests in the normal run path.** LL2 is volunteer-run
and we already treat its throttle endpoint as the sole rate-limit authority.
Region data must not cost a request per app launch. This rules out startup
discovery as the primary mechanism.

**R2 — generated output is committed.** The build must work offline and the
diff must be reviewable. Generation is a maintainer action, not a build step
and not a CI step.

**R3 — the editorial table stays hand-written.** Its whole value is that a
person decided it. Deriving regions from geography automatically would just
relocate the judgement somewhere less visible.

---

## 5. Plan

Ordered by dependency. §5.3, §5.4 and §5.5 address the D1 failure *shape* and
are independently valuable even if the generator in §5.2 never gets written.

### 5.1 — Editorial region table

`tools/launch_library_2/region-map/regions.toml`, checked in and reviewed by a human. Each entry
carries the `RegionFilter` variant name, its display label, and the ISO alpha-2
codes that constitute it:

```toml
[[region]]
variant   = "Europe"
label     = "Europe"
# French Guiana hosts ESA's spaceport at Kourou — the launches are European
# even though the territory is in South America.
countries = ["GF", "SE", "NO", "GB", "ES"]

[[region]]
variant   = "RussiaKazakhstan"
label     = "Russia/Kaz."
countries = ["RU", "KZ"]
```

…and similarly for `US` (`US`), `China` (`CN`), `India` (`IN`), `Japan` (`JP`),
`NewZealand` (`NZ`).

Europe is the only entry requiring a real decision, and the reasoning sits in a
comment beside it — exactly the kind of rationale the code cannot express on
its own.

Carrying `variant` here is what lets the generator emit an exhaustive `match`
(§5.2). This file lives in `tools/` rather than `src/vendor/` because regions
are ours, not LL2's.

### 5.2 — Generator in `tools/launch_library_2/region-map/`

Follows the `tools/splash` precedent — a stdlib-only Python script with its
input beside it:

```
tools/launch_library_2/region-map/
  build_region_map.py     # fetch, join, emit
  regions.toml            # §5.1 — the editorial input
  locations.json          # trimmed snapshot of LL2's location list
  README.md               # regeneration steps (absorbs the old §5.5)
```

The script fetches `/locations/?limit=250` once, filters to `active`, groups by
`country.alpha_2_code` through the §5.1 table, and emits the site name as a
comment on every ID. One request per regeneration, zero at runtime.

**Changed from the agreed design:** a committed `locations.json` snapshot
replaces the planned `locations.sha256`. A hash can only say *that* something
moved; the snapshot says *what*, in a reviewable diff. It also decouples the
three modes — `--fetch` refreshes the snapshot from LL2, the default rebuilds
from it, and `--check` verifies the committed table against it. Only `--fetch`
touches the network, so regeneration after a `regions.toml` edit and CI
verification are both offline and deterministic. The payload is trimmed to
`id`, `name`, `active` and `country`; the raw response is ~150 KB of
descriptions, images and launch tallies that churn constantly.

The generator emits `label()` alongside `location_ids()`, so a region's display
name lives in `regions.toml` next to its countries rather than in a parallel
hand-written match in `filter.rs`.

Output is padded to match rustfmt's comment alignment, so `cargo fmt` is a
no-op on the generated file and `--check` does not fight it.

The `active` filter is load-bearing: it excludes Svobodny (superseded by
Vostochny) and the French Algeria test centre, neither of which can appear in
upcoming launches.

**Output layout.** The generated table lands inside `src/vendor/` beside the
code that consumes it, in its own module so that regeneration cannot clobber
hand-written prose:

```
src/vendor/launch_library_2/
  region_map.rs             # hand-written: resolution logic, module docs, tests
  region_map/table.rs       # GENERATED — data only, "do not edit" header
```

Edition 2021 permits `region_map.rs` alongside `region_map/`, so the generated
file is unmistakably separate while `region_map::table` still reads as one
thing. The name is internal — nothing outside `region_map.rs` imports it — so
it is cheap to change later if `data` or similar reads better in practice.

**The emitted shape** is a `match` on `RegionFilter`, not a searchable list:

```rust
// GENERATED by tools/launch_library_2/region-map/build_region_map.py — do not edit.
pub fn location_ids(region: RegionFilter) -> &'static [u32] {
    match region {
        RegionFilter::All => &[],
        RegionFilter::US => &[
            12,  // Cape Canaveral SFS, FL
            // …
        ],
        // …
    }
}
```

This replaces today's `RegionFilter::US → "US" → find_region()` string lookup
in `filter.rs`. A variant the generator doesn't know about becomes a
non-exhaustive-match **compile error** rather than a runtime `None` that
silently disables one region — which lets `every_region_variant_resolves_to_ids`
be deleted (§5.4).

**Null-country rows must be emitted visibly.** The three locations with no
country (§8) belong to no region and are dropped. The generator writes them
into the file header as a commented list with a count, so the one category of
location that belongs nowhere is legible in the diff. Silently dropping them
would be the same class of failure as D2.

**Empty regions are reported, not fatal.** If a region resolves to no active
sites, the generator prints a warning naming it and emits the empty arm
anyway — the runtime handles it as a real state (§5.3). A provider having no
sites in a region is that provider's fact, not our bug.

### 5.3 — A region with no sites is a third state

`RegionFilter::location_ids()` returns `Option<String>` today, which conflates
*"no filter selected"* with *"filter selected, but it resolves to nothing."*
That conflation is D1's exact failure shape: an empty region falling through to
"send no parameter" would show all 193 launches under a region the user
explicitly chose.

Three states instead, defined in `tui/filter.rs` — provider-neutral, so the TUI
never learns what a location ID is:

```rust
/// Outcome of resolving a region against a data provider's launch sites.
pub enum RegionSites {
    /// No region filter applied.
    Unfiltered,
    /// Comma-separated provider site IDs.
    Sites(String),
    /// Region is known to us; the provider registers no active sites in it.
    NoneRegistered,
}
```

Resolution stays hand-written in `region_map.rs`, wrapping the generated match
so that the empty slice is only ever interpreted *after* `All` is handled:

```rust
pub fn resolve(region: RegionFilter) -> RegionSites {
    if region == RegionFilter::All {
        return RegionSites::Unfiltered;
    }
    match table::location_ids(region) {
        [] => RegionSites::NoneRegistered,
        ids => RegionSites::Sites(join_csv(ids)),
    }
}
```

**`NoneRegistered` must skip the fetch entirely.** No request goes out; the
list view shows a message stating that the data provider registers no launch
sites in this region. `list.rs:36` already renders `"No launches to display."`
for an empty list, so this is a second variant of an existing path rather than
new machinery. Wording should attribute the absence to the provider rather than
implying a failure on our side.

### 5.4 — Contract check: dropped from the crate, kept in the README

**Not implemented as a test.** The planned `#[ignore]`d contract tests would
have lived in `src/` while only ever running during a regeneration — test code
that `cargo test` never executes, carried by everyone who builds the crate. The
check itself still matters, so it moved to `tools/launch_library_2/region-map/README.md` as two
`curl` commands that compare a filtered count against an unfiltered one, next
to the procedure that actually calls for it.

This is the one thing here that no automated check covers. Detecting "LL2
stopped honouring our parameter" requires asking LL2, and nothing in the repo
does that on its own. The trade is deliberate: the failure is loud in the app
(§5.5) and cheap to confirm by hand, and a scheduled job against a
volunteer-run API needs a reason plus someone watching it go red.

The existing unit tests could not have caught D1 either. `endpoints.rs` asserts
the built URL *contains* the string the builder emits, which passes for any
parameter name including a fictional one. The test encoded the bug and went
green for it.

What *is* enforced offline:

- The generated `match` makes a missing `RegionFilter` variant a compile error.
  `every_region_variant_resolves_to_ids` was deleted along with the
  stringly-typed lookup it guarded.
- `every_region_resolves_to_at_least_one_site` fails if a regeneration empties
  one of our regions.
- `regions_do_not_share_locations` fails if two regions claim the same site.
- `build_region_map.py --check` fails if `table.rs` has drifted from
  `regions.toml` and `locations.json`.

### 5.5 — Show the filter's effect in the status bar

**Already present, no change needed.** `list.rs` renders
`Showing {launches.len()} of {total_count}` in the title bar, with a cyan
`[Filtered]` tag beside it whenever any filter is non-default. A silently
ignored region filter therefore shows `[Filtered]` next to the full unfiltered
total — which is the tell this section asked for.

Verified rather than rebuilt; adding a second count display would have
duplicated it.

---

## 6. Open questions

### 6.1 — Are suborbital flights included by default? *(not a blocker)*

One request settles it. This no longer affects the generator, which emits every
active site LL2 reports regardless — it only determines whether
`include_suborbital` is worth exposing as a fifth filter category, and whether
New Shepard flights from Corn Ranch (`29`) are visible at all. That is a
separate feature, sequenced after this work.

### 6.2 — Is a long-TTL runtime refresh ever worth adding?

Deferred, not rejected. It only pays off if we ship releases more slowly than
LL2 adds launch sites. Revisit if the generator's output starts going stale
between releases; the existing `CacheManager` would host it cleanly.

---

## 7. Explicitly not doing

- **A location-sync subsystem or in-app location browser.** Sixty-eight rows
  that change a handful of times a year do not justify it.
- **Startup discovery as the primary source of region data.** Violates R1, and
  adds a failure path where the filter panel breaks when LL2 is down. We would
  need the vendored fallback anyway, so the fallback should be the source of
  truth.
- **Inferring a broken filter at runtime by comparing counts.** Too clever, and
  it would misfire on legitimately empty regions — which are now a supported
  state (§5.3) rather than an anomaly.
- **Curating the low-volume US sites.** White Sands (`155`), Spaceport America
  (`31`), Edwards (`162`) and PMRF (`1`) stay in the US region because LL2 lists
  them as active US sites. Trimming them by hand would reintroduce exactly the
  per-ID human judgement §4 removes. They cost nothing in a comma-separated
  list.
- **A provider abstraction.** §4's naming discipline is the whole concession to
  a future second provider. A trait with one implementor would be speculation.

---

## 8. Known behavioural consequences

Both are noted in the `region_map.rs` module doc and re-emitted in the
generated file's header, so they stay visible in the diff (§5.2).

### 8.1 — Sites with no country

"Sea Launch", "Air launch to orbit" and "Air launch to Suborbital flight"
belong to no region, so air- and sea-launched missions are absent from every
region filter and visible only under "All". Defensible, but a real difference
from the unfiltered list and worth a line in the help screen if users trip
over it.

**Correction to the original §3 finding:** these three are not `country: null`
as recorded. They carry a placeholder country whose `alpha_2_code` is literally
`"??"` and whose name is "Unknown". The 15 genuinely null-country rows are
lunar *landing* sites (Mare Tranquillitatis, Taurus–Littrow, Malapert-A…),
which are not launch sites at all. The generator treats both as unregionable,
which is correct for both reasons, but the mechanism matters for anyone reading
the API.

### 8.2 — Twelve active launch sites were in no region at all *(resolved, §9)*

The first regeneration surfaced this. These countries were active in LL2 and
claimed by no region, so their launches appeared only under "All":

| Country | Sites |
|---|---|
| Australia | Woomera, Whalers Way, Koonibba, Bowen |
| Iran | Semnan, Shahrud |
| Brazil | Alcântara |
| Israel | Palmachim |
| North Korea | Sohae |
| South Korea | Naro |
| Marshall Islands | Kwajalein |
| Oman | Etlaq |

Not a defect — the seven regions were chosen before this was measurable, and
the region list is a product decision. But Australia had four active sites and
the filter offered no way to see them. §9 makes the decision.

---

## 9. Revised region set

The §5 machinery held; only `regions.toml` and the enum changed, plus one
generator feature. Verified against the live upcoming list: 186 launches, all
186 reachable through exactly one region, none orphaned.

### 9.1 — Why not continents

The obvious revision is North/South America, Europe, Asia, Oceania. Rejected on
three counts, and they are worth recording because the question will come back:

1. **It is not the convention.** Launch activity is tabulated by *launching
   state* — FAA AST's compendia, the Space Foundation's *Space Report*,
   McDowell's GCAT and Gunter's Space Page all break out US / China / Russia /
   Europe / India / Japan / other. The one supranational bucket everybody
   accepts is Europe, and it exists precisely because Kourou is in South
   America. The closest thing to a formal notion is the UN treaties' "launching
   State", which is national.
2. **Russia straddles the line.** Plesetsk, Kapustin Yar and Dombarovskiy are
   in European Russia, Vostochny is in Asia, Baikonur is in Kazakhstan. A
   continental cut scatters one programme across three buckets, and the
   generator joins on *country*, so expressing it would need per-site
   overrides — reintroducing exactly the hand-curated ID judgement §4 removed.
3. **The buckets come out lopsided.** "Asia" would hold China, Japan, India,
   Iran, Israel, both Koreas, Kazakhstan and Oman — collapsing the distinction
   the filter exists to make — while "South America" held Alcântara alone.

### 9.2 — The set

Named regions for the major programmes, geographic ones for the tail. No arm is
named a superset of another, so *"Japan isn't in Asia?"* never arises.

| Region | Label | Countries | Sites | Upcoming |
|---|---|---|---|---|
| `US` | US | US | 11 | 114 |
| `Europe` | Europe | GF, GB, NO, SE, ES | 6 | 22 |
| `RussiaKazakhstan` | Russia/Kaz. | RU, KZ | 5 | 5 |
| `China` | China | CN | 5 | 7 |
| `India` | India | IN | 1 | 11 |
| `Japan` | Japan | JP | 4 | 6 |
| `Oceania` | Oceania | AU, NZ, MH | 6 | 17 |
| `MiddleEast` | Mid. East | IR, IL, OM | 4 | 1 |
| `Korea` | Korea | KP, KR | 2 | 1 |
| `SouthAmerica` | S. America | BR | 1 | 1 |
| `Other` | Other | *(complement)* | 3 | 1 |

`NewZealand` is gone, folded into Oceania: Mahia is the only high-cadence site
in the group, and a region holding just it left Australia's four ranges with no
home. §9.4 covers what that does to existing caches.

### 9.3 — `Other` is the complement, not a country list

The alternative considered was a `Sea/Air` region listing LL2's `??` country
code. Defining the last region as *everything the named ones do not claim*
closes three leaks instead of one:

- LL2's air- and sea-launch pseudo-sites, filed under a placeholder country
  whose `alpha_2_code` is `??` (§8.1).
- A country LL2 adds that `regions.toml` does not list. This is the §8.2
  failure, and under a catch-all it self-heals on the next regeneration instead
  of waiting for someone to read the generator's warning.
- Any country we deliberately decline to give its own arm.

`unassigned` therefore stops being a maintainer to-do and becomes a user-facing
bucket — but the generator still names its members in `table.rs`'s header and
on the console, so promoting one to its own region stays a decision someone
makes rather than one that goes unnoticed.

Rows with no country object at all stay excluded: they are lunar *landing*
sites, and can never match an upcoming launch. Note that Haiyang Oriental
Spaceport is a sea-launch platform that does carry `CN`, so it stays in China
— "sea launch" is not what `Other` means.

`regions.toml` gains `catch_all = true`, which the generator requires to be
unique, countryless, and last.

### 9.4 — Consequences

**Retired regions must not invalidate a cache.** `ActiveFilters::region` reads
through a lenient deserializer: a persisted region we no longer define falls
back to `All`. The derived impl would have rejected the whole cache file as
corrupt over a dropped variant, costing every upgrading user a refetch against
a volunteer-run API to recover a filter selection that costs nothing to forget.

**The cycling order is generated.** `RegionFilter::all()` now returns
`table::ALL`, emitted from `regions.toml` order with `All` prepended, replacing
a hand-written list in `filter.rs` that would have had to be edited in lockstep.
`region_filter_cycles_all_values`, which restated every label, was replaced by
`region_filter_cycles_every_region_once` — asserting the property rather than
the editorial content, so a region set change is not a test failure.

**One pre-existing bug, found by this work.** The generator's `ROOT` counted
`..`s from a path that had since moved a level deeper, so it silently wrote
`table.rs` into a `tools/src/` tree nothing compiles. The committed table was
stale in a way `--check` could not see, because `--check` read the same wrong
path. Fixed, and it is the reason §9's regeneration is verified against the
live API rather than trusted.

**The generator identifies itself.** It sent Python's default `User-Agent`; the
launches endpoint rejects that with `403`, and The Space Devs ask callers to
identify themselves regardless. It now sends
`deltav/1.2.1 (+https://github.com/ASmith-eng/launch-api-client)`. **The app
itself still sends `reqwest`'s default** — worth fixing separately.

### 9.5 — Coverage is now checkable in one request

`README.md` gains a snippet that fetches the upcoming list once and tallies its
location IDs against `table.rs`, reporting per-region counts and, crucially,
how many launches reach no region at all. That last number should be zero while
a catch-all exists. It answers a different question from §5.4's count
comparison — *"is the table complete?"* rather than *"is the parameter still
honoured?"* — and both are worth running after a regeneration.
