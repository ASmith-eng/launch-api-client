# Splash source artwork

The source image for the artwork used in the startup splash`artwork.*` lives here.
It is the sole input to `../build_splash_asset.py`, which quantises it into
`assets/splash.cells` — the 72×20 grid of quadrant cells the splash screen
renders.

## Current artwork

| | |
|---|---|
| File | `artwork.jpg` |
| Subject | Apollo 14 Lunar Module *Antares* on the lunar surface |
| Source | NASA |
| Licence | Public domain |
| Original filename | `Apollo_14_Lunar_Module_(LM)_on_the_moon.jpg` |
| Dimensions | 3892×3892 |

## Replacing the artwork

1. Delete the existing `artwork.*` and drop the replacement in as
   `artwork.<ext>`. Any format Pillow opens works (JPEG, PNG, WebP); exactly one
   `artwork.*` may be present or the generator refuses to guess.
2. Regenerate: `python3 ../build_splash_asset.py` (needs `pip install pillow`).
3. Preview it: `python3 ../splash_prototype.py`
4. Commit the new artwork **and** the regenerated `assets/splash.cells` together.
   CI fails the PR if they disagree.
5. Update the provenance table above, and §6a of the plan.

A near-square source crops best — the generator takes a full-width band centred
at `CROP_CENTRE_Y` (0.44) of the source height and fits it to the panel's 1.8:1
aspect. Anything above 1024×1024 is discarded during decode (`WORK_PX`), so
there is no benefit to a larger file.
