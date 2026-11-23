#!/usr/bin/env python3
"""
Migrates Heritage Builder save files from v15 to v16.

v16 (docs/plans/terrain-height.md, Stage 6):
- the floods' profile (`sim.floods.profile`) gains `flood_level`: the highest ground the water
  rises over, in levels. An older save's flood reaches level-0 land only: 0.
`save_version` is bumped to 16.

Usage:
    python3 crates/tools/save-migration-scripts/v15_to_v16.py [save.json ...]

With no arguments, migrates every saves/*.json. Files are rewritten in place.
The migration is idempotent: a v16 save is skipped. A save older than v15 is
rejected with a message to run the earlier scripts first.
"""
import glob
import json
import os
import sys

FROM_VERSION = 15
CURRENT_VERSION = 16


def migrate(data):
    sim = data.get("sim")
    if not isinstance(sim, dict):
        raise ValueError("save has no `sim` object")
    floods = sim.get("floods")
    if not isinstance(floods, dict):
        raise ValueError("save's simulation has no `floods` object")
    profile = floods.get("profile")
    if not isinstance(profile, dict):
        raise ValueError("save's floods have no `profile` object")
    added = "flood_level" not in profile
    if added:
        profile["flood_level"] = 0
    data["save_version"] = CURRENT_VERSION
    return {"flood_level_added": added}


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
            counts = migrate(data)
        except ValueError as err:
            print(f"ERROR ({err}): {path}")
            continue

        with open(path, "w") as f:
            json.dump(data, f, indent=2)
            f.write("\n")
        print(f"migrated -> v{CURRENT_VERSION} ({counts}): {path}")


if __name__ == "__main__":
    main(sys.argv)
