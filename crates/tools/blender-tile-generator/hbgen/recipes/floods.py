"""Floods (docs/plans/seasons-floods.md, Phases 3 and 4): the dyke, a low earthen
embankment the player draws like a road, with a packed-earth path along its crest;
and the sluice gate, which stands beside a dyke and waters the fields behind it.

A dyke is a terrain tile drawn in 1x1 pieces, as roads are. A piece's mask names
which of its sides join more dyke (not a road: a road runs up to its foot), as four digits in the engine's order
north, east, south, west; its variation index is that mask read as a binary
number, as a road's is (crates/game/src/tile/road.rs). The engine's north is +x on
its cell grid, the upper-right edge on screen; in Blender that edge is +Y. So the
digits map to the Blender sides +Y, +X, -Y, -X.

Every piece is a cut of the same embankment: a height field over the cell that
stands at full height within CREST of the dyke's spine (from the cell's centre to
the middle of each joined side) and falls to the ground by FOOT, so two pieces
agree along the edge they share, and the cut edges get no ink. The ground around
it is grass, the slopes grassy earth, the crest a path. As a terrain tile its
sprite is drawn from its top corner, so each variation's iso_offset (in the
catalog) lifts it by its height above the ground diamond.
"""

import math

import bmesh
import bpy

# Blender side for each mask digit (north, east, south, west): the middle of that side.
MASK_SIDES = ((0.0, 0.5), (0.5, 0.0), (0.0, -0.5), (-0.5, 0.0))

HEIGHT = 0.16        # the crest's height, in cells: about 6 logical px
CREST = 0.1          # the crest's half width
FOOT = 0.46          # where the slopes meet the ground, from the spine
GRID = 48            # height field quads per side


def _smoothstep(e0, e1, x):
    t = min(1.0, max(0.0, (x - e0) / (e1 - e0)))
    return t * t * (3 - 2 * t)


def _parse_mask(mask):
    if len(mask) != 4 or set(mask) - {"0", "1"}:
        raise ValueError(f"dyke mask must be four 0/1 digits, got {mask!r}")
    return [c == "1" for c in mask]


def _segment_distance(px, py, ax, ay, bx, by):
    dx, dy = bx - ax, by - ay
    length2 = dx * dx + dy * dy
    t = 0.0 if length2 == 0 else max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / length2))
    return math.hypot(px - (ax + t * dx), py - (ay + t * dy))


def _spine_distance(u, v, joined):
    """Distance to the dyke's spine: the centre, and a line to each joined side's middle."""
    d = math.hypot(u, v)
    for (sx, sy), j in zip(MASK_SIDES, joined):
        if j:
            d = min(d, _segment_distance(u, v, 0.0, 0.0, sx, sy))
    return d


def _ripple(u, v):
    """Small lumps in the earth, repeating every cell so neighbours match."""
    tau = 2 * math.pi
    return (math.sin(tau * (2 * u + 3 * v) + 0.7) + 0.6 * math.sin(tau * (5 * u - 2 * v) + 1.9)) / 1.6


def _height(u, v, joined):
    f = 1.0 - _smoothstep(CREST, FOOT, _spine_distance(u, v, joined))
    return f * (HEIGHT + 0.006 * _ripple(u, v))


def _embankment(B, joined):
    me = bpy.data.meshes.new("dyke"); bm = bmesh.new()
    n = GRID
    verts = [[bm.verts.new((-0.5 + i / n, -0.5 + j / n, 0.0)) for i in range(n + 1)] for j in range(n + 1)]
    for row in verts:
        for vert in row:
            vert.co.z = _height(vert.co.x, vert.co.y, joined)
    faces = []
    for j in range(n):
        for i in range(n):
            quad = (verts[j][i], verts[j][i + 1], verts[j + 1][i + 1], verts[j + 1][i])
            face = bm.faces.new(quad)
            # Its material: ground grass, the earthen slopes, or the path along the crest.
            low = min(q.co.z for q in quad)
            high = max(q.co.z for q in quad)
            face.material_index = 2 if low >= HEIGHT * 0.86 else (0 if high <= HEIGHT * 0.06 else 1)
            faces.append(face)
    bm.to_mesh(me); bm.free()
    o = bpy.data.objects.new("dyke", me); bpy.context.scene.collection.objects.link(o)
    B._add(o, "dykeGrass")
    o.data.materials.append(B.M["dykeBank"])
    o.data.materials.append(B.M["dykePath"])
    B.smooth(o)
    return o


def _tufts(B, joined):
    """A few tufts of grass on the slopes, inside the cell."""
    rng = B.rng
    for _ in range(14):
        u, v = rng.uniform(-0.42, 0.42), rng.uniform(-0.42, 0.42)
        d = _spine_distance(u, v, joined)
        if not (CREST + 0.05 < d < FOOT - 0.04):
            continue
        z = _height(u, v, joined)
        o = B.cylinder("tuft", u, v, z - 0.004, 0.011, 0.026, "dykeTuft", verts=5, r2=0.002)
        o.rotation_euler = (rng.uniform(-0.3, 0.3), rng.uniform(-0.3, 0.3), 0)


def build_dyke(B, mask="0000", **_):
    """One 1x1 dyke piece; mask: which sides join more dyke (see the module doc)."""
    joined = _parse_mask(mask)
    B.outline_border = False
    B.M.noise("dykeGrass", "4a5320", "626a2b", scale=18, bump_s=0.4)
    B.M.noise("dykeBank", "565a28", "786e3c", scale=16, bump_s=0.5)
    B.M.noise("dykePath", "7e6640", "9c8256", scale=20, bump_s=0.35)
    B.M.flat("dykeTuft", "4a6624", rough=0.9)
    _embankment(B, joined)
    _tufts(B, joined)
    return 1


# ----------------------------------------------------------------------------------
# The sluice gate (Phase 4)
# ----------------------------------------------------------------------------------

GATE_GROUND_Z = 0.03

# The rotation that turns the gate's dyke side (+Y as it's modelled) to each side it can face,
# named in the engine's order: north (+Y in Blender), east (+X), south (-Y), west (-X).
GATE_SIDES = {"north": 0.0, "east": -90.0, "south": 180.0, "west": 90.0}


def build_sluice_gate(B, side="north", **_):
    """1x1: a sluice gate against the dyke on its `side` (one of GATE_SIDES): a brick
    headwall against the dyke with a wooden lift gate between its piers, the windlass
    that raises it under a small thatched roof, and a brick-lined channel carrying
    the water out across the cell to the fields. Modelled with the dyke on +Y, then
    turned to its side."""
    if side not in GATE_SIDES:
        raise ValueError(f"sluice gate side must be one of {sorted(GATE_SIDES)}, got {side!r}")
    B.M.noise("dykeGrass", "4a5320", "626a2b", scale=18, bump_s=0.4)
    B.M.flat("thatch", "b89a58", rough=0.95)
    B.M.flat("thatchDark", "7e6436", rough=0.95)
    B.M.flat("channelWater", "4f8590", rough=0.15)
    B.M.flat("gateBoard", "6a4a2c", rough=0.85)
    z = GATE_GROUND_Z
    B.box("ground", 0, 0, 0, 1.0, 1.0, z, "dykeGrass")

    # The channel: a narrow brick-lined cut with the water in it, from the gate to the fields.
    for x in (-0.105, 0.105):
        B.box("channel_wall", x, -0.1, z, 0.04, 0.8, 0.035, "brick")
    B.box("channel_bed", 0, -0.1, z, 0.17, 0.8, 0.008, "stoneDark")
    B.box("channel_water", 0, -0.1, z, 0.17, 0.8, 0.026, "channelWater")

    # The headwall against the dyke: low wing walls, two piers and the sill between them.
    hw = 0.2
    for x in (-0.37, 0.37):
        B.box("wing", x, 0.42, z, 0.24, 0.14, 0.1, "brick")
        B.box("wing_cap", x, 0.42, z + 0.1, 0.25, 0.15, 0.02, "stone")
    for x in (-0.18, 0.18):
        B.box("pier", x, 0.4, z, 0.16, 0.18, hw, "brick")
        B.box("pier_cap", x, 0.4, z + hw, 0.18, 0.2, 0.025, "stone")
    B.box("sill", 0, 0.4, z, 0.2, 0.18, 0.02, "stone")
    B.box("spill", 0, 0.29, z, 0.17, 0.05, 0.03, "channelWater")

    # The gate, raised a little so the water runs under it, its planks banded.
    gz = z + 0.05
    B.box("gate", 0, 0.4, gz, 0.2, 0.03, 0.18, "gateBoard")
    for k in range(3):
        B.box("gate_band", 0, 0.384, gz + 0.025 + k * 0.06, 0.2, 0.006, 0.012, "woodDark")

    # The lifting frame on the piers, its windlass and the ropes down to the gate.
    top = z + 0.46
    for x in (-0.13, 0.13):
        B.box("frame_post", x, 0.4, z + hw, 0.035, 0.035, top - (z + hw), "woodDark")
    B.box("frame_beam", 0, 0.4, top - 0.03, 0.34, 0.045, 0.035, "wood")
    B.cylinder("windlass", 0, 0.4, top - 0.13, 0.026, 0.24, "wood", verts=10, axis='X')
    for x in (-0.05, 0.05):
        B.pole("rope", (x, 0.4, top - 0.12), (x, 0.4, gz + 0.18), 0.005, "rope", verts=5)
    for x in (-0.12, 0.12):
        B.pole("windlass_spoke", (x, 0.4, top - 0.18), (x, 0.4, top - 0.06), 0.007, "woodDark", verts=5)

    # A small thatched roof over the frame.
    B.frustum("roof", 0, 0.4, top, 0.46, 0.3, 0.06, 0.26, 0.14, "thatch")
    B.box("roof_ridge", 0, 0.4, top + 0.14, 0.08, 0.28, 0.022, "thatchDark")

    # By the channel: a water jar and a coil of rope.
    B.jar(0.3, -0.28, z, 0.05, 0.11)
    B.cylinder("rope_coil", -0.3, -0.26, z, 0.05, 0.025, "rope", verts=12)

    # Turned to face its side's dyke.
    from mathutils import Matrix
    turn = Matrix.Rotation(math.radians(GATE_SIDES[side]), 4, 'Z')
    for o in B.col.objects:
        o.matrix_world = turn @ o.matrix_world
    return 1
