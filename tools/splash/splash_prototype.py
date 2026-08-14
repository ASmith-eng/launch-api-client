#!/usr/bin/env python3
"""Splash-screen prototype: letter-noise -> photo -> wordmark.
TO BE DELETED once the splash screen development is complete

Design mock for documentation/splash-screen-plan.md §6. Plays the proposed
choreography in a truecolor terminal so the timing and the two transitions can
be judged before any Rust is written. This is the same pure fn(elapsed) -> frame
the plan calls for (R3), written in Python.

    python3 tools/splash/splash_prototype.py                        # play it
    python3 tools/splash/splash_prototype.py --png 0.2,1.8,3.0,3.6  # dump frames

Reads assets/splash.cells — a fixed 72x20 grid of pre-quantised quadrant cells
from the source artwork, built by
tools/splash/build_splash_asset.py. The panel size is fixed to fit the app's
minimum terminal (80x24) and is *centred*, never scaled, on anything larger.
"""

import argparse
import os
import shutil
import struct
import sys
import time
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
ASSET = os.path.join(ROOT, "assets", "splash.cells")

PANEL_COLS = 72
PANEL_ROWS = 20
BYTES_PER_CELL = 7
HINT = "▶ PRESS ANY KEY TO SKIP…"
# The app frames every view in a rounded DarkGray border; bright black is what
# Color::DarkGray emits, and it is available in every colour tier.
BORDER = "\x1b[90m"

# Quadrant block glyphs indexed by mask (bit 3 = TL, 2 = TR, 1 = BL, 0 = BR).
QUADRANTS = [
    " ", "▗", "▖", "▄", "▝", "▐", "▞", "▟",
    "▘", "▚", "▌", "▙", "▀", "▜", "▛", "█",
]

# --- choreography ----------------------------------------------------------
# Authored as phase *durations* (seconds) because that is what you actually
# retime; the absolute boundaries the renderer uses are derived below. Each is
# overridable from the command line so alternative timings can be judged by
# watching rather than on paper — see --hold and friends.
PHASES = {
    "static":   0.15,  # dead noise, no image information
    "ghost":    1.05,  # image colour bleeds into the letters
    "resolve":  0.90,  # letters flip to quadrant cells, image sharpens
    "hold":     0.80,  # full photograph
    "settle":   0.70,  # photo reverts to letters and drains; wordmark ramps up
    "wordmark": 0.60,  # wordmark alone
}

T_STATIC = T_GHOST = T_RESOLVE = T_HOLD = T_SETTLE = T_END = 0.0


def apply_phases(durations):
    """Derive the absolute phase boundaries the renderer reads."""
    global T_STATIC, T_GHOST, T_RESOLVE, T_HOLD, T_SETTLE, T_END
    T_STATIC = durations["static"]
    T_GHOST = T_STATIC + durations["ghost"]
    T_RESOLVE = T_GHOST + durations["resolve"]
    T_HOLD = T_RESOLVE + durations["hold"]
    T_SETTLE = T_HOLD + durations["settle"]
    T_END = T_SETTLE + durations["wordmark"]


apply_phases(PHASES)

# The 16-colour tier has no image beat, so timing it like the full sequence
# would be dead air. It runs its own short choreography — a flat letter field,
# the drain, the wordmark — and never touches the photograph at all.
MONO_PHASES = {"field": 0.40, "settle": 0.80, "wordmark": 0.40}
M_FIELD = MONO_PHASES["field"]
M_SETTLE = M_FIELD + MONO_PHASES["settle"]
M_END = M_SETTLE + MONO_PHASES["wordmark"]

# How long the wordmark takes to reach full brightness, independent of how long
# the wordmark phase lasts. Ramping across the whole phase spends all of it
# arriving, putting peak brightness on the cut to the list — so the finished
# wordmark is never actually seen. The rest of the phase is the hold.
WORDMARK_RAMP = 0.25

# Flat colour of the un-drained letter field when there is no image to tint it.
FIELD_GREY = (70, 74, 78)

FPS = 20

WORD = "deltav"
NOISE_ALPHABET = WORD

# --- 5x7 lowercase block font, just the six letters we need ----------------
# Each font pixel is drawn 2 cells wide: a 5x7 glyph in 1:2 character cells is
# physically 1:2.8 (absurdly condensed), doubling the width gives ~1:1.4.
FONT = {
    "d": ["....#", "....#", ".####", "#...#", "#...#", "#...#", ".####"],
    "e": [".....", ".....", ".###.", "#...#", "#####", "#....", ".###."],
    "l": [".##..", "..#..", "..#..", "..#..", "..#..", "..#..", ".###."],
    "t": [".#...", ".#...", "###..", ".#...", ".#...", ".#..#", "..##."],
    "a": [".....", ".....", ".###.", "....#", ".####", "#...#", ".####"],
    "v": [".....", ".....", "#...#", "#...#", "#...#", ".#.#.", "..#.."],
}
GLYPH_W, GLYPH_H = 5, 7
CELLS_PER_PX = 2


def load_photo():
    """Read the fixed-size asset -> (cells, avg).

    cells: PANEL_ROWS x PANEL_COLS of (mask, fg, bg) — one quadrant glyph each.
    avg:   PANEL_ROWS x PANEL_COLS of a single blended colour, used by the
           letter-noise stage where a cell can only carry one colour. Derived
           from fg/bg weighted by how many sub-pixels each covers, so it is the
           exact mean of the quantised cell.
    """
    if not os.path.exists(ASSET):
        raise SystemExit(
            f"missing {os.path.relpath(ASSET, ROOT)} — "
            "run: python3 tools/splash/build_splash_asset.py"
        )
    raw = open(ASSET, "rb").read()
    expect = PANEL_COLS * PANEL_ROWS * BYTES_PER_CELL
    if len(raw) != expect:
        raise SystemExit(f"asset is {len(raw)} bytes, expected {expect}")

    cells, avg = [], []
    for cy in range(PANEL_ROWS):
        crow, arow = [], []
        for cx in range(PANEL_COLS):
            i = (cy * PANEL_COLS + cx) * BYTES_PER_CELL
            mask = raw[i]
            fg = (raw[i + 1], raw[i + 2], raw[i + 3])
            bg = (raw[i + 4], raw[i + 5], raw[i + 6])
            crow.append((mask, fg, bg))
            lit = bin(mask).count("1")
            arow.append(tuple((fg[c] * lit + bg[c] * (4 - lit)) // 4 for c in range(3)))
        cells.append(crow)
        avg.append(arow)
    return cells, avg


def hash01(x, y, salt=0):
    """Deterministic per-cell value in [0, 1) — stable across frames."""
    h = (x * 73856093) ^ (y * 19349663) ^ (salt * 83492791)
    h = (h ^ (h >> 13)) * 1274126177 & 0xFFFFFFFF
    return ((h ^ (h >> 16)) & 0xFFFF) / 65536.0


def wordmark_mask():
    """Map (col, row) -> letter for the big block wordmark, centred in the panel."""
    letter_w = GLYPH_W * CELLS_PER_PX
    width = len(WORD) * (letter_w + 1) - 1
    x0 = (PANEL_COLS - width) // 2
    y0 = (PANEL_ROWS - GLYPH_H) // 2
    mask = {}
    for i, letter in enumerate(WORD):
        gx = x0 + i * (letter_w + 1)
        for gy, line in enumerate(FONT[letter]):
            for dx, bit in enumerate(line):
                if bit == "#":
                    for k in range(CELLS_PER_PX):
                        mask[(gx + dx * CELLS_PER_PX + k, y0 + gy)] = letter
    return mask


def lerp(a, b, t):
    return a + (b - a) * max(0.0, min(1.0, t))


def dim(rgb, factor):
    f = max(0.0, min(1.0, factor))
    return tuple(int(v * f) for v in rgb)


# --- colour tiers (§6.5) ----------------------------------------------------
# Each tier emits the escape codes it would really use, so a preview shows what
# the terminal does rather than a truecolor imitation of it.
#
#   color  truecolor RGB                         — the shipping default
#   grey   xterm 232..255 greyscale ramp         — the locked 256-colour tier
#   mono   the four theme greys, letters only    — prototyped and REJECTED
#          (§6.5); kept only so the rejection stays reviewable
TIERS = ("color", "grey", "mono")

GREY_RAMP = [(8 + 10 * i,) * 3 for i in range(24)]   # xterm 232..255
MONO_RGB = [(0, 0, 0), (88, 88, 88), (170, 170, 170), (255, 255, 255)]
MONO_ANSI = [30, 90, 37, 97]  # black, bright black, white, bright white


def luminance(rgb):
    return 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2]


def grey_index(rgb):
    return min(23, max(0, round((luminance(rgb) - 8) / 10)))


def mono_index(rgb, lo, hi):
    """Stretched across the image's own range: without it the brightest of the
    four levels is never reached and legibility drops further still."""
    t = (luminance(rgb) - lo) / (hi - lo) if hi > lo else 0.0
    return min(3, max(0, round(t * 3)))


def tier_rgb(rgb, tier, lo=0.0, hi=255.0):
    """The colour a tier actually displays — used for PNG dumps."""
    if tier == "grey":
        return GREY_RAMP[grey_index(rgb)]
    if tier == "mono":
        return MONO_RGB[mono_index(rgb, lo, hi)]
    return rgb


def tier_fg(rgb, tier, lo=0.0, hi=255.0):
    if tier == "grey":
        return f"\x1b[38;5;{232 + grey_index(rgb)}m"
    if tier == "mono":
        return f"\x1b[{MONO_ANSI[mono_index(rgb, lo, hi)]}m"
    return f"\x1b[38;2;{rgb[0]};{rgb[1]};{rgb[2]}m"


def tier_bg(rgb, tier):
    if tier == "grey":
        return f"\x1b[48;5;{232 + grey_index(rgb)}m"
    return f"\x1b[48;2;{rgb[0]};{rgb[1]};{rgb[2]}m"


def wordmark_ramp(elapsed, start, end):
    """Brightness of the closing wordmark, 0 -> 1 across WORDMARK_RAMP."""
    span = WORDMARK_RAMP if 0 < WORDMARK_RAMP < (end - start) else (end - start)
    return lerp(0.0, 1.0, (elapsed - start) / span)


def frame_at(elapsed, state, tier="color"):
    """Pure fn(elapsed) -> list of rows; each cell is ('quad', mask, fg, bg)
    or ('glyph', char, colour, None). This is the shape the Rust renderer wants.

    Colours are emitted as RGB regardless of tier — mapping them to a palette is
    the renderer's job (see tier_fg/tier_bg), which is how the Rust splits it
    too. `tier` is read here only by "mono", which changes cell *types* rather
    than colours: no cell ever becomes a photo cell, so the ghost ramp (already
    clamped past T_GHOST) simply holds at peak through resolve and hold.
    """
    cells, avg = state["cells"], state["avg"]
    mask, tick = state["mask"], int(elapsed * FPS)

    if tier == "mono":
        # No image beat: the field never takes the photograph's colours and no
        # cell ever becomes a photo cell, so only the drain and the wordmark
        # remain — on their own short timeline.
        ghost = resolve = 0.0
        settle = lerp(0.0, 1.0, (elapsed - M_FIELD) / (M_SETTLE - M_FIELD))
        finale = wordmark_ramp(elapsed, M_SETTLE, M_END)
    else:
        ghost = lerp(0.0, 1.0, (elapsed - T_STATIC) / (T_GHOST - T_STATIC))
        resolve = lerp(0.0, 1.0, (elapsed - T_GHOST) / (T_RESOLVE - T_GHOST))
        settle = lerp(0.0, 1.0, (elapsed - T_HOLD) / (T_SETTLE - T_HOLD))
        finale = wordmark_ramp(elapsed, T_SETTLE, T_END)

    out = []
    for cy in range(PANEL_ROWS):
        line = []
        for cx in range(PANEL_COLS):
            # Scanline sweep with jitter: top rows resolve first, like a
            # slow-scan downlink, but ragged enough not to look like a wipe.
            order = (cy / (PANEL_ROWS - 1)) * 0.7 + hash01(cx, cy, 1) * 0.3
            drain = hash01(cx, cy, 2) * 0.6 + (1 - cy / (PANEL_ROWS - 1)) * 0.4

            in_word = (cx, cy) in mask
            resolved = tier != "mono" and resolve > order
            reverted = settle > drain

            if resolved and not reverted:
                qmask, fg, bg = cells[cy][cx]
                line.append(("quad", qmask, fg, bg))
                continue

            # Glyph cell. Letters re-roll slowly so the field feels alive.
            if in_word and settle > 0:
                char = mask[(cx, cy)]
            else:
                idx = int(hash01(cx, cy, tick // 3) * len(NOISE_ALPHABET))
                char = NOISE_ALPHABET[idx % len(NOISE_ALPHABET)]

            base = FIELD_GREY if tier == "mono" else avg[cy][cx]
            if reverted:
                # Draining back out; wordmark cells brighten instead.
                if in_word:
                    colour = dim((255, 255, 255), 0.35 + 0.65 * finale)
                else:
                    colour = dim(base, 0.55 * (1.0 - settle))
            elif in_word and finale > 0:
                colour = dim((255, 255, 255), finale)
            else:
                # Dead grey static warming into the image's own colours.
                colour = tuple(
                    int(lerp(FIELD_GREY[i], base[i] * 0.75, ghost)) for i in range(3))
            line.append(("glyph", char, colour, None))
        out.append(line)
    return out


def hint_style(elapsed, tier="color"):
    """§6.6 — fades in, then blinks on a slow 500ms cycle, hidden once settling
    starts so it never competes with the wordmark. The mono tier's whole
    sequence is shorter than the colour tier's fade-in, so it comes in earlier."""
    start, end = (0.25, M_FIELD) if tier == "mono" else (1.0, T_SETTLE)
    if elapsed < start or elapsed > end:
        return None
    bright = int(elapsed * 2) % 2 == 0
    return (200, 200, 200) if bright else (90, 94, 98)


def paint(frame, elapsed, term, tier="color", lo=0.0, hi=255.0, border=True):
    """Serialise a frame to ANSI for `tier`, centred in the terminal."""
    cols, rows = term
    # Either way the block is two rows taller than the panel: a border top and
    # bottom, or the blank spacer and the free-standing hint row.
    block_w = PANEL_COLS + 2 if border else PANEL_COLS
    block_h = PANEL_ROWS + 2
    pad_x = max(0, (cols - block_w) // 2)
    pad_y = max(0, (rows - block_h) // 2)
    lead = " " * pad_x
    colour = hint_style(elapsed, tier)

    buf = ["\x1b[H\x1b[2J"] + ["\n"] * pad_y
    if border:
        buf.append(lead + BORDER + "╭" + "─" * PANEL_COLS + "╮\x1b[0m\n")

    for row in frame:
        buf.append(lead + (BORDER + "│\x1b[0m" if border else ""))
        for kind, a, b, c in row:
            if kind == "quad":
                buf.append(tier_fg(b, tier, lo, hi) + tier_bg(c, tier) + QUADRANTS[a])
            else:
                buf.append("\x1b[49m" + tier_fg(b, tier, lo, hi) + a)
        buf.append("\x1b[0m" + (BORDER + "│\x1b[0m" if border else "") + "\n")

    if border:
        # The hint rides the bottom edge as a block title: it is chrome, so it
        # reads as chrome, and both margin rows survive.
        edge = "─" * PANEL_COLS
        if colour:
            label = f" {HINT} "
            x = (PANEL_COLS - len(label)) // 2
            edge = (edge[:x] + "\x1b[0m" + tier_fg(colour, tier, lo, hi) + label
                    + BORDER + edge[x + len(label):])
        buf.append(lead + BORDER + "╰" + edge + BORDER + "╯\x1b[0m")
    else:
        buf.append("\n")
        if colour:
            indent = pad_x + (PANEL_COLS - len(HINT)) // 2
            buf.append(" " * indent + tier_fg(colour, tier, lo, hi) + HINT + "\x1b[0m")
    return "".join(buf)


def dump_png(frame, path, cell_w=12, cell_h=24):
    """Render a frame to PNG at simulated 1:2 cell aspect (for review)."""
    h, w = PANEL_ROWS * cell_h, PANEL_COLS * cell_w
    img = [[(0, 0, 0)] * w for _ in range(h)]
    for cy, row in enumerate(frame):
        for cx, (kind, a, b, c) in enumerate(row):
            ox, oy = cx * cell_w, cy * cell_h
            if kind == "quad":
                for y in range(cell_h):
                    for x in range(cell_w):
                        bit = (1 if y < cell_h // 2 else 0) * 2 + (1 if x < cell_w // 2 else 0)
                        shift = [0, 1, 2, 3][bit]  # BR, BL, TR, TL
                        img[oy + y][ox + x] = b if (a >> shift) & 1 else c
            else:
                glyph = FONT.get(a, FONT["v"])
                for gy, line in enumerate(glyph):
                    for gx, bit in enumerate(line):
                        if bit == "#":
                            for dy in range(2):
                                for dx in range(2):
                                    img[oy + 5 + gy * 2 + dy][ox + 1 + gx * 2 + dx] = b
    raw = bytearray()
    for row in img:
        raw += b"\x00"
        for px in row:
            raw += bytes(px)

    def chunk(tag, data):
        block = tag + data
        return (struct.pack(">I", len(data)) + block
                + struct.pack(">I", zlib.crc32(block) & 0xFFFFFFFF))

    png = (b"\x89PNG\r\n\x1a\n"
           + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
           + chunk(b"IEND", b""))
    open(path, "wb").write(png)


def main():
    global WORDMARK_RAMP
    ap = argparse.ArgumentParser()
    ap.add_argument("--png", help="comma-separated times to dump as PNG instead of playing")
    ap.add_argument("--out", default=".", help="directory for --png output")
    ap.add_argument("--tier", choices=TIERS, default="color",
                    help="colour tier to preview (§6.5): color=truecolor, "
                         "grey=256-colour ramp, mono=rejected letters-only variant")
    for name, default in PHASES.items():
        ap.add_argument(f"--{name}", type=float, default=default, metavar="SEC",
                        help=f"{name} phase duration (default {default:g}s)")
    ap.add_argument("--ramp", type=float, default=WORDMARK_RAMP, metavar="SEC",
                    help=f"how long the wordmark takes to reach full brightness "
                         f"(default {WORDMARK_RAMP:g}s; 0 = the whole wordmark phase)")
    ap.add_argument("--no-border", action="store_true",
                    help="drop the rounded frame and put the skip hint on its "
                         "own row below the panel, as v1 shipped")
    args = ap.parse_args()

    WORDMARK_RAMP = args.ramp
    durations = {name: getattr(args, name) for name in PHASES}
    apply_phases(durations)
    if durations != PHASES:
        bounds = [("static", T_STATIC), ("ghost", T_GHOST), ("resolve", T_RESOLVE),
                  ("hold", T_HOLD), ("settle", T_SETTLE), ("wordmark", T_END)]
        print("retimed choreography:")
        prev = 0.0
        for name, end in bounds:
            flag = "  <-- changed" if durations[name] != PHASES[name] else ""
            print(f"  {name:9} {prev:4.2f}-{end:4.2f}  {durations[name]:4.2f}s{flag}")
            prev = end
        print(f"  total     {T_END:.2f}s\n")

    cells, avg = load_photo()
    state = {"cells": cells, "avg": avg, "mask": wordmark_mask()}
    # Luminance range of the ghost palette, for the mono contrast stretch. The
    # 0.75 matches the attenuation frame_at applies to `base`.
    ghost_lums = [luminance(tuple(v * 0.75 for v in c)) for row in avg for c in row]
    lo, hi = min(ghost_lums), max(ghost_lums)
    if args.tier == "mono":
        print(f"mono tier — no image beat, own short timeline:\n"
              f"  field     0.00-{M_FIELD:.2f}  {MONO_PHASES['field']:.2f}s  "
              f"flat letter noise\n"
              f"  settle    {M_FIELD:.2f}-{M_SETTLE:.2f}  "
              f"{MONO_PHASES['settle']:.2f}s  drain; wordmark locks in\n"
              f"  wordmark  {M_SETTLE:.2f}-{M_END:.2f}  "
              f"{MONO_PHASES['wordmark']:.2f}s  deltav alone\n"
              f"  total     {M_END:.2f}s\n")

    if args.png:
        for t in [float(v) for v in args.png.split(",")]:
            path = os.path.join(args.out, f"splash_t{t:.2f}.png")
            frame = frame_at(t, state, args.tier)
            if args.tier != "color":
                frame = [[(k, a, tier_rgb(b, args.tier, lo, hi),
                           tier_rgb(c, args.tier, lo, hi) if c else c)
                          for k, a, b, c in row] for row in frame]
            dump_png(frame, path)
            print(f"wrote {path}")
        return

    term = shutil.get_terminal_size((80, 24))
    if term.columns < 80 or term.lines < 24:
        print(f"terminal is {term.columns}x{term.lines}; the app requires 80x24",
              file=sys.stderr)
        return
    if os.environ.get("COLORTERM") not in ("truecolor", "24bit"):
        print("warning: COLORTERM is not truecolor — colours may be wrong\n",
              file=sys.stderr)

    sys.stdout.write("\x1b[?25l")
    try:
        start = time.time()
        while True:
            elapsed = time.time() - start
            if elapsed > (M_END if args.tier == "mono" else T_END) + 0.6:
                break
            sys.stdout.write(
                paint(frame_at(elapsed, state, args.tier), elapsed, term,
                      args.tier, lo, hi, not args.no_border))
            sys.stdout.flush()
            time.sleep(max(0.0, (1.0 / FPS) - (time.time() - start - elapsed)))
    except KeyboardInterrupt:
        pass
    finally:
        sys.stdout.write("\x1b[?25h\x1b[0m\n")


if __name__ == "__main__":
    main()
