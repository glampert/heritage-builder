"""Trade (docs/plans/trade.md, Phase 2): the dock, where trade ships moor while its
dockers carry their goods, and the trade ship itself, a sea junk.

The dock is a 2x2 building on open water at the shore, like the fishing wharf. Its
variations are the bank it faces (the side its land is on), named as the wharf's:
north_bank, south_bank, east_bank, west_bank (tile/water.rs picks one by name as it's
placed). It's modelled with its land on +Y, the engine's north, then turned to its
bank, as the sluice gate is (floods.GATE_SIDES).

The ship is the pipeline's first unit: a rigid model rendered in the unit sprite
format, in five of the eight directions it sails (ne, se, and the diagonals' n, s and
e; nw, sw and w are those mirrored: docs/plans/unit-movement.md), with a slow
two-frame bob for its idle and its sailing. It's modelled with its bow on +X (screen
lower right, se) and turned about Z to each facing: +Y is upper right (ne), and the
diagonals lie between, e (screen right) at 45 degrees, n (up) at 135 and s (down) at
-45. Its frame is a 2x2 footprint's, so the ship, longer than a cell, fits at every
facing (across the screen, e, it's widest: about 72 of the frame's 128 logical px);
the unit's iso_offset in the catalog puts the frame's middle on its cell.
"""

import math

import bmesh
import bpy
from mathutils import Matrix

from .floods import GATE_SIDES

# The deck's height above the water, and the piles' spacing.
DECK_Z = 0.12

# The rotation that turns the ship's bow (+X as it's modelled) to each facing.
SHIP_FACINGS = {"se": 0.0, "e": 45.0, "ne": 90.0, "n": 135.0, "s": -45.0}


def _turn(B, degrees):
    turn = Matrix.Rotation(math.radians(degrees), 4, 'Z')
    for o in B.col.objects:
        o.matrix_world = turn @ o.matrix_world


def _bale(B, x, y, z, w=0.13, d=0.1, h=0.08):
    """A cloth-wrapped bale tied with rope."""
    B.box("bale", x, y, z, w, d, h, "bale")
    B.box("bale_rope", x, y, z, 0.012, d + 0.004, h + 0.004, "rope")
    B.box("bale_rope", x, y, z, w + 0.004, 0.012, h + 0.004, "rope")


def _crate(B, x, y, z, s=0.1):
    B.box("crate", x, y, z, s, s, s * 0.8, "wood")
    B.box("crate_band", x, y, z + s * 0.35, s + 0.006, s + 0.006, 0.012, "woodDark")


def build_dock(B, side="north", **_):
    """2x2: a timber quay on piles over the water at the shore, its land on `side`
    (one of floods.GATE_SIDES): a plank deck with a stone landing against the bank, a
    small tiled storehouse at the back, a wooden crane at the water's edge with a bale
    on its rope, bollards, and bales, jars and crates waiting on the deck."""
    if side not in GATE_SIDES:
        raise ValueError(f"dock side must be one of {sorted(GATE_SIDES)}, got {side!r}")
    B.M.noise("bale", "c9b48a", "a8936a", scale=12, bump_s=0.3)
    B.M.noise("baleRed", "9c4a32", "7c3624", scale=12, bump_s=0.3)
    B.M.wood("plank", "8a6440", "6a4a2c", scale=9)

    # The stone landing against the bank (+Y): a low revetment the deck rests on.
    B.box("landing", 0, 0.82, 0, 2.0, 0.36, DECK_Z + 0.02, "stone")
    B.box("landing_cap", 0, 0.82, DECK_Z + 0.02, 2.0, 0.36, 0.02, "pave")

    # The deck on its piles, over the water: planks running across, a darker stringer at the edge.
    # (The deck reaches the footprint's edges, so its front corner touches the frame's foot.)
    B.box("deck", 0, -0.18, DECK_Z - 0.03, 2.0, 1.64, 0.03, "plank")
    for k in range(9):
        y = -0.98 + k * 0.2
        B.box("deck_seam", 0, y, DECK_Z - 0.002, 1.98, 0.008, 0.004, "woodDark")
    B.box("stringer", 0, -0.98, DECK_Z - 0.09, 1.96, 0.04, 0.06, "woodDark")
    for x in (-0.98, 0.98):
        B.box("stringer", x, -0.18, DECK_Z - 0.09, 0.04, 1.6, 0.06, "woodDark")
    for k in range(6):
        x = -0.9 + k * 0.36
        B.cylinder("pile", x, -0.98, 0, 0.035, DECK_Z + 0.06, "woodDark", verts=8)
    for k in range(4):
        y = -0.62 + k * 0.4
        for x in (-0.98, 0.98):
            B.cylinder("pile", x, y, 0, 0.035, DECK_Z + 0.06, "woodDark", verts=8)
    # A pile at each corner, from the water line, its outer edge on the footprint's corner (the
    # sprite's anchor where the deck stands over the water).
    r = 0.02
    c = 1.0 - r / math.sqrt(2)
    for x in (-c, c):
        for y in (-c, c):
            B.cylinder("corner_pile", x, y, 0, r, DECK_Z + 0.06, "woodDark", verts=8)

    # Bollards along the water's edge, a mooring rope round one.
    for x in (-0.55, 0.15, 0.85):
        B.cylinder("bollard", x, -0.9, DECK_Z, 0.035, 0.09, "woodDark", verts=10)
    B.cylinder("mooring", 0.15, -0.9, DECK_Z + 0.05, 0.045, 0.02, "rope", verts=12)

    # The storehouse at the back, on the landing's side: plaster walls, a tiled gable roof, a wide
    # door onto the deck.
    hx, hy, hw, hd, hh = -0.45, 0.48, 0.95, 0.62, 0.36
    B.box("store_base", hx, hy, DECK_Z, hw + 0.05, hd + 0.05, 0.04, "stoneDark")
    B.box("store", hx, hy, DECK_Z + 0.04, hw, hd, hh, "plaster")
    for xx in (hx - hw / 2 + 0.02, hx + hw / 2 - 0.02):
        for yy in (hy - hd / 2 + 0.02, hy + hd / 2 - 0.02):
            B.box("store_post", xx, yy, DECK_Z + 0.04, 0.05, 0.05, hh, "woodDark")
    B.box("store_door", hx + 0.12, hy - hd / 2 - 0.005, DECK_Z + 0.04, 0.3, 0.02, 0.24, "archDark")
    B.box("store_lintel", hx + 0.12, hy - hd / 2 - 0.01, DECK_Z + 0.28, 0.38, 0.03, 0.04, "woodDark")
    B.box("store_window", hx + hw / 2 + 0.005, hy, DECK_Z + 0.2, 0.02, 0.16, 0.09, "archDark")
    B.gable_roof("store_roof", hx, hy, DECK_Z + 0.04 + hh, hw, hd, 0.3, axis='X', overhang=0.09, m="roofSmallX")

    # The crane at the water's edge: a post with braces, a jib out over the water, its rope and a
    # bale hanging from it.
    cx, cy = 0.62, -0.7
    B.box("crane_base", cx, cy, DECK_Z, 0.18, 0.18, 0.04, "stoneDark")
    B.cylinder("crane_post", cx, cy, DECK_Z + 0.04, 0.035, 0.72, "wood", verts=10)
    top = DECK_Z + 0.76
    jx, jy, jz = cx + 0.1, cy - 0.5, top - 0.08
    B.pole("crane_jib", (cx, cy + 0.12, top - 0.22), (jx, jy, jz), 0.022, "wood")
    B.pole("crane_stay", (cx, cy, top), (jx, jy, jz), 0.008, "rope", verts=5)
    for dx, dy in ((0.12, 0.0), (-0.12, 0.0), (0.0, 0.12)):
        B.pole("crane_brace", (cx + dx, cy + dy, DECK_Z + 0.04), (cx, cy, DECK_Z + 0.32), 0.014, "woodDark", verts=6)
    B.cylinder("crane_drum", cx, cy + 0.06, DECK_Z + 0.22, 0.04, 0.16, "woodDark", verts=10, axis='X')
    B.pole("crane_rope", (jx, jy, jz), (jx, jy, DECK_Z + 0.3), 0.006, "rope", verts=5)
    B.box("crane_load", jx, jy, DECK_Z + 0.2, 0.12, 0.1, 0.1, "baleRed")

    # Goods on the deck: bales, jars and crates waiting for the ships.
    for (x, y, z) in ((-0.15, -0.35, DECK_Z), (0.0, -0.35, DECK_Z), (-0.08, -0.35, DECK_Z + 0.08)):
        _bale(B, x, y, z)
    for (x, y) in ((0.25, -0.1), (0.36, -0.12), (0.3, 0.0)):
        B.jar(x, y, DECK_Z, 0.045, 0.11)
    _crate(B, -0.75, -0.45, DECK_Z)
    _crate(B, -0.62, -0.5, DECK_Z)
    _crate(B, -0.7, -0.47, DECK_Z + 0.08, s=0.08)
    _bale(B, 0.55, 0.3, DECK_Z, w=0.12, d=0.12)
    B.box("bale_red", 0.72, 0.28, DECK_Z, 0.12, 0.1, 0.08, "baleRed")
    B.cylinder("rope_coil", -0.35, -0.82, DECK_Z, 0.06, 0.025, "rope", verts=14)

    _turn(B, GATE_SIDES[side])
    return 2


# ----------------------------------------------------------------------------
# The trade ship
# ----------------------------------------------------------------------------
# Its hull's sections, stern to bow: (x, half beam, deck height, bilge height). The stern stands
# high, as a junk's does; the bow is a flat transom.
HULL = ((-0.8, 0.17, 0.34, 0.08), (-0.6, 0.24, 0.26, 0.04), (-0.3, 0.27, 0.2, 0.02), (0.1, 0.27, 0.19, 0.02),
        (0.45, 0.23, 0.21, 0.03), (0.72, 0.16, 0.26, 0.07))


def _hull(B, name, m, deck_m):
    """The hull lofted through HULL's sections: a U at each, faced between them, with the transoms
    and the deck closing it."""
    me = bpy.data.meshes.new(name)
    bm = bmesh.new()
    rings = []
    for x, beam, deck, bilge in HULL:
        rings.append([bm.verts.new(p) for p in ((x, -beam, deck), (x, -beam * 0.8, bilge), (x, 0, 0.0),
                                                (x, beam * 0.8, bilge), (x, beam, deck))])
    for a, b in zip(rings, rings[1:]):
        for i in range(4):
            bm.faces.new((a[i], b[i], b[i + 1], a[i + 1]))
    bm.faces.new(rings[0][::-1])
    bm.faces.new(rings[-1])
    bm.to_mesh(me)
    bm.free()
    o = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(o)
    B._add(o, m)
    # The deck: planks laid flat between the sides, a little below the gunwale.
    for (x0, b0, d0, _), (x1, b1, d1, _) in zip(HULL, HULL[1:]):
        B.box("deck", (x0 + x1) / 2, 0, min(d0, d1) - 0.03, x1 - x0, (min(b0, b1)) * 1.9, 0.02, deck_m)
    return o


def _sail(B, x, z0, h, w, rake=8.0):
    """A battened lug sail on its mast at `x`: a panel of cloth across the hull's axis, a little
    raked, with dark battens across it."""
    sail = B.box("sail", x - w * 0.35, 0, z0, w, 0.012, h, "sail")
    battens = []
    for k in range(1, 6):
        battens.append(B.box("batten", x - w * 0.35, 0, z0 + h * k / 6, w + 0.02, 0.02, 0.012, "woodDark"))
    B.box("boom", x - w * 0.35, 0, z0 - 0.01, w + 0.03, 0.025, 0.02, "woodDark")
    tilt = Matrix.Rotation(math.radians(rake), 4, 'Y')
    pivot = Matrix.Translation((x, 0, z0))
    for o in [sail] + battens:
        o.matrix_world = pivot @ tilt @ pivot.inverted() @ o.matrix_world


def build_trade_ship(B, facing="se", bob=0, **_):
    """A sea junk (one hull for river and sea routes in every realm for now, Q15): a dark hull with
    a high stern and a red-banded gunwale, a deckhouse aft under a tiled roof, two masts with
    battened ochre sails, a rudder, and bales and jars on deck. `facing` is "se" (bow to the screen's
    lower right), "ne" (upper right), "e" (right), "n" (up) or "s" (down); `bob` 1 lifts it a little on the swell, the second frame of
    its idle and its sailing."""
    if facing not in SHIP_FACINGS:
        raise ValueError(f"ship facing must be one of {sorted(SHIP_FACINGS)}, got {facing!r}")
    B.M.wood("hull", "5a3b22", "3e2814", scale=10)
    B.M.flat("gunwale", "8e2f22", rough=0.7)
    B.M.noise("sail", "c27a3a", "a45e2a", scale=6, bump_s=0.2)
    B.M.noise("bale", "c9b48a", "a8936a", scale=12, bump_s=0.3)
    B.M.wood("plank", "8a6440", "6a4a2c", scale=9)

    _hull(B, "hull", "hull", "plank")
    # The gunwale's red band along each side.
    for (x0, b0, d0, _), (x1, b1, d1, _) in zip(HULL, HULL[1:]):
        for s in (-1, 1):
            band = B.pole("gunwale", (x0, s * b0, d0), (x1, s * b1, d1), 0.014, "gunwale", verts=6)
            band.name = "gunwale"

    # The deckhouse aft, under a small tiled roof, and the stern's rail.
    B.box("deckhouse", -0.5, 0, 0.22, 0.26, 0.32, 0.14, "plaster")
    B.box("deckhouse_door", -0.365, 0, 0.22, 0.01, 0.1, 0.1, "archDark")
    B.gable_roof("deckhouse_roof", -0.5, 0, 0.36, 0.26, 0.32, 0.1, axis='X', overhang=0.04, m="roofSmallX", tips=True)
    B.box("rudder", -0.82, 0, 0.02, 0.04, 0.03, 0.3, "woodDark")
    B.pole("tiller", (-0.8, 0, 0.32), (-0.62, 0, 0.36), 0.01, "woodDark", verts=6)

    # Two masts with their sails, the mainmast taller.
    B.cylinder("mainmast", 0.02, 0, 0.17, 0.025, 1.18, "woodDark", verts=10)
    B.cylinder("foremast", 0.48, 0, 0.19, 0.02, 0.86, "woodDark", verts=10)
    _sail(B, 0.02, 0.44, 0.82, 0.82)
    _sail(B, 0.48, 0.44, 0.56, 0.56)
    B.pole("stay", (0.02, 0, 1.35), (0.72, 0, 0.27), 0.005, "rope", verts=5)
    B.pole("stay", (0.02, 0, 1.35), (-0.78, 0, 0.36), 0.005, "rope", verts=5)

    # Cargo on deck: bales and jars amidships.
    for (x, y, z) in ((0.25, -0.1, 0.18), (0.25, 0.08, 0.18), (0.27, 0.0, 0.26)):
        _bale(B, x, y, z, w=0.12, d=0.1, h=0.08)
    for y in (-0.12, 0.0, 0.12):
        B.jar(-0.18, y, 0.17, 0.04, 0.1)

    # The swell: the second frame sits a little higher, its bow lifted.
    if bob:
        lift = Matrix.Translation((0, 0, 0.02)) @ Matrix.Rotation(math.radians(-1.5), 4, 'Y')
        for o in B.col.objects:
            o.matrix_world = lift @ o.matrix_world

    _turn(B, SHIP_FACINGS[facing])
    return 2
