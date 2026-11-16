#!/usr/bin/env python3
"""
Migrates Heritage Builder save files from v9 to v10.

v10 adds the city's faith (docs/plans/faith.md, Phase 0): `sim.faith`, with the
game's realm (the save header's `realm`, else Đại Long's, as the game reads a save
without one), favour and festivals on, no festival planned or held, no effects
under way, and no patrons: the game fills them at the starting favour when it
loads the save (`CityFaith::sync_patrons`, from the realm's faith file). Shrines
and temples need nothing: a shrine's or a temple's patron is its tile's variation,
which saves already keep. `save_version` is bumped to 10.

Usage:
    python3 crates/tools/save-migration-scripts/v9_to_v10.py [save.json ...]

With no arguments, migrates every saves/*.json. Files are rewritten in place.
The migration is idempotent: a v10 save is skipped. A save older than v9 is rejected
with a message to run the earlier scripts first.
"""
import glob
import json
import os
import sys

FROM_VERSION = 9
CURRENT_VERSION = 10
REALMS = ("dailong", "kambara", "tampadesa")


def migrate(data):
    sim = data.get("sim")
    if not isinstance(sim, dict):
        raise ValueError("save has no `sim` object")

    header = data.get("save_header")
    realm = header.get("realm") if isinstance(header, dict) else None
    if realm not in REALMS:
        realm = "dailong"

    sim.setdefault("faith", {
        "realm": realm,
        "favour_enabled": True,
        "festivals_enabled": True,
        "patrons": {},
        "plan": None,
        "log": [],
        "effects": [],
    })

    data["save_version"] = CURRENT_VERSION
    return {"realm": sim["faith"]["realm"]}


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
