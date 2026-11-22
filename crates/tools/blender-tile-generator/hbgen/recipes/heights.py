"""Terrain height (docs/plans/terrain-height.md, §5): the pieces the renderer draws for raised
terrain, worked out from the heights, never placed as tiles. PROTOTYPE: one rock material,
no terrain kinds yet.

Scale: one level is 32 logical px on screen. With the engine camera a cell's diagonal is 64 px
wide, so a world unit across is 64/√2 px, and a vertical unit projects to cos(30°) of that:
32 / (64/√2 × √3/2) = √(2/3) ≈ 0.8165 units a level.

Sides: the engine's north (+x on its grid) is +Y here, east (−y) is +X, south −Y and west −X. A
cell's front edges, the only ones a cliff shows on, are its south side (−Y, the lower-left edge
on screen) and its east side (+X, the lower right).

- cliff (edge, course): one level of rock face on a cell's south (`left`) or east (`right`) edge,
  standing on the cell's ground at z = 0, inside the cell. The renderer stacks a piece per level
  of the drop, from the lower ground up, so the strata repeat every level and stacked pieces join;
  the top course carries the grass lip. Pieces side by side along an edge join too: the ledges
  are the same in every piece (only small chips vary), and the cut ends get no ink.
- ramp (dir, kind): a wedge over the whole cell, from the ground at its downhill side to one level
  up at its uphill side (`n`, `e`, `s`, `w`: the engine's directions), with its sides, so a ramp
  beside lower ground shows its triangular flank and one beside another ramp is covered by it.
"""

import math

import bmesh
import bpy

LEVEL = math.sqrt(2.0 / 3.0)   # one level's height, in world units
DEPTH = 0.14                   # how far a face's rock reaches into the cell
COURSES = ((0.0, 0.34, 0.0), (0.34, 0.68, 0.025), (0.68, 1.0, 0.008))  # (from, to, inset), in levels


def _materials(B):
    B.M.noise("cliffRock", "6f6352", "938670", scale=14, bump_s=0.55)
    B.M.noise("cliffRockDark", "554b3e", "6f6352", scale=18, bump_s=0.5)
    B.M.noise("cliffGrass", "4a5320", "626a2b", scale=18, bump_s=0.4)
    B.M.noise("rampEarth", "6a5a3a", "86704a", scale=16, bump_s=0.45)
    B.M.noise("rampPath", "7e6640", "9c8256", scale=20, bump_s=0.35)
    B.M.flat("cliffTuft", "4a6624", rough=0.9)


def _turn(B, degrees):
    from mathutils import Matrix
    bpy.context.view_layer.update()   # new objects' matrix_world holds their scale only after this
    turn = Matrix.Rotation(math.radians(degrees), 4, 'Z')
    for o in B.col.objects:
        o.matrix_world = turn @ o.matrix_world


def build_cliff(B, edge="left", course="top", **_):
    """One level of rock face on a cell's south (`left`) or east (`right`) edge; `course` is
    `top` (with the grass lip) or `mid`. Modelled on the south edge (−Y), then turned."""
    if edge not in ("left", "right") or course not in ("top", "mid"):
        raise ValueError(f"cliff: edge left/right and course top/mid, got {edge!r}, {course!r}")
    B.outline_border = False
    _materials(B)
    y0 = -0.5
    for k, (lo, hi, inset) in enumerate(COURSES):
        z0, z1 = lo * LEVEL, hi * LEVEL
        m = "cliffRockDark" if k == 1 else "cliffRock"
        B.box(f"course{k}", 0.0, y0 + inset + (DEPTH - inset) / 2, z0, 1.0, DEPTH - inset, z1 - z0, m)
    # Small chips and cracks, away from the ends so neighbours still join.
    rng = B.rng
    for _ in range(5):
        x = rng.uniform(-0.36, 0.36)
        z = rng.uniform(0.08, 0.92) * LEVEL
        w, h = rng.uniform(0.05, 0.12), rng.uniform(0.03, 0.08)
        B.box("chip", x, y0 + 0.004, z, w, 0.02, h, "cliffRockDark")
    if course == "top":
        # The grass lip along the top, and a few tufts hanging over it.
        B.box("lip", 0.0, y0 + 0.06, LEVEL - 0.045, 1.0, 0.12, 0.045, "cliffGrass")
        for _ in range(6):
            x = rng.uniform(-0.42, 0.42)
            o = B.cylinder("tuft", x, y0 + 0.012, LEVEL - 0.09, 0.014, 0.07, "cliffTuft", verts=5, r2=0.003)
            o.rotation_euler = (rng.uniform(-0.4, 0.4), 0, 0)
    if edge == "right":
        _turn(B, 90.0)
    return 1


RAMP_TURNS = {"n": 0.0, "e": -90.0, "s": 180.0, "w": 90.0}


def build_ramp(B, dir="n", kind="grass", **_):
    """A ramp over the cell climbing towards `dir` (n, e, s, w: the engine's directions), its
    surface `kind` grass or road, with earthen flanks. Modelled climbing north (+Y), then turned."""
    if dir not in RAMP_TURNS or kind not in ("grass", "road"):
        raise ValueError(f"ramp: dir n/e/s/w and kind grass/road, got {dir!r}, {kind!r}")
    B.outline_border = False
    _materials(B)
    me = bpy.data.meshes.new("ramp"); bm = bmesh.new()
    a = bm.verts.new((-0.5, -0.5, 0.0)); b = bm.verts.new((0.5, -0.5, 0.0))
    c = bm.verts.new((0.5, 0.5, LEVEL)); d = bm.verts.new((-0.5, 0.5, LEVEL))
    e = bm.verts.new((0.5, 0.5, 0.0)); f = bm.verts.new((-0.5, 0.5, 0.0))
    top = bm.faces.new((a, b, c, d)); top.material_index = 0
    for face in (bm.faces.new((b, e, c)), bm.faces.new((a, d, f)), bm.faces.new((f, d, c, e))):
        face.material_index = 1
    bm.faces.new((a, f, e, b)).material_index = 1
    bm.to_mesh(me); bm.free()
    o = bpy.data.objects.new("ramp", me); bpy.context.scene.collection.objects.link(o)
    B._add(o, "cliffGrass")
    o.data.materials.append(B.M["rampEarth"])
    if kind == "road":
        # A packed path up the middle, laid on the slope.
        w = 0.42
        pm = bpy.data.meshes.new("path"); pb = bmesh.new()
        lift = 0.004
        p = [pb.verts.new((x, y, LEVEL * (y + 0.5) + lift)) for x, y in ((-w / 2, -0.5), (w / 2, -0.5), (w / 2, 0.5), (-w / 2, 0.5))]
        pb.faces.new(p); pb.to_mesh(pm); pb.free()
        po = bpy.data.objects.new("path", pm); bpy.context.scene.collection.objects.link(po)
        B._add(po, "rampPath")
    _turn(B, RAMP_TURNS[dir])
    return 1
