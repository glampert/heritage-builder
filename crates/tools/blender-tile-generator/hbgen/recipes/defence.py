"""Defence (docs/plans/defence.md, Phase 1): the city's walls in three tiers, drawn like roads,
the gatehouse where a road crosses one, and the cracks a damaged wall shows.

A wall is an object tile drawn in 1x1 pieces (anchored as a 1x1 building, so it sorts with the
buildings and walkers around it). A piece's mask names which of its sides join more wall (or a
gatehouse), as four digits in the engine's order north, east, south, west; its variation index is
that mask read as a binary number, as a road's is (crates/game/src/tile/wall.rs). The engine's
north is +x on its cell grid, the upper-right edge on screen; in Blender that edge is +Y, so the
digits map to the Blender sides +Y, +X, -Y, -X.

Every piece is a cut of the same wall: its spine runs from the cell's centre to the middle of each
joined side, and the wall is that spine's cross-section swept along it, open where it meets a
neighbour (no cap, so the cut gets no ink and the two pieces meet), capped where it ends inside the
cell. A lone piece (0000) is a short length across the cell's middle.

The tiers (decision Q3, GDD 5.16): the palisade, sharpened stakes lashed to two rails; the rampart,
an earthen bank with a row of stakes along its crest (Đại Long's earthen ramparts); the city wall,
brick on a stone plinth under a stone coping, with merlons. The gatehouse stands where a road
crosses a wall, in its tier's look, its variation `<tier>_ns` (walls to its north and south) or
`<tier>_ew` (east and west): modelled with the wall along +Y and turned for ew.
"""

import math

import bmesh
import bpy

# Blender side for each mask digit (north, east, south, west): the middle of that side.
MASK_SIDES = ((0.0, 0.5), (0.5, 0.0), (0.0, -0.5), (-0.5, 0.0))

TIERS = ("palisade", "rampart", "city_wall")


def _parse_mask(mask):
    if len(mask) != 4 or set(mask) - {"0", "1"}:
        raise ValueError(f"wall mask must be four 0/1 digits, got {mask!r}")
    return [c == "1" for c in mask]


def _runs(joined, half_width):
    """The spine's runs as (axis, start, end, cap_start, cap_end): along Blender Y for the north and
    south joins, along X for east and west; a run reaching a joined side is open there, one ending
    inside the cell is capped. A lone piece is a short run across the middle, along Y."""
    north, east, south, west = joined
    runs = []
    for axis, hi, lo in (("Y", north, south), ("X", east, west)):
        if not (hi or lo):
            continue
        start = -0.5 if lo else -half_width
        end = 0.5 if hi else half_width
        runs.append((axis, start, end, not lo, not hi))
    if not runs:
        runs.append(("Y", -0.22, 0.22, True, True))
    return runs


def _sweep(B, name, profile, axis, start, end, cap_start, cap_end, m):
    """A prism: `profile` (a closed list of (w, z) points, w across the wall) swept along `axis`
    from `start` to `end`, capped at the ends asked for."""
    me = bpy.data.meshes.new(name); bm = bmesh.new()

    def point(t, w, z):
        return (w, t, z) if axis == "Y" else (t, w, z)

    a = [bm.verts.new(point(start, w, z)) for w, z in profile]
    b = [bm.verts.new(point(end, w, z)) for w, z in profile]
    n = len(profile)
    for i in range(n):
        j = (i + 1) % n
        bm.faces.new((a[i], a[j], b[j], b[i]))
    if cap_start:
        bm.faces.new(a[::-1])
    if cap_end:
        bm.faces.new(b)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(me); bm.free()
    o = bpy.data.objects.new(name, me); bpy.context.scene.collection.objects.link(o)
    B._add(o, m)
    return o


def _along(run, step, inset=0.0):
    """Points along a run, `step` apart on a grid fixed to the cell (so a neighbour's continue them),
    within the run less `inset` at a capped end."""
    axis, start, end, cap_start, cap_end = run
    lo = start + (inset if cap_start else 0.0)
    hi = end - (inset if cap_end else 0.0)
    k0 = math.ceil((lo + 0.5) / step - 1e-6)
    out = []
    t = -0.5 + k0 * step
    while t <= hi + 1e-6:
        if t >= lo - 1e-6 and abs(t) < 0.5 - 1e-6:
            out.append((0.0, t) if axis == "Y" else (t, 0.0))
        t += step
    return out


def _materials(B):
    B.M.wood("stake", "6e4a2a", "8e6a40", scale=12)
    B.M.flat("stakeTip", "c8b28a", rough=0.9)
    B.M.flat("lashing", "4a3a24", rough=0.9)
    B.M.noise("earthBank", "6a5532", "84693e", scale=16, bump_s=0.5)
    B.M.noise("earthTop", "8a7148", "a08655", scale=20, bump_s=0.35)
    B.M.noise("bankGrass", "4f5a24", "66702e", scale=18, bump_s=0.4)


# ----------------------------------------------------------------------------------
# The tiers
# ----------------------------------------------------------------------------------

STAKE_STEP = 0.0625     # sixteen stakes a cell, on a grid fixed to the cell
STAKE_R = 0.03
STAKE_H = 0.5


def _stake(B, x, y, z0, h, r=STAKE_R):
    B.cylinder("stake", x, y, z0, r, h, "stake", verts=7)
    B.cone("stake_tip", x, y, z0 + h, r, 0.0, r * 2.2, "stakeTip", verts=7)


def _palisade(B, joined):
    hw = 0.06
    for run in _runs(joined, hw):
        for x, y in _along(run, STAKE_STEP, inset=0.02):
            h = STAKE_H + 0.012 * math.sin(7.3 * (x + y) + 1.1)     # a little uneven, the same where pieces meet
            _stake(B, x, y, 0.0, h)
        axis, start, end, cs, ce = run
        for z in (0.12, 0.34):
            rail = [(-0.038, z), (0.038, z), (0.038, z + 0.022), (-0.038, z + 0.022)]
            _sweep(B, "rail", rail, axis, start, end, cs, ce, "lashing")


def _rampart(B, joined):
    base, top, h = 0.36, 0.17, 0.26
    bank = [(-base, 0.0), (base, 0.0), (top, h), (-top, h)]
    for run in _runs(joined, top):
        axis, start, end, cs, ce = run
        _sweep(B, "bank", bank, axis, start, end, cs, ce, "earthBank")
        crest = [(-top, h), (top, h), (top, h + 0.012), (-top, h + 0.012)]
        _sweep(B, "crest", crest, axis, start, end, cs, ce, "earthTop")
        for x, y in _along(run, STAKE_STEP * 1.2, inset=0.03):
            _stake(B, x, y, h, 0.17, r=0.026)


CITY_WALL_H = 0.58


def _city_wall(B, joined):
    w, h = 0.17, CITY_WALL_H
    plinth = [(-0.21, 0.0), (0.21, 0.0), (0.21, 0.06), (-0.21, 0.06)]
    body = [(-w, 0.06), (w, 0.06), (w, h), (-w, h)]
    coping = [(-0.2, h), (0.2, h), (0.2, h + 0.03), (-0.2, h + 0.03)]
    for run in _runs(joined, w):
        axis, start, end, cs, ce = run
        _sweep(B, "plinth", plinth, axis, start, end, cs, ce, "stone")
        _sweep(B, "body", body, axis, start, end, cs, ce, "brick")
        _sweep(B, "coping", coping, axis, start, end, cs, ce, "stone")
        # Crenellations along both edges of the coping: a merlon every quarter of a cell.
        for x, y in _along(run, 0.125, inset=0.06):
            if round((x + y + 0.5) / 0.125) % 2:
                continue
            for e in (-1, 1):
                mx, my = (x, y + e * (w - 0.02)) if axis == "X" else (x + e * (w - 0.02), y)
                B.box("merlon", mx, my, h + 0.03, 0.07 if axis == "X" else 0.045, 0.045 if axis == "X" else 0.07, 0.08, "brick")
    if not any(joined):
        return
    # A pillar where the wall turns or meets others, as a city wall's corners are.
    if sum(joined) >= 2 and not (joined[0] and joined[2] and not joined[1] and not joined[3]) \
            and not (joined[1] and joined[3] and not joined[0] and not joined[2]):
        B.box("pillar", 0, 0, 0, 0.44, 0.44, h + 0.06, "brick")
        B.box("pillar_cap", 0, 0, h + 0.06, 0.48, 0.48, 0.035, "stone")


def build_wall_piece(B, tier="palisade", mask="0000", **_):
    """One 1x1 piece of a wall of `tier` (one of TIERS); mask: which sides join more wall."""
    if tier not in TIERS:
        raise ValueError(f"wall tier must be one of {TIERS}, got {tier!r}")
    joined = _parse_mask(mask)
    B.outline_border = False
    _materials(B)
    {"palisade": _palisade, "rampart": _rampart, "city_wall": _city_wall}[tier](B, joined)
    return 1


# ----------------------------------------------------------------------------------
# The gatehouse
# ----------------------------------------------------------------------------------

def _turn(B, degrees):
    from mathutils import Matrix
    bpy.context.view_layer.update()
    turn = Matrix.Rotation(math.radians(degrees), 4, 'Z')
    for o in B.col.objects:
        o.matrix_world = turn @ o.matrix_world


def _gate_doors(B, half_span, z0, h):
    """Two plank doors swung open against the passage's sides."""
    for s in (1, -1):
        B.box("door", 0.16 * s, s * (half_span - 0.02), z0, 0.26, 0.03, h, "wood")
        B.box("door_band", 0.16 * s, s * (half_span - 0.035), z0 + h * 0.3, 0.26, 0.008, 0.02, "woodDark")
        B.box("door_band", 0.16 * s, s * (half_span - 0.035), z0 + h * 0.75, 0.26, 0.008, 0.02, "woodDark")


def _palisade_gate(B):
    # The wall's stakes from each end of the cell to the gate's posts, and the posts with a lintel and a
    # little roof of planks; the passage between them along X.
    for s in (1, -1):
        for y in (0.45 * s, 0.35 * s):
            _stake(B, 0.0, y, 0.0, STAKE_H)
        B.cylinder("post", 0.0, 0.25 * s, 0.0, 0.05, 0.56, "woodDark", verts=8)
    B.box("lintel", 0.0, 0.0, 0.52, 0.1, 0.62, 0.06, "woodDark")
    B.gable_roof("gate_roof", 0.0, 0.0, 0.58, 0.24, 0.66, 0.1, axis='Y', overhang=0.04, m="roofSmallY", tips=True)
    _gate_doors(B, 0.22, 0.0, 0.4)


def _rampart_gate(B):
    base, top, h = 0.36, 0.17, 0.26
    bank = [(-base, 0.0), (base, 0.0), (top, h), (-top, h)]
    for s in (1, -1):
        # Open where it meets the wall beyond the cell's edge, capped beside the gate.
        _sweep(B, "bank", bank, "Y", 0.26 if s > 0 else -0.5, 0.5 if s > 0 else -0.26, s > 0, s < 0, "earthBank")
        B.box("gate_pier", 0.0, 0.24 * s, 0.0, 0.3, 0.1, 0.5, "plaster")
    B.box("gate_beam", 0.0, 0.0, 0.5, 0.32, 0.6, 0.06, "woodDark")
    B.gable_roof("gate_roof", 0.0, 0.0, 0.56, 0.36, 0.66, 0.14, axis='Y', overhang=0.05, m="roofSmallY", tips=True)
    _gate_doors(B, 0.2, 0.0, 0.42)


def _city_gate(B):
    # A brick gate block across the wall, its arched way through dark on either face, the wall's
    # brick running on to the cell's edges, and a roofed watch pavilion on top: a citadel's gate.
    w, h = 0.17, CITY_WALL_H
    plinth = [(-0.21, 0.0), (0.21, 0.0), (0.21, 0.06), (-0.21, 0.06)]
    body = [(-w, 0.06), (w, 0.06), (w, h), (-w, h)]
    coping = [(-0.2, h), (0.2, h), (0.2, h + 0.03), (-0.2, h + 0.03)]
    for s in (1, -1):
        y0, y1 = (0.3, 0.5) if s > 0 else (-0.5, -0.3)
        _sweep(B, "plinth", plinth, "Y", y0, y1, s > 0, s < 0, "stone")
        _sweep(B, "body", body, "Y", y0, y1, s > 0, s < 0, "brick")
        _sweep(B, "coping", coping, "Y", y0, y1, s > 0, s < 0, "stone")
    gh = h + 0.08
    B.box("gate_plinth", 0.0, 0.0, 0.0, 0.54, 0.66, 0.06, "stone")
    B.box("gate_block", 0.0, 0.0, 0.06, 0.5, 0.62, gh - 0.06, "brick")
    B.box("gate_cap", 0.0, 0.0, gh, 0.54, 0.66, 0.035, "stone")
    for e in (-1, 1):
        B.box("archway", e * 0.251, 0.0, 0.06, 0.004, 0.2, 0.3, "archDark")
        B.cylinder("arch_top", e * 0.251, 0.0, 0.26, 0.1, 0.004, "archDark", verts=16, axis='X')
    top = gh + 0.035
    for x in (-0.15, 0.15):
        for y in (-0.2, 0.2):
            B.box("pavilion_post", x, y, top, 0.035, 0.035, 0.17, "lantern")
    B.box("pavilion_rail", 0.0, 0.0, top, 0.34, 0.44, 0.05, "woodDark")
    B.gable_roof("pavilion_roof", 0.0, 0.0, top + 0.17, 0.36, 0.5, 0.14, axis='Y', overhang=0.06, m="roofSmallY", tips=True)


def build_gatehouse(B, tier="palisade", axis="ns", **_):
    """1x1: the gatehouse in a wall of `tier`, the wall running north and south of it (`ns`) or east
    and west (`ew`), the road through it the other way."""
    if tier not in TIERS:
        raise ValueError(f"gatehouse tier must be one of {TIERS}, got {tier!r}")
    if axis not in ("ns", "ew"):
        raise ValueError(f"gatehouse axis must be ns or ew, got {axis!r}")
    _materials(B)
    {"palisade": _palisade_gate, "rampart": _rampart_gate, "city_wall": _city_gate}[tier](B)
    if axis == "ew":
        _turn(B, -90.0)
    return 1


# ----------------------------------------------------------------------------------
# A damaged wall's cracks
# ----------------------------------------------------------------------------------

def build_wall_cracks(B, **_):
    """1x1 overlay: what a battered wall shows over its piece, whatever its tier: dark splits and
    fallen pieces, broken stakes and bricks heaped at its foot either side."""
    B.outline_border = False
    _materials(B)
    B.M.flat("crack", "1c120c", rough=1.0)
    rng = B.rng
    for _ in range(9):
        x, y = rng.uniform(-0.3, 0.3), rng.uniform(-0.3, 0.3)
        if abs(x) < 0.12 and abs(y) < 0.12:
            continue
        o = B.box("fallen", x, y, 0.0, rng.uniform(0.05, 0.1), rng.uniform(0.04, 0.07), 0.035,
                  rng.choice(("brick", "stake", "earthBank")))
        o.rotation_euler = (0.0, 0.0, rng.uniform(0, math.pi))
    for _ in range(4):
        x, y = rng.uniform(-0.16, 0.16), rng.uniform(-0.16, 0.16)
        o = B.box("split", x, y, 0.08, 0.02, 0.2, rng.uniform(0.12, 0.3), "crack")
        o.rotation_euler = (rng.uniform(-0.4, 0.4), 0.0, rng.choice((0.0, math.pi / 2)))
    return 1
