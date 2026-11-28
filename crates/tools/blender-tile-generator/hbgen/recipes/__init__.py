"""Recipe registry. A recipe is build_<name>(B, **params): it builds the model
with the Builder B (B.rng is seeded per variation) and returns its ground size G
in cells. Keep 1 world unit = 1 cell and centre the footprint on the origin.

To add one, write it in a module here and list it in RECIPES. The build cache
hashes the recipe's module file, so editing one module re-renders only the tiles
whose recipes live in it.
"""

from . import defence, disease, faith, farms, floods, fortress, heights, overlays, risks, services, test, trade, walkers, workshop

RECIPES = {
    "fortress": fortress.build_fortress,
    "bagan_fortress": fortress.build_bagan_fortress,
    "khmer_fortress": fortress.build_khmer_fortress,
    "roof_tile_kiln": workshop.build_roof_tile_kiln,
    "test_hut": test.build_test_hut,
    "scaffold": overlays.build_scaffold,
    "renovation": overlays.build_renovation,
    "rubble": risks.build_rubble,
    "fire": risks.build_fire,
    "sickness": disease.build_sickness,
    "fire_watch": services.build_fire_watch,
    "builders_guild": services.build_builders_guild,
    "festival_square": faith.build_festival_square,
    "farmstead": farms.build_farmstead,
    "rice_field": farms.build_rice_field,
    "dyke": floods.build_dyke,
    "sluice_gate": floods.build_sluice_gate,
    "dock": trade.build_dock,
    "trade_ship": trade.build_trade_ship,
    "cliff": heights.build_cliff,
    "rim": heights.build_rim,
    "ramp": heights.build_ramp,
    "settler_family": walkers.build_settler_family,
    "peasant": walkers.build_peasant,
    "medic": walkers.build_medic,
    "fire_watcher": walkers.build_fire_watcher,
    "trader": walkers.build_trader,
    "dog": walkers.build_dog,
    "buffalo": walkers.build_buffalo,
    "wall_piece": defence.build_wall_piece,
    "gatehouse": defence.build_gatehouse,
    "wall_cracks": defence.build_wall_cracks,
}

# The fortress studies predate the engine camera and keep the original one (--cells, 6 px pad).
LEGACY_CAMERA = {"fortress", "bagan_fortress", "khmer_fortress"}
