#!/usr/bin/env python3
"""
Migrates Heritage Builder save files from v18 to v19.

v19 (docs/plans/messages.md, M11 Phase 2: labour priorities, Pause work and storage orders):
- the simulation (`sim`) gains `labour`: the sectors' priorities in the new game's order and the
  allocation's clock;
- every building drops `workers_update_timer` (the city-wide allocation, crate::labour, hands the
  workers out in place of each employer's own search) and gains `paused`, false;
- every store (`StorageBuilding`) gains its `runner`, not sent out, and its `errand_timer`, for the
  Get and Empty orders' errands.
`save_version` is bumped to 19.

Usage:
    python3 crates/tools/save-migration-scripts/v18_to_v19.py [save.json ...]

With no arguments, migrates every saves/*.json. Files are rewritten in place.
The migration is idempotent: a v19 save is skipped. A save older than v18 is
rejected with a message to run the earlier scripts first.
"""
import glob
import json
import os
import sys

FROM_VERSION = 18
CURRENT_VERSION = 19

# sim::labour::LabourSector, in the new game's order.
LABOUR_ORDER = [
    "food_and_farming",
    "industry",
    "services",
    "culture_and_religion",
    "infrastructure",
    "trade",
]

# A Runner that isn't out (an invalid unit id).
INVALID_INDEX = 4294967295
IDLE_RUNNER = {"unit": {"unit_id": {"generation": INVALID_INDEX, "index": INVALID_INDEX}}}


def migrate_building(building, archetype_name):
    if not isinstance(building, dict):
        raise ValueError("a building isn't an object")
    counts = {"buildings": 0, "stores": 0}
    if "paused" not in building:
        building.pop("workers_update_timer", None)
        building["paused"] = False
        counts["buildings"] += 1
    if archetype_name == "StorageBuilding":
        archetype = building.get("archetype")
        store = archetype.get("StorageBuilding") if isinstance(archetype, dict) else None
        if not isinstance(store, dict):
            raise ValueError("a storage building has no `StorageBuilding` archetype")
        if "runner" not in store:
            store["runner"] = json.loads(json.dumps(IDLE_RUNNER))
            store["errand_timer"] = {"time_since_last_update_secs": 0.0}
            counts["stores"] += 1
    return counts


def migrate(data):
    sim = data.get("sim")
    if not isinstance(sim, dict):
        raise ValueError("save has no `sim` object")
    world = data.get("world")
    if not isinstance(world, dict):
        raise ValueError("save has no `world` object")
    pools = world.get("building_spawn_pools")
    if not isinstance(pools, list):
        raise ValueError("save's world has no `building_spawn_pools` list")

    counts = {"buildings": 0, "stores": 0, "labour": 0}
    for entry in pools:
        if not isinstance(entry, list) or len(entry) != 2:
            raise ValueError("a building pool isn't an [archetype, pool] pair")
        archetype_name, pool = entry
        if not isinstance(pool, list) or not pool:
            raise ValueError(f"the {archetype_name} pool isn't a list")
        # The first item is the pool's header; the buildings follow.
        for building in pool[1:]:
            for key, value in migrate_building(building, archetype_name).items():
                counts[key] += value

    if "labour" not in sim:
        sim["labour"] = {"order": list(LABOUR_ORDER), "allocate_timer": {"time_since_last_update_secs": 0.0}}
        counts["labour"] = 1

    data["save_version"] = CURRENT_VERSION
    return counts


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
