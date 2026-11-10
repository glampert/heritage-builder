"""Service buildings for the city risks (docs/plans/city-risks.md): the fire watch
and the builders' guild. Rendered with the engine camera: a footprint of G cells
spans [-G/2, G/2] on x and y at z = 0; the front faces are +X (screen lower
right) and -Y (screen lower left)."""

import math

import bpy

YARD_Z = 0.04


def _rail(B, name, p0, p1, z, m="woodDark", t=0.025):
    """A square rail from (x0, y0) to (x1, y1) at height z."""
    (x0, y0), (x1, y1) = p0, p1
    B.box(name, (x0 + x1) / 2, (y0 + y1) / 2, z, abs(x1 - x0) + t, abs(y1 - y0) + t, t, m)


def _ladder(B, name, foot, top, width=0.12, rungs=6, r=0.012, m="woodDark"):
    """A ladder from foot to top (x, y, z points), its rails `width` apart across x."""
    (fx, fy, fz), (tx, ty, tz) = foot, top
    for s in (-1, 1):
        B.pole(name + "_rail", (fx + s * width / 2, fy, fz), (tx + s * width / 2, ty, tz), r, m)
    for i in range(1, rungs + 1):
        t = i / (rungs + 1)
        x, y, z = fx + (tx - fx) * t, fy + (ty - fy) * t, fz + (tz - fz) * t
        B.pole(name + "_rung", (x - width / 2, y, z), (x + width / 2, y, z), r * 0.8, m)


def build_fire_watch(B, **_):
    """1x1: a raised timber lookout with a bell under its roof and a red pennant,
    a ladder up the front, and water jars and buckets at its foot."""
    Z = YARD_Z
    B.box("yard", 0, 0, 0, 1.0, 1.0, Z, "yard")
    post, p = 0.06, 0.27                                  # posts at (+-p, +-p)
    deck_z, top_z = 0.5, 0.8
    for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
        B.box("post", sx * p, sy * p, Z, post, post, top_z - Z, "woodDark")
    # Cross braces under the deck on the two back faces, so the frame reads as a tower.
    for (x0, y0), (x1, y1) in (((-p, p), (p, p)), ((-p, -p), (-p, p))):
        B.pole("brace", (x0, y0, Z + 0.06), (x1, y1, deck_z - 0.04), 0.014, "woodDark")
        B.pole("brace", (x1, y1, Z + 0.06), (x0, y0, deck_z - 0.04), 0.014, "woodDark")
    B.box("deck", 0, 0, deck_z, 2 * p + 0.14, 2 * p + 0.14, 0.04, "wood")
    e = p + 0.07
    for a, b in (((-e, -e), (e, -e)), ((e, -e), (e, e)), ((e, e), (-e, e)), ((-e, e), (-e, -e))):
        _rail(B, "railing", a, b, deck_z + 0.13)
    for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
        B.box("rail_post", sx * e, sy * e, deck_z + 0.04, 0.025, 0.025, 0.11, "woodDark")
    # A small hipped roof, its eaves inside the diamond's side corners.
    B.frustum("roof_eave", 0, 0, top_z - 0.05, 0.84, 0.84, 0.7, 0.7, 0.05, "roof")
    B.frustum("roof", 0, 0, top_z, 0.7, 0.7, 0.1, 0.1, 0.24, "roof")
    B.box("roof_cap", 0, 0, top_z + 0.24, 0.12, 0.12, 0.04, "ridge")
    # Upturned tips at the eave corners, as on the hand-made roofs; the side ones stay inside the frame.
    for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
        tip = B.box("roof_tip", sx * 0.38, sy * 0.38, top_z - 0.05, 0.045, 0.045, 0.08, "ridge")
        tip.rotation_euler = (math.radians(-14 * sy), math.radians(14 * sx), 0)
    # The alarm bell, hanging in the open top storey.
    B.pole("bell_rope", (0, 0, top_z - 0.06), (0, 0, top_z - 0.1), 0.006, "rope")
    B.cylinder("bell", 0, 0, deck_z + 0.13, 0.075, 0.13, "gold", verts=16, r2=0.045)
    # A red pennant on the roof, so the building reads from afar.
    B.pole("flag_pole", (0, 0, top_z + 0.27), (0, 0, top_z + 0.52), 0.01, "woodDark")
    B.M.flat("pennant", "c42a1c", rough=0.8)
    flag = B.box("pennant", 0.0, -0.09, top_z + 0.41, 0.012, 0.17, 0.09, "pennant")
    flag.rotation_euler = (math.radians(-8), 0, 0)
    # A ladder up the front-left face (-y) to the deck.
    _ladder(B, "ladder", (0.08, -0.46, Z), (0.08, -p - 0.08, deck_z + 0.02))
    # Water at the foot: jars by the front corner and buckets.
    for x, y, r, h in ((0.36, -0.2, 0.075, 0.17), (0.38, 0.02, 0.065, 0.15), (0.2, 0.36, 0.07, 0.16)):
        B.jar(x, y, Z, r, h)
    for x, y in ((-0.3, -0.38), (-0.18, -0.4)):
        B.cylinder("bucket", x, y, Z, 0.045, 0.06, "wood", verts=12, r2=0.055)
        B.cylinder("water", x, y, Z + 0.055, 0.047, 0.006, "water", verts=12)
    return 1


def _timber_stack(B, x, y, z, length, rows=3, per_row=4, axis='X', r=0.024):
    """Logs stacked in a pyramid, lying along axis."""
    for row in range(rows):
        n = per_row - row
        for i in range(n):
            off = (i - (n - 1) / 2) * 2 * r
            zz = z + row * r * 1.7
            if axis == 'X': B.cylinder("log", x, y + off, zz, r, length, "trunk", verts=8, axis='X')
            else:           B.cylinder("log", x + off, y, zz, r, length, "trunk", verts=8, axis='Y')


def _brick_pallet(B, x, y, z, w=0.16, d=0.12, h=0.1):
    B.box("pallet", x, y, z, w + 0.03, d + 0.03, 0.02, "woodDark")
    B.box("bricks", x, y, z + 0.02, w, d, h, "brick")


def build_builders_guild(B, **_):
    """2x2: a plastered hall with a gable roof at the back, an open timber shed
    along the back-right edge, and a yard of bricks, logs and a section of bamboo
    scaffolding where the guild's builders practise and stock their materials."""
    Z = 0.05
    B.box("yard", 0, 0, 0, 2.0, 2.0, Z, "yard")
    # The hall: back-left (-x), its door facing +x.
    hx, hy, hw, hd, hh = -0.5, 0.3, 0.8, 1.2, 0.42
    B.box("hall_base", hx, hy, Z, hw + 0.06, hd + 0.06, 0.05, "stoneDark")
    B.box("hall", hx, hy, Z + 0.05, hw, hd, hh, "plaster")
    for yy in (hy - hd / 2 + 0.02, hy + hd / 2 - 0.02):                  # timber corner posts
        for xx in (hx - hw / 2 + 0.02, hx + hw / 2 - 0.02):
            B.box("hall_post", xx, yy, Z + 0.05, 0.05, 0.05, hh, "woodDark")
    B.box("door", hx + hw / 2 + 0.005, hy - 0.15, Z + 0.05, 0.02, 0.2, 0.28, "archDark")
    B.box("lintel", hx + hw / 2 + 0.01, hy - 0.15, Z + 0.33, 0.03, 0.28, 0.04, "woodDark")
    for wy in (hy + 0.2, hy - 0.45):                                     # windows on the front faces
        B.box("window", hx + hw / 2 + 0.005, wy, Z + 0.2, 0.02, 0.14, 0.1, "archDark")
    B.box("window", hx + 0.05, hy - hd / 2 - 0.005, Z + 0.2, 0.18, 0.02, 0.1, "archDark")
    B.gable_roof("hall_roof", hx, hy, Z + 0.05 + hh, hw, hd, 0.32, axis='Y', overhang=0.08, m="roofSmallY")
    # A tool board beside the door: saw and mallet shapes on pegs.
    B.box("board", hx + hw / 2 + 0.01, hy + 0.4, Z + 0.12, 0.02, 0.22, 0.16, "wood")
    B.box("saw", hx + hw / 2 + 0.025, hy + 0.36, Z + 0.17, 0.01, 0.1, 0.03, "stoneDark")
    B.box("mallet", hx + hw / 2 + 0.025, hy + 0.46, Z + 0.15, 0.01, 0.03, 0.09, "woodDark")
    # The timber shed along the back-right edge (+y): posts, a lean-to roof, logs under it.
    sx0, sx1, sy = 0.05, 0.92, 0.72
    for xx in (sx0, (sx0 + sx1) / 2, sx1):
        for yy in (sy - 0.18, sy + 0.2):
            B.box("shed_post", xx, yy, Z, 0.045, 0.045, 0.34 if yy > sy else 0.28, "woodDark")
    roof = B.box("shed_roof", (sx0 + sx1) / 2, sy + 0.01, Z + 0.31, sx1 - sx0 + 0.14, 0.5, 0.03, "roofSmallX")
    roof.rotation_euler = (math.radians(-8), 0, 0)
    _timber_stack(B, (sx0 + sx1) / 2 - 0.05, sy + 0.02, Z, 0.62, rows=3, per_row=5, axis='X')
    # The yard, front cells: brick pallets, a log pile, a sawhorse with a log, a scaffold section.
    for i, (x, y, h) in enumerate(((0.3, 0.15, 0.1), (0.52, 0.15, 0.07), (0.3, -0.05, 0.08), (0.74, 0.15, 0.11))):
        _brick_pallet(B, x, y, Z, h=h)
    _timber_stack(B, 0.62, -0.32, Z, 0.4, rows=2, per_row=3, axis='Y')
    for sx in (-0.05, 0.12):                                            # sawhorse: two A-frames and a log
        for sy_ in (-1, 1):
            B.pole("sawhorse", (sx, -0.45 + sy_ * 0.05, Z), (sx, -0.45, Z + 0.12), 0.01, "woodDark")
    B.cylinder("sawlog", 0.035, -0.45, Z + 0.1, 0.03, 0.36, "trunk", verts=8, axis='X')
    # A section of bamboo scaffolding by the front-left edge, with a plank walk.
    r = 0.016
    for x in (-0.75, -0.45, -0.15):
        for y in (-0.88, -0.66):
            B.pole("scaf_pole", (x, y, Z), (x, y, Z + 0.62), r, "bamboo")
    for z in (Z + 0.3, Z + 0.58):
        for y in (-0.88, -0.66):
            B.pole("scaf_ledger", (-0.78, y, z), (-0.12, y, z), r * 0.8, "bamboo")
        B.box("scaf_plank", -0.45, -0.77, z + r, 0.66, 0.2, 0.018, "wood")
    B.pole("scaf_brace", (-0.75, -0.88, Z + 0.02), (-0.45, -0.88, Z + 0.3), r * 0.7, "bamboo")
    B.pole("scaf_brace", (-0.45, -0.88, Z + 0.02), (-0.15, -0.88, Z + 0.3), r * 0.7, "bamboo")
    # Bamboo poles waiting to be used, lying by the scaffold, and a coil of rope.
    for i in range(4):
        B.pole("bamboo_pile", (-0.92, -0.45 + i * 0.035, Z + r), (-0.2, -0.45 + i * 0.035, Z + r), r, "bamboo")
    B.cylinder("rope", 0.35, -0.75, Z, 0.06, 0.03, "rope", verts=16)
    return 2
