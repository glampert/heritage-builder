#!/usr/bin/env python3
"""
Migrates Heritage Builder save files from v0 (pre-versioning) to v1.

v1 reworks the unit task system:
  - Each task's per-task `internal_state` enum is replaced by a unified `state`.
  - `UnitTaskInstance.state` (the Uninitialized/Running lifecycle enum) becomes
    a `started` bool.
  - `UnitTaskDespawn` went from a unit struct (serialized as `null`) to a struct.
  - `UnitTaskDespawnWithCallback`'s `post_despawn_callback` + `callback_extra_args`
    are bundled into a single `post_despawn` field.
  - A `save_version` field is added at the save root.

Usage:
    python3 crates/tools/save_migration_scripts/v0_to_v1.py [save.json ...]

With no arguments, migrates every saves/*.json. Files are rewritten in place.
The migration is idempotent: each field is transformed only if still in the v0
layout, so re-running it (or running it on a mixed/partly-migrated save) is safe.
"""
import glob
import json
import os
import sys

CURRENT_VERSION = 1

# Old `internal_state` variant -> new `state` variant, per task type.
DELIVER = {
    "Idle": "Searching",
    "MovingToGoal": "MovingToStorage",
    "PendingBuildingVisit": "VisitingStorage",
    "Completed": "Done",
}
FETCH = {
    "Idle": "Searching",
    "MovingToGoal": "MovingToStorage",
    "PendingBuildingVisit": "VisitingStorage",
    "ReturningToOrigin": "ReturningToOrigin",
    "PendingCompletionCallback": "DeliveringToOrigin",
    "ReturningSurplusToStorage": "RoutingSurplus",
    "PendingSurplusUnload": "UnloadingSurplus",
    "Completed": "Done",
}
PATROL = {
    "Running": "Patrolling",
    "PendingCompletionCallback": "DeliveringToOrigin",
    "Completed": "Done",
}
SETTLER = {
    "Idle": "Searching",
    "PendingBuildingVisit": "VisitingHouse",
    "Completed": "Done",
}
HARVEST = {
    "PendingHarvest": "PendingHarvest",
    "PendingCompletionCallback": "DeliveringToOrigin",
    "Completed": "Done",
}


def migrate_task(name, task):
    """Migrate one archetype's inner task object. Returns the new value.

    Each transform is guarded so an already-migrated task is left untouched.
    """
    if name == "UnitTaskDespawn":
        # Old: serialized as `null` (unit struct). New: a struct (state defaults).
        return {} if task is None else task

    if name == "UnitTaskDespawnWithCallback":
        if "post_despawn_callback" in task:
            return {
                "post_despawn": {
                    "callback": task["post_despawn_callback"],
                    "args": task["callback_extra_args"],
                },
            }
        return task  # Already bundled.

    if name == "UnitTaskFollowPath":
        return task  # No task-state field; new `state` defaults on load.

    if "internal_state" not in task:
        return task  # Already migrated, or a very old save without the field.

    old = task.pop("internal_state")

    if name == "UnitTaskDeliverToStorage":
        task["state"] = DELIVER.get(old, old) if isinstance(old, str) else old
    elif name == "UnitTaskFetchFromStorage":
        task["state"] = FETCH.get(old, old) if isinstance(old, str) else old
    elif name == "UnitTaskRandomizedPatrol":
        task["state"] = PATROL.get(old, old) if isinstance(old, str) else old
    elif name == "UnitTaskSettler":
        if isinstance(old, dict) and "MovingToGoal" in old:
            task["state"] = {"MovingTo": old["MovingToGoal"]}
        elif isinstance(old, dict) and "BuildingVisited" in old:
            task["state"] = "Searching"
        elif isinstance(old, str):
            task["state"] = SETTLER.get(old, old)
        else:
            task["state"] = old
    elif name == "UnitTaskHarvestWood":
        is_returning = task.pop("is_returning_to_origin", False)
        if old == "Running":
            task["state"] = "ReturningToOrigin" if is_returning else "Searching"
        elif isinstance(old, str):
            task["state"] = HARVEST.get(old, old)
        else:
            task["state"] = old

    return task


def migrate(data):
    tasks = (
        data.get("sim", {})
        .get("task_manager", {})
        .get("task_pool", {})
        .get("tasks")
    )
    if isinstance(tasks, dict):
        for inst in tasks.values():
            if not isinstance(inst, dict):
                continue
            # UnitTaskInstance.state (lifecycle enum) -> started bool.
            # Guarded so an already-migrated instance keeps its `started` value.
            if "state" in inst:
                old_state = inst.pop("state")
                inst["started"] = old_state != "Uninitialized"

            archetype = inst.get("archetype")
            if isinstance(archetype, dict) and len(archetype) == 1:
                name = next(iter(archetype))
                archetype[name] = migrate_task(name, archetype[name])

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

        if data.get("save_version", 0) >= CURRENT_VERSION:
            print(f"skip (already v{data.get('save_version', 0)}): {path}")
            continue

        migrate(data)

        with open(path, "w") as f:
            json.dump(data, f, indent=2)
            f.write("\n")
        print(f"migrated -> v{CURRENT_VERSION}: {path}")


if __name__ == "__main__":
    main(sys.argv)
