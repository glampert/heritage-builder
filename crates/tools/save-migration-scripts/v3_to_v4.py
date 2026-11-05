#!/usr/bin/env python3
"""
Migrates Heritage Builder save files from v3 to v4.

v4 adds construction and house renovation (docs/plans/construction.md):
  - `Building::construction`: the construction site state, `null` for a
    finished building. Every building in an old save is finished.
  - `HouseUpgradeState`: `upgrade_ready_secs` and `downgrade_pending_secs`
    (0.0) and `renovation_secs_left` (`null`): no renovation or downgrade
    under way.
  - `save_version` is bumped to 4.

Usage:
    python3 crates/tools/save-migration-scripts/v3_to_v4.py [save.json ...]

With no arguments, migrates every saves/*.json. Files are rewritten in place.
The migration is idempotent: a building that already has the field keeps it,
and a v4 save is skipped. A save older than v3 is rejected with a message to
run the earlier scripts first.
"""
import glob
import json
import os
import sys

FROM_VERSION = 3
CURRENT_VERSION = 4

# A serialized Building carries this set of keys (crates/game/src/building/mod.rs).
BUILDING_KEYS = {"id", "map_cells", "road_link", "kind", "workers_update_timer", "archetype"}


def add_construction(node):
    """Give every serialized Building a `construction: null` field. Returns the count."""
    count = 0
    if isinstance(node, list):
        for item in node:
            count += add_construction(item)
    elif isinstance(node, dict):
        if BUILDING_KEYS <= node.keys():
            if "construction" not in node:
                node["construction"] = None
                count += 1
        for value in node.values():
            count += add_construction(value)
    return count


def add_house_timers(node):
    """Give every serialized HouseUpgradeState its renovation and downgrade timers."""
    if isinstance(node, list):
        for item in node:
            add_house_timers(item)
    elif isinstance(node, dict):
        state = node.get("upgrade_state")
        if isinstance(state, dict) and "level" in state:
            state.setdefault("upgrade_ready_secs", 0.0)
            state.setdefault("renovation_secs_left", None)
            state.setdefault("downgrade_pending_secs", 0.0)
        for value in node.values():
            add_house_timers(value)


def migrate(data):
    world = data.get("world")
    if not isinstance(world, dict):
        raise ValueError("save has no `world` object")

    pools = world.get("building_spawn_pools")
    add_construction(pools)
    add_house_timers(pools)

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
