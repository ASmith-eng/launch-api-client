#!/usr/bin/env python3
"""Generate the fixed-size splash asset from the source artwork.

    python3 tools/splash/build_splash_asset.py           # regenerate
    python3 tools/splash/build_splash_asset.py --check   # verify, don't write


You will need to re-run this if you have updated the artwork source for the splash
screen.

To change the artwork, drop a replacement in as `source/artwork.<ext>` (delete
the old one) and re-run. Running this script also generates a hash that can be
used to check if the artwork has been changed since the last asset generation.

To run the check call the script with the `--check` flag.

The 2-colour quantisation is done here rather than at runtime: the app never
does image processing, and this tool is only used to generate the cell maps
used in the final build.
"""

import argparse
import glob
import hashlib
import os
import struct
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
SOURCE_DIR = os.path.join(HERE, "source")
SOURCE_STEM = "artwork"
SOURCE_HASH = os.path.join(SOURCE_DIR, SOURCE_STEM + ".sha256")
OUT = os.path.join(ROOT, "assets", "splash.cells")
SCRIPT_REL = os.path.relpath(os.path.abspath(__file__), ROOT)

# Ceiling on the *decoded* source, as a decompression-bomb guard: a few hundred
# KB of crafted PNG can expand to gigabytes of RGB and take the build machine
# down. Deliberately generous — this allows ~10000x10000 against the current
# artwork's 3892x3892 (~15 MP), so a larger replacement needs no change here.
# It is a resource guard, not a quality limit: everything is resampled to
# WORK_PX regardless, so nothing above that size buys any fidelity. Raise it if
# a genuinely larger source ever turns up; the error message says as much.
MAX_SOURCE_PIXELS = 100_000_000

PANEL_COLS = 72
PANEL_ROWS = 20
PANEL_PX_W = PANEL_COLS * 2  # quadrants: 2x2 pixels per cell
PANEL_PX_H = PANEL_ROWS * 2
BYTES_PER_CELL = 7            # mask + fg rgb + bg rgb

# On-screen aspect of the panel. A character cell is ~1:2 (w:h), so PANEL_ROWS
# rows are 2*PANEL_ROWS units tall against PANEL_COLS units wide. Independent of
# how many samples we pack into each cell.
PANEL_ASPECT = PANEL_COLS / (PANEL_ROWS * 2)

# Vertical centre of the crop, as a fraction of source height. The source is
# square (3892x3892); the panel is 1.8:1, so we take a full-width band centred
# on the LM. 0.44 keeps the flag, foil, legs, footpads and surface texture,
# trimming only the rendezvous-antenna tip.
CROP_CENTRE_Y = 0.44
WORK_PX = 1024  # intermediate decode size; ~25x the panel height, ample


def decode_bmp(path):
    with open(path, "rb") as fh:
        data = fh.read()
    offset = struct.unpack_from("<I", data, 10)[0]
    width, height = struct.unpack_from("<ii", data, 18)
    bpp = struct.unpack_from("<H", data, 28)[0]
    if bpp != 24:
        raise SystemExit(f"expected 24bpp BMP, got {bpp}")
    bottom_up = height > 0
    height = abs(height)
    stride = (width * 3 + 3) // 4 * 4
    rows = []
    for y in range(height):
        sy = (height - 1 - y) if bottom_up else y
        base = offset + sy * stride
        row = []
        for x in range(width):
            b, g, r = data[base + x * 3 : base + x * 3 + 3]
            row.append((r, g, b))
        rows.append(row)
    return rows


def find_source():
    """Locate the single `artwork.*` in the source directory.

    Globbing the stem rather than hardcoding an extension is what lets a
    developer drop in a PNG or WebP without touching this script.
    """
    matches = sorted(
        p for p in glob.glob(os.path.join(SOURCE_DIR, SOURCE_STEM + ".*"))
        if not p.endswith((".md", ".txt", ".sha256"))
    )
    rel = os.path.join(os.path.relpath(SOURCE_DIR, ROOT), SOURCE_STEM)
    if not matches:
        raise SystemExit(f"no source artwork found: expected {rel}.<ext>")
    if len(matches) > 1:
        names = ", ".join(os.path.basename(p) for p in matches)
        raise SystemExit(
            f"expected exactly one {rel}.* — found {len(matches)}: {names}\n"
            "remove the ones you are not using so the source is unambiguous"
        )
    return matches[0]


def hash_source(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def read_pinned_hash():
    """The digest recorded in `source/artwork.sha256`, or None if unpinned."""
    if not os.path.exists(SOURCE_HASH):
        return None
    with open(SOURCE_HASH) as fh:
        fields = fh.readline().split()
    return fields[0] if fields else None


def oversize_error(detail):
    return SystemExit(
        f"source artwork is too large to decode safely: {detail}\n"
        f"the ceiling is {MAX_SOURCE_PIXELS / 1e6:,.0f} MP — if an image this "
        f"big is genuinely expected, raise MAX_SOURCE_PIXELS in {SCRIPT_REL}"
    )


def load_via_pillow(path):
    from PIL import Image

    # Align Pillow's own guard with ours, as a backstop. Ours below is the
    # authoritative check: Pillow only warns at 1x this and raises at 2x, which
    # is too fuzzy to rely on for a specific limit.
    Image.MAX_IMAGE_PIXELS = MAX_SOURCE_PIXELS

    try:
        with Image.open(path) as im:
            # Image.open parses the header only, so this rejects an oversized
            # image before a single pixel is decoded — decoding is the step that
            # hands attacker-shaped bytes to libjpeg/libpng/libwebp.
            w, h = im.size
            if w * h > MAX_SOURCE_PIXELS:
                raise oversize_error(f"{w}x{h} ({w * h / 1e6:,.0f} MP)")
            im = im.convert("RGB").resize((WORK_PX, WORK_PX), Image.LANCZOS)
            raw = im.tobytes()
    except Image.DecompressionBombError as exc:
        # Past 2x MAX_IMAGE_PIXELS Pillow raises inside open(), beating our
        # check to it. Convert it so an enormous image gets the same actionable
        # message instead of a traceback.
        raise oversize_error(str(exc).rstrip(".")) from None
    return [
        [
            tuple(raw[(y * WORK_PX + x) * 3:(y * WORK_PX + x) * 3 + 3])
            for x in range(WORK_PX)
        ]
        for y in range(WORK_PX)
    ]


def load_via_sips(path):
    with tempfile.TemporaryDirectory() as tmp:
        bmp = os.path.join(tmp, "work.bmp")
        subprocess.run(
            ["sips", "-s", "format", "bmp",
             "--resampleHeightWidth", str(WORK_PX), str(WORK_PX),
             path, "--out", bmp],
            check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        return decode_bmp(bmp)


def load_source(path):
    try:
        import PIL  # noqa: F401
    except ImportError:
        pass
    else:
        return load_via_pillow(path)

    if sys.platform != "darwin":
        raise SystemExit(
            "Pillow is required to decode the source artwork:\n"
            "    python3 -m pip install pillow"
        )
    print(
        "WARNING: Pillow not installed — falling back to macOS sips.\n"
        "         sips resamples differently, so the asset this produces will\n"
        "         NOT match the committed one and --check will fail in CI.\n"
        "         Install Pillow before regenerating: python3 -m pip install pillow",
        file=sys.stderr,
    )
    return load_via_sips(path)


def resample(src, cols, rows, centre_y, aspect):
    """Full-width crop to `aspect` (w:h), box-filtered down to cols x rows.

    `aspect` is the panel's *physical* aspect, which is not the cols:rows ratio
    of the sample grid — a 2x2 quadrant cell packs 144x40 samples into a panel
    that is still 1.8:1 on screen.
    """
    src_h, src_w = len(src), len(src[0])
    band_h = src_w / aspect
    top = max(0.0, min(src_h - band_h, centre_y * src_h - band_h / 2))
    out = []
    for y in range(rows):
        y0, y1 = top + y * band_h / rows, top + (y + 1) * band_h / rows
        iy0, iy1 = int(y0), max(int(y1), int(y0) + 1)
        line = []
        for x in range(cols):
            x0, x1 = x * src_w / cols, (x + 1) * src_w / cols
            ix0, ix1 = int(x0), max(int(x1), int(x0) + 1)
            acc, n = [0, 0, 0], 0
            for yy in range(iy0, min(iy1, src_h)):
                for xx in range(ix0, min(ix1, src_w)):
                    px = src[yy][xx]
                    acc[0] += px[0]; acc[1] += px[1]; acc[2] += px[2]; n += 1
            line.append(tuple(v // n for v in acc))
        out.append(line)
    return out


def luminance(px):
    return 0.299 * px[0] + 0.587 * px[1] + 0.114 * px[2]


def quantise_cell(quad):
    """Split a 2x2 patch [TL, TR, BL, BR] into two colours.

    Returns (mask, fg, bg) where mask bit 3 = TL, 2 = TR, 1 = BL, 0 = BR and a
    set bit means "this sub-pixel takes fg". Splitting on the luminance midpoint
    keeps edges crisp: the bright and dark sides of a strut each keep their own
    colour instead of averaging into mud.
    """
    lums = [luminance(p) for p in quad]
    mid = (max(lums) + min(lums)) / 2
    hi = [p for p, l in zip(quad, lums) if l >= mid]
    lo = [p for p, l in zip(quad, lums) if l < mid]
    hi = hi or lo
    lo = lo or hi
    fg = tuple(sum(p[c] for p in hi) // len(hi) for c in range(3))
    bg = tuple(sum(p[c] for p in lo) // len(lo) for c in range(3))
    mask = 0
    for i, l in enumerate(lums):
        if l >= mid:
            mask |= 1 << (3 - i)
    return mask, fg, bg


def build(source):
    print(f"decoding {os.path.basename(source)} at {WORK_PX}x{WORK_PX} …")
    src = load_source(source)
    print(f"resampling to {PANEL_PX_W}x{PANEL_PX_H} px "
          f"({PANEL_COLS} cols x {PANEL_ROWS} rows of quadrant cells) …")
    px = resample(src, PANEL_PX_W, PANEL_PX_H, CROP_CENTRE_Y, PANEL_ASPECT)

    print("quantising each 2x2 patch to two colours …")
    out = bytearray()
    for cy in range(PANEL_ROWS):
        for cx in range(PANEL_COLS):
            quad = [px[cy * 2][cx * 2], px[cy * 2][cx * 2 + 1],
                    px[cy * 2 + 1][cx * 2], px[cy * 2 + 1][cx * 2 + 1]]
            mask, fg, bg = quantise_cell(quad)
            out.append(mask)
            out.extend(fg)
            out.extend(bg)

    expect = PANEL_COLS * PANEL_ROWS * BYTES_PER_CELL
    if len(out) != expect:
        raise SystemExit(f"unexpected asset size: {len(out)} != {expect}")
    return bytes(out)


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument(
        "--check", action="store_true",
        help="verify the source hash pin and the committed asset; write nothing",
    )
    args = ap.parse_args()

    source = find_source()
    digest = hash_source(source)
    rel = os.path.relpath(OUT, ROOT)
    hash_rel = os.path.relpath(SOURCE_HASH, ROOT)

    # Deliberately before build(): if the artwork is not the one we pinned, CI
    # refuses it without ever handing those bytes to an image codec.
    if args.check:
        pinned = read_pinned_hash()
        if pinned is None:
            raise SystemExit(f"{hash_rel} is missing — run: python3 {SCRIPT_REL}")
        if pinned != digest:
            raise SystemExit(
                f"source artwork does not match {hash_rel} — it has been "
                "replaced or edited.\n"
                f"  pinned: {pinned}\n"
                f"  actual: {digest}\n"
                "if that change was intended, regenerate with:\n"
                f"    python3 {SCRIPT_REL}\n"
                "and review the artwork, asset and pin changes together."
            )

    data = build(source)

    if args.check:
        if not os.path.exists(OUT):
            raise SystemExit(f"{rel} is missing — run: python3 {SCRIPT_REL}")
        with open(OUT, "rb") as fh:
            committed = fh.read()
        if committed != data:
            raise SystemExit(
                f"{rel} is stale — it does not match the current source artwork.\n"
                f"regenerate it with: python3 {SCRIPT_REL}"
            )
        print(f"{hash_rel} matches the source artwork")
        print(f"{rel} is up to date with the source artwork ({len(data):,} bytes)")
        return

    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "wb") as fh:
        fh.write(data)
    with open(SOURCE_HASH, "w") as fh:
        fh.write(f"{digest}  {os.path.basename(source)}\n")
    print(f"wrote {rel} — {len(data):,} bytes "
          f"({PANEL_COLS}*{PANEL_ROWS}*{BYTES_PER_CELL})")
    print(f"wrote {hash_rel} — {digest}")


if __name__ == "__main__":
    main()
