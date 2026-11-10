"""City risks art (docs/plans/city-risks.md): the rubble a burnt-down or collapsed
building leaves, and the flames drawn over a burning one.

Rubble is built from 1x1 pieces, as the water tile's shores are. A piece's mask
names which of its sides border more rubble, as four digits in the engine's
order north, east, south, west (1: more rubble that way). The engine's north is
+x on its cell grid, the upper-right edge on screen; in Blender that edge is +Y.
So the digits map to the Blender sides +Y, +X, -Y, -X.

Every piece is a cut of the same pile: a height field over its cell that keeps
full height at a joined side and slopes to the ground at an open one. Its bumps
repeat every cell, so two pieces agree along the edge they share, and the cut
edges get no ink (B.outline_border = False). Debris stays inside the cell.
"""

import math

import bmesh
import bpy

# Blender side for each mask digit: (axis, sign).
MASK_SIDES = (("Y", 1), ("X", 1), ("Y", -1), ("X", -1))

PILE_HEIGHT = 0.2            # the pile's top, in cells, before the bumps
BUMP_HEIGHT = 0.06
SLOPE_WIDTH = 0.42           # how far in from an open side the pile reaches full height
GRID = 48                    # height field quads per side


def _smoothstep(e0, e1, x):
    t = min(1.0, max(0.0, (x - e0) / (e1 - e0)))
    return t * t * (3 - 2 * t)


def _bumps(u, v):
    """Lumps of fallen material, about -1 to 1. Integer frequencies make it repeat
    every cell, so neighbouring pieces match along their shared edge."""
    tau = 2 * math.pi
    return (math.sin(tau * (u + 2 * v) + 1.3)
            + 0.7 * math.sin(tau * (3 * u - v) + 0.4)
            + 0.5 * math.sin(tau * (2 * u + 3 * v) + 2.1)
            + 0.35 * math.sin(tau * (5 * u + 2 * v) + 0.9)
            + 0.3 * math.sin(tau * (-4 * u + 5 * v) + 2.7)) / 2.85


def _edge_distances(u, v):
    """Distance from each side, in mask order (+Y, +X, -Y, -X)."""
    return (0.5 - v, 0.5 - u, v + 0.5, u + 0.5)


def _parse_mask(mask):
    if len(mask) != 4 or set(mask) - {"0", "1"}:
        raise ValueError(f"rubble mask must be four 0/1 digits, got {mask!r}")
    return [c == "1" for c in mask]


def _cover(u, v, joined):
    """0 to 1: how much of the pile stands here (1 = full height)."""
    f = 1.0
    for d, j in zip(_edge_distances(u, v), joined):
        if not j:
            f *= _smoothstep(0.03, SLOPE_WIDTH, d)
    return f


def _height(u, v, joined):
    return _cover(u, v, joined) * (PILE_HEIGHT + BUMP_HEIGHT * _bumps(u, v)) + 0.004


def _ragged(u, v, side):
    """Where an open side's ash stops, a little in from the cell's edge."""
    t = (u if side % 2 == 0 else v) * 2 * math.pi
    return 0.035 + 0.03 * (1 + math.sin(3 * t + side) * math.cos(5 * t + 2 * side))


def _height_field(B, joined, m):
    me = bpy.data.meshes.new("pile"); bm = bmesh.new()
    n = GRID
    verts = [[bm.verts.new((-0.5 + i / n, -0.5 + j / n, 0.0)) for i in range(n + 1)] for j in range(n + 1)]
    for row in verts:
        for vert in row:
            vert.co.z = _height(vert.co.x, vert.co.y, joined)
    for j in range(n):
        for i in range(n):
            u, v = -0.5 + (i + 0.5) / n, -0.5 + (j + 0.5) / n
            # Open sides end raggedly a little inside the cell; joined sides run to the edge.
            if any(not jn and d < _ragged(u, v, s) for s, (d, jn) in enumerate(zip(_edge_distances(u, v), joined))):
                continue
            bm.faces.new((verts[j][i], verts[j][i + 1], verts[j + 1][i + 1], verts[j + 1][i]))
    loose = [vert for vert in bm.verts if not vert.link_faces]
    bmesh.ops.delete(bm, geom=loose, context='VERTS')
    bm.to_mesh(me); bm.free()
    o = bpy.data.objects.new("pile", me); bpy.context.scene.collection.objects.link(o)
    B._add(o, m); B.smooth(o)
    return o


def _place(B, joined, margin_joined=0.13, margin_open=0.12, min_cover=0.45, tries=40):
    """A random point on the pile, kept clear of the cell's edges."""
    rng = B.rng
    lo = [margin_joined if j else margin_open for j in joined]        # +Y, +X, -Y, -X
    for _ in range(tries):
        u = rng.uniform(-0.5 + lo[3], 0.5 - lo[1])
        v = rng.uniform(-0.5 + lo[2], 0.5 - lo[0])
        if _cover(u, v, joined) >= min_cover:
            return u, v
    return None


def _debris(B, joined):
    rng = B.rng
    cover = sum(_cover(-0.4 + 0.8 * i / 6, -0.4 + 0.8 * j / 6, joined) for i in range(7) for j in range(7)) / 49
    count = max(4, round(5 + 9 * cover))
    kinds = ["beam", "brick", "brick", "tiles", "stone", "beam", "brick", "tiles", "beam", "stone"]
    for k in range(count):
        p = _place(B, joined)
        if p is None:
            continue
        u, v = p
        z = _height(u, v, joined)
        kind = kinds[(k + rng.randrange(len(kinds))) % len(kinds)]
        yaw = rng.uniform(0, math.pi)
        if kind == "beam":
            # Charred beams, half sunk in the pile, both ends inside the margins.
            length = rng.uniform(0.3, 0.46)
            hx, hy = 0.5 * length * math.cos(yaw), 0.5 * length * math.sin(yaw)
            lo = [0.12 if j else 0.1 for j in joined]                     # +Y, +X, -Y, -X
            if not all(-0.5 + lo[3] < x < 0.5 - lo[1] and -0.5 + lo[2] < y < 0.5 - lo[0]
                       for x, y in ((u - hx, v - hy), (u + hx, v + hy))):
                continue
            tilt = math.radians(rng.uniform(-20, 20))
            o = B.box("beam", u, v, z - 0.035, length, 0.055, 0.05, "charred")
            o.rotation_euler = (0, tilt, yaw)
        elif kind == "brick":
            for i in range(rng.randint(2, 3)):
                du, dv = rng.uniform(-0.05, 0.05), rng.uniform(-0.05, 0.05)
                o = B.box("brick", u + du, v + dv, _height(u + du, v + dv, joined) - 0.015,
                          rng.uniform(0.07, 0.1), 0.05, 0.04, "brick")
                o.rotation_euler = (math.radians(rng.uniform(-25, 25)), math.radians(rng.uniform(-25, 25)), rng.uniform(0, math.pi))
        elif kind == "tiles":
            for i in range(rng.randint(2, 4)):
                du, dv = rng.uniform(-0.06, 0.06), rng.uniform(-0.06, 0.06)
                o = B.box("tile", u + du, v + dv, _height(u + du, v + dv, joined) - 0.005,
                          rng.uniform(0.06, 0.09), 0.05, 0.012, "tiles")
                o.rotation_euler = (math.radians(rng.uniform(-35, 35)), math.radians(rng.uniform(-35, 35)), rng.uniform(0, math.pi))
        else:
            bpy.ops.mesh.primitive_ico_sphere_add(radius=1.0, subdivisions=1, location=(u, v, z))
            o = bpy.context.active_object
            s = rng.uniform(0.035, 0.055)
            o.scale = (s * 1.3, s, s * 0.7); o.rotation_euler = (0, 0, yaw)
            B._add(o, "stoneDark")
    # A stub of fallen wall, now and then, where the pile is deep.
    if cover > 0.6 and rng.random() < 0.45:
        p = _place(B, joined, margin_joined=0.2, margin_open=0.2, min_cover=0.8)
        if p:
            u, v = p
            o = B.box("wall_stub", u, v, _height(u, v, joined) - 0.06, 0.2, 0.06, 0.13, "brick")
            o.rotation_euler = (math.radians(rng.uniform(-12, 12)), 0, rng.choice((0, math.pi / 2)) + rng.uniform(-0.3, 0.3))


def build_rubble(B, mask="0000", **_):
    """One 1x1 rubble piece; mask: which sides border more rubble (see the module doc)."""
    joined = _parse_mask(mask)
    B.outline_border = False
    B.M.noise("ash", "403a34", "77695a", scale=22, bump_s=0.9)
    B.M.flat("charred", "1d140e", rough=0.95)
    _height_field(B, joined, "ash")
    _debris(B, joined)
    return 1


# ----------------------------------------------
# Fire
# ----------------------------------------------

def _tongue(B, name, x, y, z0, r0, h, phase, offset, m, verts=10, rings=12):
    """A flame tongue: a teardrop that narrows to a tip and sways with the phase."""
    me = bpy.data.meshes.new(name); bm = bmesh.new()
    tau = 2 * math.pi
    rings_v = []
    for k in range(rings):
        t = k / rings
        r = r0 * math.sin(math.pi * (0.2 + 0.8 * t)) ** 0.7 * (1 - t) ** 0.35
        sway = 0.25 * r0 * t * math.sin(tau * (phase + 0.9 * t + offset))
        swy = 0.25 * r0 * t * math.cos(tau * (phase + 0.7 * t + offset * 1.7))
        ring = [bm.verts.new((x + sway + r * math.cos(tau * a / verts), y + swy + r * math.sin(tau * a / verts), z0 + h * t))
                for a in range(verts)]
        rings_v.append(ring)
    tip_sway = 0.25 * r0 * math.sin(tau * (phase + 0.9 + offset))
    tip = bm.verts.new((x + tip_sway, y + 0.25 * r0 * math.cos(tau * (phase + 0.7 + offset * 1.7)), z0 + h))
    bm.faces.new(rings_v[0][::-1])
    for k in range(rings - 1):
        a, b = rings_v[k], rings_v[k + 1]
        for i in range(verts):
            bm.faces.new((a[i], a[(i + 1) % verts], b[(i + 1) % verts], b[i]))
    for i in range(verts):
        bm.faces.new((rings_v[-1][i], rings_v[-1][(i + 1) % verts], tip))
    bm.to_mesh(me); bm.free()
    o = bpy.data.objects.new(name, me); bpy.context.scene.collection.objects.link(o)
    B._add(o, m); B.smooth(o)
    return o


def build_fire(B, phase=0.0, **_):
    """Flames over one cell of a burning building, one frame of a looping animation
    (phase 0 to 1). The tongues keep their places and seeds across frames; only
    their heights and sway follow the phase."""
    B.M.flat("flameOuter", "ff5a14", rough=0.6, emission=2.2)
    B.M.flat("flameCore", "ffc838", rough=0.6, emission=1.8)
    B.M.flat("ember", "ffe070", rough=0.6, emission=2.5)
    tau = 2 * math.pi
    # (x, y, base radius, height, offset): the back ones taller, so the cluster reads as one fire.
    tongues = ((0.0, 0.0, 0.15, 0.62, 0.0), (-0.12, 0.1, 0.11, 0.52, 0.37), (0.1, 0.12, 0.11, 0.48, 0.71),
               (0.13, -0.08, 0.1, 0.4, 0.19), (-0.1, -0.12, 0.1, 0.38, 0.55), (0.02, 0.2, 0.09, 0.44, 0.83),
               (-0.2, -0.02, 0.08, 0.32, 0.28))
    for i, (x, y, r, h, off) in enumerate(tongues):
        hh = h * (1 + 0.22 * math.sin(tau * (phase + off)))
        _tongue(B, f"flame{i}", x, y, 0.02, r, hh, phase, off, "flameOuter")
        # A yellow core, smaller and shifted to the front faces so it shows through.
        _tongue(B, f"core{i}", x + 0.025, y - 0.025, 0.02, r * 0.55, hh * 0.62, phase, off + 0.1, "flameCore", verts=8, rings=8)
    # Embers rising: each climbs one step per frame and wraps around.
    for i, (x, y, off) in enumerate(((0.08, 0.05, 0.1), (-0.1, 0.12, 0.45), (0.15, -0.1, 0.7), (-0.05, -0.15, 0.9))):
        t = (phase + off) % 1.0
        bpy.ops.mesh.primitive_ico_sphere_add(radius=0.014, subdivisions=1, location=(x + 0.04 * math.sin(tau * t), y, 0.55 + 0.4 * t))
        B._add(bpy.context.active_object, "ember")
    return 1
