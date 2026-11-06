"""Geometry primitives. 1 world unit = 1 grid cell; front faces are +X and -Y."""

import math
import random
from math import radians

import bmesh
import bpy


class Builder:
    def __init__(self, mats, collection_name="Tile", seed=0):
        self.M = mats
        self.rng = random.Random(seed)       # recipes draw variation from this, so a seed renders the same every time
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

    def pole(self, name, p0, p1, r, m, verts=8):
        """A round pole from point p0 to point p1 (scaffolding, rafters, braces)."""
        from mathutils import Vector
        a, b = Vector(p0), Vector(p1)
        d = b - a
        bpy.ops.mesh.primitive_cylinder_add(vertices=verts, radius=r, depth=d.length, location=(a + b) / 2)
        o = bpy.context.active_object; o.name = name
        o.rotation_mode = 'QUATERNION'; o.rotation_quaternion = d.to_track_quat('Z', 'Y')
        self._add(o, m)
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
