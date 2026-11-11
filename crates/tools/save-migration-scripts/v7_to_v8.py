#!/usr/bin/env python3
"""
Migrates Heritage Builder save files from v7 to v8.

v8 adds disease for houses (docs/plans/city-risks.md, Phase 4):
  - Every building's `risks` gets `disease` (0), `near_apothecary` and `has_food`
    (false: the first check, 10 simulated seconds after the load, finds out), a
    `disease_check_timer` and no `outbreak`. Only house levels have a disease rate,
    from their config on load, so nothing else is stored.
  - `Simulation::risks` gets `disease` (true: houses can fall sick while risks are
    on) and `disease_deaths` (0).
  - The fire watcher's task, `UnitTaskFightFire`, is now `UnitTaskRespond`, which a
    hospital's medic runs too: it gets `emergency: "Fire"`, its `fire` and
    `fire_tile` become `target` and `target_tile`, its `MovingToFire` state becomes
    `MovingToTarget`, and its completion callback, `ServiceBuilding::on_fire_fought`,
    is now `ServiceBuilding::on_response_done`. Callbacks are saved by the FNV-1a
    hash of their name, so the hash is rewritten.
`save_version` is bumped to 8.

Usage:
    python3 crates/tools/save-migration-scripts/v7_to_v8.py [save.json ...]

With no arguments, migrates every saves/*.json. Files are rewritten in place.
The migration is idempotent: a v8 save is skipped. A save older than v7 is rejected
with a message to run the earlier scripts first.
"""
import glob
import json
import os
import sys

FROM_VERSION = 7
CURRENT_VERSION = 8


def fnv1a(text):
    """common::hash::fnv1a_from_str: 64-bit FNV-1a over the UTF-8 bytes."""
    value = 0xCBF29CE484222325
    for byte in text.encode("utf-8"):
        value ^= byte
        value = (value * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return value


OLD_CALLBACK_HASH = fnv1a("ServiceBuilding::on_fire_fought")
NEW_CALLBACK_HASH = fnv1a("ServiceBuilding::on_response_done")


def migrate_building_risks(risks):
    risks.setdefault("disease", 0.0)
    risks.setdefault("near_apothecary", False)
    risks.setdefault("has_food", False)
    risks.setdefault("disease_check_timer", {"time_since_last_update_secs": 0.0})
    risks.setdefault("outbreak", None)


def migrate_fight_fire_task(task):
    """A v7 UnitTaskFightFire's fields, as a v8 UnitTaskRespond's."""
    respond = {"emergency": "Fire"}
    for key, value in task.items():
        if key == "fire":
            respond["target"] = value
        elif key == "fire_tile":
            respond["target_tile"] = value
        elif key == "state" and value == "MovingToFire":
            respond["state"] = "MovingToTarget"
        elif key == "completion_callback" and isinstance(value, dict):
            hash_holder = value.get("key", {})
            if hash_holder.get("hash") == OLD_CALLBACK_HASH:
                hash_holder["hash"] = NEW_CALLBACK_HASH
            respond[key] = value
        else:
            respond[key] = value
    return respond


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
                risks = entry.get("risks")
                if not isinstance(risks, dict):
                    raise ValueError("a building has no `risks` (is this a v7 save?)")
                migrate_building_risks(risks)
                buildings += 1

    city_risks = sim.get("risks")
    if not isinstance(city_risks, dict):
        raise ValueError("save has no `sim.risks` object")
    city_risks.setdefault("disease", True)
    city_risks.setdefault("disease_deaths", 0)

    tasks = 0
    task_pool = sim.get("task_manager", {}).get("task_pool", {}).get("tasks", {})
    for slot in task_pool.values():
        archetype = slot.get("archetype") if isinstance(slot, dict) else None
        if isinstance(archetype, dict) and "UnitTaskFightFire" in archetype:
            slot["archetype"] = {"UnitTaskRespond": migrate_fight_fire_task(archetype["UnitTaskFightFire"])}
            tasks += 1

    data["save_version"] = CURRENT_VERSION
    return {"buildings": buildings, "fire_tasks": tasks}


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
