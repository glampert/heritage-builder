#!/usr/bin/env python3
"""
Migrates Heritage Builder save files from v10 to v11.

v11 (docs/plans/seasons-floods.md, Phases 0 and 1):
- adds the river's floods and the land's fertility, `sim.floods`, off, with the
  default flood and no fertility kept (the game lays them out when a mission or a
  custom game turns floods on);
- adds a farmstead's fields to every producer building's state, `fields`, empty;
- turns every rice farm into a livestock farm, since the 3x3 rice farm is gone
  (farmsteads and their fields replace it): its object tile's def hash becomes
  livestock_farm's (the same 3x3 footprint and producer state), and the rice it
  held is dropped (its output stock becomes an empty stock of meat). A runner
  already carrying rice still delivers it.
`save_version` is bumped to 11.

Usage:
    python3 crates/tools/save-migration-scripts/v10_to_v11.py [save.json ...]

With no arguments, migrates every saves/*.json. Files are rewritten in place.
The migration is idempotent: a v11 save is skipped. A save older than v10 is
rejected with a message to run the earlier scripts first.
"""
import glob
import json
import os
import sys

FROM_VERSION = 10
CURRENT_VERSION = 11


def fnv1a(text):
    """The game's FNV-1a 64-bit hash of a name (common::hash::fnv1a_from_str)."""
    h = 0xCBF29CE484222325
    for byte in text.encode("utf-8"):
        h ^= byte
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


RICE_FARM = fnv1a("rice_farm")
LIVESTOCK_FARM = fnv1a("livestock_farm")

FLOODS = {
    "enabled": False,
    "profile": {"month": 7, "reach": [3, 6], "months": 2, "sources": [], "river": "", "fertility": [70, 40]},
    "fertility": {"width": 0, "height": 0, "cells": []},
    "neutral_fertility": 50,
    "dyke_wear": [],
    "flood": None,
    "next_severity": 1.0,
    "log": [],
}


def walk(value, visit):
    """Calls visit(dict) on every object in the JSON tree."""
    if isinstance(value, dict):
        visit(value)
        for child in value.values():
            walk(child, visit)
    elif isinstance(value, list):
        for child in value:
            walk(child, visit)


def migrate(data):
    sim = data.get("sim")
    if not isinstance(sim, dict):
        raise ValueError("save has no `sim` object")
    sim.setdefault("floods", json.loads(json.dumps(FLOODS)))

    # Rice farms' object tiles: their def hash, and the cells they stand on.
    rice_farm_cells = set()

    def retile(obj):
        tile = obj.get("ObjectTile")
        if isinstance(tile, dict):
            handle = tile.get("def")
            if isinstance(handle, list) and len(handle) == 3 and handle[2] == RICE_FARM:
                handle[2] = LIVESTOCK_FARM
                start = tile.get("cell_range", {}).get("start", {})
                rice_farm_cells.add((start.get("x"), start.get("y")))

    walk(data.get("tile_map"), retile)

    # Every producer's fields; the former rice farms' stock.
    counts = {"producers": 0, "rice_farms": len(rice_farm_cells)}

    def producers(obj):
        archetype = obj.get("archetype")
        if not isinstance(archetype, dict):
            return
        producer = archetype.get("ProducerBuilding")
        if not isinstance(producer, dict):
            return
        producer.setdefault("fields", [])
        counts["producers"] += 1
        start = obj.get("map_cells", {}).get("start", {})
        if (start.get("x"), start.get("y")) in rice_farm_cells:
            stock = producer.get("production_output_stock")
            if isinstance(stock, dict):
                stock["item"] = {"kind": "Meat", "count": 0}

    walk(data.get("world"), producers)

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
