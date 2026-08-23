# Region Filter Hardening — Design Plan

Status: **PROPOSED — root-cause fix applied (§2), hardening work not started**

The region filter silently returned unfiltered results for its entire life. The
immediate fix is in the working tree; this document covers what we do so that
the *next* change on LL2's side is loud instead of silent. Like the other plans
in this directory, it is the contract we agree on before writing code.

---

## 1. Goal & scope

- **In scope:** removing hand-transcribed LL2 location IDs from the codebase,
  and adding the one test that can actually detect a filter silently becoming a
  no-op.
- **Out of scope:** any runtime location discovery, an in-app location browser,
  or changes to the filter panel UI beyond a single status-bar count (§5.4).

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
3166 alpha-2 codes (`country.alpha_2_code`), and 23 distinct countries are
represented — but launches cannot be filtered by them. Numeric location IDs are
the only geographic handle LL2 offers, so we cannot delete the ID table. We
have to own it properly instead.

---

## 4. Design principle — put the human on the right half

Today the code hardcodes the volatile external facts and buries the stable
editorial judgement. That is backwards.

- *"Is French Guiana part of Europe?"* is a product decision. It never changes
  unless we change it.
- *"What is location 25?"* is a row in someone else's database. It moved
  without telling us, and it will move again.

The seam belongs between those two:

| Layer | Owner | Volatility |
|---|---|---|
| Region → ISO country codes | us, hand-written | stable indefinitely |
| Country code → location IDs | LL2, generated | changes a few times a year |

A new Norwegian spaceport then joins Europe on the next regeneration without
anyone having to notice it needed to.

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

Ordered by dependency. §5.3 and §5.4 address D1 and are independently valuable
even if the generator in §5.2 never gets written.

### 5.1 — Editorial country–region table

A map from region name to a set of ISO alpha-2 codes, checked in and reviewed
by a human:

```
Europe            → GF, SE, NO, GB, ES
US                → US
Russia/Kazakhstan → RU, KZ
China             → CN
India             → IN
Japan             → JP
New Zealand       → NZ
```

Europe is the only entry requiring a real decision: `GF` (French Guiana)
belongs there because it hosts ESA's spaceport. That reasoning should sit in a
comment beside the entry — it is exactly the kind of rationale the code cannot
express on its own.

### 5.2 — Generator in `tools/`

Fetches `/locations/?limit=250` once, filters to `active`, groups by
`country.alpha_2_code` through the §5.1 table, and emits `region_map.rs` with
the site name as a comment on every ID. One request per regeneration, zero at
runtime. Follows the precedent set by the splash asset generator.

The `active` filter is load-bearing: it excludes Svobodny (superseded by
Vostochny) and the French Algeria test centre, neither of which can appear in
upcoming launches.

### 5.3 — Ignored contract test

Asserts that a filtered query returns strictly fewer results than an unfiltered
one, for each of the four filters. This is the only mechanism that detects
"LL2 stopped honouring our parameter."

Marked `#[ignore]` so it runs on demand or on a schedule, never on every
`cargo test` — that keeps the default test run offline and hermetic, and keeps
our traffic to the API deliberate.

The existing unit tests could not have caught D1. `endpoints.rs` asserts the
built URL *contains* the string the builder emits, which passes for any
parameter name including a fictional one. The test encoded the bug and went
green for it.

A cheaper partial guard is already in the working tree:
`every_region_variant_resolves_to_ids` in `filter.rs` catches the internal half
— a name mismatch between the `RegionFilter` enum and the `REGIONS` table,
which would otherwise yield `None` and silently disable that one region.

### 5.4 — Show the filter's effect in the status bar

`app.total_count` is already tracked. Rendering `23 of 193` when a filter is
active turns a silent no-op into something obvious within a second of use.

No extra requests, and it is consistent with how the app already handles rate
limiting — improve observability rather than adding cleverness.

### 5.5 — Version-bump checklist

The 2.2 → 2.3 migration is almost certainly what invalidated the original ID
table; endpoints were pluralised in the same move, and the stale `/location/`
path in our module doc comment was a fossil of it. Record the steps in
`documentation/`: re-fetch the browsable filterset form, regenerate the region
map, run the contract test, diff the result.

---

## 6. Open questions

### 6.1 — Are suborbital flights included by default?

One request settles it. If they are excluded, Corn Ranch (`29`) is dead weight
in the US region and New Shepard flights never appear — arguably a bug in its
own right, independent of regions. If they are included, we may want
`include_suborbital` exposed as a fifth filter category.

### 6.2 — Do the low-volume US sites stay?

White Sands (`155`), Spaceport America (`31`), Edwards (`162`) and PMRF (`1`)
are currently in the US region. They cost nothing in a comma-separated list,
but they are only meaningful if suborbital flights are visible at all — so this
resolves with §6.1.

### 6.3 — Is a long-TTL runtime refresh ever worth adding?

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
  it would misfire on legitimately empty regions.

---

## 8. Known behavioural consequence

LL2 has three locations with no country — "Sea Launch", "Air launch to orbit",
and "Air launch to Suborbital flight". They belong to no region, so air- and
sea-launched missions are absent from every region filter and visible only
under "All". This is noted in the `region_map.rs` module doc. It is defensible,
but it is a real difference from the unfiltered list and worth a line in the
help screen if users trip over it.
