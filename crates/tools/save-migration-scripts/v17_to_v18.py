#!/usr/bin/env python3
"""
Migrates Heritage Builder save files from v17 to v18.

v18 (docs/plans/ratings.md, Phase 2):
- each of the city's requests (`sim.trade`: those scheduled, those under way and those in the log)
  gains `again`, how many times the court has asked again for it after it failed: 0, since no
  request was asked again before v18.
`save_version` is bumped to 18.

Usage:
    python3 crates/tools/save-migration-scripts/v17_to_v18.py [save.json ...]

With no arguments, migrates every saves/*.json. Files are rewritten in place.
The migration is idempotent: a v18 save is skipped. A save older than v17 is
rejected with a message to run the earlier scripts first.
"""
import glob
import json
import os
import sys

FROM_VERSION = 17
CURRENT_VERSION = 18


def add_again(request):
    if not isinstance(request, dict):
        raise ValueError("a request isn't an object")
    if "again" in request:
        return 0
    request["again"] = 0
    return 1


def migrate(data):
    sim = data.get("sim")
    if not isinstance(sim, dict):
        raise ValueError("save has no `sim` object")
    trade = sim.get("trade")
    if not isinstance(trade, dict):
        raise ValueError("save's simulation has no `trade` object")

    added = 0
    for key in ("scheduled", "requests"):
        requests = trade.get(key)
        if not isinstance(requests, list):
            raise ValueError(f"save's trade has no `{key}` list")
        for request in requests:
            added += add_again(request)
    log = trade.get("request_log")
    if not isinstance(log, list):
        raise ValueError("save's trade has no `request_log` list")
    for record in log:
        if not isinstance(record, dict):
            raise ValueError("a request record isn't an object")
        added += add_again(record.get("request"))
    data["save_version"] = CURRENT_VERSION
    return {"requests": added}


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
