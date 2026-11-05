"""Workshops for the game, rendered with the engine camera."""

import bpy


def build_roof_tile_kiln(B, **_):
    """2x2 workshop: a brick bottle kiln with its chimney, a long open drying shed
    with racks of tiles along the back, and a yard of finished tile stacks and jars.
    Rendered with the engine camera: the footprint is [-1, 1] x [-1, 1] at z = 0."""
    Z = 0.05                                                  # top of the yard slab
    B.box("yard", 0, 0, 0, 2.0, 2.0, Z, "yard")
    # Kiln on the left cell: round brick base, dome, firing mouth facing -y, chimney behind.
    kx, ky = -0.45, -0.3
    B.cylinder("kiln_base", kx, ky, Z, 0.34, 0.24, "brick", verts=20)
    B.dome("kiln_dome", kx, ky, Z + 0.24, 0.34, 0.3, "brick")
    B.box("kiln_mouth", kx + 0.02, ky - 0.33, Z, 0.2, 0.05, 0.18, "archDark")
    B.box("kiln_fire", kx + 0.02, ky - 0.355, Z + 0.02, 0.12, 0.03, 0.08, "fire")
    B.box("kiln_lintel", kx + 0.02, ky - 0.34, Z + 0.18, 0.26, 0.07, 0.04, "brick")
    cx_, cy_ = kx - 0.16, ky + 0.17
    B.box("chimney", cx_, cy_, Z + 0.24, 0.14, 0.14, 0.82, "brick")
    B.box("chimney_cap", cx_, cy_, Z + 1.06, 0.18, 0.18, 0.05, "brick")
    B.box("chimney_hole", cx_, cy_, Z + 1.105, 0.09, 0.09, 0.005, "archDark")
    for lx, ly, lz in ((0.0, -0.86, 0.0), (0.0, -0.78, 0.0), (0.0, -0.82, 0.065)):   # firewood beside the mouth
        B.cylinder("log", -0.08 + lx, ly, Z + lz, 0.035, 0.3, "trunk", verts=8, axis='X')
    # Drying shed along the back-right edge (y = +1): posts, back wall, racks, gable roof.
    sx, sy, sw, sd, sh = 0.05, 0.6, 1.6, 0.6, 0.46
    for px in (-sw / 2 + 0.05, 0.0, sw / 2 - 0.05):
        for py in (sy - sd / 2 + 0.04, sy + sd / 2 - 0.04):
            B.box("post", sx + px, py, Z, 0.06, 0.06, sh, "woodDark")
    B.box("shed_wall", sx, sy + sd / 2 - 0.03, Z, sw - 0.04, 0.04, sh, "plaster")
    B.box("shed_floor", sx, sy, Z, sw - 0.02, sd - 0.02, 0.02, "woodDark")
    for lvl, zz in enumerate((0.12, 0.27)):
        B.box("rack", sx, sy + 0.05, Z + zz, sw - 0.16, 0.3, 0.025, "woodDark")
        for i in range(9):
            B.box("drying_tile", sx - sw / 2 + 0.17 + i * 0.158, sy + 0.05, Z + zz + 0.025, 0.11, 0.22, 0.03, "jar")
    B.gable_roof("shed_roof", sx, sy, Z + sh, sw, sd, 0.28, axis='X', overhang=0.1, m="roofSmallX")
    # Yard: two rows of finished tile stacks, jars by the front corner, a clay heap.
    hs = (0.16, 0.12, 0.19, 0.14, 0.1, 0.18, 0.13, 0.17, 0.15, 0.11, 0.16, 0.12)
    for j, yy in enumerate((-0.05, -0.27, -0.49)):
        for i, xx in enumerate((0.22, 0.41, 0.6, 0.79)):
            if (i, j) == (3, 2): continue                       # room for the jars
            B.tile_stack(xx, yy, Z, 0.13, 0.11, hs[j * 4 + i])
    for x, y, r, h in ((0.72, -0.78, 0.09, 0.2), (0.52, -0.84, 0.075, 0.16), (0.86, -0.6, 0.07, 0.15), (-0.86, 0.2, 0.08, 0.18)):
        B.jar(x, y, Z, r, h)
    bpy.ops.mesh.primitive_ico_sphere_add(radius=1.0, subdivisions=2, location=(0.72, 0.18, Z))
    o = bpy.context.active_object; o.scale = (0.2, 0.16, 0.12); B._add(o, "clay"); B.smooth(o)
    for dx, dy, dz, r in ((0, 0, 0.07, 0.08), (0.07, 0.05, 0.05, 0.06), (-0.05, 0.06, 0.04, 0.055)):   # a bush by the left corner
        bpy.ops.mesh.primitive_ico_sphere_add(radius=r, subdivisions=2, location=(-0.8 + dx, -0.82 + dy, Z + dz))
        B.smooth(B._add(bpy.context.active_object, "leaf"))
    return 2
