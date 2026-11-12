"""City risks art (docs/plans/city-risks.md, §6): what's drawn over each cell of a
sick house. A yellow cloth hangs from a bamboo pole, as a sign of sickness in the
house, and a curl of incense smoke rises from a clay burner at its foot. It loops:
the cloth ripples and the smoke's waves climb with the frame's phase (0 to 1).

It's an overlay, drawn over the house's own sprite, so it keeps to the front of
the cell and stays small; the house shows through around it.
"""

import math

import bmesh
import bpy

TAU = 2 * math.pi

POLE_X, POLE_Y = 0.12, -0.18     # front of the cell (front faces are +X and -Y)
POLE_HEIGHT = 0.62
CLOTH_W, CLOTH_H = 0.22, 0.26


def _cloth(B, name, x, y, z_top, w, h, phase, m, cols=10, rows=8):
    """A cloth hanging from the pole's top, out along +X, in the XZ plane so it
    faces the camera. It ripples more towards its free edge and its foot."""
    me = bpy.data.meshes.new(name); bm = bmesh.new()
    grid = []
    for j in range(rows + 1):
        t = j / rows                                   # 0 at the top, 1 at the foot
        row = []
        for i in range(cols + 1):
            u = i / cols                               # 0 at the pole, 1 at the free edge
            free = u * (0.4 + 0.6 * t)
            ripple = 0.05 * free * math.sin(TAU * (phase + 0.9 * u + 0.35 * t))
            sag = 0.03 * u * (1 + math.sin(TAU * (phase + 0.5 * u)))
            row.append(bm.verts.new((x + u * w, y + ripple, z_top - t * h - sag)))
        grid.append(row)
    for j in range(rows):
        for i in range(cols):
            bm.faces.new((grid[j][i], grid[j][i + 1], grid[j + 1][i + 1], grid[j + 1][i]))
    bm.to_mesh(me); bm.free()
    o = bpy.data.objects.new(name, me); bpy.context.scene.collection.objects.link(o)
    B._add(o, m); B.smooth(o)
    return o


def _smoke(B, name, x, y, z0, h, phase, m, rings=28, verts=10):
    """A tube along a wavy path from (x, y, z0) up h: its radius grows and its sway widens
    with height, and the waves move up as the phase goes from 0 to 1."""
    me = bpy.data.meshes.new(name); bm = bmesh.new()
    loops = []
    for k in range(rings + 1):
        t = k / rings
        sway = (0.015 + 0.05 * t) * math.sin(TAU * (1.6 * t - phase))
        cx = x + sway + 0.03 * t
        cy = y - 0.02 * t + 0.4 * sway * math.cos(TAU * (1.1 * t - phase))
        z = z0 + h * t
        r = (0.006 + 0.03 * t ** 0.8) * min(1.0, (1.0 - t) / 0.25 + 0.08)   # thins out at the top
        loops.append([bm.verts.new((cx + r * math.cos(TAU * a / verts), cy + r * math.sin(TAU * a / verts), z))
                      for a in range(verts)])
    bm.faces.new(loops[0][::-1])
    for k in range(rings):
        lo, hi = loops[k], loops[k + 1]
        for i in range(verts):
            bm.faces.new((lo[i], lo[(i + 1) % verts], hi[(i + 1) % verts], hi[i]))
    bm.faces.new(loops[-1])
    bm.to_mesh(me); bm.free()
    o = bpy.data.objects.new(name, me); bpy.context.scene.collection.objects.link(o)
    B._add(o, m); B.smooth(o)
    return o


def build_sickness(B, phase=0.0, **_):
    """One frame of the sickness overlay (phase 0 to 1). The pole, the burner and
    the cloth's place stay put across frames; the ripple and the smoke follow the
    phase, so the frames loop."""
    B.M.flat("pole", "7a5a32", rough=0.8)
    B.M.flat("cloth", "e6c234", rough=0.7)
    B.M.flat("clothTie", "b08a20", rough=0.7)
    B.M.flat("burner", "8a5a3c", rough=0.9)
    B.M.flat("smoke", "dfe2d2", rough=1.0, emission=0.4)
    B.M.flat("ember", "ffb050", rough=0.6, emission=2.0)

    # The pole, a thin bamboo cane with a knot or two, and the crossbar the cloth hangs from.
    B.cylinder("pole", POLE_X, POLE_Y, 0.0, 0.018, POLE_HEIGHT, "pole", verts=8)
    for z in (0.2, 0.42):
        B.cylinder(f"knot{z}", POLE_X, POLE_Y, z, 0.022, 0.015, "pole", verts=8)
    top = POLE_HEIGHT - 0.03
    B.cylinder("bar", POLE_X + CLOTH_W / 2, POLE_Y, top - 0.012, 0.012, CLOTH_W + 0.04, "pole", verts=6, axis='X')
    _cloth(B, "cloth", POLE_X, POLE_Y - 0.01, top - 0.01, CLOTH_W, CLOTH_H, phase, "cloth")
    B.box("tie", POLE_X + 0.01, POLE_Y - 0.012, top - 0.035, 0.035, 0.012, 0.03, "clothTie")

    # A clay burner beside the pole, with a glowing stick of incense.
    bx, by = POLE_X - 0.13, POLE_Y + 0.02
    B.cylinder("burner", bx, by, 0.0, 0.05, 0.06, "burner", verts=12, r2=0.065)
    B.cylinder("stick", bx, by, 0.06, 0.006, 0.08, "pole", verts=6)
    bpy.ops.mesh.primitive_ico_sphere_add(radius=0.012, subdivisions=1, location=(bx, by, 0.145))
    B._add(bpy.context.active_object, "ember")

    # The smoke: one column rising from the stick, thin at the foot and swelling as it climbs,
    # in a curl whose waves travel up with the phase, so the frames loop.
    _smoke(B, "smoke", bx, by, 0.15, 0.58, phase, "smoke")
    return 1
