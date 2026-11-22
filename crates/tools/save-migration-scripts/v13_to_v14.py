#!/usr/bin/env python3
"""
Migrates Heritage Builder save files from v13 to v14.

v14 (docs/plans/trade.md, Phase 4):
- the city's trade (`sim.trade`) gains `scheduled`: the mission's requests not
  open yet (none in an older save: its mission's schedule didn't exist);
- each request under way or logged gains `legitimacy` and `relation`, its
  effects for M9 and game Phase 2 (0: no save has a request yet).
`save_version` is bumped to 14.

Usage:
    python3 crates/tools/save-migration-scripts/v13_to_v14.py [save.json ...]

With no arguments, migrates every saves/*.json. Files are rewritten in place.
The migration is idempotent: a v14 save is skipped. A save older than v13 is
rejected with a message to run the earlier scripts first.
"""
import glob
import json
import os
import sys

FROM_VERSION = 13
CURRENT_VERSION = 14


def add_effects(request):
    """Adds the effects a request lacks. Returns 1 if it changed."""
    changed = 0
    for field in ("legitimacy", "relation"):
        if field not in request:
            request[field] = 0
            changed = 1
    return changed


def migrate(data):
    sim = data.get("sim")
    if not isinstance(sim, dict):
        raise ValueError("save has no `sim` object")
    trade = sim.get("trade")
    if not isinstance(trade, dict):
        raise ValueError("save has no `sim.trade` object (run v11_to_v12.py first)")
    if "scheduled" not in trade:
        trade["scheduled"] = []
    requests = 0
    for request in trade.get("requests", []):
        requests += add_effects(request)
    for record in trade.get("request_log", []):
        if isinstance(record.get("request"), dict):
            requests += add_effects(record["request"])
    data["save_version"] = CURRENT_VERSION
    return {"requests": requests}


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
