#!/usr/bin/env python3
"""
Migrates Heritage Builder save files from v5 to v6.

v6 keeps all of the city's gold in one treasury that can go into debt, with yearly
accounts (docs/plans/finance.md, Phase 0):
  - `Simulation::treasury` ({"gold_units": n}) becomes `Simulation::finance`: the
    balance, the tax level and wage, the debt counter, the monthly carries and the
    accounts for this year and last year (empty, starting from the balance).
  - Tax offices no longer hold gold. Each one's `stock_or_treasury`
    ({"Treasury": {"gold_units": n}}) is added to the balance and becomes "None".
  - The field is renamed `service_stock` on every service building.
`save_version` is bumped to 6.

Usage:
    python3 crates/tools/save-migration-scripts/v5_to_v6.py [save.json ...]

With no arguments, migrates every saves/*.json. Files are rewritten in place.
The migration is idempotent: a v6 save is skipped. A save older than v5 is rejected
with a message to run the earlier scripts first.
"""
import glob
import json
import os
import sys

FROM_VERSION = 5
CURRENT_VERSION = 6

# FinanceConfigs defaults (crates/game/src/config.rs).
DEFAULT_TAX_LEVEL = "Normal"
DEFAULT_WAGE = 6


def migrate_node(node, tally):
    """Rename every `stock_or_treasury` under `node`, taking tax offices' gold into `tally`."""
    if isinstance(node, list):
        for item in node:
            migrate_node(item, tally)
    elif isinstance(node, dict):
        if "stock_or_treasury" in node:
            stock = node.pop("stock_or_treasury")
            if isinstance(stock, dict) and "Treasury" in stock:
                tally["gold"] += int(stock["Treasury"].get("gold_units", 0))
                tally["tax_offices"] += 1
                stock = "None"
            node["service_stock"] = stock
            tally["services"] += 1
        for value in node.values():
            migrate_node(value, tally)


def migrate(data):
    if not isinstance(data.get("world"), dict):
        raise ValueError("save has no `world` object")
    sim = data.get("sim")
    if not isinstance(sim, dict) or not isinstance(sim.get("treasury"), dict):
        raise ValueError("save has no `sim.treasury` object")

    tally = {"gold": 0, "tax_offices": 0, "services": 0}
    migrate_node(data["world"], tally)

    balance = int(sim.pop("treasury").get("gold_units", 0)) + tally["gold"]
    sim["finance"] = {
        "balance": balance,
        "tax_level": DEFAULT_TAX_LEVEL,
        "wage": DEFAULT_WAGE,
        "months_in_debt": 0,
        "carries": {},
        "this_year": {"start_balance": balance, "totals": {}},
        "last_year": {"start_balance": balance, "totals": {}},
    }
    data["save_version"] = CURRENT_VERSION
    return {"balance": balance, **tally}


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
