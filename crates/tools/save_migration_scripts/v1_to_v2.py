#!/usr/bin/env python3
"""
Migrates Heritage Builder save files from v1 to v2.

v2 removes the pre-versioning backward-compatibility hacks now that a real
migration path exists:
  - `#[serde(default)]` was dropped from 12 save-state fields; this script
    ensures each is present so the stricter v2 deserialize succeeds:
    `Simulation::paused_update_timer`, `Unit::path_is_blocked`,
    `UnitTaskInstance::started`, `UnitTaskFollowPath::terminate_if_stuck`,
    and the `state` field of all 8 unit task types.
  - The `#[serde(flatten)]` `unit_id` shim on the Runner/Harvester/Patrol task
    helpers is gone; their flat `unit_id` is re-nested under a `unit` object.
  - `Simulation` no longer carries a serialized `graph` (the pathfinding graph
    now lives on `TileMap` and is rebuilt on load); the dead `sim.graph` block
    is deleted.
  - `save_version` is bumped to 2.

Usage:
    python3 crates/tools/save_migration_scripts/v1_to_v2.py [save.json ...]

With no arguments, migrates every saves/*.json. Files are rewritten in place.
The migration is idempotent: each field is transformed only if still in the v1
layout, so re-running it (or running a partly-migrated save) is safe. A v0 save
is rejected with a message to run v0_to_v1.py first.
"""
import glob
import json
import os
import sys

FROM_VERSION = 1
CURRENT_VERSION = 2

# Default `state` (the #[default] enum variant) per task archetype, used when a
# v1 task has no serialized `state` (FollowPath / Despawn tasks never stored one).
DEFAULT_STATE = {
    "UnitTaskDespawn": "Despawning",
    "UnitTaskDespawnWithCallback": "Despawning",
    "UnitTaskFollowPath": "Following",
    "UnitTaskRandomizedPatrol": "Patrolling",
    "UnitTaskDeliverToStorage": "Searching",
    "UnitTaskFetchFromStorage": "Searching",
    "UnitTaskSettler": "Searching",
    "UnitTaskHarvestWood": "Searching",
}


def migrate_tasks(data):
    """Ensure every task instance carries the fields v2 no longer defaults."""
    tasks = (
        data.get("sim", {})
        .get("task_manager", {})
        .get("task_pool", {})
        .get("tasks")
    )
    if not isinstance(tasks, dict):
        return

    for inst in tasks.values():
        if not isinstance(inst, dict):
            continue

        # UnitTaskInstance::started lost its #[serde(default)].
        inst.setdefault("started", True)

        archetype = inst.get("archetype")
        if not isinstance(archetype, dict) or len(archetype) != 1:
            continue

        name = next(iter(archetype))
        task = archetype[name]

        # UnitTaskDespawn was serialized as `null` / `{}`; make it a dict.
        if task is None:
            task = {}
            archetype[name] = task
        if not isinstance(task, dict):
            continue

        # Every task's `state` field lost its #[serde(default)].
        if name in DEFAULT_STATE:
            task.setdefault("state", DEFAULT_STATE[name])

        # UnitTaskFollowPath::terminate_if_stuck lost its #[serde(default)].
        if name == "UnitTaskFollowPath":
            task.setdefault("terminate_if_stuck", False)


def unnest_unit_id(node):
    """Re-nest the flattened `unit_id` of Runner/Harvester/Patrol task helpers
    under a `unit` object. v2 drops the `#[serde(flatten)]` so the helper that
    serialized as `{ "unit_id": N }` becomes `{ "unit": { "unit_id": N } }`.

    Keyed on the containing field name (`runner` / `harvester` / `patrol`) so
    unrelated `unit_id` references elsewhere are left untouched.
    """
    if isinstance(node, list):
        for item in node:
            unnest_unit_id(item)
        return
    if not isinstance(node, dict):
        return

    for key in ("runner", "harvester", "patrol"):
        helper = node.get(key)
        if isinstance(helper, dict) and "unit_id" in helper:
            helper["unit"] = {"unit_id": helper.pop("unit_id")}

    for value in node.values():
        unnest_unit_id(value)


def ensure_path_is_blocked(node):
    """Add `path_is_blocked: false` to any serialized Unit that lacks it
    (the field lost its #[serde(default)] in v2).
    """
    if isinstance(node, list):
        for item in node:
            ensure_path_is_blocked(item)
        return
    if not isinstance(node, dict):
        return

    # A serialized Unit carries this distinctive set of keys.
    if {"map_cell", "tile_index", "config_key", "navigation"} <= node.keys():
        node.setdefault("path_is_blocked", False)

    for value in node.values():
        ensure_path_is_blocked(value)


def migrate(data):
    migrate_tasks(data)
    unnest_unit_id(data)
    ensure_path_is_blocked(data)

    sim = data.get("sim")
    if isinstance(sim, dict):
        # Simulation::paused_update_timer lost its #[serde(default)].
        # UpdateTimer serializes only `time_since_last_update_secs`; the
        # frequency is re-applied from config by Simulation::post_load.
        sim.setdefault("paused_update_timer", {"time_since_last_update_secs": 0.0})
        # `graph` is no longer a Simulation field; drop the dead block.
        sim.pop("graph", None)

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
            print(f"ERROR (v{version}, run v0_to_v1.py first): {path}")
            continue

        migrate(data)

        with open(path, "w") as f:
            json.dump(data, f, indent=2)
            f.write("\n")
        print(f"migrated -> v{CURRENT_VERSION}: {path}")


if __name__ == "__main__":
    main(sys.argv)
