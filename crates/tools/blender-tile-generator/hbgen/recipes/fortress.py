"""The fortress studies: the reference for the look. They use the legacy camera
(--cells, a 6 px pad under the earth slab), not the engine one."""


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


def build_fortress(B, **_):
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


def build_bagan_fortress(B, **_):
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


def build_khmer_fortress(B, **_):
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
