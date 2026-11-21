#!/usr/bin/env python3
"""
Migrates Heritage Builder save files from v12 to v13.

v13 (docs/plans/trade.md, Phase 2):
- every service building's archetype (`ServiceBuilding`) gains `dock`: a dock's
  berth and dockers, null for every other service (no save has a dock yet).
`save_version` is bumped to 13.

Usage:
    python3 crates/tools/save-migration-scripts/v12_to_v13.py [save.json ...]

With no arguments, migrates every saves/*.json. Files are rewritten in place.
The migration is idempotent: a v13 save is skipped. A save older than v12 is
rejected with a message to run the earlier scripts first.
"""
import glob
import json
import os
import sys

FROM_VERSION = 12
CURRENT_VERSION = 13


def add_dock_fields(node):
    """Adds `dock: null` to every ServiceBuilding archetype under node. Returns how many."""
    count = 0
    if isinstance(node, dict):
        service = node.get("ServiceBuilding")
        if isinstance(service, dict) and "workers" in service and "dock" not in service:
            service["dock"] = None
            count += 1
        for value in node.values():
            count += add_dock_fields(value)
    elif isinstance(node, list):
        for value in node:
            count += add_dock_fields(value)
    return count


def migrate(data):
    world = data.get("world")
    if not isinstance(world, dict):
        raise ValueError("save has no `world` object")
    count = add_dock_fields(world)
    data["save_version"] = CURRENT_VERSION
    return {"services": count}


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
