"""Terrain height (docs/plans/terrain-height.md, §5; placeholder-art Phase 5): the pieces the
renderer draws for raised terrain, worked out from the heights, never placed as tiles. They live in
the objects tile set's overlays category (catalog/heights.json), so they share the objects atlas
with the buildings.

Scale: one level is 32 logical px on screen (tile::elevation::LEVEL_STEP_PX). With the engine
camera a cell's diagonal is 64 px wide, so a world unit across is 64/√2 px, and a vertical unit
projects to cos(30°) of that: 32 / (64/√2 × √3/2) = √(2/3) ≈ 0.8165 units a level.

Sides: the engine's north (+x on its grid) is +Y here, east (−y) is +X, south −Y and west −X. A
cell's front edges, the only ones a cliff shows on, are its south side (−Y, the lower-left edge on
screen: `left`) and its east side (+X, the lower right: `right`).

- cliff (edge, course, lip): one level of rock face on a cell's south (`left`) or east (`right`)
  edge, standing on the cell's ground at z = 0, inside the cell. The renderer stacks a course per
  level of the drop, from the lower ground up: `mid` courses, then a `top` course capped by a lip
  (`grass` or `dirt`, as the cell's terrain) with an inked top edge. Courses join stacked and side
  by side: each is one open mesh (no end caps, so its ends get no ink), its ledges at the same
  heights in every piece, and its rock's noise wraps along the edge and repeats every level. Only
  small cracks vary, away from the ends.
- rim (side): a light lip with an inked edge along one side of a raised cell's top, drawn after the
  top wherever the ground drops on that side, front or back, so a terrace's whole edge reads apart
  from the ground below.
- ramp (dir, half, kind): half of a ramp, which climbs one level over two cells towards its uphill
  side (`n`, `e`, `s`, `w`: the engine's directions): its `low` half rises from the cell's ground to
  half a level up, its `high` half from half a level up to one level up, standing on its flanks.
  (A level in one cell, about 39°, is steeper than the camera's 30°: a ramp climbing towards the
  viewer would face away from it.) The flanks are rock, so a ramp beside lower ground shows its
  side and one beside another ramp or higher ground is covered by it. Its top is the terrain's own
  sprite (grass, dirt, or the straight road piece along the ramp) mapped onto the slope and shaded
  by it, so it matches the ground it joins. Its ends are open, so they get no ink where they meet
  the ground, its other half and the cell above.
"""

import math
import os

import bmesh
import bpy

from ..palettes import LIGHTING

LEVEL = math.sqrt(2.0 / 3.0)   # one level's height, in world units
DEPTH = 0.14                   # how far a face's rock reaches into the cell
# The ledges within a level: (from, to, inset), in levels from the course's foot. The same in every
# piece, so courses join side by side; the last course's inset meets the next level's first.
COURSES = ((0.0, 0.36, 0.0), (0.36, 0.7, 0.022), (0.7, 1.0, 0.008))
LIP = 0.05                     # the top course's lip: its thickness at the top of the face
RIM_WIDTH = 0.055              # a rim's band, in from the edge
RIM_HEIGHT = 0.022
SLOPE_SHADE = 0.55             # how much a ramp's top darkens or lightens with the way it faces the sun
# How far a course or a rim reaches past its cell's ends, so where two pieces meet the soft edge of
# one lies over solid pixels of the other instead of two soft edges showing a seam. Clipped by the
# frame where an end is a side corner.
OVERLAP = 0.025

# The repository root, for the terrain sprites a ramp's top is made of.
REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", "..", ".."))
TERRAIN = os.path.join(REPO, "assets", "tiles", "terrain", "land")

# A ramp's top: the terrain sprite of each kind, by whether the ramp climbs north-south (n, s) or
# east-west (e, w): a road's straight piece runs along the ramp (its mask: north, east, south, west).
RAMP_SPRITES = {
    "grass": ("grass/var0/grass.png", "grass/var0/grass.png"),
    "dirt": ("dirt/var0/dirt.png", "dirt/var0/dirt.png"),
    "dirt_road": ("dirt_road/1010/1010.png", "dirt_road/0101/0101.png"),
    "paved_road": ("paved_road/1010/1010.png", "paved_road/0101/0101.png"),
}

# Where a terrain sprite's ground diamond lies in its image (draw size 65 × 36 over a 64 × 32
# diamond, measured on grass var0), as (u, v) with v up: its left, front (bottom), right and back
# (top) corners, a hair inside so the slope samples no transparent edge.
DIAMOND_UV = {
    (-1, -1): (0.012, 1.0 - 0.439),   # left
    (1, -1): (0.499, 1.0 - 0.866),    # front
    (1, 1): (0.986, 1.0 - 0.439),     # right
    (-1, 1): (0.499, 1.0 - 0.010),    # back
}

# The rotation (degrees about Z) that turns a piece modelled on its south side (−Y) to each side.
SIDE_TURNS = {"s": 0.0, "e": 90.0, "n": 180.0, "w": -90.0}
# The rotation that turns a ramp modelled climbing north (+Y) to climb each way.
RAMP_TURNS = {"n": 0.0, "e": -90.0, "s": 180.0, "w": 90.0}


# ----------------------------------------------------------------------------------
# Materials
# ----------------------------------------------------------------------------------

def _lin(hexc):
    r, g, b = [int(hexc[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    f = lambda c: c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4
    return (f(r), f(g), f(b), 1.0)


def _new_material(B, name):
    m = bpy.data.materials.new(name); m.use_nodes = True
    nt = m.node_tree
    for n in list(nt.nodes):
        nt.nodes.remove(n)
    B.M.m[name] = m
    return m, nt


def _wrapping_rock(B, name, dark, light, scale=2.2, fine=7.0):
    """Rock whose noise wraps every cell along X and every level up Z: the noise is sampled on two
    circles (4D), so a course's ends match its neighbours' and its top matches the next course's
    foot."""
    m, nt = _new_material(B, name)
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    bsdf = nt.nodes.new("ShaderNodeBsdfPrincipled"); nt.links.new(bsdf.outputs[0], out.inputs[0])
    if "Specular IOR Level" in bsdf.inputs:
        bsdf.inputs["Specular IOR Level"].default_value = 0.15
    bsdf.inputs["Roughness"].default_value = 0.95
    tc = nt.nodes.new("ShaderNodeTexCoord")
    sep = nt.nodes.new("ShaderNodeSeparateXYZ"); nt.links.new(tc.outputs["Object"], sep.inputs[0])

    def angle(src, period):
        node = nt.nodes.new("ShaderNodeMath"); node.operation = 'MULTIPLY'
        node.inputs[1].default_value = 2.0 * math.pi / period
        nt.links.new(src, node.inputs[0]); return node.outputs[0]

    def trig(src, op):
        node = nt.nodes.new("ShaderNodeMath"); node.operation = op
        nt.links.new(src, node.inputs[0]); return node.outputs[0]

    u = angle(sep.outputs[0], 1.0)
    w = angle(sep.outputs[2], LEVEL)
    vec = nt.nodes.new("ShaderNodeCombineXYZ")
    nt.links.new(trig(u, 'COSINE'), vec.inputs[0]); nt.links.new(trig(u, 'SINE'), vec.inputs[1])
    nt.links.new(trig(w, 'COSINE'), vec.inputs[2])

    def noise4d(s):
        nz = nt.nodes.new("ShaderNodeTexNoise"); nz.noise_dimensions = '4D'
        nz.inputs["Scale"].default_value = s; nz.inputs["Detail"].default_value = 3; nz.inputs["Roughness"].default_value = 0.6
        nt.links.new(vec.outputs[0], nz.inputs["Vector"]); nt.links.new(trig(w, 'SINE'), nz.inputs["W"])
        return nz.outputs["Fac"]

    coarse, detail = noise4d(scale), noise4d(fine)
    mix = nt.nodes.new("ShaderNodeMath"); mix.operation = 'MULTIPLY_ADD'
    nt.links.new(detail, mix.inputs[0]); mix.inputs[1].default_value = 0.35; nt.links.new(coarse, mix.inputs[2])
    ramp = nt.nodes.new("ShaderNodeValToRGB"); el = ramp.color_ramp.elements
    el[0].position, el[0].color = 0.45, _lin(dark)
    el[1].position, el[1].color = 0.95, _lin(light)
    nt.links.new(mix.outputs[0], ramp.inputs[0]); nt.links.new(ramp.outputs[0], bsdf.inputs["Base Color"])
    bump = nt.nodes.new("ShaderNodeBump"); bump.inputs["Strength"].default_value = 0.5; bump.inputs["Distance"].default_value = 0.04
    nt.links.new(mix.outputs[0], bump.inputs["Height"]); nt.links.new(bump.outputs[0], bsdf.inputs["Normal"])


def _sun_towards():
    """The direction towards the key light, in world space (palettes.LIGHTING's sun)."""
    from mathutils import Euler, Vector
    rot = Euler([math.radians(a) for a in LIGHTING["sun_rot"]])
    return -(rot.to_matrix() @ Vector((0.0, 0.0, -1.0)))


def _terrain_top(B, name, sprite):
    """A terrain sprite on a slope: unlit (emission), so flat it renders as the sprite does, times
    a shade from how the slope faces the sun, so it still reads as a slope."""
    path = os.path.join(TERRAIN, sprite)
    if not os.path.isfile(path):
        raise FileNotFoundError(f"ramp top: no terrain sprite {path}")
    m, nt = _new_material(B, name)
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    emit = nt.nodes.new("ShaderNodeEmission"); emit.inputs["Strength"].default_value = 1.0
    nt.links.new(emit.outputs[0], out.inputs[0])
    tex = nt.nodes.new("ShaderNodeTexImage"); tex.interpolation = 'Closest'
    tex.image = bpy.data.images.load(path, check_existing=True)
    uv = nt.nodes.new("ShaderNodeTexCoord"); nt.links.new(uv.outputs["UV"], tex.inputs["Vector"])

    sun = _sun_towards()
    geo = nt.nodes.new("ShaderNodeNewGeometry")
    dot = nt.nodes.new("ShaderNodeVectorMath"); dot.operation = 'DOT_PRODUCT'
    nt.links.new(geo.outputs["Normal"], dot.inputs[0]); dot.inputs[1].default_value = tuple(sun)
    # shade = 1 + SLOPE_SHADE × (n·sun − up·sun): 1 on flat ground.
    shade = nt.nodes.new("ShaderNodeMath"); shade.operation = 'MULTIPLY_ADD'
    nt.links.new(dot.outputs["Value"], shade.inputs[0]); shade.inputs[1].default_value = SLOPE_SHADE
    shade.inputs[2].default_value = 1.0 - SLOPE_SHADE * sun.z
    mul = nt.nodes.new("ShaderNodeMix"); mul.data_type = 'RGBA'; mul.blend_type = 'MULTIPLY'
    mul.inputs["Factor"].default_value = 1.0
    nt.links.new(tex.outputs["Color"], mul.inputs[6])
    col = nt.nodes.new("ShaderNodeCombineColor")
    for i in range(3):
        nt.links.new(shade.outputs[0], col.inputs[i])
    nt.links.new(col.outputs[0], mul.inputs[7])
    nt.links.new(mul.outputs[2], emit.inputs["Color"])


def _materials(B):
    _wrapping_rock(B, "cliffRock", "5f5446", "a09178")
    _wrapping_rock(B, "cliffRockDark", "4a4136", "7d705c")
    B.M.noise("flankRock", "5f5446", "9a8a72", scale=12, bump_s=0.5)
    B.M.noise("cliffGrass", "4f5622", "6a7030", scale=18, bump_s=0.4)
    B.M.noise("cliffDirt", "6a5236", "8a6c48", scale=18, bump_s=0.4)
    B.M.noise("cliffRim", "b5a27a", "d8c79c", scale=22, bump_s=0.25)
    B.M.flat("cliffCrack", "3a332a", rough=0.95)
    B.M.flat("cliffTuft", "4a6624", rough=0.9)


# ----------------------------------------------------------------------------------
# Geometry
# ----------------------------------------------------------------------------------

def _turn(B, degrees):
    from mathutils import Matrix
    bpy.context.view_layer.update()   # new objects' matrix_world holds their scale only after this
    turn = Matrix.Rotation(math.radians(degrees), 4, 'Z')
    for o in B.col.objects:
        o.matrix_world = turn @ o.matrix_world


def _sweep(B, name, profile, materials):
    """Sweeps a profile in the (y, z) plane along X, across the cell and OVERLAP past its ends: a
    strip of quads between consecutive points, each with its material index; the ends are left
    open. Listed from the foot up along the outside, each quad faces out (towards −Y, up, or down
    under a lip)."""
    me = bpy.data.meshes.new(name); bm = bmesh.new()
    left = [bm.verts.new((-0.5 - OVERLAP, y, z)) for y, z in profile]
    right = [bm.verts.new((0.5 + OVERLAP, y, z)) for y, z in profile]
    for i in range(len(profile) - 1):
        face = bm.faces.new((left[i], right[i], right[i + 1], left[i + 1]))
        face.material_index = materials[i]
    bm.to_mesh(me); bm.free()
    o = bpy.data.objects.new(name, me); bpy.context.scene.collection.objects.link(o)
    return o


# Material slots of a cliff course's face.
ROCK, ROCK_DARK, LIP_TURF = 0, 1, 2


def build_cliff(B, edge="left", course="top", lip="grass", **_):
    """One level of rock face on a cell's south (`left`) or east (`right`) edge; `course` is `top`
    (capped by a `grass` or `dirt` lip) or `mid`. Modelled on the south edge (−Y), then turned."""
    if edge not in ("left", "right") or course not in ("top", "mid") or lip not in ("grass", "dirt"):
        raise ValueError(f"cliff: edge left/right, course top/mid, lip grass/dirt; got {edge!r}, {course!r}, {lip!r}")
    B.outline_border = False
    _materials(B)
    y0 = -0.5

    # The face's profile, from its foot up: each course's face, and the ledge into the next.
    profile, materials = [(y0 + COURSES[0][2], 0.0)], []
    for k, (_, hi, inset) in enumerate(COURSES):
        if k > 0:
            profile.append((y0 + inset, profile[-1][1]))
            materials.append(ROCK)
        top = hi * LEVEL
        if course == "top" and k == len(COURSES) - 1:
            top -= LIP
        profile.append((y0 + inset, top))
        materials.append(ROCK_DARK if k == 1 else ROCK)
    if course == "top":
        # The lip: a thin edge of turf over the face's top, out to the cell's edge, inked along it.
        profile += [(y0, LEVEL - LIP), (y0, LEVEL), (y0 + DEPTH, LEVEL)]
        materials += [LIP_TURF, LIP_TURF, LIP_TURF]
    else:
        profile.append((y0 + DEPTH, LEVEL))
        materials.append(ROCK)

    o = _sweep(B, "cliff_face", profile, materials)
    B._add(o, "cliffRock")
    o.data.materials.append(B.M["cliffRockDark"])
    o.data.materials.append(B.M["cliffGrass" if lip == "grass" else "cliffDirt"])

    # A few cracks, away from the ends so neighbours still join.
    rng = B.rng
    for _ in range(rng.randint(2, 4)):
        x = rng.uniform(-0.34, 0.34)
        z = rng.uniform(0.1, 0.7) * LEVEL
        crack = B.box("crack", x, y0 - 0.002, z, rng.uniform(0.012, 0.02), 0.006, rng.uniform(0.08, 0.2), "cliffCrack")
        crack.rotation_euler = (0.0, rng.uniform(-0.35, 0.35), 0.0)
    if course == "top" and lip == "grass":
        for _ in range(5):
            x = rng.uniform(-0.4, 0.4)
            o = B.cylinder("tuft", x, y0 + 0.006, LEVEL - LIP - 0.05, 0.012, 0.06, "cliffTuft", verts=5, r2=0.003)
            o.rotation_euler = (rng.uniform(-0.5, -0.2), 0, 0)
    if edge == "right":
        _turn(B, SIDE_TURNS["e"])
    return 1


def build_rim(B, side="s", **_):
    """The lip along one side of a raised cell's top (`n`, `e`, `s`, `w`), inked along its edge.
    Modelled on the south side (−Y), then turned."""
    if side not in SIDE_TURNS:
        raise ValueError(f"rim: side must be one of {sorted(SIDE_TURNS)}, got {side!r}")
    B.outline_border = False
    _materials(B)
    y0 = -0.5
    profile = [(y0, 0.0), (y0, RIM_HEIGHT), (y0 + RIM_WIDTH, RIM_HEIGHT), (y0 + RIM_WIDTH + 0.02, 0.0)]
    o = _sweep(B, "rim", profile, [0, 0, 0])
    B._add(o, "cliffRim")
    _turn(B, SIDE_TURNS[side])
    return 1


def _turned(x, y, degrees):
    a = math.radians(degrees)
    return (x * math.cos(a) - y * math.sin(a), x * math.sin(a) + y * math.cos(a))


def build_ramp(B, dir="n", half="low", kind="grass", **_):
    """Half a ramp over the cell climbing towards `dir` (n, e, s, w: the engine's directions):
    `low` from the ground to half a level up, `high` from half a level up to a level up; its top
    `kind`'s terrain sprite (RAMP_SPRITES), its flanks rock. Built in place (modelled climbing north,
    +Y, and turned point by point) so the sprite's corners stay on the cell's screen corners."""
    if dir not in RAMP_TURNS or half not in ("low", "high") or kind not in RAMP_SPRITES:
        raise ValueError(f"ramp: dir n/e/s/w, half low/high, kind one of {sorted(RAMP_SPRITES)}; got {dir!r}, {half!r}, {kind!r}")
    B.outline_border = False
    _materials(B)
    sprite = RAMP_SPRITES[kind][0 if dir in ("n", "s") else 1]
    _terrain_top(B, f"rampTop_{kind}_{dir}", sprite)
    turn = RAMP_TURNS[dir]
    z0 = 0.0 if half == "low" else LEVEL / 2   # its top's downhill edge
    z1 = z0 + LEVEL / 2                         # its uphill edge

    def vert(bm, x, y, z):
        tx, ty = _turned(x, y, turn)
        return bm.verts.new((tx, ty, z))

    me = bpy.data.meshes.new("ramp"); bm = bmesh.new()
    # The top, downhill (−Y) to uphill (+Y), and its flanks down to the cell's ground on ±X.
    a = vert(bm, -0.5, -0.5, z0); b = vert(bm, 0.5, -0.5, z0)
    c = vert(bm, 0.5, 0.5, z1); d = vert(bm, -0.5, 0.5, z1)
    e = vert(bm, 0.5, 0.5, 0.0); f = vert(bm, -0.5, 0.5, 0.0)
    top = bm.faces.new((a, b, c, d)); top.material_index = 0
    if half == "low":
        flanks = (bm.faces.new((b, e, c)), bm.faces.new((a, d, f)))
    else:
        g = vert(bm, 0.5, -0.5, 0.0); h = vert(bm, -0.5, -0.5, 0.0)
        flanks = (bm.faces.new((g, e, c, b)), bm.faces.new((h, a, d, f)))
    for face in flanks:
        face.material_index = 1
    # Every face out of the wedge: the top up, each flank away from the cell's middle.
    bm.normal_update()
    if top.normal.z < 0:
        top.normal_flip()
    for face in flanks:
        centre = face.calc_center_median()
        if face.normal.x * centre.x + face.normal.y * centre.y < 0:
            face.normal_flip()
    # The top's UVs: each corner on the sprite's diamond corner for where it lands on screen.
    uv_layer = bm.loops.layers.uv.new("UVMap")
    for loop in top.loops:
        x, y = loop.vert.co.x, loop.vert.co.y
        loop[uv_layer].uv = DIAMOND_UV[(1 if x > 0 else -1, 1 if y > 0 else -1)]
    bm.to_mesh(me); bm.free()
    o = bpy.data.objects.new("ramp", me); bpy.context.scene.collection.objects.link(o)
    B._add(o, f"rampTop_{kind}_{dir}")
    o.data.materials.append(B.M["flankRock"])
    return 1
