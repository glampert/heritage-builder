#!/usr/bin/env python3
"""
Migrates Heritage Builder save files from v8 to v9.

v9 adds a store's orders (docs/plans/realm-ui.md, Phase 8): each storage yard's and
granary's `storage_slots` gets `orders`, the player's order for each good, saved by
good name with accepted goods left out. A v8 store accepted every good it could, so
its orders are empty (`{}`). Undo records keep whole `storage_slots` copies too, but
they aren't saved. `save_version` is bumped to 9.

Usage:
    python3 crates/tools/save-migration-scripts/v8_to_v9.py [save.json ...]

With no arguments, migrates every saves/*.json. Files are rewritten in place.
The migration is idempotent: a v9 save is skipped. A save older than v8 is rejected
with a message to run the earlier scripts first.
"""
import glob
import json
import os
import sys

FROM_VERSION = 8
CURRENT_VERSION = 9


def migrate(data):
    world = data.get("world")
    if not isinstance(world, dict) or not isinstance(world.get("building_spawn_pools"), list):
        raise ValueError("save has no `world.building_spawn_pools` list")

    stores = 0
    for pool in world["building_spawn_pools"]:
        # [archetype name, [pool header, building, building, ...]]
        if not (isinstance(pool, list) and len(pool) == 2 and isinstance(pool[1], list)):
            raise ValueError("unexpected building pool layout")
        for entry in pool[1][1:]:
            if not (isinstance(entry, dict) and "map_cells" in entry):
                continue
            archetype = entry.get("archetype")
            storage = archetype.get("StorageBuilding") if isinstance(archetype, dict) else None
            if isinstance(storage, dict):
                slots = storage.get("storage_slots")
                if not isinstance(slots, dict):
                    raise ValueError("a storage building has no `storage_slots`")
                slots.setdefault("orders", {})
                stores += 1

    data["save_version"] = CURRENT_VERSION
    return {"stores": stores}


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
