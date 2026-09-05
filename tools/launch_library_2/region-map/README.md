# Region map generator

Builds `src/vendor/launch_library_2/region_map/table.rs` — the table that turns
a selected region into the `location__ids` value the app queries LL2 with.

LL2 has no country or region filter on the launches endpoint - numeric location IDs are the
only geographic filter option it provides. When a user selects a region filter we need to 
translate this internally into a list of location ids that are all within that region. 
This script is designed to keep the mapping table of region -> LL2 active location id list
up-to-date, and should be run manually if the LL2 data changes.

This approach is favoured to letting the app discover the launch location ids at runtime
to reduce API usage for each user.

## Files

| | |
|---|---|
| `regions.toml` | Which countries make up each region. Hand-written, the only editorial input. |
| `locations.json` | Trimmed snapshot of LL2's location list. Committed so builds and checks are offline and reproducible. |
| `build_region_map.py` | Joins the two and writes the Rust table. |

## How the regions are cut

Named regions follow how launch activity is conventionally tabulated — by
launching state for the six major programmes (US, Europe, Russia/Kazakhstan,
China, India, Japan), geographically for the rest. A purely continental split
was considered and rejected: Russia's sites straddle Europe and Asia, and one
"Asia" arm would merge China, Japan and India — the distinction the filter
exists to make.

The last entry carries `catch_all = true` and no countries. It is generated as
the *complement* of the named regions, so every active site LL2 lists reaches
some region: today that is Sea Launch and the two air-launch pseudo-sites,
which LL2 files under a placeholder country code of `??`, and tomorrow it is
the first spaceport in a country nobody added here. `table.rs` names its
contents in the header and the generator prints them, so promoting one to a
region of its own stays a decision someone makes rather than one that goes
unnoticed.

Locations with no country at all are excluded outright rather than swept into
the catch-all — they are lunar landing sites, not launch sites.

Adding a region is three steps: an entry in `regions.toml`, a matching variant
on `RegionFilter` in `src/models.rs`, and a regeneration. The generated table
matches exhaustively on the enum and supplies the filter panel's cycling order,
so a variant added in one place and not the other is a compile error.

## Regenerating

```sh
TOOL=tools/launch_library_2/region-map/build_region_map.py
python3 $TOOL --fetch   # refresh snapshot from LL2, rebuild
python3 $TOOL           # rebuild from committed snapshot
python3 $TOOL --check   # verify the table is current, write nothing
cargo fmt               # should be a no-op; the generator matches rustfmt
```

Only `--fetch` makes a network request, and it goes to the dev mirror rather
than production. Stdlib only — no `pip install`.

Commit `locations.json` and `table.rs` together. Regenerating without
`--fetch` after editing `regions.toml` keeps the two consistent; `--check`
fails if they have drifted, which makes it suitable for CI.

## When to regenerate

- LL2 bumps its API version.
- A region turns up empty in the app, or a launch site you expect is missing
  from its region.
- You edit `regions.toml`.

After regenerating, read the diff. Sites appearing and disappearing is normal;
a region losing *all* its sites is not, and `cargo test` will fail if that
happens to one of ours.
