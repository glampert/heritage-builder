#!/usr/bin/env python3
"""
Migrates Heritage Builder save files from v11 to v12.

v12 (docs/plans/trade.md, Phase 0):
- adds the city's trade, `sim.trade`: off, with no partners, requests or log (a
  mission's or a custom game's trade block lists its partners when it starts).
`save_version` is bumped to 12.

Usage:
    python3 crates/tools/save-migration-scripts/v11_to_v12.py [save.json ...]

With no arguments, migrates every saves/*.json. Files are rewritten in place.
The migration is idempotent: a v12 save is skipped. A save older than v11 is
rejected with a message to run the earlier scripts first.
"""
import glob
import json
import os
import sys

FROM_VERSION = 11
CURRENT_VERSION = 12

TRADE = {
    "enabled": False,
    "world_map": "",
    "partners": [],
    "requests": [],
    "request_log": [],
}


def migrate(data):
    sim = data.get("sim")
    if not isinstance(sim, dict):
        raise ValueError("save has no `sim` object")
    added = "trade" not in sim
    sim.setdefault("trade", json.loads(json.dumps(TRADE)))
    data["save_version"] = CURRENT_VERSION
    return {"trade_added": added}


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
