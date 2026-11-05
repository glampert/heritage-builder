# hb_tile_gen.py — Heritage Builder isometric tile generator for Blender 5.x
#
# Builds a building from a recipe, applies a palette, sets up the 2:1 isometric
# camera, lighting, Freestyle outlines and compositor grade, and renders a
# transparent PNG sized to the game grid (64x32 px per cell, times --scale).
#
# Headless:
#   blender -b -P hb_tile_gen.py -- --building fortress --cells 15 --scale 1 \
#       --palette viet --out ~/Desktop/fortress_tile.png [--blend out.blend]
#
# Inside Blender (Scripting tab or the MCP execute tool):
#   exec(open('/path/hb_tile_gen.py').read()); generate(building='fortress', out='/tmp/x.png')
#
# Design notes (see README_blender_tile_pipeline.md):
# - Colours in palettes are sRGB hex. They are converted to linear with lin()
#   before being fed to shader nodes. Feeding hex-as-linear produces the pastel
#   look from the early tests; never skip lin().
# - 1 world unit == 1 grid cell. The ground footprint is `cells` units wide, so
#   the rendered diamond is exactly cells*64*scale px wide. Height is free.
# - Camera: ortho, rotation (60, 0, 45) deg, placed on +X/-Y so the front faces
#   are +X and -Y. Screen left corner = (-x,-y), right = (+x,+y), back = (-x,+y).

import bpy, bmesh, math, os, sys
from math import radians, sqrt

# ----------------------------------------------------------------------------
# Palettes (sRGB hex). Add one per civilisation.
# ----------------------------------------------------------------------------
PALETTES = {
    "viet": {
        "roof":      ("3e170c", "82402a", "b8683e"),   # shadow / mid / highlight
        "ridge":     "2a1a10",
        "stone":     ("a49e8a", "7c766a", "3e3a32"),   # block A / block B / mortar
        "stoneDark": ("7e7865", "5c5748", "32302a"),
        "pave":      ("b2a88c", "9a9075", "5a5343"),
        "wood":      ("4a2a16", "8a5630"),
        "woodDark":  ("26140a", "4e2e18"),
        "grass":     ("4a6e26", "7c9e4a"),
        "leaf":      ("3a6a2a", "7eaa56"),
        "trunk":     ("3a2415", "5a3a22"),
        "earth":     ("36241a", "62452c"),
        "water":     ("142c48", "346890"),
        "lantern":   "d83c1c",
        "gold":      "d4902e",
        "archDark":  "120c08",
        # Workshop and yard materials (first used by the roof-tile kiln).
        "brick":      ("a85a3a", "8a4630", "4e3226"),   # fired brick A / B / mortar
        "terracotta": ("5a2412", "a4502e", "d08050"),   # roof tiles: shadow / mid / highlight
        "yard":       ("a87e4e", "cfa26a"),             # packed sandy earth
        "clay":       ("5e5246", "8a7a66"),             # raw clay
        "fire":       "ff8c2a",
        "plaster":    ("b88a44", "d8b064"),             # ochre lime plaster
        "roofGrey":   ("2a221c", "5a4e44", "8c7e6c"),   # weathered grey-brown tiles, as on the hand-made workshops
    },
    # Bagan-inspired: red brick, teak, sandstone paving, gilded central shikhara.
    "burma": {
        "roof":      ("3a1a0e", "6e3420", "9a5030"),
        "ridge":     "26130a",
        "stone":     ("9a4a34", "7a3626", "4a2016"),   # red brick A / B / mortar
        "stoneDark": ("7a3a2a", "5e2c1e", "3a1810"),
        "pave":      ("c4ae86", "a8936c", "7a6a4a"),   # sandstone flags
        "wood":      ("4a2a16", "8a5630"),
        "woodDark":  ("26140a", "4e2e18"),
        "grass":     ("4a6e26", "7c9e4a"),
        "leaf":      ("3a6a2a", "7eaa56"),
        "trunk":     ("3a2415", "5a3a22"),
        "earth":     ("36241a", "62452c"),
        "water":     ("142c48", "346890"),
        "lantern":   "d83c1c",
        "gold":      "e0a63a",
        "goldStone": ("c99a2e", "a87c22", "6a4c14"),   # gilded carved masonry
        "sandstone": ("a8926a", "d2bc92"),
        "archDark":  "120c08",
    },
    # Angkor-inspired: grey laterite/sandstone with moss, red-tiled galleries.
    "khmer": {
        "roof":      ("3e1e12", "7a3a24", "985232"),
        "ridge":     "2a1a10",
        "stone":     ("aaa697", "8c8878", "5a574c"),
        "stoneDark": ("807d70", "64625a", "3e3d38"),
        "pave":      ("aca898", "928e7e", "605d54"),
        "wood":      ("4a2a16", "8a5630"),
        "woodDark":  ("26140a", "4e2e18"),
        "grass":     ("4a6e26", "7c9e4a"),
        "leaf":      ("3a6a2a", "7eaa56"),
        "trunk":     ("3a2415", "5a3a22"),
        "earth":     ("36241a", "62452c"),
        "water":     ("142c48", "346890"),
        "lantern":   "d83c1c",
        "gold":      "d4902e",
        "moss":      ("3e6020", "6a8c36"),             # overlay on all stone materials
        "archDark":  "120c08",
    },
}

LIGHTING = dict(
    sun_energy=5.4, sun_color=(1.0, 0.93, 0.82), sun_rot=(50, -10, 210), sun_angle=2,
    fill_energy=0.8, fill_color=(0.9, 0.93, 1.0), fill_rot=(60, 0, 40),
    world_color=(0.82, 0.80, 0.74), world_strength=0.37,
)
OUTLINE = dict(thickness=1.6, color=(0.09, 0.05, 0.03), alpha=0.9, noise_amp=0.6, noise_period=8, jitter_amp=0.7, jitter_freq=12)
GRADE = dict(saturation=1.0, contrast=0.08)
EEVEE = dict(samples=96, fast_gi_distance=1.5)


def lin(h):
    r, g, b = [int(h[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    f = lambda c: c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4
    return (f(r), f(g), f(b), 1.0)


# ----------------------------------------------------------------------------
# Materials
# ----------------------------------------------------------------------------
class Materials:
    def __init__(self, palette):
        self.p = palette
        self.m = {}
        p = palette
        moss = p.get("moss")
        self.brick("stone", *p["stone"], scale=3.0, moss=moss, moss_amount=0.75)
        self.brick("stoneDark", *p["stoneDark"], scale=3.0, moss=moss, moss_amount=0.75)
        self.brick("pave", *p["pave"], scale=1.6, mortar_size=0.03, bump_s=0.3, use_z=False, moss=moss, moss_amount=0.5)
        if "goldStone" in p: self.brick("goldStone", *p["goldStone"], scale=5.0, mortar_size=0.03, rough=0.55, bump_s=0.6)
        if "sandstone" in p: self.noise("sandstone", *p["sandstone"], scale=7, rough=0.9, bump_s=0.25)
        self.wood("wood", *p["wood"])
        self.wood("woodDark", *p["woodDark"], scale=8)
        self.noise("grass", *p["grass"], scale=6)
        self.noise("leaf", *p["leaf"], scale=5, bump_s=0.6)
        self.noise("trunk", *p["trunk"], scale=10)
        self.noise("earth", *p["earth"], scale=5)
        self.roof("roof", *p["roof"])
        self.water("water", *p["water"])
        for n in ("ridge", "lantern", "gold", "archDark"):
            self.flat(n, p[n], rough=0.4 if n in ("lantern", "gold") else 0.9)
        # Workshop and yard materials. Palettes that lack a key borrow the viet one.
        g = lambda k: p.get(k, PALETTES["viet"][k])
        self.brick("brick", *g("brick"), scale=11.0, mortar_size=0.06)
        self.roof("tiles", *g("terracotta"), rows_per_unit=30)          # stacks of loose roof tiles
        self.roof("roofSmallX", *g("roofGrey"), rows_per_unit=26, cols_axis='X', cols_per_unit=22)   # small gable roofs, ridge along X
        self.roof("roofSmallY", *g("roofGrey"), rows_per_unit=26, cols_axis='Y', cols_per_unit=22)   # small gable roofs, ridge along Y
        self.noise("plaster", *g("plaster"), scale=5, bump_s=0.15)
        self.noise("jar", g("terracotta")[0], g("terracotta")[1], scale=8, rough=0.6, bump_s=0.15)
        self.noise("yard", *g("yard"), scale=9, bump_s=0.2)
        self.noise("clay", *g("clay"), scale=6, bump_s=0.5)
        self.flat("fire", g("fire"), rough=0.5, emission=6.0)
        self.noise("tileTop", g("terracotta")[1], g("terracotta")[2], scale=30, bump_s=0.4)     # top of a tile stack

    def __getitem__(self, k): return self.m[k]

    def _new(self, name):
        m = bpy.data.materials.new(name); m.use_nodes = True
        nt = m.node_tree
        for n in list(nt.nodes): nt.nodes.remove(n)
        out = nt.nodes.new("ShaderNodeOutputMaterial"); bsdf = nt.nodes.new("ShaderNodeBsdfPrincipled")
        nt.links.new(bsdf.outputs[0], out.inputs[0])
        if "Specular IOR Level" in bsdf.inputs: bsdf.inputs["Specular IOR Level"].default_value = 0.2
        tc = nt.nodes.new("ShaderNodeTexCoord")
        self.m[name] = m
        return nt, bsdf, tc

    @staticmethod
    def _bump(nt, bsdf, src, strength, distance):
        b = nt.nodes.new("ShaderNodeBump"); b.inputs["Strength"].default_value = strength; b.inputs["Distance"].default_value = distance
        nt.links.new(src, b.inputs["Height"]); nt.links.new(b.outputs[0], bsdf.inputs["Normal"])

    @staticmethod
    def _ramp(nt, src, stops):
        r = nt.nodes.new("ShaderNodeValToRGB"); el = r.color_ramp.elements
        while len(el) > 1: el.remove(el[-1])
        el[0].position, el[0].color = stops[0]
        for pos, col in stops[1:]:
            e = el.new(pos); e.color = col
        nt.links.new(src, r.inputs[0]); return r

    def flat(self, name, hexc, rough=0.9, emission=0.0):
        nt, bsdf, tc = self._new(name)
        bsdf.inputs["Base Color"].default_value = lin(hexc); bsdf.inputs["Roughness"].default_value = rough
        if emission > 0.0:
            bsdf.inputs["Emission Color"].default_value = lin(hexc); bsdf.inputs["Emission Strength"].default_value = emission

    def brick(self, name, c1, c2, mortar, scale, mortar_size=0.04, rough=0.9, bump_s=0.5, use_z=True, moss=None, moss_amount=1.0):
        nt, bsdf, tc = self._new(name)
        mp = nt.nodes.new("ShaderNodeMapping")
        if use_z:   # project onto vertical faces using (x+y, z)
            sep = nt.nodes.new("ShaderNodeSeparateXYZ"); nt.links.new(tc.outputs["Object"], sep.inputs[0])
            add = nt.nodes.new("ShaderNodeMath"); add.operation = 'ADD'
            nt.links.new(sep.outputs[0], add.inputs[0]); nt.links.new(sep.outputs[1], add.inputs[1])
            comb = nt.nodes.new("ShaderNodeCombineXYZ"); nt.links.new(add.outputs[0], comb.inputs[0]); nt.links.new(sep.outputs[2], comb.inputs[1])
            nt.links.new(comb.outputs[0], mp.inputs[0])
        else:
            nt.links.new(tc.outputs["Object"], mp.inputs[0])
        br = nt.nodes.new("ShaderNodeTexBrick"); br.inputs["Scale"].default_value = scale; br.inputs["Mortar Size"].default_value = mortar_size
        br.inputs["Color1"].default_value = lin(c1); br.inputs["Color2"].default_value = lin(c2); br.inputs["Mortar"].default_value = lin(mortar)
        br.inputs["Row Height"].default_value = 0.5; br.inputs["Brick Width"].default_value = 0.5
        nt.links.new(mp.outputs[0], br.inputs["Vector"])
        nz = nt.nodes.new("ShaderNodeTexNoise"); nz.inputs["Scale"].default_value = scale * 3; nz.inputs["Detail"].default_value = 3
        nt.links.new(mp.outputs[0], nz.inputs["Vector"])
        mix = nt.nodes.new("ShaderNodeMix"); mix.data_type = 'RGBA'; mix.blend_type = 'MULTIPLY'; mix.inputs["Factor"].default_value = 0.35
        nt.links.new(br.outputs["Color"], mix.inputs[6]); nt.links.new(nz.outputs["Color"], mix.inputs[7])
        color_out = mix.outputs[2]
        if moss:
            # Moss overlay: a second, larger noise gates a moss colour ramp onto the masonry.
            mn = nt.nodes.new("ShaderNodeTexNoise"); mn.inputs["Scale"].default_value = 2.2; mn.inputs["Detail"].default_value = 4; mn.inputs["Roughness"].default_value = 0.7
            nt.links.new(tc.outputs["Object"], mn.inputs["Vector"])
            gate = self._ramp(nt, mn.outputs["Fac"], [(0.50, (0, 0, 0, 1)), (0.64, (1, 1, 1, 1))])
            mcol = self._ramp(nt, nz.outputs["Fac"], [(0.3, lin(moss[0])), (0.7, lin(moss[1]))])
            amt = nt.nodes.new("ShaderNodeMath"); amt.operation = 'MULTIPLY'; amt.inputs[1].default_value = moss_amount; nt.links.new(gate.outputs[0], amt.inputs[0])
            mm = nt.nodes.new("ShaderNodeMix"); mm.data_type = 'RGBA'; mm.blend_type = 'MIX'
            nt.links.new(amt.outputs[0], mm.inputs["Factor"]); nt.links.new(color_out, mm.inputs[6]); nt.links.new(mcol.outputs[0], mm.inputs[7])
            color_out = mm.outputs[2]
        nt.links.new(color_out, bsdf.inputs["Base Color"]); bsdf.inputs["Roughness"].default_value = rough
        inv = nt.nodes.new("ShaderNodeInvert"); nt.links.new(br.outputs["Fac"], inv.inputs[1])
        mb = nt.nodes.new("ShaderNodeMix"); mb.data_type = 'FLOAT'; mb.inputs["Factor"].default_value = 0.3
        nt.links.new(inv.outputs[0], mb.inputs[2]); nt.links.new(nz.outputs["Fac"], mb.inputs[3])
        self._bump(nt, bsdf, mb.outputs[0], bump_s, 0.04)

    def wood(self, name, c1, c2, scale=6.0):
        nt, bsdf, tc = self._new(name)
        wv = nt.nodes.new("ShaderNodeTexWave"); wv.wave_type = 'BANDS'; wv.bands_direction = 'Z'
        wv.inputs["Scale"].default_value = scale; wv.inputs["Distortion"].default_value = 4; wv.inputs["Detail"].default_value = 3; wv.inputs["Detail Scale"].default_value = 2
        nt.links.new(tc.outputs["Object"], wv.inputs["Vector"])
        r = self._ramp(nt, wv.outputs["Fac"], [(0.0, lin(c1)), (1.0, lin(c2))])
        nt.links.new(r.outputs[0], bsdf.inputs["Base Color"]); bsdf.inputs["Roughness"].default_value = 0.75
        self._bump(nt, bsdf, wv.outputs["Fac"], 0.2, 0.02)

    def noise(self, name, c1, c2, scale=4.0, rough=0.95, bump_s=0.3):
        nt, bsdf, tc = self._new(name)
        nz = nt.nodes.new("ShaderNodeTexNoise"); nz.inputs["Scale"].default_value = scale; nz.inputs["Detail"].default_value = 4; nz.inputs["Roughness"].default_value = 0.6
        nt.links.new(tc.outputs["Object"], nz.inputs["Vector"])
        r = self._ramp(nt, nz.outputs["Fac"], [(0.3, lin(c1)), (0.7, lin(c2))])
        nt.links.new(r.outputs[0], bsdf.inputs["Base Color"]); bsdf.inputs["Roughness"].default_value = rough
        self._bump(nt, bsdf, nz.outputs["Fac"], bump_s, 0.05)

    def roof(self, name, c_shadow, c_mid, c_hi, rows_per_unit=10.0, cols_axis=None, cols_per_unit=None):
        # Rows of curved tiles along the slope (z-based). A column pattern on x+y
        # streaks diagonally across the slopes that face other ways, so columns are
        # only for gable roofs, keyed on the ridge's axis (cols_axis 'X' or 'Y').
        nt, bsdf, tc = self._new(name)
        sep = nt.nodes.new("ShaderNodeSeparateXYZ"); nt.links.new(tc.outputs["Object"], sep.inputs[0])
        rows = nt.nodes.new("ShaderNodeMath"); rows.operation = 'MULTIPLY'; rows.inputs[1].default_value = rows_per_unit; nt.links.new(sep.outputs[2], rows.inputs[0])
        fr = nt.nodes.new("ShaderNodeMath"); fr.operation = 'FRACT'; nt.links.new(rows.outputs[0], fr.inputs[0])
        prof = nt.nodes.new("ShaderNodeMath"); prof.operation = 'POWER'; prof.inputs[1].default_value = 0.5; nt.links.new(fr.outputs[0], prof.inputs[0])
        if cols_axis:
            # Each column of tiles is a half-round channel: darken towards its edges.
            cols = nt.nodes.new("ShaderNodeMath"); cols.operation = 'MULTIPLY'; cols.inputs[1].default_value = cols_per_unit or rows_per_unit * 0.8
            nt.links.new(sep.outputs[0 if cols_axis == 'X' else 1], cols.inputs[0])
            cf = nt.nodes.new("ShaderNodeMath"); cf.operation = 'PINGPONG'; cf.inputs[1].default_value = 0.5; nt.links.new(cols.outputs[0], cf.inputs[0])
            cs = nt.nodes.new("ShaderNodeMath"); cs.operation = 'MULTIPLY'; cs.inputs[1].default_value = 2.0; nt.links.new(cf.outputs[0], cs.inputs[0])
            cp = nt.nodes.new("ShaderNodeMath"); cp.operation = 'POWER'; cp.inputs[1].default_value = 0.35; nt.links.new(cs.outputs[0], cp.inputs[0])
            both = nt.nodes.new("ShaderNodeMath"); both.operation = 'MULTIPLY'; nt.links.new(prof.outputs[0], both.inputs[0]); nt.links.new(cp.outputs[0], both.inputs[1])
            prof = both
        nz = nt.nodes.new("ShaderNodeTexNoise"); nz.inputs["Scale"].default_value = 5; nz.inputs["Detail"].default_value = 2; nt.links.new(tc.outputs["Object"], nz.inputs["Vector"])
        r = self._ramp(nt, prof.outputs[0], [(0.0, lin(c_shadow)), (0.35, lin(c_mid)), (1.0, lin(c_hi))])
        mix = nt.nodes.new("ShaderNodeMix"); mix.data_type = 'RGBA'; mix.blend_type = 'MULTIPLY'; mix.inputs["Factor"].default_value = 0.25
        nt.links.new(r.outputs[0], mix.inputs[6]); nt.links.new(nz.outputs["Color"], mix.inputs[7]); nt.links.new(mix.outputs[2], bsdf.inputs["Base Color"])
        bsdf.inputs["Roughness"].default_value = 0.95
        if "Specular IOR Level" in bsdf.inputs: bsdf.inputs["Specular IOR Level"].default_value = 0.1
        self._bump(nt, bsdf, prof.outputs[0], 0.35, 0.03)

    def water(self, name, c1, c2):
        nt, bsdf, tc = self._new(name)
        nz = nt.nodes.new("ShaderNodeTexNoise"); nz.inputs["Scale"].default_value = 6; nz.inputs["Detail"].default_value = 5; nz.inputs["Distortion"].default_value = 1.2
        nt.links.new(tc.outputs["Object"], nz.inputs["Vector"])
        r = self._ramp(nt, nz.outputs["Fac"], [(0.35, lin(c1)), (0.65, lin(c2))])
        nt.links.new(r.outputs[0], bsdf.inputs["Base Color"]); bsdf.inputs["Roughness"].default_value = 0.15
        if "Specular IOR Level" in bsdf.inputs: bsdf.inputs["Specular IOR Level"].default_value = 0.5
        self._bump(nt, bsdf, nz.outputs["Fac"], 0.25, 0.02)


# ----------------------------------------------------------------------------
# Geometry primitives
# ----------------------------------------------------------------------------
class Builder:
    def __init__(self, mats, collection_name="Tile"):
        self.M = mats
        self.col = bpy.data.collections.new(collection_name)
        bpy.context.scene.collection.children.link(self.col)

    def _add(self, obj, m):
        for c in obj.users_collection: c.objects.unlink(obj)
        self.col.objects.link(obj); obj.data.materials.append(self.M[m]); return obj

    def box(self, name, cx, cy, z0, w, d, h, m):
        bpy.ops.mesh.primitive_cube_add(location=(cx, cy, z0 + h / 2))
        o = bpy.context.active_object; o.name = name; o.scale = (w / 2, d / 2, h / 2)
        return self._add(o, m)

    def frustum(self, name, cx, cy, z0, bw, bd, tw, td, h, m):
        me = bpy.data.meshes.new(name); bm = bmesh.new()
        C = ((-1, -1), (1, -1), (1, 1), (-1, 1))
        b = [bm.verts.new((cx + sx * bw / 2, cy + sy * bd / 2, z0)) for sx, sy in C]
        t = [bm.verts.new((cx + sx * tw / 2, cy + sy * td / 2, z0 + h)) for sx, sy in C]
        bm.faces.new(b[::-1]); bm.faces.new(t)
        for i in range(4): bm.faces.new((b[i], b[(i + 1) % 4], t[(i + 1) % 4], t[i]))
        bm.to_mesh(me); bm.free()
        o = bpy.data.objects.new(name, me); bpy.context.scene.collection.objects.link(o)
        return self._add(o, m)

    def pagoda_roof(self, name, cx, cy, z0, w, d, h, top_w=0.0, top_d=0.0, eave=0.25):
        tw, td = max(top_w, 0.05), max(top_d, 0.05)
        self.frustum(name + "_eave", cx, cy, z0 - 0.08, w + eave * 2, d + eave * 2, w, d, 0.08, "roof")
        self.frustum(name, cx, cy, z0, w, d, tw, td, h, "roof")
        self.frustum(name + "_ridge", cx, cy, z0 + h, tw + 0.12, td + 0.12, tw + 0.12, td + 0.12, 0.08, "ridge")
        for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
            self.box(name + "_tip", cx + sx * (w / 2 + eave), cy + sy * (d / 2 + eave), z0 - 0.08, 0.16, 0.16, 0.22, "ridge")

    def lantern(self, x, y, z):
        bpy.ops.mesh.primitive_uv_sphere_add(radius=0.14, location=(x, y, z), segments=12, ring_count=8)
        o = bpy.context.active_object; o.scale = (1, 1, 1.25); self._add(o, "lantern"); self.smooth(o)
        self.box("lstring", x, y, z + 0.15, 0.02, 0.02, 0.25, "ridge")

    def tree(self, x, y, z0=0.3, s=1.0):
        bpy.ops.mesh.primitive_cylinder_add(radius=0.12 * s, depth=0.9 * s, location=(x, y, z0 + 0.45 * s), vertices=8)
        self._add(bpy.context.active_object, "trunk")
        for dx, dy, dz, r in ((0, 0, 1.25, 0.62), (0.3, 0.1, 1.05, 0.42), (-0.3, -0.15, 1.1, 0.4), (0.05, -0.3, 1.5, 0.38), (-0.1, 0.3, 1.45, 0.36)):
            bpy.ops.mesh.primitive_ico_sphere_add(radius=r * s, subdivisions=2, location=(x + dx * s, y + dy * s, z0 + dz * s))
            o = self._add(bpy.context.active_object, "leaf"); self.smooth(o)

    @staticmethod
    def smooth(o):
        for p in o.data.polygons: p.use_smooth = True

    def cone(self, name, cx, cy, z0, r1, r2, h, m, verts=16):
        bpy.ops.mesh.primitive_cone_add(vertices=verts, radius1=r1, radius2=r2, depth=h, location=(cx, cy, z0 + h / 2))
        o = bpy.context.active_object; o.name = name; o.rotation_euler = (0, 0, math.pi / verts)
        return self._add(o, m)

    def prang(self, name, cx, cy, z0, w, h, m, tiers=7, cap=True):
        """Khmer lotus-bud tower: stacked square frustums whose width follows a
        convex curve, each tier with a small overhanging cornice, topped by a bud."""
        z = z0; wprev = w
        for i in range(tiers):
            t0, t1 = i / tiers, (i + 1) / tiers
            w0 = w * (1 - t0 ** 1.6) ** 0.5; w1 = w * (1 - t1 ** 1.6) ** 0.5
            th = h / tiers
            self.frustum(f"{name}_t{i}", cx, cy, z, w0, w0, max(w1, 0.08), max(w1, 0.08), th * 0.78, m)
            self.frustum(f"{name}_c{i}", cx, cy, z + th * 0.78, max(w1, 0.08) * 1.12, max(w1, 0.08) * 1.12, max(w1, 0.08) * 1.12, max(w1, 0.08) * 1.12, th * 0.22, m)
            z += th
        if cap:
            self.cone(name + "_bud", cx, cy, z, w * 0.1, 0.0, h * 0.12, m, verts=8)

    def shikhara(self, name, cx, cy, z0, w, h, m, finial_m, tiers=6):
        """Bagan-style tower: a square base block, a tapering curved spire of
        stacked frustums with carved bands, and a tall pointed finial."""
        z = z0
        self.box(name + "_base", cx, cy, z, w, w, h * 0.30, m); z += h * 0.30
        wb = w * 0.9
        for i in range(tiers):
            t0, t1 = i / tiers, (i + 1) / tiers
            w0 = wb * (1 - t0 ** 1.4) ** 0.6; w1 = wb * (1 - t1 ** 1.4) ** 0.6
            th = h * 0.5 / tiers
            self.frustum(f"{name}_s{i}", cx, cy, z, w0, w0, max(w1, 0.1), max(w1, 0.1), th * 0.8, m)
            self.frustum(f"{name}_b{i}", cx, cy, z + th * 0.8, max(w1, 0.1) * 1.08, max(w1, 0.1) * 1.08, max(w1, 0.1) * 1.08, max(w1, 0.1) * 1.08, th * 0.2, m)
            z += th
        self.cone(name + "_spire", cx, cy, z, w * 0.12, 0.0, h * 0.2, finial_m, verts=12)

    def ground_diamond(self, G, ring=0.9, water=True, P=None):
        # Outer grass ring, moat, raised stone island. Returns island half-width.
        self.box("earth", 0, 0, -0.45, G, G, 0.25, "earth")
        if water:
            self.box("grass_n", 0, G / 2 - ring / 2, -0.1, G, ring, 0.1, "grass"); self.box("grass_s", 0, -G / 2 + ring / 2, -0.1, G, ring, 0.1, "grass")
            self.box("grass_e", G / 2 - ring / 2, 0, -0.1, ring, G - 2 * ring, 0.1, "grass"); self.box("grass_w", -G / 2 + ring / 2, 0, -0.1, ring, G - 2 * ring, 0.1, "grass")
            self.box("water", 0, 0, -0.35, G - 2 * ring, G - 2 * ring, 0.3, "water")
        else:
            self.box("grass", 0, 0, -0.1, G, G, 0.1, "grass")
        return P

    # -- Workshop and yard primitives (engine-anchored tiles) ------------------
    def cylinder(self, name, cx, cy, z0, r, h, m, verts=16, axis='Z', r2=None):
        """Upright cylinder (or a tapered one with r2), or one lying along X or Y."""
        bpy.ops.mesh.primitive_cone_add(vertices=verts, radius1=r, radius2=r if r2 is None else r2, depth=h, location=(0, 0, 0))
        o = bpy.context.active_object; o.name = name
        if axis == 'Z':   o.location = (cx, cy, z0 + h / 2)
        elif axis == 'X': o.rotation_euler = (0, radians(90), 0); o.location = (cx, cy, z0 + r)
        else:             o.rotation_euler = (radians(90), 0, 0); o.location = (cx, cy, z0 + r)
        self._add(o, m); self.smooth(o)
        return o

    def dome(self, name, cx, cy, z0, r, h, m, segments=20, rings=10):
        """Upper half of a sphere of radius r, squashed or stretched to height h."""
        me = bpy.data.meshes.new(name); bm = bmesh.new()
        bmesh.ops.create_uvsphere(bm, u_segments=segments, v_segments=rings * 2, radius=1.0)
        bmesh.ops.delete(bm, geom=[v for v in bm.verts if v.co.z < -1e-4], context='VERTS')
        for v in bm.verts: v.co = (cx + v.co.x * r, cy + v.co.y * r, z0 + max(v.co.z, 0.0) * h)
        bm.to_mesh(me); bm.free()
        o = bpy.data.objects.new(name, me); bpy.context.scene.collection.objects.link(o)
        self._add(o, m); self.smooth(o)
        return o

    def gable_roof(self, name, cx, cy, z0, w, d, h, axis='X', overhang=0.1, m="roof", tips=True):
        """Two-slope roof whose ridge runs along `axis`, over a w x d (x, y) plan,
        with a ridge cap and upturned tips at both ends of the ridge."""
        L, S = (w, d) if axis == 'X' else (d, w)              # ridge length, span across it
        L2, S2 = L / 2 + overhang, S / 2 + overhang
        pt = (lambda a, b, z: (cx + a, cy + b, z)) if axis == 'X' else (lambda a, b, z: (cx + b, cy + a, z))
        me = bpy.data.meshes.new(name); bm = bmesh.new()
        e = [bm.verts.new(pt(a, b, z0 - 0.06)) for a, b in ((-L2, -S2), (L2, -S2), (L2, S2), (-L2, S2))]
        r = [bm.verts.new(pt(a, 0, z0 + h)) for a in (-L2, L2)]
        bm.faces.new((e[0], e[1], r[1], r[0])); bm.faces.new((e[2], e[3], r[0], r[1]))
        bm.faces.new((e[3], e[0], r[0])); bm.faces.new((e[1], e[2], r[1]))
        bm.faces.new(e[::-1])
        bm.to_mesh(me); bm.free()
        o = bpy.data.objects.new(name, me); bpy.context.scene.collection.objects.link(o); self._add(o, m)
        cw = 0.07
        if axis == 'X': self.box(name + "_ridge", cx, cy, z0 + h - 0.02, 2 * L2, cw, 0.07, "ridge")
        else:           self.box(name + "_ridge", cx, cy, z0 + h - 0.02, cw, 2 * L2, 0.07, "ridge")
        if tips:
            for sgn in (-1, 1):
                x, y, _ = pt(sgn * L2, 0, 0)
                t = self.box(name + "_tip", x, y, z0 + h - 0.02, 0.08, 0.08, 0.16, "ridge")
                if axis == 'X': t.rotation_euler = (0, radians(-25 * sgn), 0)
                else:           t.rotation_euler = (radians(25 * sgn), 0, 0)
        return o

    def jar(self, x, y, z, r=0.09, h=0.2, m="jar"):
        bpy.ops.mesh.primitive_uv_sphere_add(radius=1.0, location=(x, y, z + h * 0.45), segments=14, ring_count=8)
        o = bpy.context.active_object; o.scale = (r, r, h * 0.5); self._add(o, m); self.smooth(o)
        self.cylinder("jar_neck", x, y, z + h * 0.8, r * 0.55, h * 0.2, m, verts=12)

    def tile_stack(self, x, y, z, w, d, h):
        """A stack of roof tiles on a low pallet: a block striped one row per tile."""
        self.box("pallet", x, y, z, w + 0.03, d + 0.03, 0.025, "woodDark")
        self.box("stack", x, y, z + 0.025, w, d, h, "tiles")
        self.box("stack_top", x, y, z + 0.025 + h, w, d, 0.006, "tileTop")


# ----------------------------------------------------------------------------
# Building recipes. Each takes a Builder and returns the ground size G (cells).
# ----------------------------------------------------------------------------
def enclosure(B, G=15.0, ring=0.9, P=11.0, W=8.4, gate_style="pagoda", tower_style="crenel", trees=True):
    """Shared moated enclosure: grass ring, moat, stone island, curtain walls,
    corner towers, two gates on the front faces, bridges, ledge grass, trees.
    Returns (G, P, W, z) so the recipe can place the central complex."""
    B.ground_diamond(G, ring)
    B.box("platform", 0, 0, -0.4, P, P, 0.7, "stoneDark"); B.box("courtyard", 0, 0, 0.3, P - 0.1, P - 0.1, 0.02, "pave")
    L = P / 2 - 0.1; lw = L - W / 2 - 0.3; lc = (L + W / 2 + 0.3) / 2
    B.box("ledge_n", 0, lc, 0.32, P - 0.2, lw, 0.02, "grass"); B.box("ledge_s", 0, -lc, 0.32, P - 0.2, lw, 0.02, "grass")
    B.box("ledge_e", lc, 0, 0.32, lw, P - 0.2, 0.02, "grass"); B.box("ledge_w", -lc, 0, 0.32, lw, P - 0.2, 0.02, "grass")
    wt, wh, z = 0.55, 1.3, 0.3
    for name, cx, cy, w, d in (("wall_n", 0, W / 2, W, wt), ("wall_s", 0, -W / 2, W, wt), ("wall_e", W / 2, 0, wt, W), ("wall_w", -W / 2, 0, wt, W)):
        B.box(name, cx, cy, z, w, d, wh, "stone")
        n = 14
        for i in range(n + 1):
            t = -W / 2 + i * (W / n)
            if abs(t) < 1.15: continue
            if tower_style == "lotus":   # Khmer: small lotus finials along the parapet
                if w > d: B.cone("cren", t, cy, z + wh, 0.14, 0.0, 0.3, "stone", verts=8)
                else:     B.cone("cren", cx, t, z + wh, 0.14, 0.0, 0.3, "stone", verts=8)
            else:
                if w > d: B.box("cren", t, cy, z + wh, 0.3, wt + 0.1, 0.25, "stone")
                else:     B.box("cren", cx, t, z + wh, wt + 0.1, 0.3, 0.25, "stone")
    for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
        tx, ty = sx * W / 2, sy * W / 2
        if tower_style == "lotus":
            B.box("tower", tx, ty, z, 1.2, 1.2, 1.5, "stone"); B.prang("tower_top", tx, ty, z + 1.5, 1.0, 1.6, "stone", tiers=5)
        elif tower_style == "stupa":
            B.box("tower", tx, ty, z, 1.2, 1.2, 1.5, "stone"); B.box("tower2", tx, ty, z + 1.5, 0.9, 0.9, 0.4, "stone")
            B.cone("tower_bell", tx, ty, z + 1.9, 0.42, 0.18, 0.7, "gold", verts=12); B.cone("tower_spire", tx, ty, z + 2.6, 0.18, 0.0, 0.6, "gold", verts=8)
        else:
            B.box("tower", tx, ty, z, 1.1, 1.1, 1.75, "stone")
            for ax, ay in ((-0.4, -0.4), (0.4, -0.4), (0.4, 0.4), (-0.4, 0.4), (0, -0.4), (0, 0.4), (-0.4, 0), (0.4, 0)):
                B.box("tcren", tx + ax, ty + ay, z + 1.75, 0.25, 0.25, 0.25, "stone")

    def gate(cx, cy, axis):
        w, d = (1.9, 1.3) if axis == 'y' else (1.3, 1.9)
        B.box("gate_base", cx, cy, z, w, d, 1.9, "stone")
        if axis == 'y': B.box("arch", cx, cy - d / 2 - 0.01, z, 0.8, 0.08, 1.2, "archDark")
        else:           B.box("arch", cx + w / 2 + 0.01, cy, z, 0.08, 0.8, 1.2, "archDark")
        if gate_style == "pagoda":
            for s in (-1, 1):
                if axis == 'y': B.box("orn", cx + s * 0.65, cy - d / 2 - 0.06, z + 0.9, 0.3, 0.06, 0.5, "gold")
                else:           B.box("orn", cx + w / 2 + 0.06, cy + s * 0.65, z + 0.9, 0.06, 0.3, 0.5, "gold")
            B.box("gate_top", cx, cy, z + 1.9, w - 0.4, d - 0.4, 0.7, "wood")
            B.pagoda_roof("gate_roof", cx, cy, z + 2.6, w, d, 0.55, 0.3, 0.3, 0.2)
            B.lantern(cx + (0 if axis == 'y' else w / 2 + 0.15), cy - (d / 2 + 0.15 if axis == 'y' else 0), z + 2.3)
        elif gate_style == "gopura":   # Khmer: a lotus-bud tower over the gate
            B.frustum("gate_cornice", cx, cy, z + 1.9, w + 0.2, d + 0.2, w, d, 0.15, "stone")
            B.prang("gate_tower", cx, cy, z + 2.05, min(w, d) * 0.95, 2.3, "stone", tiers=6)
        else:                           # Bagan: stepped pointed brick gatehouse
            B.frustum("gate_step1", cx, cy, z + 1.9, w, d, w * 0.8, d * 0.8, 0.45, "stone")
            B.frustum("gate_step2", cx, cy, z + 2.35, w * 0.8, d * 0.8, w * 0.5, d * 0.5, 0.45, "stone")
            B.cone("gate_spire", cx, cy, z + 2.8, min(w, d) * 0.22, 0.0, 0.9, "gold", verts=8)
    gate(0, -W / 2, 'y'); gate(W / 2, 0, 'x')
    mw = G / 2 - ring - P / 2
    B.box("bridge_s", 0, -(P / 2 + mw / 2), -0.13, 1.1, mw + 0.8, 0.4, "stone"); B.box("bridge_e", (P / 2 + mw / 2), 0, -0.13, mw + 0.8, 1.1, 0.4, "stone")
    for s in (-1, 1):
        B.box("rail", s * 0.5, -(P / 2 + mw / 2), 0.27, 0.1, mw + 0.8, 0.22, "stoneDark"); B.box("rail", (P / 2 + mw / 2), s * 0.5, 0.27, mw + 0.8, 0.1, 0.22, "stoneDark")
    B.box("path_s", 0, -(W / 2 + lw / 2 + 0.3), 0.33, 1.1, lw + 0.3, 0.02, "pave"); B.box("path_e", (W / 2 + lw / 2 + 0.3), 0, 0.33, lw + 0.3, 1.1, 0.02, "pave")
    if trees:
        for x, y, sc in ((-5.05, -5.05, 0.85), (5.05, 5.05, 0.85), (-5.05, 5.05, 0.8), (5.05, -5.05, 0.8), (1.3, -2.3, 0.7), (-2.6, 1.7, 0.65)):
            B.tree(x, y, 0.3, sc)
    return G, P, W, z


def halls(B, body_m="wood", positions=((-2.9, -2.9), (2.9, -2.9), (2.9, 2.9), (-2.9, 2.9)), shrines=((-3.0, 0), (0, 3.0))):
    for cx, cy in positions:
        B.box("hall", cx, cy, 0.3, 1.5, 1.1, 0.8, body_m); B.box("hall_dark", cx, cy, 0.3, 1.52, 1.12, 0.25, "woodDark")
        B.pagoda_roof("hall_roof", cx, cy, 1.1, 1.5, 1.1, 0.5, 0.5, 0.1, 0.18)
    for cx, cy in shrines:
        B.box("shrine", cx, cy, 0.3, 1.0, 1.0, 0.7, body_m); B.pagoda_roof("shrine_roof", cx, cy, 1.0, 1.0, 1.0, 0.5, 0, 0, 0.15)


def build_fortress(B):
    """Viet-style: three-tier timber pagoda tower inside a crenellated enclosure."""
    G, P, W, z = enclosure(B, gate_style="pagoda", tower_style="crenel")
    B.box("plinth", 0, 0, 0.3, 4.6, 4.6, 0.45, "stone"); B.box("plinth_top", 0, 0, 0.75, 4.4, 4.4, 0.03, "pave")
    B.box("stair_s", 0, -2.6, 0.3, 1.0, 0.7, 0.45, "stone"); B.box("stair_e", 2.6, 0, 0.3, 0.7, 1.0, 0.45, "stone")
    B.box("t1", 0, 0, 0.78, 3.3, 3.3, 1.5, "wood"); B.box("t1_dark", 0, 0, 0.78, 3.32, 3.32, 0.5, "woodDark")
    for i in range(4):
        for s in (-1, 1):
            x = s * (0.5 + i * 0.4) if i < 3 else s * 1.6
            B.box("col", x, -1.68, 0.78, 0.12, 0.1, 1.5, "woodDark"); B.box("col", 1.68, x, 0.78, 0.1, 0.12, 1.5, "woodDark")
    B.pagoda_roof("roof1", 0, 0, 2.3, 4.3, 4.3, 0.9, 2.5, 2.5, 0.3)
    B.box("balc2", 0, 0, 3.2, 2.9, 2.9, 0.12, "woodDark"); B.box("t2", 0, 0, 3.3, 2.3, 2.3, 1.3, "wood")
    for s in (-1, 1):
        B.box("win", s * 0.6, -1.17, 3.7, 0.35, 0.06, 0.5, "woodDark"); B.box("win", 1.17, s * 0.6, 3.7, 0.06, 0.35, 0.5, "woodDark")
    B.pagoda_roof("roof2", 0, 0, 4.6, 3.2, 3.2, 0.75, 1.7, 1.7, 0.3)
    B.box("balc3", 0, 0, 5.35, 2.0, 2.0, 0.1, "woodDark"); B.box("t3", 0, 0, 5.45, 1.55, 1.55, 1.1, "wood")
    B.box("win", 0, -0.8, 5.8, 0.4, 0.06, 0.5, "woodDark"); B.box("win", 0.8, 0, 5.8, 0.06, 0.4, 0.5, "woodDark")
    B.pagoda_roof("roof3", 0, 0, 6.55, 2.3, 2.3, 1.1, 0, 0, 0.3)
    B.box("finial", 0, 0, 7.65, 0.12, 0.12, 0.5, "gold")
    for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
        B.lantern(sx * 2.3, sy * 2.3, 2.0); B.lantern(sx * 1.7, sy * 1.7, 4.3)
    halls(B, "wood")
    return G


def build_bagan_fortress(B):
    """Burma-style: gilded shikhara on three stepped brick terraces with corner
    shrines, inside a red-brick enclosure with stupa-topped towers."""
    G, P, W, z = enclosure(B, gate_style="pointed", tower_style="stupa")
    # stepped terraces (brick, sandstone tops) with stairs on the two front faces
    for i, (w, h) in enumerate(((5.0, 0.45), (4.1, 0.45), (3.2, 0.45))):
        zt = z + i * 0.45
        B.box(f"terrace{i}", 0, 0, zt, w, w, h, "stone"); B.box(f"terrace{i}_top", 0, 0, zt + h, w - 0.1, w - 0.1, 0.03, "sandstone")
        B.box(f"stair_s{i}", 0, -(w / 2 + 0.3), zt, 0.9, 0.6, h, "sandstone"); B.box(f"stair_e{i}", (w / 2 + 0.3), 0, zt, 0.6, 0.9, h, "sandstone")
    ztop = z + 3 * 0.45
    # corner shrines on the first terrace
    for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
        cx, cy = sx * 2.05, sy * 2.05
        B.box("cshrine", cx, cy, z + 0.45, 0.7, 0.7, 0.8, "stone")
        B.frustum("cshrine_top", cx, cy, z + 1.25, 0.8, 0.8, 0.35, 0.35, 0.5, "stone")
        B.cone("cshrine_spire", cx, cy, z + 1.75, 0.16, 0.0, 0.55, "gold", verts=8)
    # central gilded shikhara with four small gilded corner spires at its base
    B.shikhara("shikhara", 0, 0, ztop, 2.4, 7.6, "goldStone", "gold", tiers=6)
    for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
        B.cone("sub_spire", sx * 1.25, sy * 1.25, ztop + 1.9, 0.22, 0.0, 0.9, "goldStone", verts=8)
    # porch niches on the front faces of the base block
    B.box("niche", 0, -1.21, ztop + 0.2, 0.6, 0.06, 1.1, "archDark"); B.box("niche", 1.21, 0, ztop + 0.2, 0.06, 0.6, 1.1, "archDark")
    halls(B, "wood")
    return G


def build_khmer_fortress(B):
    """Khmer-style: quincunx of lotus-bud prangs on a raised gallery platform,
    inside a moss-streaked grey stone enclosure with gopura gates."""
    G, P, W, z = enclosure(B, gate_style="gopura", tower_style="lotus")
    B.box("plinth", 0, 0, z, 5.0, 5.0, 0.6, "stone"); B.box("plinth_top", 0, 0, z + 0.6, 4.9, 4.9, 0.03, "pave")
    B.box("stair_s", 0, -2.8, z, 1.0, 0.7, 0.6, "stone"); B.box("stair_e", 2.8, 0, z, 0.7, 1.0, 0.6, "stone")
    zp = z + 0.6
    # cruciform gallery with tiled roofs linking the towers
    for name, cx, cy, w, d in (("gal_x", 0, 0, 4.6, 1.3), ("gal_y", 0, 0, 1.3, 4.6)):
        B.box(name, cx, cy, zp, w, d, 1.1, "stone")
        B.frustum(name + "_roof", cx, cy, zp + 1.1, w + 0.3, d + 0.3, max(w - 0.9, 0.3), max(d - 0.9, 0.3), 0.55, "roof")
    # four corner prangs and the taller central one
    for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
        cx, cy = sx * 1.75, sy * 1.75
        B.box("cbase", cx, cy, zp, 1.4, 1.4, 1.0, "stone")
        B.prang("cprang", cx, cy, zp + 1.0, 1.25, 3.2, "stone", tiers=7)
    B.box("mbase", 0, 0, zp, 2.0, 2.0, 1.4, "stone")
    B.box("mdoor", 0, -1.01, zp + 0.1, 0.5, 0.04, 0.9, "archDark"); B.box("mdoor", 1.01, 0, zp + 0.1, 0.04, 0.5, 0.9, "archDark")
    B.prang("mprang", 0, 0, zp + 1.4, 1.9, 5.2, "stone", tiers=8)
    halls(B, "stone")
    return G


def build_roof_tile_kiln(B):
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


BUILDINGS = {"fortress": build_fortress, "bagan_fortress": build_bagan_fortress, "khmer_fortress": build_khmer_fortress,
             "roof_tile_kiln": build_roof_tile_kiln}
# The fortress studies predate the engine camera and keep the original one (--cells, 6 px pad).
LEGACY_CAMERA = {"fortress", "bagan_fortress", "khmer_fortress"}


# ----------------------------------------------------------------------------
# Scene: camera, lights, render settings, outlines, grade
# ----------------------------------------------------------------------------
def clear_scene():
    bpy.ops.object.select_all(action='SELECT'); bpy.ops.object.delete()
    for coll in (bpy.data.meshes, bpy.data.materials, bpy.data.lights, bpy.data.cameras, bpy.data.images):
        for d in list(coll):
            if d.users == 0: coll.remove(d)
    for c in list(bpy.data.collections):
        if not c.objects and not c.children: bpy.data.collections.remove(c)


def setup_camera(G, cells, scale, height_factor=1.0):
    sc = bpy.context.scene
    cam_d = bpy.data.cameras.new("IsoCam"); cam_d.type = 'ORTHO'
    cam = bpy.data.objects.new("IsoCam", cam_d); sc.collection.objects.link(cam); sc.camera = cam
    cam.rotation_euler = (radians(60), 0, radians(45))
    cam.location = (18.4, -18.4, 15.0)
    sc.render.resolution_x = int(cells * 64 * scale)
    sc.render.resolution_y = int(cells * 64 * scale * height_factor)
    sc.render.resolution_percentage = 100
    diamond_w = G * sqrt(2)                      # world width of the ground diamond on screen
    cam_d.ortho_scale = diamond_w * max(1.0, height_factor)  # ortho_scale spans the larger dimension
    # vertical placement: put the lowest point of the tile a few px above the bottom edge
    bpy.context.view_layer.update()
    from bpy_extras.object_utils import world_to_camera_view
    from mathutils import Vector
    lowest = world_to_camera_view(sc, cam, Vector((G / 2, -G / 2, -0.45))).y  # front corner, bottom of earth slab
    pad = 6.0 / sc.render.resolution_y
    cam_d.shift_y = (lowest - pad) / max(1.0, height_factor)    # positive shift moves the scene down in frame
    return cam


# Engine sprite format (see calc_object_iso_coords in crates/game/src/tile/mod.rs):
# - the sprite is centred on the footprint using its logical width, so the PNG is
#   exactly G * 64 * density px wide and the diamond's left and right corners touch
#   the frame's sides;
# - its bottom edge sits on the footprint's bottom vertex, so the front corner of the
#   footprint at z = 0, world (G/2, -G/2, 0), is the bottom-centre of the frame.
# draw_size is the PNG size / density. Anything below z = 0 is cut off.
DEFAULT_DENSITY = 8          # PNG px per logical px (placeholder-art plan, Q1): about the hand-made 2x2 art's 7.6
TOP_MARGIN_PX = 2            # empty logical px kept above the model


def model_extents(G, collection):
    """Screen-space extents of the model, in world units, relative to the anchor:
    (up, half-width) as seen by the isometric camera."""
    from mathutils import Euler, Vector
    R = Euler((radians(60), 0, radians(45))).to_matrix()
    up, right = R @ Vector((0, 1, 0)), R @ Vector((1, 0, 0))
    anchor = Vector((G / 2, -G / 2, 0))
    bpy.context.view_layer.update()
    max_up, max_side = 0.0, 0.0
    for o in collection.all_objects:
        if o.type != 'MESH': continue
        mw = o.matrix_world
        for v in o.data.vertices:
            d = mw @ v.co - anchor
            max_up = max(max_up, d.dot(up)); max_side = max(max_side, abs(d.dot(right)))
    return max_up, max_side


def setup_camera_engine(G, density, collection):
    """Orthographic camera whose frame matches the engine's sprite anchor exactly.
    The height is measured from the model and rounded up to whole logical px.
    Returns (camera, width_px, height_px)."""
    from mathutils import Euler, Vector
    sc = bpy.context.scene
    cam_d = bpy.data.cameras.new("IsoCam"); cam_d.type = 'ORTHO'; cam_d.sensor_fit = 'HORIZONTAL'
    cam = bpy.data.objects.new("IsoCam", cam_d); sc.collection.objects.link(cam); sc.camera = cam
    rot = Euler((radians(60), 0, radians(45))); cam.rotation_euler = rot
    R = rot.to_matrix(); up, fwd = R @ Vector((0, 1, 0)), R @ Vector((0, 0, -1))
    width_px = int(round(G * 64 * density))
    cam_d.ortho_scale = G * sqrt(2)                          # the diamond spans the full width
    world_per_px = cam_d.ortho_scale / width_px
    max_up, max_side = model_extents(G, collection)
    if max_side > cam_d.ortho_scale / 2 + 1e-4:
        print(f"WARNING: model is {max_side:.3f} wide of the anchor; the frame holds {cam_d.ortho_scale / 2:.3f}. The sides get clipped.")
    logical_h = math.ceil(max_up / world_per_px / density) + TOP_MARGIN_PX
    height_px = logical_h * density
    sc.render.resolution_x, sc.render.resolution_y, sc.render.resolution_percentage = width_px, height_px, 100
    sc.render.pixel_aspect_x = sc.render.pixel_aspect_y = 1.0
    cam_d.shift_x = cam_d.shift_y = 0.0
    dist = 40.0 + 4.0 * G
    anchor = Vector((G / 2, -G / 2, 0))
    cam.location = anchor + up * (height_px * world_per_px / 2) - fwd * dist   # anchor on the bottom edge's centre
    cam_d.clip_start, cam_d.clip_end = 0.1, dist * 2 + 20
    return cam, width_px, height_px


def setup_lighting():
    L = LIGHTING; sc = bpy.context.scene
    sun_d = bpy.data.lights.new("Sun", 'SUN'); sun_d.energy = L["sun_energy"]; sun_d.color = L["sun_color"]; sun_d.angle = radians(L["sun_angle"])
    sun = bpy.data.objects.new("Sun", sun_d); sc.collection.objects.link(sun); sun.rotation_euler = [radians(a) for a in L["sun_rot"]]
    fill_d = bpy.data.lights.new("Fill", 'SUN'); fill_d.energy = L["fill_energy"]; fill_d.color = L["fill_color"]
    fill = bpy.data.objects.new("Fill", fill_d); sc.collection.objects.link(fill); fill.rotation_euler = [radians(a) for a in L["fill_rot"]]
    world = sc.world or bpy.data.worlds.new("World"); sc.world = world; world.use_nodes = True
    bg = world.node_tree.nodes.get("Background"); bg.inputs[0].default_value = (*L["world_color"], 1); bg.inputs[1].default_value = L["world_strength"]


def setup_render():
    sc = bpy.context.scene
    for eng in ("BLENDER_EEVEE", "BLENDER_EEVEE_NEXT"):
        try: sc.render.engine = eng; break
        except TypeError: pass
    sc.render.film_transparent = True
    sc.render.image_settings.file_format = 'PNG'; sc.render.image_settings.color_mode = 'RGBA'
    sc.view_settings.view_transform = 'Standard'; sc.view_settings.look = 'None'
    ev = sc.eevee
    for k, v in (("taa_render_samples", EEVEE["samples"]), ("use_shadows", True), ("use_fast_gi", True), ("fast_gi_distance", EEVEE["fast_gi_distance"]), ("shadow_ray_count", 2), ("shadow_step_count", 4)):
        if hasattr(ev, k): setattr(ev, k, v)


def setup_outlines(px_scale=1.0, wobble_scale=None):
    """px_scale multiplies the line thickness, for renders at more than one pixel
    per logical pixel; wobble_scale (default px_scale) multiplies the noise and jitter."""
    O = dict(OUTLINE); sc = bpy.context.scene
    # The scene's line thickness multiplies the line style's, so only the style's
    # measures scale: the legacy renders get 1.6 x 1.6 = 2.56 px lines.
    sc.render.use_freestyle = True; sc.render.line_thickness_mode = 'ABSOLUTE'; sc.render.line_thickness = O["thickness"]
    w = px_scale if wobble_scale is None else wobble_scale
    O["thickness"] *= px_scale
    for k in ("noise_amp", "jitter_amp", "noise_period"): O[k] *= w
    fs = bpy.context.view_layer.freestyle_settings; fs.use_culling = True; fs.crease_angle = radians(120)
    for ls in list(fs.linesets): fs.linesets.remove(ls)
    ls = fs.linesets.new("Ink"); ls.select_silhouette = True; ls.select_border = True; ls.select_crease = True; ls.select_contour = False
    st = ls.linestyle; st.color = O["color"]; st.thickness = O["thickness"]; st.alpha = O["alpha"]
    m = st.thickness_modifiers.new(name="Noise", type='NOISE'); m.amplitude = O["noise_amp"]; m.period = O["noise_period"]; m.seed = 3
    g = st.geometry_modifiers.new(name="Jitter", type='PERLIN_NOISE_1D'); g.amplitude = O["jitter_amp"]; g.frequency = O["jitter_freq"]; g.octaves = 2


def setup_grade():
    # Blender 5.x compositor: tree lives on scene.compositing_node_group and
    # outputs through NodeGroupOutput (the old CompositorNodeComposite is gone).
    sc = bpy.context.scene
    ct = getattr(sc, "compositing_node_group", None)
    if ct is None:
        ct = bpy.data.node_groups.new("Comp", "CompositorNodeTree"); sc.compositing_node_group = ct
    for n in list(ct.nodes): ct.nodes.remove(n)
    if not ct.interface.items_tree: ct.interface.new_socket("Image", in_out='OUTPUT', socket_type='NodeSocketColor')
    rl = ct.nodes.new("CompositorNodeRLayers"); outn = ct.nodes.new("NodeGroupOutput")
    hs = ct.nodes.new("CompositorNodeHueSat"); hs.inputs["Saturation"].default_value = GRADE["saturation"]
    bc = ct.nodes.new("CompositorNodeBrightContrast"); bc.inputs["Contrast"].default_value = GRADE["contrast"]
    ct.links.new(rl.outputs["Image"], hs.inputs["Image"]); ct.links.new(hs.outputs[0], bc.inputs["Image"]); ct.links.new(bc.outputs[0], outn.inputs[0])
    sc.render.use_compositing = True


def trim_top(path, keep_px=4):
    """Crop fully-transparent rows from the top of a rendered PNG, leaving keep_px of margin.
    Width and the bottom anchor are untouched, so the diamond still maps to the grid."""
    import numpy as np
    im = bpy.data.images.load(path); W, H = im.size
    px = np.array(im.pixels[:], dtype=np.float32).reshape(H, W, 4)      # row 0 = bottom
    rows = np.where((px[:, :, 3] > 0.002).any(axis=1))[0]
    top = min(H, int(rows.max()) + 1 + keep_px) if len(rows) else H
    cropped = px[:top]
    out = bpy.data.images.new("trimmed", W, top, alpha=True, float_buffer=False)
    out.colorspace_settings.name = im.colorspace_settings.name
    out.pixels.foreach_set(cropped.ravel())
    out.file_format = 'PNG'; out.filepath_raw = path; out.save()
    bpy.data.images.remove(im); bpy.data.images.remove(out)
    return W, top


# ----------------------------------------------------------------------------
# Entry point
# ----------------------------------------------------------------------------
# Outline thickness per logical px for engine renders. The legacy 1.6 px was tuned
# for 1x fortress renders, where a 15-cell tile shrinks a lot on screen; game tiles
# are drawn near 1:1, so their ink is thinner per logical px.
ENGINE_OUTLINE_SCALE = 0.4


def generate_engine(building, palette="viet", density=DEFAULT_DENSITY, out=None, blend=None, render=True):
    """Render a tile in the engine's sprite format. Returns the PNG size and the
    draw_size for tile_set.json (PNG size / density)."""
    clear_scene()
    mats = Materials(PALETTES[palette])
    B = Builder(mats, collection_name=building)
    G = BUILDINGS[building](B)
    # Ink: thickness scales with density, but the wobble stays small so the lines stay crisp.
    setup_lighting(); setup_render(); setup_outlines(density * ENGINE_OUTLINE_SCALE, wobble_scale=1.0); setup_grade()
    _, w, h = setup_camera_engine(G, density, B.col)
    sc = bpy.context.scene
    if blend:
        bpy.ops.wm.save_as_mainfile(filepath=os.path.abspath(os.path.expanduser(blend)))
    if out and render:
        sc.render.filepath = os.path.abspath(os.path.expanduser(out))
        bpy.ops.render.render(write_still=True)
    return dict(building=building, cells=G, density=density, png_size=[w, h],
                logical_size=dict(width=G * 64, height=G * 32), draw_size=dict(width=w // density, height=h // density), out=out)


def generate(building="fortress", palette="viet", cells=15, scale=1.0, height_factor=1.0, out=None, blend=None, render=True, trim=True):
    clear_scene()
    mats = Materials(PALETTES[palette])
    B = Builder(mats, collection_name=building)
    G = BUILDINGS[building](B)
    setup_lighting(); setup_render(); setup_outlines(); setup_grade()
    setup_camera(G, cells, scale, height_factor)
    sc = bpy.context.scene
    if blend:
        bpy.ops.wm.save_as_mainfile(filepath=os.path.abspath(os.path.expanduser(blend)))
    if out and render:
        sc.render.filepath = os.path.abspath(os.path.expanduser(out))
        bpy.ops.render.render(write_still=True)
        if trim: final = trim_top(sc.render.filepath)
        else: final = (sc.render.resolution_x, sc.render.resolution_y)
    else:
        final = (sc.render.resolution_x, sc.render.resolution_y)
    return dict(objects=len(B.col.objects), render_res=(sc.render.resolution_x, sc.render.resolution_y), output_size=final, ortho=sc.camera.data.ortho_scale, out=out)


def _cli():
    import argparse
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    ap = argparse.ArgumentParser()
    ap.add_argument("--building", default="fortress", choices=list(BUILDINGS))
    ap.add_argument("--palette", default="viet", choices=list(PALETTES))
    ap.add_argument("--cells", type=int, default=15, help="legacy camera: footprint in grid cells (render width = cells*64*scale px)")
    ap.add_argument("--scale", type=float, default=1.0, help="legacy camera: pixels per logical pixel")
    ap.add_argument("--height", type=float, default=1.0, help="legacy camera: render height as a factor of width")
    ap.add_argument("--density", type=int, default=DEFAULT_DENSITY, help="engine camera: PNG px per logical px")
    ap.add_argument("--out", required=True); ap.add_argument("--blend", default=None)
    ap.add_argument("--no-trim", action="store_true", help="legacy camera: keep the full square render instead of cropping empty rows off the top")
    a = ap.parse_args(argv)
    if a.building in LEGACY_CAMERA:
        print(generate(a.building, a.palette, a.cells, a.scale, a.height, a.out, a.blend, trim=not a.no_trim))
    else:
        import json
        print("HB_RESULT " + json.dumps(generate_engine(a.building, a.palette, a.density, a.out, a.blend)))


if __name__ == "__main__" and bpy.app.background:
    _cli()
