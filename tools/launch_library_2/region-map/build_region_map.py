#!/usr/bin/env python3
"""Generate the region filter's location ID table from LL2's location list.

    python3 tools/launch_library_2/region-map/build_region_map.py --fetch   # refresh snapshot, regenerate
    python3 tools/launch_library_2/region-map/build_region_map.py           # regenerate from snapshot
    python3 tools/launch_library_2/region-map/build_region_map.py --check    # verify, don't write

LL2 offers no country or region filter on launches — numeric location IDs are
the only geographic handle it has. So the app ships a table of them, and this
script builds it: `regions.toml` says which countries are in each region, LL2
says which sites are in each country, and the join lands in

    src/vendor/launch_library_2/region_map/table.rs

Regeneration is a maintainer action, run when LL2's location set changes. The
default mode rebuilds from the committed `locations.json` snapshot, so the
normal path and `--check` need no network and give the same answer every time.
Only `--fetch` talks to the API.

Sites are filtered to `active`. Inactive ones cannot appear in upcoming
launches, and several sit in countries that would otherwise misfile them.
"""

import argparse
import json
import os
import sys
import tomllib
import urllib.request
from collections import defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
# tools/launch_library_2/region-map -> crate root.
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(HERE)))
REGIONS_TOML = os.path.join(HERE, "regions.toml")
SNAPSHOT = os.path.join(HERE, "locations.json")
OUT = os.path.join(ROOT, "src", "vendor", "launch_library_2", "region_map", "table.rs")
SCRIPT_REL = os.path.relpath(os.path.abspath(__file__), ROOT)
REGIONS_REL = os.path.relpath(REGIONS_TOML, ROOT)

# The dev mirror carries the same location data as production on far looser
# rate limits, and production's budget belongs to the app's users.
SOURCE_URL = "https://lldev.thespacedevs.com/2.3.0/locations/?limit=250"

# The Space Devs run LL2 on donated time and ask callers to identify
# themselves. The URL is the contact route: it is what they have to go on if
# this script ever starts misbehaving against their mirror.
USER_AGENT = "deltav/1.2.1 (+https://github.com/ASmith-eng/launch-api-client)"

# Refuse to write a table built from an implausibly short location list. `trim`
# already rejects a partial page by comparing against LL2's own count, but an
# errored response reporting zero of zero would satisfy that and empty every
# region at once, which reads downstream as "no launch sites anywhere" rather
# than as a fault.
MIN_PLAUSIBLE_LOCATIONS = 40


def fetch_locations():
    """Fetch the full location list from LL2. The only network call here."""
    request = urllib.request.Request(SOURCE_URL, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=60) as resp:
        return json.load(resp)


def trim(payload):
    """Reduce the API payload to the four fields the region map depends on.

    The raw response is ~150 KB of descriptions, images and launch tallies that
    churn constantly. Committing only these fields keeps the snapshot's diff
    readable and makes it change when the region map would change.
    """
    locations = []
    for r in payload["results"]:
        country = r.get("country")
        locations.append(
            {
                "id": r["id"],
                "name": r["name"],
                "active": r["active"],
                "country": country["alpha_2_code"] if country else None,
            }
        )
    locations.sort(key=lambda loc: loc["id"])

    # LL2 reports its own total, so a short list is a page rather than the
    # whole set. Checking here rather than on the response covers --raw too: a
    # saved page-one file would otherwise build a table missing whole
    # countries and report success.
    if len(locations) != payload["count"]:
        sys.exit(
            f"error: got {len(locations)} of {payload['count']} locations; the "
            f"response is paginated.\n"
            f"       raise the limit so the whole list arrives in one page."
        )

    return {
        "source": SOURCE_URL,
        "count": payload["count"],
        "locations": locations,
    }


def load_snapshot():
    if not os.path.exists(SNAPSHOT):
        sys.exit(
            f"error: no snapshot at {os.path.relpath(SNAPSHOT, ROOT)}\n"
            f"       run with --fetch to create one."
        )
    with open(SNAPSHOT, "r", encoding="utf-8") as fh:
        return json.load(fh)


def write_snapshot(snapshot):
    with open(SNAPSHOT, "w", encoding="utf-8") as fh:
        json.dump(snapshot, fh, indent=2, ensure_ascii=False)
        fh.write("\n")


def load_regions():
    with open(REGIONS_TOML, "rb") as fh:
        data = tomllib.load(fh)

    regions = data.get("region")
    if not regions:
        sys.exit(f"error: no [[region]] entries in {os.path.relpath(REGIONS_TOML, ROOT)}")

    seen_countries = {}
    catch_all = None
    for entry in regions:
        for key in ("variant", "label", "countries"):
            if key not in entry:
                sys.exit(f"error: region entry missing '{key}': {entry}")
        if entry.get("catch_all"):
            if catch_all:
                sys.exit(
                    f"error: {catch_all} and {entry['variant']} are both "
                    f"catch_all; only one region can hold the remainder."
                )
            if entry["countries"]:
                sys.exit(
                    f"error: {entry['variant']} is catch_all and also lists "
                    f"countries. It is defined as what the other regions do "
                    f"not claim, so naming countries would double-count them."
                )
            if entry is not regions[-1]:
                sys.exit(
                    f"error: catch_all region {entry['variant']} must be last, "
                    f"so the filter cycles to it after the named regions."
                )
            catch_all = entry["variant"]
        for code in entry["countries"]:
            if code in seen_countries:
                sys.exit(
                    f"error: country {code} is claimed by both "
                    f"{seen_countries[code]} and {entry['variant']}; a location "
                    f"can only belong to one region."
                )
            seen_countries[code] = entry["variant"]
    return regions


def group(regions, snapshot):
    """Resolve each region to its active location IDs.

    Returns (assignments, no_country, leftover). `leftover` is every active site
    the named regions do not claim — LL2's air- and sea-launch pseudo-sites,
    which it files under a placeholder country whose code is literally "??",
    plus any country nobody has added to regions.toml. A catch_all region takes it;
    without one it is reported and those launches are reachable only under
    "All". Either way it is named here and in the generated header, so a new
    spaceport in an unlisted country is visible rather than silently missing.
    """
    by_country = defaultdict(list)
    no_country = []
    for loc in snapshot["locations"]:
        if not loc["active"]:
            continue
        # Sites with no country object at all are lunar landing sites, not
        # launch sites — they can never match an upcoming launch, so they are
        # excluded outright rather than swept into the catch-all.
        if loc["country"] is None:
            no_country.append(loc)
        else:
            by_country[loc["country"]].append(loc)

    # A code matching no location is indistinguishable downstream from a
    # country with no active sites, so a typo would quietly hand that
    # country's sites to the catch-all. Warn rather than fail: a country
    # falling dark between regenerations is normal and expected.
    unmatched = sorted(
        {code for entry in regions for code in entry["countries"] if code not in by_country}
    )
    if unmatched:
        print(
            f"warning: no active sites for {', '.join(unmatched)}; "
            f"check the code(s) against {REGIONS_REL}",
            file=sys.stderr,
        )

    assignments = []
    claimed = set()
    for entry in regions:
        sites = []
        for code in sorted(entry["countries"]):
            for loc in sorted(by_country.get(code, []), key=lambda l: l["id"]):
                sites.append(loc)
                claimed.add(loc["id"])
        assignments.append((entry, sites))

    leftover = [
        loc for locs in by_country.values() for loc in locs if loc["id"] not in claimed
    ]
    leftover.sort(key=lambda loc: (loc["country"], loc["id"]))

    if assignments and assignments[-1][0].get("catch_all"):
        assignments[-1] = (assignments[-1][0], leftover)

    return assignments, no_country, leftover


def render(assignments, no_country, leftover, snapshot):
    catch_all = assignments[-1][0] if assignments and assignments[-1][0].get("catch_all") else None

    out = []
    w = out.append

    w("//! Region to LL2 location ID table.")
    w("//!")
    w(f"//! @generated by `{SCRIPT_REL}` — do not edit by hand.")
    w("//! Regenerate after LL2 changes its location list, or after editing")
    w(f"//! `{REGIONS_REL}`.")
    w("//!")
    w(f"//! Built from {snapshot['count']} locations, filtered to active sites.")

    if no_country:
        w("//!")
        w("//! Active locations LL2 gives no country. These are lunar landing")
        w("//! sites rather than launch sites, so they are in no region and")
        w("//! cannot appear in the launch list at all:")
        for loc in sorted(no_country, key=lambda l: l["id"]):
            w(f"//!   {loc['id']:>3}  {loc['name']}")

    if leftover:
        w("//!")
        if catch_all:
            w(f"//! Sites no named region claims, held by `{catch_all['variant']}`.")
            w("//! Give one its own region in `regions.toml` if it earns one:")
        else:
            w("//! Active sites in countries no region claims. Add the country to")
            w("//! `regions.toml` if one of these should be filterable:")
        for loc in leftover:
            w(f"//!   {loc['id']:>3}  [{loc['country']}] {loc['name']}")

    w("")
    w("use crate::models::RegionFilter;")
    w("")
    w("/// Every region, in the order the filter panel cycles through them.")
    w("///")
    w(f"/// Ordered by `{REGIONS_REL}`; `All` leads because it is")
    w("/// the unfiltered default.")
    w(f"pub const ALL: [RegionFilter; {len(assignments) + 1}] = [")
    w("    RegionFilter::All,")
    for entry, _ in assignments:
        w(f"    RegionFilter::{entry['variant']},")
    w("];")
    w("")
    w("/// Active LL2 location IDs for a region, empty if it has none.")
    w("pub fn location_ids(region: RegionFilter) -> &'static [u32] {")
    w("    match region {")
    w("        RegionFilter::All => &[],")

    for entry, sites in assignments:
        if not sites:
            w(f"        RegionFilter::{entry['variant']} => &[],")
            continue
        w(f"        RegionFilter::{entry['variant']} => &[")
        # Pad to the widest ID in the block so the comment column lines up the
        # way rustfmt would, keeping `cargo fmt` a no-op on this file.
        width = max(len(str(loc["id"])) for loc in sites) + 1
        for loc in sites:
            entry_text = f"{loc['id']},".ljust(width)
            w(f"            {entry_text} // [{loc['country']}] {loc['name']}")
        w("        ],")

    w("    }")
    w("}")
    w("")
    w("/// Display name for a region, as shown in the filter panel.")
    w("pub fn label(region: RegionFilter) -> &'static str {")
    w("    match region {")
    w('        RegionFilter::All => "All",')
    for entry, _ in assignments:
        w(f"        RegionFilter::{entry['variant']} => {json.dumps(entry['label'])},")
    w("    }")
    w("}")
    w("")
    return "\n".join(out)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--fetch", action="store_true", help="refresh the snapshot from LL2 first")
    ap.add_argument("--check", action="store_true", help="verify the committed table, write nothing")
    ap.add_argument("--raw", metavar="FILE", help="trim a previously saved API response instead of fetching")
    args = ap.parse_args()

    if args.fetch and args.raw:
        sys.exit("error: --fetch and --raw are alternative sources; pass only one.")
    if args.check and (args.fetch or args.raw):
        sys.exit("error: --check verifies the committed snapshot; it cannot also refresh it.")

    if args.fetch or args.raw:
        if args.raw:
            with open(args.raw, "r", encoding="utf-8") as fh:
                payload = json.load(fh)
        else:
            payload = fetch_locations()

        snapshot = trim(payload)
        if len(snapshot["locations"]) < MIN_PLAUSIBLE_LOCATIONS:
            sys.exit(
                f"error: only {len(snapshot['locations'])} locations returned, "
                f"expected at least {MIN_PLAUSIBLE_LOCATIONS}. Refusing to "
                f"rebuild from what looks like a truncated response."
            )
        write_snapshot(snapshot)
        print(f"snapshot: {len(snapshot['locations'])} locations -> {os.path.relpath(SNAPSHOT, ROOT)}")
    else:
        snapshot = load_snapshot()

    regions = load_regions()
    assignments, no_country, leftover = group(regions, snapshot)
    rendered = render(assignments, no_country, leftover, snapshot)

    if args.check:
        if not os.path.exists(OUT):
            sys.exit(f"error: {os.path.relpath(OUT, ROOT)} does not exist; run without --check.")
        with open(OUT, "r", encoding="utf-8") as fh:
            current = fh.read()
        if current != rendered:
            sys.exit(
                f"error: {os.path.relpath(OUT, ROOT)} is out of date with "
                f"regions.toml and locations.json.\n"
                f"       run: python3 {SCRIPT_REL}"
            )
        print(f"ok: {os.path.relpath(OUT, ROOT)} is up to date")
        return

    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w", encoding="utf-8") as fh:
        fh.write(rendered)

    catch_all = assignments[-1][0] if assignments and assignments[-1][0].get("catch_all") else None
    for entry, sites in assignments:
        note = "  <- no active sites" if not sites else ""
        print(f"  {entry['variant']:<18} {len(sites):>2} site(s){note}")
    if leftover:
        where = f"held by {catch_all['variant']}" if catch_all else "in no region"
        print(f"\n{len(leftover)} active site(s) {where}:")
        for loc in leftover:
            print(f"  [{loc['country']}] {loc['id']:>3}  {loc['name']}")
    print(f"\nwrote {os.path.relpath(OUT, ROOT)}")


if __name__ == "__main__":
    main()
