#!/usr/bin/env python3
"""
Migrates Heritage Builder save files from v4 to v5.

v5 saves goods by name instead of by position (docs/plans/goods.md, Phase 0), so
that adding a good no longer breaks saves. Three per-good arrays, indexed by the
good's bit in `ResourceKind`, become maps from the good's name to its value, with
zero entries left out:
  - `ResourceStock::counts`: [2, 0, 0, ...] -> {"Rice": 2};
  - `BuildingStock::capacities`: [5, 5, ...] -> {"Rice": 5, "Meat": 5, ...};
  - `HouseBuilding::consumption_accumulator`: [0.5, 0.0, ...] -> {"Rice": 0.5}.
    Houses saved before the field existed (it was `#[serde(default)]`) get `{}`.
`save_version` is bumped to 5.

Usage:
    python3 crates/tools/save-migration-scripts/v4_to_v5.py [save.json ...]

With no arguments, migrates every saves/*.json. Files are rewritten in place.
The migration is idempotent: a map is left as it is, and a v5 save is skipped.
A save older than v4 is rejected with a message to run the earlier scripts first.
"""
import glob
import json
import os
import sys

FROM_VERSION = 4
CURRENT_VERSION = 5

# ResourceKind's bit order in v4 (crates/game/src/sim/resources.rs): a good's bit
# was its index in the arrays.
V4_RESOURCE_KINDS = ["Rice", "Meat", "Fish", "Wine", "Pottery", "Wood", "Metal", "Clay", "Bricks", "Gold"]


def to_name_map(values, field):
    """[v0, v1, ...] indexed by v4 bit -> {name: value}, zero entries left out."""
    if isinstance(values, dict):
        return values  # already migrated
    if not isinstance(values, list) or len(values) > len(V4_RESOURCE_KINDS):
        raise ValueError(f"unexpected `{field}`: {values!r}")
    return {V4_RESOURCE_KINDS[i]: v for i, v in enumerate(values) if v != 0}


def migrate_node(node, counts):
    """Rewrite every per-good array under `node`. `counts` tallies them by field."""
    if isinstance(node, list):
        for item in node:
            migrate_node(item, counts)
    elif isinstance(node, dict):
        # ResourceStock: {"kinds": "Rice | Meat", "counts": [...]}
        if "kinds" in node and "counts" in node:
            node["counts"] = to_name_map(node["counts"], "counts")
            counts["counts"] += 1
        # BuildingStock: {"resources": ResourceStock, "capacities": [...]}
        if "resources" in node and "capacities" in node:
            node["capacities"] = to_name_map(node["capacities"], "capacities")
            counts["capacities"] += 1
        # HouseBuilding, in a building's `archetype`
        house = node.get("HouseBuilding")
        if isinstance(house, dict) and "consumption_accumulator" not in house:
            house["consumption_accumulator"] = {}
        if "consumption_accumulator" in node:
            node["consumption_accumulator"] = to_name_map(node["consumption_accumulator"], "consumption_accumulator")
            counts["consumption_accumulator"] += 1
        for value in node.values():
            migrate_node(value, counts)


def migrate(data):
    if not isinstance(data.get("world"), dict):
        raise ValueError("save has no `world` object")
    counts = {"counts": 0, "capacities": 0, "consumption_accumulator": 0}
    migrate_node(data, counts)
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
