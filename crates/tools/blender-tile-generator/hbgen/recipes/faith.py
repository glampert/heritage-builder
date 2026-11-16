"""Faith and festivals (docs/plans/faith.md, Phase 4): the festival square, where
the city holds its festivals, and the same square dressed for one (bunting from
its pole, lanterns, food stalls and mats), which the renderer draws in its place
while a festival is held. Rendered with the engine camera: a footprint of G
cells spans [-G/2, G/2] on x and y at z = 0; the front faces are +X (screen
lower right) and -Y (screen lower left)."""

import math

import bmesh
import bpy

YARD_Z = 0.05

# The festive colours: bunting, canopies, mats.
FLAG_COLOURS = (("flagRed", "c42a1c"), ("flagYellow", "e0b030"), ("flagGreen", "3f8a3a"), ("flagBlue", "2f5f9a"),
                ("flagWhite", "e8e0cc"))


def _materials(B):
    for name, hexc in FLAG_COLOURS:
        B.M.flat(name, hexc, rough=0.8)
    B.M.flat("drum", "8e2416", rough=0.6)
    B.M.flat("drumSkin", "d9c49a", rough=0.8)
    B.M.flat("mat", "a8341f", rough=0.95)
    B.M.flat("matBorder", "d8a640", rough=0.95)
    B.M.flat("fruit", "e07a1e", rough=0.6)


def _triangle(B, name, a, b, c, m):
    """A flat triangle through three points: a pennant on a string."""
    me = bpy.data.meshes.new(name)
    bm = bmesh.new()
    bm.faces.new([bm.verts.new(p) for p in (a, b, c)])
    bm.to_mesh(me)
    bm.free()
    o = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(o)
    return B._add(o, m)


def _sag(p0, p1, t, sag):
    """A point t along a string from p0 to p1 that hangs `sag` lower at its middle."""
    return tuple(p0[i] + (p1[i] - p0[i]) * t for i in range(2)) + (p0[2] + (p1[2] - p0[2]) * t - sag * 4 * t * (1 - t),)


def _bunting(B, name, p0, p1, sag=0.12, flags=7, lanterns=()):
    """A sagging string from p0 to p1 hung with pennants in turn of the festive
    colours, and small lanterns at the fractions in `lanterns`."""
    steps = 10
    points = [_sag(p0, p1, i / steps, sag) for i in range(steps + 1)]
    for a, b in zip(points, points[1:]):
        B.pole(name + "_string", a, b, 0.008, "rope", verts=6)
    dx, dy = p1[0] - p0[0], p1[1] - p0[1]
    length = math.hypot(dx, dy)
    ux, uy = dx / length, dy / length
    fw, fh = 0.085, 0.11
    for k in range(flags):
        t = (k + 1) / (flags + 1)
        x, y, z = _sag(p0, p1, t, sag)
        colour = FLAG_COLOURS[k % 4][0]
        _triangle(B, name + "_flag", (x - ux * fw / 2, y - uy * fw / 2, z), (x + ux * fw / 2, y + uy * fw / 2, z),
                  (x, y, z - fh), colour)
    for t in lanterns:
        x, y, z = _sag(p0, p1, t, sag)
        _lantern(B, x, y, z - 0.03, r=0.035)


def _lantern(B, x, y, z, r=0.045):
    """A small round paper lantern hanging from (x, y, z): a cord, a cap and the body."""
    B.pole("lantern_cord", (x, y, z), (x, y, z - r * 0.6), 0.004, "rope", verts=6)
    B.cylinder("lantern_cap", x, y, z - r * 0.75, r * 0.45, r * 0.18, "gold", verts=10)
    bpy.ops.mesh.primitive_uv_sphere_add(radius=1.0, location=(x, y, z - r * 0.75 - r * 1.1), segments=12, ring_count=8)
    o = bpy.context.active_object
    o.scale = (r, r, r * 1.2)
    B._add(o, "lantern")
    B.smooth(o)
    B.box("lantern_tassel", x, y, z - r * 0.75 - r * 2.4, 0.012, 0.012, r * 0.3, "gold")


def _stall(B, x, y, along_x, canopy):
    """A food stall: four posts, a pitched cloth canopy and a counter of jars and fruit."""
    w, d, h = (0.44, 0.3, 0.3) if along_x else (0.3, 0.44, 0.3)
    for sx in (-1, 1):
        for sy in (-1, 1):
            B.box("stall_post", x + sx * (w / 2 - 0.02), y + sy * (d / 2 - 0.02), YARD_Z, 0.025, 0.025, h, "woodDark")
    tw, td = (w * 0.8, 0.04) if along_x else (0.04, d * 0.8)        # a ridge along the stall's length
    B.frustum("stall_canopy", x, y, YARD_Z + h, w + 0.08, d + 0.08, tw, td, 0.1, canopy)
    B.box("stall_counter", x, y, YARD_Z, w - 0.06, d - 0.08, 0.13, "wood")
    for i in range(3):
        t = (i - 1) * 0.12
        jx, jy = (x + t, y) if along_x else (x, y + t)
        if i == 1:
            for k in range(3):
                bpy.ops.mesh.primitive_uv_sphere_add(radius=0.022, location=(jx + (k - 1) * 0.025, jy, YARD_Z + 0.152),
                                                     segments=8, ring_count=6)
                B._add(bpy.context.active_object, "fruit")
        else:
            B.jar(jx, jy, YARD_Z + 0.13, 0.03, 0.07)


def _mat(B, x, y, w, d):
    B.box("mat_border", x, y, YARD_Z + 0.02, w, d, 0.006, "matBorder")
    B.box("mat", x, y, YARD_Z + 0.026, w - 0.05, d - 0.05, 0.004, "mat")


def _hip_roof(B, name, cx, cy, z, w, top):
    """A hipped tile roof over a square pavilion, with upturned tips at the eave corners."""
    B.frustum(name + "_eave", cx, cy, z - 0.05, w + 0.14, w + 0.14, w, w, 0.05, "roof")
    B.frustum(name, cx, cy, z, w, w, top, top, 0.26, "roof")
    B.box(name + "_cap", cx, cy, z + 0.26, top + 0.04, top + 0.04, 0.04, "ridge")
    e = w / 2 + 0.05
    for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
        tip = B.box(name + "_tip", cx + sx * e, cy + sy * e, z - 0.05, 0.05, 0.05, 0.09, "ridge")
        tip.rotation_euler = (math.radians(-16 * sy), math.radians(16 * sx), 0)


def build_festival_square(B, festive=False, **_):
    """3x3: an open paved square for the city's festivals. A small open stage
    pavilion with a hipped roof stands in the back corner, a drum beside its
    steps; low walls close the back edges; a tall pole with a pennant stands in
    the open front of the square, and stone lantern posts at its front corners.
    `festive`: dressed for a festival, with bunting from the pole to the posts
    and the stage, lanterns, two food stalls, mats and an offering table."""
    _materials(B)
    Z = YARD_Z
    B.box("yard", 0, 0, 0, 3.0, 3.0, Z, "yard")
    B.box("paving", 0.05, -0.05, Z, 2.7, 2.7, 0.015, "pave")
    Zp = Z + 0.015

    # Low walls along the back edges (-x and +y), with posts at their ends and middles.
    for name, x, y, w, d in (("wall_w", -1.44, 0.03, 0.1, 2.8), ("wall_n", -0.03, 1.44, 2.8, 0.1)):
        B.box(name, x, y, Z, w, d, 0.14, "stoneDark")
        B.box(name + "_cap", x, y, Z + 0.14, w + 0.03, d + 0.03, 0.025, "stone")
    for x, y in ((-1.44, -1.34), (-1.44, 0.0), (-1.44, 1.44), (0.0, 1.44), (1.34, 1.44)):
        B.box("wall_post", x, y, Z, 0.14, 0.14, 0.22, "stoneDark")
        B.box("wall_post_cap", x, y, Z + 0.22, 0.17, 0.17, 0.03, "stone")

    # The stage: a raised stone platform, four red posts and a hipped roof, steps to the square.
    sx, sy, sw, ph, post_h = -0.72, 0.72, 1.0, 0.16, 0.44
    B.box("stage", sx, sy, Z, sw + 0.06, sw + 0.06, ph, "stone")
    B.box("stage_floor", sx, sy, Z + ph, sw, sw, 0.02, "wood")
    zf = Z + ph + 0.02
    p = sw / 2 - 0.05
    for kx, ky in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
        B.box("stage_post", sx + kx * p, sy + ky * p, zf, 0.06, 0.06, post_h, "drum")
    B.box("stage_beam", sx, sy - p, zf + post_h - 0.05, sw, 0.05, 0.05, "woodDark")
    B.box("stage_beam", sx + p, sy, zf + post_h - 0.05, 0.05, sw, 0.05, "woodDark")
    B.box("stage_screen", sx, sy + p - 0.01, zf, sw - 0.1, 0.03, post_h - 0.06, "plaster")
    _hip_roof(B, "stage_roof", sx, sy, zf + post_h, sw + 0.04, 0.12)
    for k in range(3):
        B.box("stage_step", sx + sw / 2 + 0.05 + k * 0.06, sy - 0.05, Z, 0.06, 0.4, ph - k * 0.05, "stoneDark")

    # The festival drum on its stand beside the stage's steps, its skin to the square.
    dx, dy = 0.05, 1.05
    for ky in (-1, 1):
        B.box("drum_stand", dx, dy + ky * 0.1, Z, 0.03, 0.03, 0.2, "woodDark")
    B.cylinder("drum", dx - 0.08, dy, Z + 0.12, 0.09, 0.16, "drum", verts=16, axis='X')
    B.cylinder("drum_skin", dx + 0.08, dy, Z + 0.125, 0.085, 0.005, "drumSkin", verts=16, axis='X')

    # The pole in the open square, on a stone footing, with a pennant at the top.
    px, py, top = 0.3, -0.3, 1.25
    B.box("pole_footing", px, py, Z, 0.18, 0.18, 0.08, "stone")
    B.pole("pole", (px, py, Z + 0.08), (px, py, top), 0.02, "bamboo")
    B.cylinder("pole_finial", px, py, top, 0.03, 0.05, "gold", verts=10, r2=0.0)
    pennant = B.box("pennant", px, py - 0.12, top - 0.14, 0.012, 0.22, 0.1, "flagRed")
    pennant.rotation_euler = (math.radians(-6), 0, 0)

    # Stone lantern posts at the square's front corners.
    posts = ((-1.25, -1.25), (1.25, -1.25), (1.25, 1.25))
    for x, y in posts:
        B.box("lamp_base", x, y, Z, 0.12, 0.12, 0.06, "stoneDark")
        B.box("lamp_shaft", x, y, Z + 0.06, 0.06, 0.06, 0.24, "stone")
        B.box("lamp_box", x, y, Z + 0.3, 0.1, 0.1, 0.08, "stone")
        B.box("lamp_light", x, y, Z + 0.32, 0.104, 0.06, 0.04, "lantern")
        B.frustum("lamp_cap", x, y, Z + 0.38, 0.15, 0.15, 0.02, 0.02, 0.06, "stoneDark")

    # Benches along the left-front edge, and jars by the stage.
    for x in (-0.65, 0.05):
        B.box("bench", x, -1.22, Z, 0.42, 0.1, 0.07, "woodDark")
        for kx in (-1, 1):
            B.box("bench_leg", x + kx * 0.16, -1.22, Z, 0.03, 0.08, 0.06, "woodDark")
    B.jar(-1.18, 0.02, Z, 0.05, 0.11)
    B.jar(-0.98, 0.08, Z, 0.04, 0.09)

    if not festive:
        return 3

    # Bunting from the pole's top to the side lantern posts and the stage's front corner, and
    # garlands along the front edges from post to post.
    hub = (px, py, top - 0.08)
    for x, y in (posts[0], posts[2]):
        _bunting(B, "bunting", hub, (x, y, Z + 0.44), sag=0.1, flags=6, lanterns=(0.55,))
    _bunting(B, "bunting", hub, (sx + p, sy - p, zf + post_h + 0.02), sag=0.08, flags=4)
    for a, b in ((posts[0], posts[1]), (posts[1], posts[2])):
        _bunting(B, "garland", (a[0], a[1], Z + 0.44), (b[0], b[1], Z + 0.44), sag=0.12, flags=9, lanterns=(0.25, 0.75))
    # Lanterns under the stage's eaves.
    e = sw / 2 + 0.06
    for kx, ky in ((-1, -1), (1, -1), (1, 1)):
        _lantern(B, sx + kx * e, sy + ky * e, zf + post_h - 0.06)
    # Banners on the stage's front posts.
    for kx, ky, colour in ((1, -1, "flagYellow"), (-1, -1, "flagRed"), (1, 1, "flagRed")):
        B.box("banner", sx + kx * (p + 0.035), sy + ky * (p + 0.035), zf + 0.06, 0.012 if kx > 0 else 0.07,
              0.07 if kx > 0 else 0.012, 0.3, colour)
    # An offering table before the stage's steps: fruit and incense.
    B.box("offering_table", 0.02, 0.62, Z, 0.24, 0.32, 0.11, "drum")
    for k in range(3):
        bpy.ops.mesh.primitive_uv_sphere_add(radius=0.03, location=(0.02, 0.52 + k * 0.09, Z + 0.14), segments=8,
                                             ring_count=6)
        B._add(bpy.context.active_object, "fruit")
    # Two food stalls on the open front edges, and mats on the square.
    _stall(B, 1.05, 0.35, False, "flagRed")
    _stall(B, -0.35, -0.98, True, "flagBlue")
    _mat(B, 0.85, -0.6, 0.42, 0.3)
    _mat(B, -0.5, -0.35, 0.34, 0.44)
    return 3
