# Splash Screen & Animation — Design Plan

Status: **SHIPPED — v1 implemented (§11), plus the two post-v1 refinements (§12)**

A startup splash screen featuring a telemetry downlink — a field of `deltav`
letters resolving into a photograph of the Apollo 14 Lunar Module, which then
settles back into the app's wordmark — playing over the initial data fetch,
skippable at any time. This document is the contract we agree on
*before* writing code — behaviour and animation design are meant to be edited
here first.

---

## 1. Goal & scope

- **In scope (v1):** a single animated splash screen shown at startup, built
  entirely with the current stack (ratatui 0.29 + crossterm 0.28, no new deps).
  Rendered as a fixed 72×20 grid of quadrant-block and letter cells, from a
  pre-quantised 10 KB asset embedded at compile time (§6.0).
- **Out of scope (v1):** in-list animations and transition effects between
  screens. Noted in §10 as future work.

The splash is a *presentation* layer only. It must not change any fetch,
cache, or rate-limit behaviour — it renders on top of the existing boot flow.

---

## 2. Bonafide rules (non-negotiable constraints)

These are the invariants the implementation must uphold. They exist because the
event loop (`src/tui/event.rs`) is async and shared with live network I/O.

### R1 — Never sleep or block inside the event loop
No `std::thread::sleep`, no `tokio::time::sleep().await` sprinkled in the render
path, no busy-wait. Blocking the loop would freeze input handling and stall
in-flight API futures in the `tokio::select!`. **Frame pacing comes only from a
timer branch in the existing `select!`**, exactly like the current 1-second
countdown tick (`event.rs:63`, `event.rs:127`).

### R2 — The fast animation tick is gated to the animated screen only
A ~30–60 ms frame timer must never redraw the List/Detail screens. In an idle
app, redrawing at 20–30 fps for no reason wastes CPU and battery.

Mechanism: add the frame timer as a **separate `select!` branch guarded by a
precondition** — `_ = frame_tick.tick(), if app.screen.is_animating() => { … }`.
A `select!` branch whose `if` precondition is `false` is **not polled**, so a
disabled frame timer costs nothing. Once the splash exits, the branch is dead for
the rest of the process. (This mirrors how the 1 s tick is gated to
`AppScreen::Detail` today.)

### R3 — Render stays a pure function of state
`render()` (`src/tui/render.rs:21`) takes `&App` and must not mutate. The current
animation frame is **derived from elapsed time**, not computed with side effects
during draw. State advancement (if any) happens in the timer branch, not the
renderer.

### R4 — Respect the existing terminal-size guard
`is_terminal_too_small()` (`app.rs:125`, min 80×24) already short-circuits
rendering. The splash must honour it and skip straight to the existing size
warning. No panics on tiny terminals.

Because the panel is sized *to* the minimum terminal (§6.0), this is the only
case to handle — there is no band between "too small to render" and "fits", so
no compact variant is needed.

### R5 — Always skippable, always bounded
Any keypress dismisses the splash immediately. Because the sequence is a fixed
length decoupled from the network (§5), its duration is inherently bounded — it
can never trap the user regardless of fetch state.

### R6 — Purely additive & config-gated
The splash is **on by default** and sits behind a single opt-out flag
(`disable_startup_splash`, see §8), reusing the existing screen machinery. Setting
the flag returns the exact current boot behaviour, so the feature is fully
reversible for any user.

---

## 3. Integration points (mapped to current code)

Five small touch points plus one asset, no structural rewrite:

| # | File | Change |
|---|------|--------|
| 1 | `src/tui/app.rs:23` | Add `AppScreen::Splash { started_at: Instant }` variant + an `is_animating()` helper on `AppScreen`. |
| 2 | `src/tui/render.rs:31` | Add a `match` arm that renders the current splash frame; new `views/splash.rs` module for the drawing. |
| 3 | `src/tui/event.rs:63/93` | Create a second `interval` (frame timer); add a gated `select!` branch (R2). Add keypress-skip + exit-condition handling. |
| 4 | `src/main.rs:134` | Start the app on `AppScreen::Splash{..}` instead of `List` when the flag is on (and terminal is large enough). |
| 5 | `src/config/mod.rs:111` (`UiConfig`) | Add `disable_startup_splash: bool`. `#[serde(default)]` makes an omitted key `false`, i.e. splash on. No range sanitisation needed. |

New file: `src/tui/views/splash.rs` (renderer + pure frame logic + tests).

Committed asset: `assets/splash.cells` (10,080 bytes), embedded with
`include_bytes!` and regenerated only by `tools/splash/build_splash_asset.py`
(§6.0.2). No build script, no codegen — it is a checked-in binary blob.

**Why no build script (considered and rejected):** `build.rs` runs on every
machine that compiles, including `cargo install deltav` and any build from a
clone — so generating the asset at build time would make Python (or an image
decoding crate, plus the multi-megabyte source artwork in the crate tarball) a
requirement for *installing* the app. Because `include_bytes!` bakes the asset
into the binary, a committed blob is already fully self-contained for every
distribution path: the released binaries carry the splash inside them, and no
user ever sees an asset file, a source image, or a generator. Regeneration is a
developer task, guarded by CI (§6.0.2).

---

## 4. Timing model (the heart of R1/R2)

```
before the loop:
    let mut frame_tick = interval(Duration::from_millis(SPLASH_FRAME_MS)); // 50ms → 20fps
    frame_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);       // no burst catch-up

inside tokio::select! { … existing branches … }:
    // Advance the splash. Gated so it is inert on every non-animated screen.
    _ = frame_tick.tick(), if app.screen.is_animating() => {
        // No heavy work here. Frame index is derived from elapsed time at
        // render, so this branch may even be a no-op that just wakes the loop
        // to trigger the unconditional render() at the top of the next iter.
    }
```

Key points:
- **Elapsed-time driven (R3):** the renderer computes
  `frame = started_at.elapsed() / frame_duration`. A dropped/skipped tick can't
  desync the animation, and the frame function is a pure `fn(elapsed) -> Frame`,
  trivially unit-testable without a terminal.
- **`MissedTickBehavior::Skip`** matches the existing countdown tick — if a frame
  runs long (e.g. during a slow draw), we skip rather than burst.
- The existing 1 s countdown tick is untouched and remains gated to `Detail`.
  During the splash it is simply not selected.
- **Named constants (locked):** `SPLASH_FPS = 20` (`SPLASH_FRAME_MS = 50`) and a
  total `SPLASH_DURATION = 4.2 s`, plus the six phase boundaries in §6.2;
  `SPLASH_FPS` is comfortably bumpable to 25.
- **Performance:** ratatui double-buffers and writes only changed cells to the
  terminal, so a 20–25 fps splash is negligible CPU on any modern machine — the
  per-frame budget is ~50 ms while a redraw touches only a few dozen cells.

---

## 5. Splash lifecycle & exit policy — LOCKED

**Predictable, fixed-duration.** The splash plays the full choreographed
sequence (§6) exactly once, then transitions to `AppScreen::List` — *regardless
of whether the initial fetch has returned*. It never waits on the network.

It ends on the **first** of:

1. **Sequence complete** — the choreography (§6) reached its final beat. This is
   the normal path; the total duration is a fixed **4.2 s** constant.
2. **Keypress** — any key skips immediately (R5). The skip key is *consumed*, not
   also handled as a List command.

On exit → `AppScreen::List`. If the fetch is still in flight, List simply shows
its normal loading state — the splash has already handed off.

### Relationship to the initial fetch
The app kicks off the first fetch from event-loop state (`check_needs_fetch`,
`event.rs:78`) on cold/stale starts. The splash *overlaps* this latency for free
but is fully **decoupled** from it — no "hold until ready", no branching on fetch
state. That decoupling is exactly what makes the timing predictable.

### Every startup or never
When enabled, the splash plays on **every** startup, warm cache or cold — the
behaviour never branches on cache state. The single config flag (§8) is the only
switch: on for every launch, or off entirely.

---

## 6. Splash animation design — telemetry downlink

Concept: a field of `deltav` letters resolves into a photograph of the Apollo 14
Lunar Module *Antares* on the surface, holds, then drains back into letters that
settle into the app's wordmark. The name is the raw material the image is built
from, and what remains when it disperses.

Thematically it is the boot fetch made visible — the splash plays over the
initial API call, and what the user watches is a downlink resolving.

### 6a Source artwork — LOCKED

| | |
|---|---|
| Original | `tools/splash/source/artwork.jpg` — 3892×3892 |
| Subject | Apollo 14 LM *Antares* on the lunar surface |
| Licence | NASA — public domain. No attribution obligation, no licence risk. |

The filename is generic by design: the pipeline is artwork-agnostic, so swapping
in a different image is a drop-in replacement plus a regeneration, with no
renaming anywhere downstream (§6.0.2). Provenance and licence for whatever is
currently in place are recorded in `tools/splash/source/README.md`.

This supersedes the found "JRO" character art of the previous draft, and with it
the licensing caveat that draft raised.

The letter-noise idea originated in a found piece of character art — a 200×100
grid of `0`/`1` glyphs whose per-character colour carried this same photograph.
It is not a source: it is a quantised derivative, and resampling it would
compound error. The asset is built from the original photograph instead
(§6.0.2).

### 6.0 Fixed panel, centred — LOCKED

The panel is a **fixed 72 × 20 cells**, framed in the app's rounded border
(§12.2) for a 74 × 22 block. On terminals larger than the 80×24 minimum it is
**centred, never scaled**.

Budget at exactly 80×24 (`MIN_COLS` / `MIN_ROWS`, `app.rs:17`–`19`):

```
      3 cols               74 cols               3 cols
     ┌──────┬───────────────────────────────────┬──────┐
 1   │                  (top margin)                   │
 22  │      ╭─── splash panel  72 × 20 ───╮            │
     │      │                             │            │
     │      ╰─ ▶ PRESS ANY KEY TO SKIP… ──╯            │
 1   │                (bottom margin)                  │
     └─────────────────────────────────────────────────┘
```

The hint rides the bottom border as a block title rather than costing a row of
its own, which is what keeps both margin rows inside the 24-row budget (§12.2).

Consequences, all of them simplifications:

- **No runtime resampling.** The asset is built offline at exactly this size.
- **No small-terminal variant.** The panel is sized *to* the minimum, so anything
  smaller is already caught by `is_terminal_too_small()` (`app.rs:125`) and gets
  the existing size warning. This deletes the previous draft's §6.5 and its
  "TBD thresholds" open decision outright.
- **Deterministic snapshots.** With no resampling, a given frame is byte-identical
  every run, so the `TestBackend` assertions in §7 are exact rather than fuzzy.

### 6.0.1 Why quadrant cells

The photograph's information is almost entirely **colour**, not glyph shape —
which rules out braille. Braille buys 8 dots per cell but only *one* foreground
colour, so it discards exactly what this image is made of. This supersedes the
previous draft's `Canvas` + `Marker::Braille` decision.

Measured reconstruction error, mean absolute error against a high-resolution
reference of the same crop (lower is better):

| Scheme | Samples | MAE | |
|---|---|---|---|
| 72×20 half-block `▀` | 72×40 | 13.52 | baseline |
| 80×22 half-block (largest panel that fits) | 80×44 | 13.29 | +21% cells, −1.7% error |
| **72×20 quadrant** | **144×40** | **12.25** | **−9.4%, same panel** ✅ |
| 80×22 quadrant | 160×44 | 11.95 | −11.6%, no margins left |
| 72×20 sextant | 144×60 | 11.67 | −13.7%, font-support risk |

Two findings drove the choice:

- **Growing the panel is nearly worthless.** Taking the largest panel that fits
  the minimum terminal adds 21% more cells for a 1.7% error reduction — and
  spends every margin to do it. Resolution here is not gated by panel size.
- **Sub-cell glyphs are where the gain is.** Quadrants double the horizontal
  sample count at the *same* panel size for a 9.4% error reduction.

Sextants (2×3 sub-pixels) score better still, but live in the Unicode 13
*Symbols for Legacy Computing* block (U+1FB00), whose font coverage is patchy.
Quadrants (U+2596–259F) sit in the same Block Elements range as `▀` and are
effectively universal. For an on-by-default splash, that trade is not worth 2%
of MAE.

### 6.0.2 Asset format — LOCKED

Each cell holds a 2×2 pixel patch quantised to two colours: a quadrant glyph
plus a foreground and a background. **The quantisation is deterministic and
depends only on the photograph, so it happens offline** — the app never does
image processing.

```
assets/splash.cells            72 × 20 cells, row-major, 7 bytes each:

    [mask u8][fg r,g,b][bg r,g,b]
    mask bit 3 = TL, 2 = TR, 1 = BL, 0 = BR;  set bit = that sub-pixel takes fg

    10,080 bytes total, embedded with include_bytes!
```

The 16 masks index a static glyph table (`" ▗▖▄▝▐▞▟▘▚▌▙▀▜▛█"`), so rendering a
photo cell is one table lookup and two colour assignments — no arithmetic.

A single derived value is computed once at load: a per-cell **blend** of fg and
bg weighted by set-bit count, used by the letter stage (§6.1) where a cell can
only carry one colour.

The crop is full-width, centred at 0.44 of source height: it keeps the flag,
foil, legs, footpads and surface texture, trimming only the rendezvous-antenna
tip.

**Regeneration — a developer task, not a build step.** Only needed if the
artwork, the crop, or the panel size changes:

```
python3 -m pip install pillow==12.3.0               # canonical decoder
python3 tools/splash/build_splash_asset.py          # regenerate
python3 tools/splash/build_splash_asset.py --check  # verify, write nothing
```

The version is pinned because Pillow's LANCZOS output *defines* the committed
bytes — the CI gates pin the same `12.3.0`, so an unpinned local install can
produce a spurious mismatch. Bumping it is fine, but regenerate and commit the
asset in the same change.

The generator globs `tools/splash/source/artwork.*` and refuses to run if that
is ambiguous, so replacing the artwork means dropping in a file and re-running —
no code edit, no rename.

Alongside the artwork sits `source/artwork.sha256`, a digest of the source the
committed asset was built from. `--check` verifies it *before* decoding, so
swapping the artwork fails loudly and names both digests rather than silently
regenerating against a new image.

**Pillow is canonical.** macOS `sips` is kept as a no-install fallback but
resamples differently: measured against the same source, the two decoders
disagree on 40.6% of asset bytes (mean delta 1.7/255 — visually identical,
byte-wise unusable for an exact check). The script warns loudly when it falls
back.

`--check` runs as a CI gate in two places, both already wired up:

- **On PRs and pushes to `main`** (`.github/workflows/ci.yml`) — catches a stale
  asset while a human is still reviewing, when the change can simply be fixed in
  the branch.
- **On tags, before the build matrix** (`.github/workflows/release.yml`) — a tag
  can be cut from any commit, so this is what actually guarantees no release
  ships an asset that disagrees with its source. Because the gate precedes the
  matrix, a failure produces no artifacts and no partial release.

Together they catch the one silent failure here: artwork swapped without
regenerating, which would otherwise compile cleanly and ship the previous image.
Running `--check` locally before tagging still saves a round trip.

A *missing* asset needs no guard — `include_bytes!` already fails the build with
the path in the error.

### 6.1 Two cell types

Every frame is a 72×20 grid in which each cell is in one of two states:

| State | Renders as | Colour |
|---|---|---|
| **Photo** | quadrant glyph from the asset | fg + bg from the asset |
| **Letter** | one of `d e l t a v` | one colour — the cell's blend, scaled |

The entire animation is the *migration* of cells between these two states, plus
a brightness scale. There is no motion, no sprite, and no geometry anywhere in
it.

### 6.2 Choreography — 4.2 s, LOCKED

| Phase | Window | What happens |
|---|---|---|
| 1. Static | 0.00–0.15 s | All letter cells, flat grey. No image information present. |
| 2. Ghost | 0.15–1.20 s | Letter colours ramp from grey into the photograph's own palette. The LM appears *latent* in the letter field before any shape resolves. |
| 3. Resolve | 1.20–2.10 s | Cells flip letter → photo on a jittered top-down sweep. |
| 4. Hold | 2.10–2.90 s | Full photograph. |
| 5. Settle | 2.90–3.60 s | Cells revert to letters and drain toward black; wordmark cells lock to their correct letter and brighten. |
| 6. Wordmark | 3.60–4.20 s | `deltav` alone — reaching full brightness at 3.85 s, then **held** for 0.35 s. |

The wordmark's brightness ramp is a **separate 0.25 s constant**
(`WORDMARK_RAMP_SECS`), not the length of phase 6. Ramping across the whole phase
spends all of it arriving, so peak brightness lands on the cut to the list and
the finished mark is never actually seen — which is what §12.1 was really
observing. Lengthening the phase alone would only have slowed the arrival.

Cell ordering comes from a deterministic hash, so it is stable frame to frame
and unit-testable:

```
resolve order = (row / (ROWS-1)) * 0.7  +  hash01(col, row, 1) * 0.3
drain   order = hash01(col, row, 2) * 0.6  +  (1 - row / (ROWS-1)) * 0.4
```

The 0.7 / 0.3 weighting is what makes phase 3 read as a slow-scan downlink
acquiring rather than a hard wipe — mostly top-down, ragged at the leading edge.
The drain reverses the row bias, so the image dissolves from the bottom up
instead of retracing the same sweep.

Phases 2 and 5 are the beats that carry the concept, and both are pure ramps —
no state, no accumulation, nothing to desync.

### 6.3 The wordmark

`deltav`, spelled in `d`s, `e`s, `l`s, `t`s, `a`s and `v`s — the block letters'
strokes drawn with the same six characters the noise field is made of.

- **Font:** a hand-authored 5×7 mask covering six glyphs. That is the entire
  alphabet this splash needs.
- **Each font pixel is 2 cells wide.** A 5×7 glyph in 1:2 character cells is
  physically 1:2.8 — absurdly condensed. Doubling the width gives ~1:1.4. This
  was the real cause of the thin, sparse look in the first mock; the glyph
  shapes were never the problem.
- **Footprint:** 6 letters × 10 cells + 5 gaps = **65 × 7**, centred in the panel.
- **It is assembled, not overlaid.** Wordmark cells are ordinary letter cells
  that brighten while their neighbours drain out. The name is the residue of the
  image rather than a title card dropped on top — which is the entire point of
  the beat.

### 6.4 Skip hint — LOCKED

`▶ PRESS ANY KEY TO SKIP…`, centred on the panel's **bottom border** as a
`Block::title_bottom` (§12.2). Fades in at ~1.0 s, then blinks on a ~500 ms
cycle, derived from elapsed time (R3, no timer of its own). It advertises only
the any-key skip already specified in §5 and adds no new input handling. Hidden
once the settle phase begins — the title simply goes empty, leaving a plain
border — so it never competes with the wordmark.

### 6.5 Colour: truecolor and fallback

This is the app's **first use of 24-bit colour** — `style.rs` is currently
entirely the 16 named colours.

Three tiers, degrading to nothing rather than to something broken:

| Tier | Detection | Behaviour |
|---|---|---|
| Truecolor | `COLORTERM` ∈ {`truecolor`, `24bit`} | Full RGB photograph, 4.2 s |
| 256-colour | `TERM` contains `256color` | **Greyscale** photograph via `Indexed(232..255)`, 4.2 s |
| Anything less | neither matches | **Wordmark sequence only** — no image, 1.6 s (§6.5.1) |

Nothing is ever skipped outright: the lowest tier still gets a branded opening,
just not a photographic one.

- **The image's colours come entirely from the asset.** The only semantic
  palette the splash reads is the border's `DarkGray` (§12.2), which is
  available in every tier and so needs no tier handling; `style.rs` itself is
  unchanged.
- **The greyscale map is precomputed** when the asset is parsed — one dot
  product per cell across 1440 cells at load, and **zero per-frame work**. It is
  strictly less work than the palette search this section used to specify.

#### Why not colour on 256-colour terminals — measured

The previous draft assumed the 6×6×6 cube would carry "muted greys, browns and
golds … without visible banding". Banding was never the problem; **hue collapse**
was. Three quantisers were measured against the real asset (1735 distinct
colours, 621 chromatic at OKLab C ≥ 0.02):

| Quantiser | OKLab ΔE mean | Chromatic colours flattened to neutral |
|---|---|---|
| Naive Euclidean RGB | 0.0246 | 81.0% |
| OKLab (perceptual) | 0.0227 | 85.7% |
| OKLab, chroma weighted ×8 | 0.0244 | **72.9%** (best) |

Euclidean RGB overweights luminance, so a warm brown maps to a neutral grey of
similar brightness — and switching to a perceptual space made it *worse*, not
better. The constraint is the palette, not the metric: this image's chroma sits
at OKLab C ≈ 0.02–0.05, where the cube offers only its neutral diagonal or a
40-unit jump into saturation. No distance function invents entries that do not
exist. Forcing chroma (excluding the grey ramp above a saturation threshold)
produced blotchy reds and olives — visibly worse than either.

Since roughly 80% of the colour is lost regardless, a *deliberate* greyscale is
the honest rendering: 24 clean luminance steps (mean error 2.52/255, max 5.7)
that read as an archival monochrome print, rather than a near-grey image with
~7% of colours surviving as stray pink and yellow speckle. Apollo surface
photography carries monochrome well.

Below 256 colours the grey ramp is unavailable too, so the splash is skipped
outright — the one case where dropping it beats degrading it.

### 6.5.1 The low-colour tier — wordmark only — LOCKED

Below 256 colours the photograph is abandoned entirely and only §6.2's closing
movement plays: a flat field of `deltav` letters, the hash-ordered drain, and
the wordmark assembling out of it. Its own short timeline, since there is no
image to reveal and the full 4.2 s would be dead air:

| Phase | Window | What happens |
|---|---|---|
| 1. Field | 0.00–0.40 s | Flat letter noise, one dim grey. No image data is read at all. |
| 2. Settle | 0.40–1.20 s | Cells drain on the same `hash01` order; wordmark cells lock and brighten. |
| 3. Wordmark | 1.20–1.60 s | `deltav` alone. |

Two colours carry it — dim for the field, bright for the wordmark — so it works
on any ANSI terminal. The drain ordering, wordmark mask and letter alphabet are
all shared with the full sequence; the only additions are three constants and a
branch that never emits photo cells.

**Why not keep the image as brightness (rejected).** The tempting middle option
is to run the full choreography and let the photograph appear as *brightness*
in the letter field rather than dropping it. Prototyped and measured: it fails,
and on legibility rather than palette. Quantised to four greys as **solid
cells** the LM reads clearly — legs, descent stage, horizon. Drawn as actual
glyphs it does not: letters cover ~30% of a cell, so neighbours blur into
texture instead of forming edges. Even stretching contrast across the image's
own luminance range (1.8–172.8, which recovers a top level a naive 0–255 map
never reaches) leaves only 20 of 1440 cells at the brightest step. The result is
an atmospheric gradient with a visible horizon band — not a lunar module.

That finding is what makes §6.5.1 the right shape: the wordmark needs only two
levels and renders perfectly, while the photograph needs more than four and
cannot be rescued. So the low tier keeps precisely the half that survives.

The four base greys are also theme-defined rather than specified, so their
ordering is not guaranteed — another reason to depend on two of them, not four.

### 6.6 Prototype

The full choreography is implemented and playable before any Rust is written:

```
python3 tools/splash/splash_prototype.py                      # play it
python3 tools/splash/splash_prototype.py --png 1.1,2.6,4.1    # dump frames
python3 tools/splash/splash_prototype.py --static 0.15 --hold 1.2   # retime
```

Every phase duration in §6.2 is overridable (`--static`, `--ghost`, `--resolve`,
`--hold`, `--settle`, `--wordmark`), as is the wordmark brightness ramp
(`--ramp`, where `0` means "ramp across the whole phase"), so alternative
timings are judged by watching rather than argued on paper. The prototype prints
the derived timeline whenever it differs from the defaults. **Whatever is locked
in §6.2 must match the prototype's `PHASES` and `WORDMARK_RAMP` defaults** —
they are the signed-off timing.

`--no-border` drops the §12.2 frame and puts the hint on its own row below the
panel, the layout v1 shipped with, for A/B against the bordered default.

`--tier` previews each colour tier from §6.5, emitting that tier's real escape
codes rather than a truecolor imitation, so a preview shows what the terminal
will actually do:

```
python3 tools/splash/splash_prototype.py --tier color   # truecolor (default)
python3 tools/splash/splash_prototype.py --tier grey    # 256-colour ramp
python3 tools/splash/splash_prototype.py --tier mono    # wordmark only, 1.6 s
```

`mono` runs §6.5.1's short timeline and prints it on start. The phase-duration
flags apply to the full sequence only; `MONO_PHASES` in the prototype is the
signed-off timing for the low tier, the same way `PHASES` is for the others.

Colour-tier mapping lives in the *renderer* (`tier_fg`/`tier_bg`), not in
`frame_at`, which always emits RGB. The Rust splits the same way: pure frame
logic is tier-agnostic, and the tier is applied when cells are blitted.

It reads the same `assets/splash.cells`, uses the same hash-ordered
migration and the same phase constants, and is structured as the pure
`fn(elapsed) -> frame` the renderer should be (R3). It is the reference for the
Rust implementation — where the two disagree, the prototype is what was signed
off.

## 7. Testing strategy

- **Pure frame logic:** `fn frame_at(elapsed) -> SplashFrame` returns a 72×20
  grid of cell *descriptions* (`Photo { mask, fg, bg }` or `Letter { char, colour }`)
  which the renderer blits — deterministic and terminal-free. It takes no `area`:
  the panel is a fixed size (§6.0), so only elapsed time varies. Assert the six
  phase boundaries (§6.2), that resolved-cell count rises monotonically through
  phase 3 and falls through phase 5, that every cell is `Photo` at the end of the
  hold and every cell is `Letter` at 4.2 s, and that the wordmark cells carry
  their correct letters once settling begins.
- **Asset integrity:** `assets/splash.cells` is exactly
  `72 * 20 * 7 = 10,080` bytes and every mask byte is ≤ 15. Cheap guard against a
  truncated or mis-generated blob reaching a release.
- **Renderer:** ratatui's `TestBackend` + `Buffer` assertions to snapshot a few
  representative frames (static, ghost, hold, wordmark). Because nothing is
  resampled at runtime, these snapshots are byte-identical run to run.
- **Centring:** on an oversized area the bordered block's origin is
  `((w - 74) / 2, (h - 22) / 2)` and the panel sits one cell inside it; on
  exactly 80×24 the block lands at `(3, 1)` and the panel at `(4, 2)`, the §6.0
  budget. Note the panel's *x* is unchanged by the border at any width — only
  *y* shifts down a row.
- **Loop integration:** assert `is_animating()` gating — the frame branch is inert
  off the splash; a keypress transitions `Splash → List` and is consumed.
- **Config:** an omitted `disable_startup_splash` deserialises to `false` (splash
  on); `disable_startup_splash = true` turns it off. Mirrors existing `UiConfig`
  partial-table tests.

---

## 8. Config surface — LOCKED

A single opt-out boolean under `[ui]` in `config.toml`:

```toml
[ui]
disable_startup_splash = false   # omit or false → splash every startup; true → off
```

That is the entire user-facing surface. The splash is **on by default**: an
absent key deserialises to `false` via `#[serde(default)]`, so users only ever
touch this to turn the animation *off*. No fps or duration knobs — frame rate and
sequence length are internal constants (§4/§6), keeping config minimal and
behaviour predictable.

Follows the existing `UiConfig` default + `#[serde(default)]` pattern
(`config/mod.rs:150`); a bare `bool` needs no range sanitisation.

---

## 9. Decisions

**Locked:**
- [x] **Default:** on by default. Config is a single opt-out
      `disable_startup_splash` bool; omitting it == `false` == splash shown (§8).
- [x] **Exit policy:** fixed-duration show; transition to List on completion,
      independent of fetch state (§5).
- [x] **Warm start:** plays on every startup when enabled; never branches on cache.
- [x] **Skip hint:** retro blinking "PRESS ANY KEY TO SKIP…" prompt (§6.4).
- [x] **Reduced-motion / static variant:** deferred to a follow-up task (§10).
- [x] **Duration: 4.2 s** at 20 fps (§6.2). *Raised from 3 s.* Three transitions
      plus a hold do not fit in 3 s — at that length the photograph is on screen
      for under a second, which removes the reason to have a photograph at all.
      R5 keeps it skippable regardless.
- [x] **Concept: telemetry downlink** — letter-noise → photograph → wordmark
      (§6.2). Replaces the lunar-descent draft.
- [x] **Artwork: Apollo 14 LM *Antares***, NASA, public domain (§6a).
- [x] **Panel: fixed 72 × 20, centred, never scaled** (§6.0).
- [x] **Rendering: quadrant cells** (U+2596–259F), chosen on measured
      reconstruction error, not preference (§6.0.1).
- [x] **Asset: pre-quantised cell grid**, 10,080 bytes, built offline; the app
      does no image processing (§6.0.2).
- [x] **Wordmark: `deltav` set in its own letters**, 65 × 7, assembled from the
      draining cells rather than overlaid (§6.3).

- [x] **Tagline: none — `deltav` alone** (§6.3). The splash plays on every
      startup, so a line read hundreds of times becomes noise; and an
      artwork-specific callout would couple the wordmark to the photograph,
      contradicting §10's "a second artwork is a build-script run, not a code
      change". A tagline belongs in the deferred first-run splash (§10), where
      it is seen once and actually informs.
- [x] **Non-truecolor: greyscale, then wordmark-only** (§6.5). Three tiers — RGB
      photograph on truecolor, `Indexed(232..255)` greyscale photograph on
      256-colour, and below that the wordmark sequence alone (§6.5.1). Chosen on
      measured data: every colour quantiser tried flattens ≥73% of the image's
      chroma to neutral, so a deliberate monochrome beats a broken colour
      rendering; and below 256 the photograph cannot be carried at all, while
      the wordmark needs only two levels. Nothing is skipped outright.

- [x] **Hold length: 0.8 s, total 4.2 s** — kept as authored (§6.2 phase 4). A
      1.2 s hold funded by trimming the static opening was prototyped
      (`--static 0.15 --hold 1.2`) and judged unnecessary; the photograph is
      substantially on screen from ~1.4 s to ~3.8 s across resolve, hold and
      settle, so the pure hold reads longer in motion than on paper. Retiming
      remains a one-line change to `PHASES` if that judgement changes.
- [x] **Wordmark hold: 0.6 s phase with a 0.25 s ramp, total still 4.2 s**
      (§12.1). Funded by trimming static 0.35 → 0.15 — the slack §9's
      hold-length entry had already identified as costing nothing — so the
      total is unchanged and no startup gets longer. Decoupling the ramp from
      the phase is the part that actually delivers the beat; see §6.2.
- [x] **Bordered panel, hint on the bottom edge** (§12.2, option B). Reuses
      `TitleBar` so the splash wears the same rounded `DarkGray` frame as the
      list and detail views, and keeps both margin rows by rendering the hint
      as chrome rather than as a row of content.

**Still open:** none. §11 is unblocked.

## 10. Future work (explicitly deferred)

Ideas parked before v1 shipped. For the two refinements that came *out* of
running v1, see §12.

- **Extended first-run splash.** A longer, richer variant shown *only* the first
  time the app is ever started on an installation (e.g. a welcome/tagline beat and
  a brief controls hint after the launch sequence).
  - **Detection is essentially free:** the persisted `AppState` already carries
    `startup_count: u32` (`models.rs:301`), incremented *before* the event loop
    on every boot (`main.rs:86`). A fresh install has no `app_state.json` → count
    defaults to `0` → becomes `1` on the first run. So **`startup_count == 1`
    selects the extended variant**; no new persisted field or marker file needed.
  - **Same rules apply:** it is just a longer fixed-duration choreography — R1–R6
    hold unchanged (skippable, decoupled from fetch, honours `disable_startup_splash`
    and the size guard). Only the duration constant and content differ by variant.
  - **Edge case (acceptable):** deleting the cache/state dir resets the counter,
    so the first-run splash would replay — which matches "fresh installation"
    semantics anyway.
- **Static / reduced-motion splash variant** for low-power terminals or a
  `NO_COLOR`-style preference (split out of v1 per §9). §6.5 records a
  prototyped letters-only variant and why it was rejected — the blocker is glyph
  sparsity destroying legibility, not palette size, so any revisit needs a
  denser cell treatment rather than more colours.
- **Sextant rendering.** 2×3 sub-pixels per cell measured ~4% better than the
  quadrants we shipped (§6.0.1), but the U+1FB00 block's font coverage is patchy.
  Worth revisiting as terminal fonts catch up — the asset format would change,
  nothing else would.
- **A second artwork.** The panel is one fixed-size asset behind a stable format,
  so alternate images (a different mission, a launch vehicle) are a build-script
  run rather than a code change.
- Reusable animation timer so other screens can animate (e.g. a subtle "in
  flight" pulse on live launches).
- Seasonal / launch-provider-themed splash variants.

---

## 11. Implementation checklist (once decisions are locked)

1. `AppScreen::Splash { started_at }` + `is_animating()` (`app.rs`).
2. Asset loading: `include_bytes!("../../../assets/splash.cells")`, parsed
   into a `[Cell; 72*20]` once, plus the derived per-cell blend (§6.0.2).
3. `views/splash.rs`: the 5×7 six-glyph font, `hash01()`, the quadrant glyph
   table, pure `frame_at(elapsed)` (§6.2), renderer, centring, tests.
4. Colour: `COLORTERM` detection + 256-cube quantisation fallback (§6.5).
5. `render.rs`: dispatch arm (respect the existing size guard).
6. `event.rs`: frame-timer interval + gated branch + skip/exit handling.
7. `config/mod.rs`: `disable_startup_splash: bool` field (serde default false).
8. `main.rs`: start on `Splash` unless disabled, and terminal large enough.
9. Tests (§7) green; `cargo clippy` clean; run against
   `tools/splash/splash_prototype.py` side by side to confirm the timing and
   the two transitions match the signed-off mock.

---

## 12. Post-v1 refinements — DONE

Both came out of watching the shipped v1 in the real app. Neither was a defect —
v1 behaved as designed — so both were amendments to the design, implemented
together. The sections above now describe the current behaviour; this one
records what changed and why, so the reasoning is not lost.

### 12.1 Hold the finished wordmark for longer — DONE

**Observation:** the sequence cut to the list too soon after `deltav` finished
assembling. The wordmark is the beat the whole animation builds toward, and at
0.40 s it was gone almost as soon as it was readable.

**The observation was right; the diagnosis was incomplete.** The brightness ramp
was stretched across the whole wordmark phase, so the mark hit full white
*exactly on the cut*. Lengthening the phase alone would only have slowed the
arrival — there would still have been no instant of a finished, held wordmark.
Two changes, and the second is the one that delivers the beat:

1. **Phase 0.40 → 0.60 s, funded by trimming static 0.35 → 0.15 s.** The total
   stays 4.20 s, so no startup got longer, and the donor was the slack §9's
   hold-length entry had already prototyped as costing nothing.
2. **The ramp became its own 0.25 s constant** (`WORDMARK_RAMP_SECS`), independent
   of the phase length. Full brightness is reached at 3.85 s and *held* for
   0.35 s.

The tier asymmetry noted before the work — mono's wordmark was already 25% of
its 1.60 s sequence against 9.5% for the colour tiers — is why `MONO_PHASES` was
left alone. The ramp is shared, though, so the mono tier gains a 0.15 s hold for
free, at no cost to its timing.

**Landed in:** `PHASES` / `WORDMARK_RAMP` in `tools/splash/splash_prototype.py`;
`STATIC_SECS`, `WORDMARK_SECS`, `WORDMARK_RAMP_SECS` and `wordmark_ramp()` in
`src/tui/views/splash.rs`; the §6.2 table and §9's duration entries above.
`mono_tier_runs_the_shorter_sequence` still asserts the 1.6 s and 4.2 s totals
as literals and still passes untouched — which is the point of funding the extra
time internally.

### 12.2 Frame the panel in a rounded border — DONE

**Observation:** the list and detail views are framed in a rounded `DarkGray`
border and the splash was not, so the animation read as detached from the app it
introduces. The same frame ties the two together.

**Option B, as anticipated.** A border costs 2 rows and 2 columns. Columns were
easy — 72 + 2 = 74 leaves 3 either side at 80 wide. Rows were the constraint,
since v1's budget spent all 24:

```
        v1 layout                   A: border, hint outside      B: hint in the border ✅
  1   top margin                  ┌ 22  bordered panel        1   top margin
  20  splash panel                │                           ┌ 22  bordered panel
  1   (blank)                     1   (blank)                 │     (hint on the bottom edge)
  1   ▶ PRESS ANY KEY TO SKIP…    1   ▶ PRESS ANY KEY…        └
  1   bottom margin               └ (no margins left)         1   bottom margin
  = 24                            = 24                        = 24
```

Rendering the skip hint as a `Block::title_bottom` keeps both margin rows, drops
the now-redundant blank spacer, and makes the hint read as chrome — which is
what it is. §6.4's fade/blink and its disappearance at settle all still apply;
the title simply goes empty, leaving a plain border. At 24 characters the hint
sits comfortably inside a 74-wide edge.

`TitleBar` (`src/tui/views/mod.rs`) gained a `bottom()` segment and a `Default`
impl for the untitled case, so the splash reuses the shared idiom rather than
hand-rolling a block — the border styling is not duplicated anywhere.

**Landed in:** `TitleBar::bottom` in `src/tui/views/mod.rs`; `block_rect`,
`BLOCK_COLS` and `render_splash` in `src/tui/views/splash.rs`, with
`panel_origin` now returning the origin *inside* the border; the centring tests;
`--no-border` in the prototype for A/B against the v1 layout; the §6.0 budget
diagram and §6.4/§6.5 above.
