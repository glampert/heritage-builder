"""Procedural node materials, driven by object coordinates (no UVs)."""

import bpy

from .palettes import PALETTES, lin


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
