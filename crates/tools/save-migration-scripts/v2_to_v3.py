#!/usr/bin/env python3
"""
Migrates Heritage Builder save files from v2 to v3.

v3 adds the game calendar (`Simulation::calendar`, see crates/game/src/sim/calendar.rs):
  - `settings`: the start World Year and month, the wet season's months and
    the eras. Old saves get `CalendarSettings::default()`: day 1 of month 1,
    World Year 1, wet months 5 to 10, no eras.
  - `months_elapsed` and `secs_into_month`: 0, so an old save starts its
    calendar on that first day.
  - `save_version` is bumped to 3.

Usage:
    python3 crates/tools/save-migration-scripts/v2_to_v3.py [save.json ...]

With no arguments, migrates every saves/*.json. Files are rewritten in place.
The migration is idempotent: a save that already has a calendar keeps it, and
a v3 save is skipped. A save older than v2 is rejected with a message to run
the earlier scripts first.
"""
import glob
import json
import os
import sys

FROM_VERSION = 2
CURRENT_VERSION = 3

# Must match `CalendarSettings::default()` in crates/game/src/sim/calendar.rs.
DEFAULT_CALENDAR = {
    "settings": {
        "start_year": 1,
        "start_month": 1,
        "wet_months": [5, 6, 7, 8, 9, 10],
        "eras": [],
    },
    "months_elapsed": 0,
    "secs_into_month": 0.0,
}


def migrate(data):
    sim = data.get("sim")
    if not isinstance(sim, dict):
        raise ValueError("save has no `sim` object")

    sim.setdefault("calendar", json.loads(json.dumps(DEFAULT_CALENDAR)))

    data["save_version"] = CURRENT_VERSION
    return data


def main(argv):
    paths = argv[1:]
    if not paths:
        here = os.path.dirname(os.path.abspath(__file__))
        saves_dir = os.path.join(here, "..", "..", "..", "saves")
        paths = sorted(glob.glob(os.path.join(saves_dir, "*.json")))

    if not paths:
        print("No save files found.")
        return

    for path in paths:
        with open(path) as f:
            data = json.load(f)

        version = data.get("save_version", 0)
        if version >= CURRENT_VERSION:
            print(f"skip (already v{version}): {path}")
            continue
        if version < FROM_VERSION:
            print(f"ERROR (v{version}, run the v{version}_to_v{version + 1}.py script first): {path}")
            continue

        try:
            migrate(data)
        except ValueError as err:
            print(f"ERROR ({err}): {path}")
            continue

        with open(path, "w") as f:
            json.dump(data, f, indent=2)
            f.write("\n")
        print(f"migrated -> v{CURRENT_VERSION}: {path}")


if __name__ == "__main__":
    main(sys.argv)
