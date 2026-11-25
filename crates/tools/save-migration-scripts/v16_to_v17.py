#!/usr/bin/env python3
"""
Migrates Heritage Builder save files from v16 to v17.

v17 (docs/plans/ratings.md, Phase 0):
- the simulation gains `ratings`: the four ratings, not rated yet (`rated_month` null, so the
  game rates the city as the save loads), the failures the game can end in (on for a campaign
  mission under way, off for any other game), no failure clock running, not lost, and no
  distinction out of reach;
- the city's risks (`sim.risks`) gain `disasters`, the months with a building lost or a death:
  none recorded before v17.
`save_version` is bumped to 17.

Usage:
    python3 crates/tools/save-migration-scripts/v16_to_v17.py [save.json ...]

With no arguments, migrates every saves/*.json. Files are rewritten in place.
The migration is idempotent: a v17 save is skipped. A save older than v16 is
rejected with a message to run the earlier scripts first.
"""
import glob
import json
import os
import sys

FROM_VERSION = 16
CURRENT_VERSION = 17

RATING_KINDS = ["prosperity", "culture", "security", "legitimacy"]


def new_ratings(in_mission):
    return {
        "ratings": {kind: {"value": 0, "drivers": []} for kind in RATING_KINDS},
        "rated_month": None,
        "fails_on": {"debt": in_mission, "legitimacy": in_mission},
        "legitimacy_low_months": 0,
        "failure": None,
        "spoiled": [],
    }


def migrate(data):
    sim = data.get("sim")
    if not isinstance(sim, dict):
        raise ValueError("save has no `sim` object")
    risks = sim.get("risks")
    if not isinstance(risks, dict):
        raise ValueError("save's simulation has no `risks` object")

    progress = data.get("campaign_progress")
    in_mission = isinstance(progress, dict) and isinstance(progress.get("active"), dict)

    ratings_added = "ratings" not in sim
    if ratings_added:
        sim["ratings"] = new_ratings(in_mission)
    disasters_added = "disasters" not in risks
    if disasters_added:
        risks["disasters"] = []
    data["save_version"] = CURRENT_VERSION
    return {"ratings_added": ratings_added, "disasters_added": disasters_added, "can_fail": in_mission}


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
