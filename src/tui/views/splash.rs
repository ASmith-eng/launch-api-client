//! Startup splash animation — a telemetry downlink resolving into the precomputed
//! image asset, then settling into the `deltav` wordmark.
//!
//! [`frame_at`] is a pure `fn(elapsed) -> SplashFrame` emitting RGB; mapping
//! those colours onto a terminal's palette is [`render_splash`]'s job.
//!
//! `tools/splash/splash_prototype.py` plays the same choreography from the
//! same asset and is the reference for how this should look.

use std::sync::OnceLock;
use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::tui::views::TitleBar;

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

/// Panel width in cells. The asset is built offline at exactly this size.
pub const PANEL_COLS: u16 = 72;
/// Panel height in cells.
pub const PANEL_ROWS: u16 = 20;
/// Panel plus the border framing it.
pub const BLOCK_COLS: u16 = PANEL_COLS + 2;
/// Panel plus the border framing it. At 80×24 this leaves one margin row above
/// and below — the skip hint rides the bottom edge rather than costing a row.
pub const BLOCK_ROWS: u16 = PANEL_ROWS + 2;

const CELL_COUNT: usize = PANEL_COLS as usize * PANEL_ROWS as usize;
const BYTES_PER_CELL: usize = 7;

/// Frame interval; `1000 / 50` = 20 fps.
pub const FRAME_MS: u64 = 50;
const FPS: f64 = 1000.0 / FRAME_MS as f64;

const HINT: &str = "▶ PRESS ANY KEY TO SKIP…";

// ---------------------------------------------------------------------------
// Choreography — phase durations in seconds, and the boundaries derived from them
// ---------------------------------------------------------------------------

const STATIC_SECS: f64 = 0.15;
const GHOST_SECS: f64 = 1.05;
const RESOLVE_SECS: f64 = 0.90;
const HOLD_SECS: f64 = 0.80;
const SETTLE_SECS: f64 = 0.70;
const WORDMARK_SECS: f64 = 0.60;

/// How long the wordmark takes to reach full brightness, independent of how
/// long the wordmark phase lasts. Ramping across the whole phase spends all of
/// it arriving, putting peak brightness on the cut to the list — so the
/// finished wordmark is never actually seen. The rest of the phase is the hold.
const WORDMARK_RAMP_SECS: f64 = 0.25;

const _: () = assert!(
    WORDMARK_RAMP_SECS < WORDMARK_SECS,
    "the ramp must be shorter than the phase it sits in, or there is no hold"
);

const STATIC_ENDS: f64 = STATIC_SECS;
const GHOST_ENDS: f64 = STATIC_ENDS + GHOST_SECS;
const RESOLVE_ENDS: f64 = GHOST_ENDS + RESOLVE_SECS;
const HOLD_ENDS: f64 = RESOLVE_ENDS + HOLD_SECS;
const SETTLE_ENDS: f64 = HOLD_ENDS + SETTLE_SECS;
const SEQUENCE_ENDS: f64 = SETTLE_ENDS + WORDMARK_SECS;

const MONO_FIELD_ENDS: f64 = 0.40;
const MONO_SETTLE_ENDS: f64 = MONO_FIELD_ENDS + 0.80;
const MONO_SEQUENCE_ENDS: f64 = MONO_SETTLE_ENDS + 0.40;

/// Flat colour of the un-drained letter field when there is no image to tint it.
const FIELD_GREY: Rgb = (70, 74, 78);
const WHITE: Rgb = (255, 255, 255);

// ---------------------------------------------------------------------------
// Asset — a pre-quantised cell grid; the app never does image processing
// ---------------------------------------------------------------------------

const ASSET: &[u8] = include_bytes!("../../../assets/splash.cells");

const _: () = assert!(
    ASSET.len() == CELL_COUNT * BYTES_PER_CELL,
    "assets/splash.cells is the wrong size — regenerate with tools/splash/build_splash_asset.py"
);

/// A 24-bit colour as emitted by [`frame_at`], before any palette mapping.
pub type Rgb = (u8, u8, u8);

/// One quantised 2×2 patch: a quadrant glyph plus two colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PhotoCell {
    mask: u8,
    fg: Rgb,
    bg: Rgb,
}

struct Asset {
    cells: [PhotoCell; CELL_COUNT],
    /// Per-cell mean of fg/bg weighted by set-bit count, for the letter stage
    /// where a cell can carry only one colour.
    blend: [Rgb; CELL_COUNT],
    /// Luminance range across `blend`, which the mono tier stretches its four
    /// levels over — a naive 0..255 map never reaches the brightest one.
    ghost_luminance_range: (f64, f64),
}

fn asset() -> &'static Asset {
    static CACHE: OnceLock<Asset> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut cells = [PhotoCell {
            mask: 0,
            fg: (0, 0, 0),
            bg: (0, 0, 0),
        }; CELL_COUNT];
        let mut blend = [(0u8, 0u8, 0u8); CELL_COUNT];

        for (index, cell) in cells.iter_mut().enumerate() {
            let bytes = &ASSET[index * BYTES_PER_CELL..][..BYTES_PER_CELL];
            let (mask, fg, bg) =
                (bytes[0], (bytes[1], bytes[2], bytes[3]), (bytes[4], bytes[5], bytes[6]));
            *cell = PhotoCell { mask, fg, bg };

            let lit = mask.count_ones();
            let mix = |front: u8, back: u8| {
                ((u32::from(front) * lit + u32::from(back) * (4 - lit)) / 4) as u8
            };
            blend[index] = (mix(fg.0, bg.0), mix(fg.1, bg.1), mix(fg.2, bg.2));
        }

        let luminances = blend.iter().map(|colour| luminance(dim(*colour, 0.75)));
        let ghost_luminance_range = luminances
            .fold((f64::MAX, f64::MIN), |(lowest, highest), lum| {
                (lowest.min(lum), highest.max(lum))
            });

        Asset {
            cells,
            blend,
            ghost_luminance_range,
        }
    })
}

// ---------------------------------------------------------------------------
// Wordmark
// ---------------------------------------------------------------------------

const WORD: [char; 6] = ['d', 'e', 'l', 't', 'a', 'v'];
const GLYPH_WIDTH: usize = 5;
const GLYPH_HEIGHT: usize = 7;
/// Each font pixel is 2 cells wide; at 1:1 a 5×7 glyph would read as 1:2.8.
const CELLS_PER_PIXEL: usize = 2;

const FONT: [[&str; GLYPH_HEIGHT]; 6] = [
    [
        "....#", "....#", ".####", "#...#", "#...#", "#...#", ".####",
    ],
    [
        ".....", ".....", ".###.", "#...#", "#####", "#....", ".###.",
    ],
    [
        ".##..", "..#..", "..#..", "..#..", "..#..", "..#..", ".###.",
    ],
    [
        ".#...", ".#...", "###..", ".#...", ".#...", ".#..#", "..##.",
    ],
    [
        ".....", ".....", ".###.", "....#", ".####", "#...#", ".####",
    ],
    [
        ".....", ".....", "#...#", "#...#", "#...#", ".#.#.", "..#..",
    ],
];

fn wordmark() -> &'static [Option<char>; CELL_COUNT] {
    static CACHE: OnceLock<[Option<char>; CELL_COUNT]> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut mask = [None; CELL_COUNT];
        let letter_width = GLYPH_WIDTH * CELLS_PER_PIXEL;
        let word_width = WORD.len() * (letter_width + 1) - 1;
        let origin_col = (PANEL_COLS as usize - word_width) / 2;
        let origin_row = (PANEL_ROWS as usize - GLYPH_HEIGHT) / 2;

        for (index, letter) in WORD.iter().enumerate() {
            let letter_col = origin_col + index * (letter_width + 1);
            for (glyph_row, line) in FONT[index].iter().enumerate() {
                for (glyph_col, bit) in line.bytes().enumerate() {
                    if bit != b'#' {
                        continue;
                    }
                    for offset in 0..CELLS_PER_PIXEL {
                        let col = letter_col + glyph_col * CELLS_PER_PIXEL + offset;
                        let row = origin_row + glyph_row;
                        mask[row * PANEL_COLS as usize + col] = Some(*letter);
                    }
                }
            }
        }
        mask
    })
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// Deterministic per-cell value in `[0, 1)`, stable across frames.
fn hash01(col: u32, row: u32, salt: u32) -> f64 {
    let hash = (u64::from(col) * 73_856_093)
        ^ (u64::from(row) * 19_349_663)
        ^ (u64::from(salt) * 83_492_791);
    let hash = (hash ^ (hash >> 13)).wrapping_mul(1_274_126_177) & 0xFFFF_FFFF;
    ((hash ^ (hash >> 16)) & 0xFFFF) as f64 / 65536.0
}

fn lerp(start: f64, end: f64, factor: f64) -> f64 {
    start + (end - start) * factor.clamp(0.0, 1.0)
}

/// Brightness of the closing wordmark over the phase spanning
/// `phase_start..phase_end`.
fn wordmark_ramp(secs: f64, phase_start: f64, phase_end: f64) -> f64 {
    lerp(0.0, 1.0, (secs - phase_start) / WORDMARK_RAMP_SECS.min(phase_end - phase_start))
}

fn dim(rgb: Rgb, factor: f64) -> Rgb {
    let scale = factor.clamp(0.0, 1.0);
    (
        (f64::from(rgb.0) * scale) as u8,
        (f64::from(rgb.1) * scale) as u8,
        (f64::from(rgb.2) * scale) as u8,
    )
}

fn luminance(rgb: Rgb) -> f64 {
    0.299 * f64::from(rgb.0) + 0.587 * f64::from(rgb.1) + 0.114 * f64::from(rgb.2)
}

/// Half-to-even rounding, as Python's `round` does.
///
/// Integer RGB luminance lands on exact `.5` boundaries often enough that
/// Rust's half-away-from-zero would put a cell one palette step off the
/// prototype in `tools/splash/`.
fn round_half_even(value: f64) -> f64 {
    let rounded = value.round();
    if (value - value.trunc()).abs() == 0.5 && rounded % 2.0 != 0.0 {
        rounded - value.signum()
    } else {
        rounded
    }
}

// ---------------------------------------------------------------------------
// Frame model — every cell is either photograph or letter, and the animation
// is the migration of cells between those two states
// ---------------------------------------------------------------------------

/// One cell of a rendered frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplashCell {
    /// Quadrant glyph carrying two colours from the asset.
    Photo { mask: u8, fg: Rgb, bg: Rgb },
    /// One of `d e l t a v` in a single colour.
    Letter { ch: char, colour: Rgb },
}

/// A full 72×20 grid of cell descriptions for one instant.
#[derive(Debug, Clone)]
pub struct SplashFrame {
    cells: [SplashCell; CELL_COUNT],
}

impl SplashFrame {
    /// The cell at `(col, row)`.
    ///
    /// # Panics
    /// If `col >= PANEL_COLS` or `row >= PANEL_ROWS`.
    pub fn cell(&self, col: u16, row: u16) -> SplashCell {
        assert!(col < PANEL_COLS && row < PANEL_ROWS, "cell out of panel");
        self.cells[row as usize * PANEL_COLS as usize + col as usize]
    }

    /// How many cells currently show the photograph.
    #[cfg(test)]
    pub fn photo_count(&self) -> usize {
        self.cells
            .iter()
            .filter(|cell| matches!(cell, SplashCell::Photo { .. }))
            .count()
    }
}

/// Which palette the terminal can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColourTier {
    /// 24-bit RGB — the full photograph.
    Truecolor,
    /// 256-colour — the photograph as a 24-step grey ramp.
    Greyscale,
    /// Below 256 colours — wordmark sequence only, no photograph.
    Mono,
}

impl ColourTier {
    /// Detect the tier from the environment, once per process.
    pub fn detect() -> Self {
        static CACHE: OnceLock<ColourTier> = OnceLock::new();
        *CACHE.get_or_init(|| {
            let colorterm = std::env::var("COLORTERM").ok();
            let term = std::env::var("TERM").ok();
            Self::from_env(colorterm.as_deref(), term.as_deref())
        })
    }

    fn from_env(colorterm: Option<&str>, term: Option<&str>) -> Self {
        if matches!(colorterm, Some("truecolor" | "24bit")) {
            Self::Truecolor
        } else if term.is_some_and(|term| term.contains("256color")) {
            Self::Greyscale
        } else {
            Self::Mono
        }
    }

    /// Total length of this tier's sequence.
    pub fn duration(self) -> Duration {
        Duration::from_secs_f64(if self == Self::Mono {
            MONO_SEQUENCE_ENDS
        } else {
            SEQUENCE_ENDS
        })
    }
}

/// Build the frame for `elapsed` into the sequence.
///
/// Colours are always RGB; `tier` is read only by [`ColourTier::Mono`], which
/// changes cell *types* — it never emits a photo cell, so the sequence reduces
/// to the drain and the wordmark on its own shorter timeline.
pub fn frame_at(elapsed: Duration, tier: ColourTier) -> SplashFrame {
    let secs = elapsed.as_secs_f64();
    let mono = tier == ColourTier::Mono;
    let asset = asset();
    let word = wordmark();

    let (ghost, resolve, settle, finale) = if mono {
        (
            0.0,
            0.0,
            lerp(0.0, 1.0, (secs - MONO_FIELD_ENDS) / (MONO_SETTLE_ENDS - MONO_FIELD_ENDS)),
            wordmark_ramp(secs, MONO_SETTLE_ENDS, MONO_SEQUENCE_ENDS),
        )
    } else {
        (
            lerp(0.0, 1.0, (secs - STATIC_ENDS) / (GHOST_ENDS - STATIC_ENDS)),
            lerp(0.0, 1.0, (secs - GHOST_ENDS) / (RESOLVE_ENDS - GHOST_ENDS)),
            lerp(0.0, 1.0, (secs - HOLD_ENDS) / (SETTLE_ENDS - HOLD_ENDS)),
            wordmark_ramp(secs, SETTLE_ENDS, SEQUENCE_ENDS),
        )
    };

    let tick = (secs * FPS) as u32;
    let last_row = f64::from(PANEL_ROWS - 1);
    let mut cells = [SplashCell::Letter {
        ch: ' ',
        colour: (0, 0, 0),
    }; CELL_COUNT];

    for (index, out_cell) in cells.iter_mut().enumerate() {
        let (col, row) = (index % PANEL_COLS as usize, index / PANEL_COLS as usize);
        let (hash_col, hash_row) = (col as u32, row as u32);
        let row_fraction = f64::from(row as u16) / last_row;

        // Mostly top-down but ragged, so the reveal reads as a slow-scan
        // downlink rather than a wipe; the drain reverses the row bias.
        let order = row_fraction * 0.7 + hash01(hash_col, hash_row, 1) * 0.3;
        let drain = hash01(hash_col, hash_row, 2) * 0.6 + (1.0 - row_fraction) * 0.4;

        let in_word = word[index];
        let resolved = !mono && resolve > order;
        let reverted = settle > drain;

        if resolved && !reverted {
            let cell = asset.cells[index];
            *out_cell = SplashCell::Photo {
                mask: cell.mask,
                fg: cell.fg,
                bg: cell.bg,
            };
            continue;
        }

        let ch = match in_word {
            Some(letter) if settle > 0.0 => letter,
            // Letters re-roll every third frame so the field feels alive.
            _ => {
                let letter_index =
                    (hash01(hash_col, hash_row, tick / 3) * WORD.len() as f64) as usize;
                WORD[letter_index % WORD.len()]
            }
        };

        let base = if mono { FIELD_GREY } else { asset.blend[index] };
        let colour = if reverted {
            match in_word {
                Some(_) => dim(WHITE, 0.35 + 0.65 * finale),
                None => dim(base, 0.55 * (1.0 - settle)),
            }
        } else if in_word.is_some() && finale > 0.0 {
            dim(WHITE, finale)
        } else {
            let warm = |from: u8, to: u8| lerp(f64::from(from), f64::from(to) * 0.75, ghost) as u8;
            (
                warm(FIELD_GREY.0, base.0),
                warm(FIELD_GREY.1, base.1),
                warm(FIELD_GREY.2, base.2),
            )
        };

        *out_cell = SplashCell::Letter { ch, colour };
    }

    SplashFrame { cells }
}

/// Colour of the skip hint, or `None` while it is hidden.
///
/// Blinks on a 500 ms cycle derived from elapsed time, and disappears once
/// settling starts so it never competes with the wordmark.
fn hint_colour(elapsed: Duration, tier: ColourTier) -> Option<Rgb> {
    let secs = elapsed.as_secs_f64();
    let (start, end) = if tier == ColourTier::Mono {
        (0.25, MONO_FIELD_ENDS)
    } else {
        (1.0, SETTLE_ENDS)
    };
    if secs < start || secs > end {
        return None;
    }
    Some(if (secs * 2.0) as i64 % 2 == 0 {
        (200, 200, 200)
    } else {
        (90, 94, 98)
    })
}

// ---------------------------------------------------------------------------
// Palette mapping
// ---------------------------------------------------------------------------

fn grey_index(rgb: Rgb) -> u8 {
    round_half_even((luminance(rgb) - 8.0) / 10.0).clamp(0.0, 23.0) as u8
}

fn mono_index(rgb: Rgb, lowest: f64, highest: f64) -> usize {
    let position = if highest > lowest {
        (luminance(rgb) - lowest) / (highest - lowest)
    } else {
        0.0
    };
    round_half_even(position * 3.0).clamp(0.0, 3.0) as usize
}

/// xterm 232..255 is the 24-step grey ramp.
const GREY_BASE: u8 = 232;
const MONO_COLOURS: [Color; 4] = [Color::Black, Color::DarkGray, Color::Gray, Color::White];

fn tier_colour(rgb: Rgb, tier: ColourTier, ghost_luminance_range: (f64, f64)) -> Color {
    match tier {
        ColourTier::Truecolor => Color::Rgb(rgb.0, rgb.1, rgb.2),
        ColourTier::Greyscale => Color::Indexed(GREY_BASE + grey_index(rgb)),
        ColourTier::Mono => {
            MONO_COLOURS[mono_index(rgb, ghost_luminance_range.0, ghost_luminance_range.1)]
        }
    }
}

const QUADRANTS: [&str; 16] = [
    " ", "▗", "▖", "▄", "▝", "▐", "▞", "▟", "▘", "▚", "▌", "▙", "▀", "▜", "▛", "█",
];

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

/// The bordered block within `area`, centred and never scaled.
fn block_rect(area: Rect) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(BLOCK_COLS) / 2,
        y: area.y + area.height.saturating_sub(BLOCK_ROWS) / 2,
        width: BLOCK_COLS.min(area.width),
        height: BLOCK_ROWS.min(area.height),
    }
}

/// Top-left corner of the panel within `area` — just inside the border.
pub fn panel_origin(area: Rect) -> (u16, u16) {
    let block = block_rect(area);
    (block.x + 1, block.y + 1)
}

/// Draw the splash for `elapsed` into `area`.
pub fn render_splash(frame: &mut Frame, area: Rect, elapsed: Duration, tier: ColourTier) {
    let ghost_luminance_range = asset().ghost_luminance_range;
    let grid = frame_at(elapsed, tier);
    let block = block_rect(area);

    // The same rounded frame the list and detail views wear, so the splash
    // reads as part of the app rather than as something in front of it.
    let mut title_bar = TitleBar::default();
    if let Some(colour) = hint_colour(elapsed, tier) {
        title_bar = title_bar.bottom(Line::from(Span::styled(
            format!(" {HINT} "),
            Style::default().fg(tier_colour(colour, tier, ghost_luminance_range)),
        )));
    }
    frame.render_widget(title_bar.build(), block);

    let (origin_col, origin_row) = panel_origin(area);
    let buffer = frame.buffer_mut();

    for row in 0..PANEL_ROWS {
        for col in 0..PANEL_COLS {
            let Some(target) = buffer.cell_mut((origin_col + col, origin_row + row)) else {
                continue;
            };
            match grid.cell(col, row) {
                SplashCell::Photo { mask, fg, bg } => {
                    target.set_symbol(QUADRANTS[mask as usize]);
                    target.set_style(
                        Style::default()
                            .fg(tier_colour(fg, tier, ghost_luminance_range))
                            .bg(tier_colour(bg, tier, ghost_luminance_range)),
                    );
                }
                SplashCell::Letter { ch, colour } => {
                    target.set_char(ch);
                    target.set_style(Style::default().fg(tier_colour(
                        colour,
                        tier,
                        ghost_luminance_range,
                    )));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn at(secs: f64) -> Duration {
        Duration::from_secs_f64(secs)
    }

    fn photo_at(secs: f64) -> usize {
        frame_at(at(secs), ColourTier::Truecolor).photo_count()
    }

    // --- asset integrity ---

    #[test]
    fn asset_is_one_cell_per_panel_position() {
        assert_eq!(ASSET.len(), 10_080);
        assert_eq!(ASSET.len(), CELL_COUNT * BYTES_PER_CELL);
    }

    #[test]
    fn every_mask_indexes_the_quadrant_table() {
        for (index, chunk) in ASSET.chunks(BYTES_PER_CELL).enumerate() {
            assert!(
                (chunk[0] as usize) < QUADRANTS.len(),
                "cell {index} has mask {}, outside the 16 quadrant glyphs",
                chunk[0]
            );
        }
    }

    #[test]
    fn blend_is_the_mean_of_a_fully_lit_cell() {
        // A full mask means every sub-pixel takes fg, so the blend is just fg.
        let asset = asset();
        for (index, cell) in asset.cells.iter().enumerate() {
            if cell.mask == 0b1111 {
                assert_eq!(asset.blend[index], cell.fg);
            }
        }
    }

    // --- phase boundaries ---

    #[test]
    fn no_photograph_before_the_resolve_begins() {
        assert_eq!(photo_at(0.0), 0);
        assert_eq!(photo_at(STATIC_ENDS), 0);
        assert_eq!(photo_at(GHOST_ENDS), 0);
    }

    #[test]
    fn photograph_is_whole_through_the_hold() {
        assert_eq!(photo_at(RESOLVE_ENDS), CELL_COUNT);
        assert_eq!(photo_at(2.6), CELL_COUNT);
        assert_eq!(photo_at(HOLD_ENDS), CELL_COUNT);
    }

    #[test]
    fn every_cell_is_a_letter_by_the_end() {
        assert_eq!(photo_at(SEQUENCE_ENDS), 0);
        let frame = frame_at(at(SEQUENCE_ENDS), ColourTier::Truecolor);
        for row in 0..PANEL_ROWS {
            for col in 0..PANEL_COLS {
                assert!(matches!(frame.cell(col, row), SplashCell::Letter { .. }));
            }
        }
    }

    #[test]
    fn resolve_only_ever_adds_photo_cells() {
        let mut prev = 0;
        for step in 0..=18 {
            let count = photo_at(GHOST_ENDS + f64::from(step) * 0.05);
            assert!(count >= prev, "photo count fell from {prev} to {count}");
            prev = count;
        }
        assert_eq!(prev, CELL_COUNT);
    }

    #[test]
    fn settle_only_ever_removes_photo_cells() {
        let mut prev = CELL_COUNT;
        for step in 0..=14 {
            let count = photo_at(HOLD_ENDS + f64::from(step) * 0.05);
            assert!(count <= prev, "photo count rose from {prev} to {count}");
            prev = count;
        }
        assert_eq!(prev, 0);
    }

    // --- wordmark ---

    #[test]
    fn wordmark_occupies_65_by_7_cells() {
        let mask = wordmark();
        let set: Vec<usize> = (0..CELL_COUNT)
            .filter(|index| mask[*index].is_some())
            .collect();
        let cols: Vec<usize> = set
            .iter()
            .map(|index| index % PANEL_COLS as usize)
            .collect();
        let rows: Vec<usize> = set
            .iter()
            .map(|index| index / PANEL_COLS as usize)
            .collect();

        let width = cols.iter().max().unwrap() - cols.iter().min().unwrap() + 1;
        let height = rows.iter().max().unwrap() - rows.iter().min().unwrap() + 1;
        assert_eq!((width, height), (65, 7));
    }

    #[test]
    fn wordmark_is_spelled_in_its_own_letters() {
        let mask = wordmark();
        let letters: std::collections::BTreeSet<char> = mask.iter().flatten().copied().collect();
        assert_eq!(letters, WORD.iter().copied().collect());
    }

    #[test]
    fn wordmark_cells_lock_to_their_letter_once_settling() {
        let mask = wordmark();
        for secs in [3.2, 3.5, SETTLE_ENDS, 4.0, SEQUENCE_ENDS] {
            let frame = frame_at(at(secs), ColourTier::Truecolor);
            for (index, expected) in mask
                .iter()
                .enumerate()
                .filter_map(|(index, letter)| letter.map(|ch| (index, ch)))
            {
                let (col, row) = (index % PANEL_COLS as usize, index / PANEL_COLS as usize);
                if let SplashCell::Letter { ch, .. } = frame.cell(col as u16, row as u16) {
                    assert_eq!(ch, expected, "wrong letter at {col},{row} at t={secs}");
                }
            }
        }
    }

    #[test]
    fn wordmark_is_fully_assembled_at_the_end() {
        let mask = wordmark();
        let frame = frame_at(at(SEQUENCE_ENDS), ColourTier::Truecolor);
        for (index, expected) in mask
            .iter()
            .enumerate()
            .filter_map(|(index, letter)| letter.map(|ch| (index, ch)))
        {
            let (col, row) = (index % PANEL_COLS as usize, index / PANEL_COLS as usize);
            match frame.cell(col as u16, row as u16) {
                SplashCell::Letter { ch, .. } => assert_eq!(ch, expected),
                other => panic!("wordmark cell is not a letter: {other:?}"),
            }
        }
    }

    // --- colour tiers ---

    #[test]
    fn tier_is_detected_from_colorterm_then_term() {
        use ColourTier::{Greyscale, Mono, Truecolor};
        assert_eq!(ColourTier::from_env(Some("truecolor"), None), Truecolor);
        assert_eq!(ColourTier::from_env(Some("24bit"), None), Truecolor);
        assert_eq!(ColourTier::from_env(Some("truecolor"), Some("xterm")), Truecolor);
        assert_eq!(ColourTier::from_env(None, Some("xterm-256color")), Greyscale);
        assert_eq!(ColourTier::from_env(Some("8bit"), Some("screen-256color")), Greyscale);
        assert_eq!(ColourTier::from_env(None, Some("xterm")), Mono);
        assert_eq!(ColourTier::from_env(None, None), Mono);
    }

    #[test]
    fn mono_tier_never_shows_the_photograph() {
        for step in 0..=16 {
            let secs = f64::from(step) * 0.1;
            let frame = frame_at(at(secs), ColourTier::Mono);
            assert_eq!(frame.photo_count(), 0, "photo cell leaked at t={secs}");
        }
    }

    #[test]
    fn mono_tier_runs_the_shorter_sequence() {
        assert_eq!(ColourTier::Mono.duration(), Duration::from_secs_f64(1.6));
        assert_eq!(ColourTier::Truecolor.duration(), Duration::from_secs_f64(4.2));
        assert_eq!(ColourTier::Greyscale.duration(), ColourTier::Truecolor.duration());
    }

    #[test]
    fn mono_wordmark_assembles_by_the_end_of_its_own_timeline() {
        let mask = wordmark();
        let frame = frame_at(at(MONO_SEQUENCE_ENDS), ColourTier::Mono);
        for (index, expected) in mask
            .iter()
            .enumerate()
            .filter_map(|(index, letter)| letter.map(|ch| (index, ch)))
        {
            let (col, row) = (index % PANEL_COLS as usize, index / PANEL_COLS as usize);
            match frame.cell(col as u16, row as u16) {
                SplashCell::Letter { ch, .. } => assert_eq!(ch, expected),
                other => panic!("wordmark cell is not a letter: {other:?}"),
            }
        }
    }

    #[test]
    fn greyscale_maps_into_the_xterm_grey_ramp() {
        for rgb in [(0, 0, 0), (128, 128, 128), (255, 255, 255), (70, 74, 78)] {
            match tier_colour(rgb, ColourTier::Greyscale, (0.0, 255.0)) {
                Color::Indexed(index) => {
                    assert!((232..=255).contains(&index), "index {index} off ramp")
                }
                other => panic!("expected an indexed colour, got {other:?}"),
            }
        }
    }

    #[test]
    fn python_style_rounding_breaks_ties_to_even() {
        assert_eq!(round_half_even(0.5), 0.0);
        assert_eq!(round_half_even(1.5), 2.0);
        assert_eq!(round_half_even(2.5), 2.0);
        assert_eq!(round_half_even(3.5), 4.0);
        assert_eq!(round_half_even(1.4), 1.0);
        assert_eq!(round_half_even(1.6), 2.0);
    }

    // --- skip hint ---

    #[test]
    fn hint_appears_after_a_beat_and_hides_before_the_wordmark() {
        assert!(hint_colour(at(0.0), ColourTier::Truecolor).is_none());
        assert!(hint_colour(at(0.9), ColourTier::Truecolor).is_none());
        assert!(hint_colour(at(1.0), ColourTier::Truecolor).is_some());
        assert!(hint_colour(at(2.5), ColourTier::Truecolor).is_some());
        assert!(hint_colour(at(SETTLE_ENDS + 0.01), ColourTier::Truecolor).is_none());
    }

    #[test]
    fn hint_comes_in_earlier_on_the_short_sequence() {
        assert!(hint_colour(at(0.3), ColourTier::Mono).is_some());
        assert!(hint_colour(at(MONO_FIELD_ENDS + 0.01), ColourTier::Mono).is_none());
    }

    // --- centring ---

    #[test]
    fn panel_lands_on_the_budget_at_the_minimum_terminal() {
        // A margin row above and below the 22-row block, per §6.0.
        assert_eq!(block_rect(Rect::new(0, 0, 80, 24)), Rect::new(3, 1, 74, 22));
        assert_eq!(panel_origin(Rect::new(0, 0, 80, 24)), (4, 2));
    }

    #[test]
    fn panel_is_centred_and_never_scaled_on_larger_terminals() {
        assert_eq!(panel_origin(Rect::new(0, 0, 120, 40)), (24, 10));
        assert_eq!(panel_origin(Rect::new(0, 0, 100, 30)), (14, 5));
    }

    #[test]
    fn panel_origin_never_underflows_below_the_minimum() {
        assert_eq!(panel_origin(Rect::new(0, 0, 40, 10)), (1, 1));
    }

    // --- renderer ---

    fn render_to_buffer(secs: f64, tier: ColourTier) -> ratatui::buffer::Buffer {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| render_splash(frame, frame.area(), at(secs), tier))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    #[test]
    fn frames_are_byte_identical_run_to_run() {
        for secs in [0.2, 1.1, 2.6, 4.1] {
            assert_eq!(
                render_to_buffer(secs, ColourTier::Truecolor),
                render_to_buffer(secs, ColourTier::Truecolor),
                "frame at t={secs} is not deterministic"
            );
        }
    }

    #[test]
    fn hold_frame_draws_quadrant_glyphs() {
        let buf = render_to_buffer(2.6, ColourTier::Truecolor);
        let (origin_col, origin_row) = panel_origin(Rect::new(0, 0, 80, 24));
        let mut quadrants = 0;
        for row in 0..PANEL_ROWS {
            for col in 0..PANEL_COLS {
                if QUADRANTS.contains(&buf[(origin_col + col, origin_row + row)].symbol()) {
                    quadrants += 1;
                }
            }
        }
        assert_eq!(quadrants, CELL_COUNT);
    }

    #[test]
    fn static_frame_draws_only_the_six_letters() {
        let buf = render_to_buffer(0.1, ColourTier::Truecolor);
        let (origin_col, origin_row) = panel_origin(Rect::new(0, 0, 80, 24));
        for row in 0..PANEL_ROWS {
            for col in 0..PANEL_COLS {
                let symbol = buf[(origin_col + col, origin_row + row)].symbol();
                let ch = symbol.chars().next().unwrap();
                assert!(WORD.contains(&ch), "unexpected glyph {symbol:?} in letter field");
            }
        }
    }

    /// The bottom border row, which the hint rides as a block title.
    fn hint_row_text(buf: &ratatui::buffer::Buffer) -> String {
        let block = block_rect(Rect::new(0, 0, 80, 24));
        (block.x..block.x + block.width)
            .map(|col| buf[(col, block.y + block.height - 1)].symbol())
            .collect()
    }

    #[test]
    fn skip_hint_rides_the_bottom_border_while_visible() {
        let text = hint_row_text(&render_to_buffer(1.1, ColourTier::Truecolor));
        assert!(text.contains("PRESS ANY KEY TO SKIP"), "hint row was {text:?}");
    }

    #[test]
    fn hint_row_falls_back_to_a_plain_border_for_the_wordmark() {
        let text = hint_row_text(&render_to_buffer(4.1, ColourTier::Truecolor));
        assert!(!text.contains("PRESS"), "hint still visible: {text:?}");
        assert_eq!(text, format!("╰{}╯", "─".repeat(PANEL_COLS as usize)));
    }

    #[test]
    fn panel_is_framed_in_the_apps_rounded_border() {
        let buf = render_to_buffer(2.6, ColourTier::Truecolor);
        let block = block_rect(Rect::new(0, 0, 80, 24));
        let (right, bottom) = (block.x + block.width - 1, block.y + block.height - 1);
        for (pos, corner) in [
            ((block.x, block.y), "╭"),
            ((right, block.y), "╮"),
            ((block.x, bottom), "╰"),
            ((right, bottom), "╯"),
        ] {
            assert_eq!(buf[pos].symbol(), corner, "wrong corner at {pos:?}");
        }
        assert_eq!(buf[(block.x, block.y)].style().fg, Some(Color::DarkGray));
    }

    #[test]
    fn the_margin_rows_either_side_of_the_block_stay_clear() {
        let buf = render_to_buffer(2.6, ColourTier::Truecolor);
        for row in [0, 23] {
            for col in 0..80 {
                assert_eq!(buf[(col, row)].symbol(), " ", "margin row {row} is painted");
            }
        }
    }

    // --- wordmark hold (§12.1) ---

    /// Brightness of a known wordmark cell, which ramps to white then holds.
    fn wordmark_brightness(secs: f64, tier: ColourTier) -> Rgb {
        let index = wordmark().iter().position(Option::is_some).unwrap();
        let frame = frame_at(at(secs), tier);
        let (col, row) = (index % PANEL_COLS as usize, index / PANEL_COLS as usize);
        match frame.cell(col as u16, row as u16) {
            SplashCell::Letter { colour, .. } => colour,
            other => panic!("wordmark cell is not a letter: {other:?}"),
        }
    }

    #[test]
    fn wordmark_reaches_full_brightness_before_the_cut_and_holds() {
        assert!(
            wordmark_brightness(SETTLE_ENDS, ColourTier::Truecolor).0 < WHITE.0,
            "wordmark started the phase already at full brightness"
        );
        // Full white for the rest of the phase, not still arriving at the cut —
        // the whole point of §12.1. Sampled just past the boundary because the
        // ramp divides out to a hair under 1.0 at exactly WORDMARK_RAMP_SECS.
        for secs in [SETTLE_ENDS + WORDMARK_RAMP_SECS + 0.01, 4.0, SEQUENCE_ENDS] {
            assert_eq!(
                wordmark_brightness(secs, ColourTier::Truecolor),
                WHITE,
                "wordmark was not held at full brightness at t={secs}"
            );
        }
    }

    #[test]
    fn the_short_sequence_holds_its_wordmark_too() {
        let held = MONO_SETTLE_ENDS + WORDMARK_RAMP_SECS + 0.01;
        assert_eq!(wordmark_brightness(held, ColourTier::Mono), WHITE);
        assert_eq!(wordmark_brightness(MONO_SEQUENCE_ENDS, ColourTier::Mono), WHITE);
    }
}
