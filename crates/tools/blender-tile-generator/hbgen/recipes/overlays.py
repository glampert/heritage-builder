"""Construction art shared by every building (GDD 5.1.2): a construction site's
scaffolding, drawn instead of the building, and the renovation overlay drawn
over a house that is renovating. Both are generic by footprint size.

Footprints span [-size/2, size/2] on x and y. The front edges, which face the
viewer, are y = -size/2 (screen lower left) and x = +size/2 (screen lower right).
"""

import math

YARD_Z = 0.04


def _poles_along(B, name, a0, a1, fixed, axis, z0, z1, r, spacing=0.42):
    """Upright poles every ~spacing from a0 to a1 along `axis` ('X': x varies, y = fixed)."""
    n = max(2, round(abs(a1 - a0) / spacing) + 1)
    pts = [a0 + (a1 - a0) * i / (n - 1) for i in range(n)]
    for t in pts:
        x, y = (t, fixed) if axis == 'X' else (fixed, t)
        B.pole(name, (x, y, z0), (x, y, z1), r, "bamboo")
    return pts


def _scaffold_face(B, name, a0, a1, fixed, axis, z0, top, levels, r, plank_w, inward, braces=True):
    """One face of bamboo scaffolding: poles, a ledger and a plank walk per level,
    and a diagonal brace in alternate bays. inward (+1/-1) points from the poles to the building."""
    pts = _poles_along(B, name + "_pole", a0, a1, fixed, axis, z0, top, r)
    lo, hi = min(a0, a1), max(a0, a1)
    for li, z in enumerate(levels):
        if axis == 'X':
            B.pole(name + "_ledger", (lo, fixed, z), (hi, fixed, z), r * 0.8, "bamboo")
            B.box(name + "_plank", (lo + hi) / 2, fixed + inward * plank_w / 2, z + r, hi - lo, plank_w, 0.018, "wood")
        else:
            B.pole(name + "_ledger", (fixed, lo, z), (fixed, hi, z), r * 0.8, "bamboo")
            B.box(name + "_plank", fixed + inward * plank_w / 2, (lo + hi) / 2, z + r, plank_w, hi - lo, 0.018, "wood")
    if braces:
        zb0, zb1 = z0 + 0.03, (levels[0] if levels else top)
        for i in range(0, len(pts) - 1, 2):
            p, q = pts[i], pts[i + 1]
            if axis == 'X': B.pole(name + "_brace", (p, fixed, zb0), (q, fixed, zb1), r * 0.7, "bamboo")
            else:           B.pole(name + "_brace", (fixed, p, zb0), (fixed, q, zb1), r * 0.7, "bamboo")


def _brick_stack(B, x, y, z, w=0.12, d=0.1, h=0.1):
    B.box("bricks", x, y, z, w, d, h, "brick")


def _timber(B, x, y, z, length, n=3, axis='X'):
    for i in range(n):
        off = (i - (n - 1) / 2) * 0.05
        if axis == 'X': B.cylinder("timber", x, y + off, z + (0.04 if i == 1 else 0), 0.022, length, "trunk", verts=8, axis='X')
        else:           B.cylinder("timber", x + off, y, z + (0.04 if i == 1 else 0), 0.022, length, "trunk", verts=8, axis='Y')


def wall_height(size):
    """Full wall height of a site's generic building, in cells."""
    return 0.3 + 0.3 * size


def build_scaffold(B, size=1, stage=0, **_):
    """A construction site, by footprint size and stage (0-3; the engine draws a
    quarter of the progress per stage):
    0: a levelled yard, the foundation, stakes and string lines, piles of materials;
    1: brick walls a third up, the first scaffold level, the piles running down;
    2: walls two thirds up, a second scaffold level, bracing;
    3: full walls, scaffolding to the top, a roof frame half tiled."""
    h = size / 2
    m = 0.15 + 0.03 * size                  # strip between the slab edge and the walls, for the scaffolding
    a = size - 2 * m                        # the walls' square
    H = wall_height(size)
    Z = YARD_Z
    r = 0.022 + 0.004 * size                # pole radius
    B.box("yard", 0, 0, 0, size, size, Z, "yard")
    # The foundation: a stone footing where the walls go, round a dug earth floor.
    ft = 0.09
    B.box("floor", 0, 0, Z, a, a, 0.02, "earth")
    for name, x, y, w, d in (("footing_n", 0, a / 2 - ft / 2, a, ft), ("footing_s", 0, -a / 2 + ft / 2, a, ft),
                             ("footing_e", a / 2 - ft / 2, 0, ft, a), ("footing_w", -a / 2 + ft / 2, 0, ft, a)):
        B.box(name, x, y, Z, w + 0.04, d + 0.04, 0.05, "stoneDark")
    Zf = Z + 0.05
    rng = B.rng

    if stage == 0:
        for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
            B.box("stake", sx * (a / 2 + 0.06), sy * (a / 2 + 0.06), Z, 0.03, 0.03, 0.16, "woodDark")
        e = a / 2 + 0.06
        for (x0, y0), (x1, y1) in (((-e, -e), (e, -e)), ((e, -e), (e, e)), ((e, e), (-e, e)), ((-e, e), (-e, -e))):
            B.pole("string", (x0, y0, Z + 0.13), (x1, y1, Z + 0.13), 0.006, "rope", verts=6)
        # Materials waiting inside the footing, spread over the floor in proportion to its area:
        # blocks of brick stacks, bundles of timber and bamboo, stacks of roof tiles.
        inner = a - 2 * ft - 0.1
        cells = max(2, size + 1)                # a grid of cells x cells spots
        step = inner / cells
        kinds = ["bricks", "bricks", "timber", "bamboo", "tiles", "bricks"]
        for j in range(cells):
            for i in range(cells):
                if rng.random() < 0.25 and size > 1:
                    continue                    # leave some floor showing
                x = -inner / 2 + (i + 0.5) * step + rng.uniform(-0.02, 0.02)
                y = -inner / 2 + (j + 0.5) * step + rng.uniform(-0.02, 0.02)
                k = kinds[(i * 7 + j * 3 + size) % len(kinds)]
                if k == "bricks":
                    for dx in (-0.05, 0.05):
                        _brick_stack(B, x + dx, y, Z + 0.02, w=0.08, d=0.12, h=0.08 + 0.05 * rng.random())
                elif k == "timber":
                    _timber(B, x, y, Z + 0.02, min(step * 0.85, 0.5), n=4, axis='X' if (i + j) % 2 else 'Y')
                elif k == "bamboo":
                    for t in range(4):
                        B.pole("bamboo_pile", (x - step * 0.4, y - 0.06 + t * 0.04, Z + 0.02 + r), (x + step * 0.4, y - 0.06 + t * 0.04, Z + 0.02 + r), r, "bamboo")
                else:
                    B.tile_stack(x, y, Z + 0.02, 0.14, 0.12, 0.1 + 0.05 * rng.random())
        return size

    frac = (0.35, 0.7, 1.0)[stage - 1]
    wh = H * frac
    t = 0.07                                # wall thickness
    door = min(0.3, a * 0.3)
    # Brick walls, open on top; the front (-y) wall keeps a doorway.
    B.box("wall_n", 0, a / 2 - t / 2, Zf, a, t, wh, "brick")
    B.box("wall_w", -a / 2 + t / 2, 0, Zf, t, a, wh, "brick")
    B.box("wall_e", a / 2 - t / 2, 0, Zf, t, a, wh, "brick")
    seg = (a - door) / 2
    B.box("wall_s", -a / 2 + seg / 2, -a / 2 + t / 2, Zf, seg, t, wh, "brick")
    B.box("wall_s", a / 2 - seg / 2, -a / 2 + t / 2, Zf, seg, t, wh, "brick")
    if wh > 0.32:
        B.box("lintel", 0, -a / 2 + t / 2, Zf + 0.28, door + 0.06, t + 0.01, 0.04, "woodDark")
    B.box("floor", 0, 0, Zf, a - 2 * t, a - 2 * t, 0.01, "earth")

    # Scaffolding all round, a level every ~0.3 up to the wall top; Blender hides what the walls cover.
    levels = [Zf + z for z in [0.3 * (i + 1) for i in range(int(wh / 0.3))] if z < wh + 0.01] or [Zf + wh * 0.8]
    top = Zf + wh + (0.25 if stage < 3 else 0.12)
    d = a / 2 + m * 0.55                    # the poles' line, between the walls and the slab edge
    pw = m * 0.5
    for name, fixed, axis, inward in (("s", -d, 'X', 1), ("n", d, 'X', -1)):
        _scaffold_face(B, "scaf_" + name, -d, d, fixed, axis, Z, top, levels, r, pw, inward, braces=(stage >= 2 or name == "s"))
    for name, fixed, axis, inward in (("e", d, 'Y', -1), ("w", -d, 'Y', 1)):
        _scaffold_face(B, "scaf_" + name, -d, d, fixed, axis, Z, top, levels, r, pw, inward, braces=(stage >= 2 or name == "e"))
    B.pole("ladder", (d - 0.02, -d + 0.25, Z), (d - 0.12, -d + 0.25, levels[0] + 0.05), r * 0.9, "woodDark")

    if stage == 1:                          # what's left of the materials, inside the walls
        for i in range(size):
            _brick_stack(B, (i - (size - 1) / 2) * 0.2, a * 0.1, Zf, h=0.08)
        _timber(B, 0, -a * 0.15, Zf, min(0.8, a * 0.5), axis='X')
    if stage == 3:                          # a gable roof frame along x, the front slope half tiled
        rh = 0.18 + 0.12 * size
        zt = Zf + wh
        span = a / 2 + 0.05
        B.pole("ridge", (-a / 2, 0, zt + rh), (a / 2, 0, zt + rh), r * 1.2, "woodDark")
        n = max(3, round(a / 0.28) + 1)
        for i in range(n):
            x = -a / 2 + a * i / (n - 1)
            for sy in (-1, 1):
                B.pole("rafter", (x, sy * span, zt - 0.02), (x, 0, zt + rh), r, "woodDark")
        ang = math.atan2(rh, span); L = math.hypot(rh, span)
        o = B.box("roof_part", -a / 4, -span / 2, zt + rh / 2 - 0.01, a / 2 + 0.04, L, 0.025, "roofSmallX")
        o.rotation_euler = (ang, 0, 0)        # rises from the front eave (-y) to the ridge
        o.location = (-a / 4, -span / 2, zt + rh / 2 + 0.03)
    return size


def renovation_height(size):
    """How high the renovation scaffold reaches, in cells: about a house's eaves."""
    return 0.4 + 0.3 * size


def build_renovation(B, size=1, **_):
    """Drawn over a renovating house: a tall L of bamboo scaffolding around the
    footprint's front corner, part way along both front edges, with plank walks,
    braces and a ladder, and materials at its foot. It's drawn on top of the
    house, so it holds nothing that should stand behind it. Full-length
    scaffolding read as a fence around the property, since the bigger houses
    keep their buildings at the back."""
    h = size / 2
    e = h - 0.06                            # the poles stand just inside the front edges
    run = max(0.5, 0.45 * size)             # how far along each edge from the front corner
    Hr = renovation_height(size)
    r = 0.02 + 0.004 * size
    levels = [Hr * f for f in ((0.3, 0.62, 0.95) if size >= 2 else (0.45, 0.9))]
    top = Hr + 0.1
    _scaffold_face(B, "ren_s", e, e - run, -e, 'X', 0, top, levels, r, 0.1, 1)
    _scaffold_face(B, "ren_e", -e, -e + run, e, 'Y', 0, top, levels, r, 0.1, -1)
    B.pole("ladder", (e - run * 0.55, -e - 0.03, 0), (e - run * 0.62, -e + 0.05, levels[0] + 0.05), r, "woodDark")
    _brick_stack(B, e - run - 0.12, -e + 0.1, 0, w=0.1, d=0.08, h=0.07)
    B.tile_stack(e - 0.1, -e + run + 0.12, 0, 0.1, 0.08, 0.08)
    return size
