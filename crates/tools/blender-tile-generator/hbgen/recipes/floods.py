"""Floods (docs/plans/seasons-floods.md, Phase 3): the dyke, a low earthen embankment
the player draws like a road, with a packed-earth path along its crest.

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
