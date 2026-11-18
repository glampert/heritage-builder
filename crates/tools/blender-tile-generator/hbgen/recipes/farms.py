"""Farms (docs/plans/seasons-floods.md, Phase 1): the farmstead, whose workers tend
the rice fields the player lays around it, and a rice field in each stage of its
crop (fallow, sown, growing, ripe), the field tile's variations. Rendered with
the engine camera: a footprint of G cells spans [-G/2, G/2] on x and y at z = 0;
the front faces are +X (screen lower right) and -Y (screen lower left)."""

import math

GROUND_Z = 0.03


def _materials(B):
    B.M.flat("thatch", "b89a58", rough=0.95)
    B.M.flat("thatchDark", "7e6436", rough=0.95)
    B.M.flat("mud", "6e5638", rough=0.95)
    B.M.flat("mudWet", "463626", rough=0.6)
    B.M.flat("bund", "8e7448", rough=0.95)
    B.M.flat("paddyWater", "8fb2a8", rough=0.15)
    B.M.flat("stubble", "a08a52", rough=0.95)
    B.M.flat("seedling", "a8dc58", rough=0.8)
    B.M.flat("riceGreen", "6aac38", rough=0.8)
    B.M.flat("riceGold", "eec04c", rough=0.8)
    B.M.flat("hay", "c8a85a", rough=0.95)
    B.M.flat("buffalo", "3a3430", rough=0.8)


def _clump(B, x, y, z, r, h, material, lean):
    """A clump of rice: a narrow cone, leaning a little."""
    o = B.cylinder("rice", x, y, z, r, h, material, verts=6, r2=r * 0.15)
    o.rotation_euler = (lean[0], lean[1], 0)
    return o


def build_rice_field(B, stage="fallow", **_):
    """1x1: a paddy ringed by a low earthen bund. `stage`: fallow (bare mud with
    stubble), sown (shallow water with rows of seedlings), growing (dense green
    rice) or ripe (bent golden rice)."""
    _materials(B)
    B.box("ground", 0, 0, 0, 1.0, 1.0, GROUND_Z, "earth")
    z = GROUND_Z
    # The bund along each edge, a little inside the cell so neighbouring fields show their own.
    for name, x, y, w, d in (("bund_n", 0, 0.45, 0.98, 0.08), ("bund_s", 0, -0.45, 0.98, 0.08),
                             ("bund_e", 0.45, 0, 0.08, 0.98), ("bund_w", -0.45, 0, 0.08, 0.98)):
        B.box(name, x, y, z, w, d, 0.045, "bund")

    inner = 0.82
    if stage == "fallow":
        B.box("paddy", 0, 0, z, inner, inner, 0.012, "mud")
        for i in range(30):
            x = B.rng.uniform(-0.36, 0.36)
            y = B.rng.uniform(-0.36, 0.36)
            B.box("stubble", x, y, z + 0.012, 0.012, 0.012, B.rng.uniform(0.02, 0.04), "stubble")
        return 1

    if stage == "sown":
        B.box("paddy", 0, 0, z, inner, inner, 0.008, "mudWet")
        B.box("water", 0, 0, z + 0.008, inner, inner, 0.006, "paddyWater")
        rows, size, height, material = 7, 0.018, 0.07, "seedling"
    elif stage == "growing":
        B.box("paddy", 0, 0, z, inner, inner, 0.008, "mudWet")
        B.box("water", 0, 0, z + 0.008, inner, inner, 0.006, "paddyWater")
        rows, size, height, material = 6, 0.045, 0.17, "riceGreen"
    else:  # ripe
        B.box("paddy", 0, 0, z, inner, inner, 0.012, "mud")
        rows, size, height, material = 6, 0.048, 0.16, "riceGold"

    zc = z + 0.014
    step = 0.72 / rows
    for i in range(rows):
        for j in range(rows):
            x = -0.36 + step * (i + 0.5) + B.rng.uniform(-0.012, 0.012)
            y = -0.36 + step * (j + 0.5) + B.rng.uniform(-0.012, 0.012)
            bend = 0.35 if stage == "ripe" else 0.08
            lean = (B.rng.uniform(-bend, bend), B.rng.uniform(-bend, bend))
            _clump(B, x, y, zc, size, height * B.rng.uniform(0.85, 1.1), material, lean)
    return 1


def _stilt_house(B, cx, cy, w, d):
    """A house on posts with a steep thatched roof and a ladder to its floor."""
    floor = 0.22
    for sx in (-1, 1):
        for sy in (-1, 1):
            B.box("post", cx + sx * (w / 2 - 0.04), cy + sy * (d / 2 - 0.04), GROUND_Z, 0.05, 0.05, floor, "woodDark")
    B.box("floor", cx, cy, GROUND_Z + floor, w, d, 0.04, "wood")
    zf = GROUND_Z + floor + 0.04
    B.box("walls", cx, cy, zf, w - 0.06, d - 0.06, 0.26, "wood")
    B.box("door", cx + w / 2 - 0.035, cy - 0.05, zf, 0.01, 0.12, 0.18, "woodDark")
    B.frustum("roof", cx, cy, zf + 0.26, w + 0.2, d + 0.2, w * 0.15, d + 0.1, 0.34, "thatch")
    B.box("ridge", cx, cy, zf + 0.6, w * 0.17, d + 0.12, 0.03, "thatchDark")
    # The ladder up to the door.
    for k in (-1, 1):
        B.pole("ladder_rail", (cx + w / 2 + 0.16, cy - 0.05 + k * 0.05, GROUND_Z),
               (cx + w / 2 + 0.02, cy - 0.05 + k * 0.05, zf), 0.01, "woodDark", verts=6)
    for k in range(4):
        t = (k + 1) / 5
        x = cx + w / 2 + 0.16 - 0.14 * t
        z = GROUND_Z + (zf - GROUND_Z) * t
        B.box("ladder_rung", x, cy - 0.05, z, 0.015, 0.11, 0.012, "woodDark")


def _rice_store(B, cx, cy):
    """A small granary on posts: a round wicker store under a conical thatch roof."""
    for a in range(4):
        ang = math.pi / 4 + a * math.pi / 2
        B.box("store_post", cx + 0.12 * math.cos(ang), cy + 0.12 * math.sin(ang), GROUND_Z, 0.035, 0.035, 0.16, "woodDark")
    B.cylinder("store_floor", cx, cy, GROUND_Z + 0.16, 0.2, 0.03, "wood", verts=12)
    B.cylinder("store", cx, cy, GROUND_Z + 0.19, 0.17, 0.2, "hay", verts=14)
    B.cylinder("store_roof", cx, cy, GROUND_Z + 0.39, 0.24, 0.22, "thatch", verts=14, r2=0.0)


def _buffalo(B, x, y, facing):
    """A water buffalo, as blocks: body, head, horns and legs."""
    ca, sa = math.cos(facing), math.sin(facing)

    def at(u, v):
        return x + u * ca - v * sa, y + u * sa + v * ca

    for u in (-0.08, 0.08):
        for v in (-0.04, 0.04):
            lx, ly = at(u, v)
            B.box("buffalo_leg", lx, ly, GROUND_Z, 0.025, 0.025, 0.07, "buffalo")
    bx, by = at(0, 0)
    body = B.box("buffalo_body", bx, by, GROUND_Z + 0.07, 0.24, 0.12, 0.09, "buffalo")
    body.rotation_euler = (0, 0, facing)
    hx, hy = at(0.15, 0)
    head = B.box("buffalo_head", hx, hy, GROUND_Z + 0.09, 0.08, 0.07, 0.06, "buffalo")
    head.rotation_euler = (0, 0, facing)
    for v in (-1, 1):
        ax, ay = at(0.16, v * 0.04)
        bx2, by2 = at(0.13, v * 0.1)
        B.pole("buffalo_horn", (ax, ay, GROUND_Z + 0.15), (bx2, by2, GROUND_Z + 0.17), 0.008, "stubble", verts=6)


def build_farmstead(B, **_):
    """2x2: a farmstead among its paddies: a house on posts with a thatched roof
    at the back, a rice store on posts, a fenced buffalo pen with its buffalo, a
    haystack and jars in the packed-earth yard."""
    _materials(B)
    B.box("yard", 0, 0, 0, 2.0, 2.0, GROUND_Z, "yard")

    _stilt_house(B, -0.4, 0.4, 0.9, 0.7)
    _rice_store(B, 0.55, 0.55)

    # The buffalo pen in the front corner: posts and two rails.
    px, py, pw = 0.45, -0.45, 0.7
    corners = [(px - pw / 2, py - pw / 2), (px + pw / 2, py - pw / 2), (px + pw / 2, py + pw / 2),
               (px - pw / 2, py + pw / 2)]
    for x, y in corners:
        B.box("pen_post", x, y, GROUND_Z, 0.04, 0.04, 0.16, "woodDark")
    for (x0, y0), (x1, y1) in zip(corners, corners[1:] + corners[:1]):
        for h in (0.07, 0.13):
            B.pole("pen_rail", (x0, y0, GROUND_Z + h), (x1, y1, GROUND_Z + h), 0.01, "wood", verts=6)
    _buffalo(B, px, py, math.radians(30))

    # A haystack and jars by the house.
    B.cylinder("haystack", -0.6, -0.45, GROUND_Z, 0.18, 0.16, "hay", verts=14)
    B.cylinder("haystack_top", -0.6, -0.45, GROUND_Z + 0.16, 0.18, 0.12, "hay", verts=14, r2=0.02)
    B.jar(0.05, 0.15, GROUND_Z, 0.05, 0.11)
    B.jar(-0.05, 0.02, GROUND_Z, 0.04, 0.09)
    return 2
