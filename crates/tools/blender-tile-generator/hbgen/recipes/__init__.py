"""Recipe registry. A recipe is build_<name>(B, **params): it builds the model
with the Builder B (B.rng is seeded per variation) and returns its ground size G
in cells. Keep 1 world unit = 1 cell and centre the footprint on the origin.

To add one, write it in a module here and list it in RECIPES. The build cache
hashes the recipe's module file, so editing one module re-renders only the tiles
whose recipes live in it.
"""

from . import disease, fortress, overlays, risks, services, test, workshop

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
}

# The fortress studies predate the engine camera and keep the original one (--cells, 6 px pad).
LEGACY_CAMERA = {"fortress", "bagan_fortress", "khmer_fortress"}
