#!/usr/bin/env python3
"""
Migrates Heritage Builder save files from v14 to v15.

v15 (docs/plans/terrain-height.md, Stage 1):
- the tile map (`tile_map`) gains `elevation`: each cell's level and shape, a byte a cell saved
  as runs ([value, count], ...). An older save's map is flat: every cell 0, one run.
`save_version` is bumped to 15.

Usage:
    python3 crates/tools/save-migration-scripts/v14_to_v15.py [save.json ...]

With no arguments, migrates every saves/*.json. Files are rewritten in place.
The migration is idempotent: a v15 save is skipped. A save older than v14 is
rejected with a message to run the earlier scripts first.
"""
import glob
import json
import os
import sys

FROM_VERSION = 14
CURRENT_VERSION = 15


def migrate(data):
    tile_map = data.get("tile_map")
    if not isinstance(tile_map, dict):
        raise ValueError("save has no `tile_map` object")
    size = tile_map.get("size_in_cells")
    if not isinstance(size, dict) or "width" not in size or "height" not in size:
        raise ValueError("save's tile map has no `size_in_cells`")
    width, height = int(size["width"]), int(size["height"])
    if "elevation" not in tile_map:
        cells = width * height
        tile_map["elevation"] = {
            "width": width,
            "height": height,
            "cells": [[0, cells]] if cells > 0 else [],
        }
    data["save_version"] = CURRENT_VERSION
    return {"cells": width * height}


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
