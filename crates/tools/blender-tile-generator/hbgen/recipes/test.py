"""A minimal engine recipe: the template for new ones, and the pipeline's own
test tile (README §10 has the round trip). Not in the catalog."""


def build_test_hut(B, roof_height=None, **_):
    """1x1: a yard slab, a plaster hut with a gable roof, a jar. The roof's
    height and the jar's place come from B.rng, so seeds give variations of
    different heights."""
    Z = 0.05
    B.box("yard", 0, 0, 0, 1.0, 1.0, Z, "yard")
    B.box("hut", -0.05, 0.08, Z, 0.55, 0.45, 0.32, "plaster")
    B.box("door", -0.05, 0.08 - 0.226, Z, 0.12, 0.01, 0.2, "archDark")      # on the -y face, which faces the viewer
    rh = roof_height if roof_height is not None else B.rng.uniform(0.18, 0.4)
    B.gable_roof("roof", -0.05, 0.08, Z + 0.32, 0.55, 0.45, rh, axis='X', overhang=0.07, m="roofSmallX")
    B.jar(0.3 if B.rng.random() < 0.5 else -0.32, -0.3, Z, 0.06, 0.13)
    return 1
