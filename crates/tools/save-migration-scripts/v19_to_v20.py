#!/usr/bin/env python3
"""
Migrates Heritage Builder save files from v19 to v20.

v20 (docs/plans/defence.md, Phase 0: the defence's data and state):
- the simulation (`sim`) gains `defence`: no random raids, no raids scheduled, under way or logged,
  and no walls (a v19 game is a mission or custom game started before raids existed, so it gets
  none);
- every unit gains `health`, full: its config's `max_health` (100 for every unit config so far).
The tiles' game object handles hold a building's kind in a u64 now, but the numbers saved are the
same, so they need nothing. `save_version` is bumped to 20.

Usage:
    python3 crates/tools/save-migration-scripts/v19_to_v20.py [save.json ...]

With no arguments, migrates every saves/*.json. Files are rewritten in place.
The migration is idempotent: a v20 save is skipped. A save older than v19 is
rejected with a message to run the earlier scripts first.
"""
import glob
import json
import os
import sys

FROM_VERSION = 19
CURRENT_VERSION = 20

# unit::config::DEFAULT_MAX_HEALTH: no unit config sets its own max_health yet.
FULL_HEALTH = 100

EMPTY_DEFENCE = {
    "random_raids": False,
    "next_random_raid": None,
    "scheduled": [],
    "under_way": [],
    "log": [],
    "walls": [],
}


def migrate(data):
    sim = data.get("sim")
    if not isinstance(sim, dict):
        raise ValueError("save has no `sim` object")
    world = data.get("world")
    if not isinstance(world, dict):
        raise ValueError("save has no `world` object")
    pool = world.get("unit_spawn_pool")
    if not isinstance(pool, list) or not pool:
        raise ValueError("save's world has no `unit_spawn_pool` list")

    counts = {"units": 0, "defence": 0}
    # The first item is the pool's header; the units follow.
    for unit in pool[1:]:
        if not isinstance(unit, dict):
            raise ValueError("a unit isn't an object")
        if "health" not in unit:
            unit["health"] = FULL_HEALTH
            counts["units"] += 1

    if "defence" not in sim:
        sim["defence"] = json.loads(json.dumps(EMPTY_DEFENCE))
        counts["defence"] = 1

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
