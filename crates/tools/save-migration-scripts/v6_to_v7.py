#!/usr/bin/env python3
"""
Migrates Heritage Builder save files from v6 to v7.

v7 adds fire and collapse risk (docs/plans/city-risks.md, Phase 0):
  - Every building gets `risks`: no fire or collapse risk yet, not near water (the
    first well check, 10 simulated seconds after the load, finds out), and not
    burning. Its rates come from its building config on load, so nothing else is
    stored.
  - `Simulation::risks` ({"enabled": true}): risks build up, as in a new game
    outside a campaign. A campaign mission's load applies its own setting.
`save_version` is bumped to 7.

Usage:
    python3 crates/tools/save-migration-scripts/v6_to_v7.py [save.json ...]

With no arguments, migrates every saves/*.json. Files are rewritten in place.
The migration is idempotent: a v7 save is skipped. A save older than v6 is rejected
with a message to run the earlier scripts first.
"""
import glob
import json
import os
import sys

FROM_VERSION = 6
CURRENT_VERSION = 7


def new_building_risks():
    return {
        "fire": 0.0,
        "collapse": 0.0,
        "near_water": False,
        "water_check_timer": {"time_since_last_update_secs": 0.0},
        "burning": None,
    }


def migrate(data):
    world = data.get("world")
    if not isinstance(world, dict) or not isinstance(world.get("building_spawn_pools"), list):
        raise ValueError("save has no `world.building_spawn_pools` list")
    sim = data.get("sim")
    if not isinstance(sim, dict):
        raise ValueError("save has no `sim` object")

    buildings = 0
    for pool in world["building_spawn_pools"]:
        # [archetype name, [pool header, building, building, ...]]
        if not (isinstance(pool, list) and len(pool) == 2 and isinstance(pool[1], list)):
            raise ValueError("unexpected building pool layout")
        for entry in pool[1][1:]:
            if isinstance(entry, dict) and "map_cells" in entry:
                entry.setdefault("risks", new_building_risks())
                buildings += 1

    sim.setdefault("risks", {"enabled": True})
    data["save_version"] = CURRENT_VERSION
    return {"buildings": buildings}


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
