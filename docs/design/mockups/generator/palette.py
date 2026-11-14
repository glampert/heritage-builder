"""Renders the build palette's icon mockups for each realm into build/:

- build-icons-<realm>: every building's icon, by sector, and the four states
  (idle, hovered, pressed, disabled);
- build-palette-<realm>: the HUD with the sector column in the realm's icon
  style, one sector open as a grid of icons left of it, and the hovered icon's
  tooltip with its name and cost.

Each realm treats the same pictograms its own way: Đại Long stamps them in
vermilion seal ink on paper, Kambara carves them in relief in sandstone, and
Tampadesa gilds them on red lacquer in an arched niche. The pictograms are in
palette_glyphs.py, drawn in each realm's architecture.
"""
from world_map import T
import dialogs as D
import dialogs_content as DC
from menus import W, H
import palette_glyphs as G

# The game's sectors (TilePaletteMainButtonDef), in order, with their buildings
# (the sectors in assets/tiles/objects/tile_set.json).
SECTORS = [
    ("clear", "Clear land", "Demolish and clear", []),
    ("housing", "Housing", "Vacant lot · 1 gold", []),
    ("roads", "Roads", "Dirt and paved roads", []),
    None,
    ("food", "Food and farming", "Farms, fishing and markets", ["rice_farm", "livestock_farm", "fishing_wharf", "granary", "market"]),
    ("industry", "Industry and resources", "Raw materials and workshops",
     ["lumberyard", "clay_pit", "mine", "brick_maker", "pottery_kiln", "roof_tile_kiln", "distillery", "storage_yard"]),
    ("services", "Services", "Guards, taxes, health and fire",
     ["police_station", "tax_office", "fire_watch", "builders_guild", "apothecary", "hospital"]),
    ("infrastructure", "Infrastructure", "Water and government", ["small_well", "large_well", "governor_palace", "citadel"]),
    ("culture", "Culture and religion", "Faith, learning and theatre", ["shrine", "temple", "theater", "university"]),
    ("trade", "Trade and economy", "Nothing to build here yet", []),
    ("beauty", "Beautification", "Gardens and plazas", ["garden"]),
    None,
    ("undo", "Undo", "", []),
    ("redo", "Redo", "", []),
]

SHORT = {"clear": "Clear", "food": "Food", "industry": "Industry", "infrastructure": "Civic", "culture": "Culture",
         "trade": "Trade", "beauty": "Beauty"}

# The game's costs (assets/tiles/objects/tile_set.json) and sizes in tiles.
COSTS = {
    "rice_farm": (5, {}, "3×3"), "livestock_farm": (6, {}, "3×3"), "fishing_wharf": (5, {}, "2×2"), "granary": (10, {}, "3×3"),
    "market": (10, {}, "2×2"), "lumberyard": (4, {}, "2×2"), "clay_pit": (4, {}, "2×2"), "mine": (8, {}, "2×2"),
    "brick_maker": (10, {}, "2×2"), "pottery_kiln": (10, {}, "2×2"), "roof_tile_kiln": (10, {}, "2×2"),
    "distillery": (15, {"Wood": 4, "Bricks": 3}, "2×2"), "storage_yard": (10, {}, "3×3"),
    "police_station": (10, {"Wood": 3, "Bricks": 2}, "2×2"), "tax_office": (15, {"Wood": 4, "Bricks": 4}, "2×2"),
    "fire_watch": (10, {"Wood": 3}, "1×1"), "builders_guild": (15, {"Wood": 4, "Bricks": 2}, "2×2"),
    "apothecary": (4, {"Wood": 3, "Bricks": 2}, "1×1"), "hospital": (12, {"Wood": 6, "Bricks": 6, "Roof Tiles": 3}, "2×2"),
    "small_well": (2, {}, "1×1"), "large_well": (5, {}, "2×2"),
    "governor_palace": (500, {"Bricks": 16, "Wood": 8, "Metal": 4, "Roof Tiles": 8}, "4×4"),
    "citadel": (1000, {"Bricks": 24, "Wood": 8, "Metal": 8, "Roof Tiles": 8}, "5×5"),
    "shrine": (2, {}, "1×1"), "temple": (15, {"Wood": 6, "Bricks": 8, "Roof Tiles": 4}, "3×3"),
    "theater": (15, {"Wood": 8, "Bricks": 6, "Roof Tiles": 4}, "2×2"), "university": (20, {"Wood": 8, "Bricks": 10, "Roof Tiles": 5}, "3×3"),
    "garden": (10, {}, "3×3"),
}
STOCK = {"Wood": 140, "Bricks": 2, "Roof Tiles": 42, "Metal": 0}

# Realm names for the buildings whose look and name are the realm's own.
REALM_NAMES = {
    "dailong": {"shrine": "Spirit shrine", "temple": "Temple", "university": "Temple of Literature"},
    "kambara": {"shrine": "Neak ta shrine", "temple": "Prasat", "university": "Library"},
    "tampadesa": {"shrine": "Nat shrine", "temple": "Stupa", "university": "Monastery school"},
}


# ---------------------------------------------------------------- sector pictograms
def sector_glyph(key, realm):
    P, R, Ci = G.P, G.R, G.Ci
    if key == "clear":  # a spade over a cleared patch
        return [P("M-30,28 Q0,20 30,28 Q0,36 -30,28Z"), P("M8,-30 L-4,8", True, 4), P("M0,-32 L16,-28", True, 4),
                P("M-12,4 L4,10 L-2,22 Q-12,24 -16,16Z")]
    if key == "housing":
        return G.hall(realm, 0, 44, 30, 16, windows=2) + G.ground_line(-36, 36)
    if key == "roads":
        return G.road(realm)
    if key == "food":
        return G.rice_farm(realm)
    if key == "industry":
        return G.pottery_kiln(realm)
    if key == "services":  # a watch drum on its stand
        return [G.E(0, -2, 22, 18), G.E(-6, -6, 5, 3, False), P("M-22,-2 Q0,10 22,-2", False, 2),
                P("M-18,12 L-26,34 M18,12 L26,34 M-22,26 H22", True, 3.4), P("M20,-26 L34,-38", True, 3), G.Ci(35, -39, 3)]
    if key == "infrastructure":
        return G.large_well(realm)
    if key == "culture":
        return G.temple(realm)
    if key == "trade":  # a boat with a coin over it
        return [P("M-36,14 Q-30,28 -10,30 H18 Q34,28 38,12 Q20,18 0,18 Q-20,18 -36,14Z"), P("M-4,18 V-10", True, 2.6),
                P("M-2,-10 Q14,-2 16,14 H-2Z")] + G.coin(-22, -12, 10)
    if key == "beauty":
        return G.lotus(0, 26, 1.9) + G.waves(-36, 36, 32, 3, 3, 2.6)
    if key == "undo":
        return [P("M-20,-6 H8 Q26,-6 26,10 Q26,26 8,26 H-6", True, 6), P("M-30,-6 L-14,-20 V8Z")]
    if key == "redo":
        return [P("M20,-6 H-8 Q-26,-6 -26,10 Q-26,26 -8,26 H6", True, 6), P("M30,-6 L14,-20 V8Z")]
    raise KeyError(key)


class Palette:
    short_col = None

    """The icon treatments, mixed into each realm's Dialogs class."""

    def pid(self, base):
        self._pid = getattr(self, "_pid", 0) + 1
        return f"pal-{base}-{self._pid}"

    def mask_of(self, elements, x, y, s, inverse=False, box=None):
        """A mask of the glyph at (x, y), scaled s from its 100-unit box."""
        mid = self.pid("m")
        sol, cut = ("#000", "#fff") if inverse else ("#fff", "#000")
        body = "".join(elements).replace('"S"', f'"{sol}"').replace('"C"', f'"{cut}"')
        under = ""
        if inverse and box:
            bx, by, bw, bh, rx = box
            under = f'<rect x="{bx}" y="{by}" width="{bw}" height="{bh}" rx="{rx}" fill="#fff"/>'
        self.defs.append(f'<mask id="{mid}" maskUnits="userSpaceOnUse" x="0" y="0" width="{W}" height="{H}">{under}'
                         f'<g transform="translate({x},{y}) scale({s})">{body}</g></mask>')
        return f"url(#{mid})"

    def filters(self):
        if getattr(self, "_filters", None):
            return self._filters
        ink, stone, relief, gild, shadow = (self.pid(n) for n in ("ink", "stone", "relief", "gild", "shadow"))
        self.defs.append(
            # Seal ink: speckled where the stamp missed the paper, edges a little ragged.
            f'<filter id="{ink}" x="-5%" y="-5%" width="110%" height="110%">'
            f'<feTurbulence type="fractalNoise" baseFrequency="0.9" numOctaves="2" seed="7" result="n"/>'
            f'<feDisplacementMap in="SourceGraphic" in2="n" scale="1.6" result="d"/>'
            f'<feTurbulence type="fractalNoise" baseFrequency="0.45" numOctaves="3" seed="3" result="n2"/>'
            f'<feColorMatrix in="n2" type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  -3.2 0 0 0 2.65" result="a"/>'
            f'<feComposite in="d" in2="a" operator="in"/></filter>'
            # Sandstone: grain and pits over the fill.
            f'<filter id="{stone}" x="0" y="0" width="100%" height="100%">'
            f'<feTurbulence type="fractalNoise" baseFrequency="0.6" numOctaves="3" seed="11" result="n"/>'
            f'<feColorMatrix in="n" type="matrix" values="0 0 0 0 0.45  0 0 0 0 0.38  0 0 0 0 0.28  0 0 0 -1.6 1.0" result="g"/>'
            f'<feComposite in="g" in2="SourceAlpha" operator="in" result="gi"/>'
            f'<feBlend in="gi" in2="SourceGraphic" mode="multiply"/></filter>'
            # Relief: the shape lit from the top left, its shadow cast down right.
            f'<filter id="{relief}" x="-10%" y="-10%" width="120%" height="120%">'
            f'<feGaussianBlur in="SourceAlpha" stdDeviation="1.4" result="b"/>'
            f'<feDiffuseLighting in="b" surfaceScale="5" diffuseConstant="1.35" lighting-color="#fff4e2" result="l"><feDistantLight azimuth="225" elevation="40"/></feDiffuseLighting>'
            f'<feComposite in="l" in2="SourceAlpha" operator="in" result="lit"/>'
            f'<feBlend in="SourceGraphic" in2="lit" mode="multiply" result="shaded"/>'
            f'<feOffset in="b" dx="1.8" dy="2.2" result="o"/><feFlood flood-color="#1e170c" flood-opacity="0.8"/><feComposite in2="o" operator="in" result="sh"/>'
            f'<feMerge><feMergeNode in="sh"/><feMergeNode in="shaded"/></feMerge></filter>'
            # Gold leaf: a slight emboss and a shine.
            f'<filter id="{gild}" x="-10%" y="-10%" width="120%" height="120%">'
            f'<feGaussianBlur in="SourceAlpha" stdDeviation="0.9" result="b"/>'
            f'<feSpecularLighting in="b" surfaceScale="2.5" specularConstant="0.9" specularExponent="16" lighting-color="#fff6d8" result="s"><feDistantLight azimuth="225" elevation="48"/></feSpecularLighting>'
            f'<feComposite in="s" in2="SourceAlpha" operator="in" result="si"/>'
            f'<feOffset in="b" dx="0.8" dy="1.2" result="o"/><feFlood flood-color="#1a0604" flood-opacity="0.7"/><feComposite in2="o" operator="in" result="sh"/>'
            f'<feMerge><feMergeNode in="sh"/><feMergeNode in="SourceGraphic"/><feMergeNode in="si"/></feMerge></filter>'
            f'<filter id="{shadow}" x="-20%" y="-20%" width="140%" height="140%"><feGaussianBlur in="SourceAlpha" stdDeviation="2.5"/>'
            f'<feOffset dx="0" dy="2.5" result="o"/><feFlood flood-color="#120a04" flood-opacity="0.45"/><feComposite in2="o" operator="in"/>'
            f'<feMerge><feMergeNode/><feMergeNode in="SourceGraphic"/></feMerge></filter>')
        gold, lacq, goldbg = self.pid("gold"), self.pid("lacq"), self.pid("goldbg")
        self.defs.append(
            f'<linearGradient id="{gold}" x1="0" y1="0" x2="0.6" y2="1"><stop offset="0" stop-color="#f7dc8a"/><stop offset="0.45" stop-color="#d9a945"/>'
            f'<stop offset="0.7" stop-color="#f0c968"/><stop offset="1" stop-color="#a87526"/></linearGradient>'
            f'<linearGradient id="{goldbg}" x1="0" y1="0" x2="0.4" y2="1"><stop offset="0" stop-color="#f3d27c"/><stop offset="1" stop-color="#b98432"/></linearGradient>'
            f'<radialGradient id="{lacq}" cx="0.35" cy="0.25" r="0.9"><stop offset="0" stop-color="#8a2a1c"/><stop offset="0.6" stop-color="#5e1810"/>'
            f'<stop offset="1" stop-color="#3a0c08"/></radialGradient>')
        self._filters = dict(ink=ink, stone=stone, relief=relief, gild=gild, shadow=shadow, gold=gold, lacq=lacq, goldbg=goldbg)
        return self._filters

    # ------------------------------------------------------------ one icon
    def icon_tile(self, x, y, size, elements, state="idle"):
        """An icon tile with its top-left corner at (x, y), in the realm's treatment."""
        return getattr(self, f"tile_{self.dd['key']}")(x, y, size, elements, state)

    def tile_dailong(self, x, y, size, els, state):
        fl = self.filters()
        s, cx, cy = size / 100 * 0.78, x + size / 2, y + size / 2 + size * 0.02
        out = []
        lift = state == "hover"
        card = "#f8eed6" if lift else "#f1e3c3"
        out.append(f'<g filter="url(#{fl["shadow"]})"><rect x="{x}" y="{y - (2 if lift else 0)}" width="{size}" height="{size}" rx="5" fill="{card}" '
                   f'stroke="{"#a8261c" if lift else "#c9b083"}" stroke-width="{2 if lift else 1.2}"/></g>')
        if lift:
            y -= 2
            cy -= 2
        ink = "#a8261c" if state != "disabled" else "#8d7b62"
        m = size * 0.08
        sx, sy, sw = x + m, y + m, size - 2 * m
        if state == "pressed":  # the seal cut the other way: red ground, the building left bare
            mask = self.mask_of(els, cx, cy, s * 0.98, inverse=True, box=(sx, sy, sw, sw, 5))
            out.append(f'<g filter="url(#{fl["ink"]})"><rect x="{sx}" y="{sy}" width="{sw}" height="{sw}" rx="5" fill="#b3271b" mask="{mask}"/></g>')
        else:
            mask = self.mask_of(els, cx, cy, s * 0.95)
            op = 0.55 if state == "disabled" else 1
            out.append(f'<g filter="url(#{fl["ink"]})" opacity="{op}">'
                       f'<rect x="{sx}" y="{sy}" width="{sw}" height="{sw}" rx="6" fill="none" stroke="{ink}" stroke-width="{size * 0.045}"/>'
                       f'<rect x="{sx + size * 0.065}" y="{sy + size * 0.065}" width="{sw - size * 0.13}" height="{sw - size * 0.13}" rx="3" fill="none" stroke="{ink}" stroke-width="{size * 0.014}"/>'
                       f'<rect x="{x}" y="{y}" width="{size}" height="{size}" fill="{ink}" mask="{mask}"/></g>')
        return "".join(out)

    def tile_kambara(self, x, y, size, els, state):
        fl = self.filters()
        s, cx, cy = size / 100 * 0.8, x + size / 2, y + size / 2 + size * 0.02
        b = size * 0.07
        face = {"idle": "#b8a176", "hover": "#c7b086", "pressed": "#a68e62", "disabled": "#9d937e"}[state]
        out = [f'<g filter="url(#{fl["shadow"]})"><rect x="{x}" y="{y}" width="{size}" height="{size}" rx="3" fill="{face}" filter="url(#{fl["stone"]})"/></g>']
        # The block's chiselled bevel: lit top and left, shaded bottom and right (sunk when pressed).
        hi, lo = ("#5d4f36", "#efe0bc") if state == "pressed" else ("#efe0bc", "#5d4f36")
        out.append(f'<path d="M{x},{y} H{x + size} L{x + size - b},{y + b} H{x + b} V{y + size - b} L{x},{y + size}Z" fill="{hi}" opacity="0.55"/>'
                   f'<path d="M{x + size},{y + size} H{x} L{x + b},{y + size - b} H{x + size - b} V{y + b} L{x + size},{y}Z" fill="{lo}" opacity="0.55"/>')
        if state == "hover":
            out.append(f'<rect x="{x - 2}" y="{y - 2}" width="{size + 4}" height="{size + 4}" rx="4" fill="none" stroke="#e0b25a" stroke-width="2.5"/>')
        if state == "pressed":
            out.append(f'<rect x="{x + b + 2}" y="{y + b + 2}" width="{size - 2 * b - 4}" height="{size - 2 * b - 4}" fill="none" stroke="#9a4a2e" stroke-width="2"/>')
        # A little moss in a corner.
        if state != "pressed":
            out.append(f'<path d="M{x},{y + size * 0.62} q{size * 0.05},-{size * 0.06} {size * 0.03},-{size * 0.13} q{size * 0.06},{size * 0.05} {size * 0.04},{size * 0.13} '
                       f'q{size * 0.05},{size * 0.02} {size * 0.03},{size * 0.1} q{size * 0.06},{size * 0.06} {size * 0.02},{size * 0.28} H{x}Z" fill="#5f7a35" opacity="{0.45 if state == "disabled" else 0.85}"/>')
        mask = self.mask_of(els, cx, cy, s)
        glyph_fill = {"idle": "#e4d3ab", "hover": "#efdfb8", "pressed": "#e8c894", "disabled": "#b8ad94"}[state]
        op = 0.6 if state == "disabled" else 1
        out.append(f'<g filter="url(#{fl["relief"]})" opacity="{op}"><rect x="{x}" y="{y}" width="{size}" height="{size}" fill="{glyph_fill}" mask="{mask}"/></g>')
        return "".join(out)

    def tile_tampadesa(self, x, y, size, els, state):
        fl = self.filters()
        s, cx, cy = size / 100 * 0.74, x + size / 2, y + size / 2 + size * 0.07
        arch = size * 0.22
        # The niche: an arched plaque of red lacquer in a gilt frame.
        def niche(ix, iy, w, h, a):
            return (f"M{ix},{iy + h} V{iy + a} Q{ix},{iy + a * 0.35} {ix + w * 0.32},{iy + a * 0.25} Q{ix + w / 2},{iy - a * 0.15} {ix + w / 2},{iy - a * 0.3} "
                    f"Q{ix + w / 2},{iy - a * 0.15} {ix + w * 0.68},{iy + a * 0.25} Q{ix + w},{iy + a * 0.35} {ix + w},{iy + a} V{iy + h}Z")
        outer = niche(x, y + arch * 0.3, size, size - arch * 0.3, arch)
        g = size * 0.07
        inner = niche(x + g, y + arch * 0.3 + g * 1.2, size - 2 * g, size - arch * 0.3 - 2.2 * g, arch * 0.85)
        lift = state == "hover"
        frame = f"url(#{fl['gold']})" if state != "disabled" else "#7d6544"
        out = [f'<g filter="url(#{fl["shadow"]})"><path d="{outer}" fill="{frame}" stroke="#3a1a08" stroke-width="1.2"/></g>']
        ground = f"url(#{fl['goldbg']})" if state == "pressed" else f"url(#{fl['lacq']})"
        out.append(f'<path d="{inner}" fill="{ground}" stroke="#3a1a08" stroke-width="1"/>')
        out.append(f'<path d="{inner}" fill="none" stroke="{"#f7dc8a" if lift else "#b98432"}" stroke-width="{1.6 if lift else 0.8}"/>')
        if lift:
            out.append(f'<path d="{outer}" fill="none" stroke="#ffe8a0" stroke-width="2.4" opacity="0.9"/>')
        mask = self.mask_of(els, cx, cy, s)
        if state == "pressed":
            out.append(f'<rect x="{x}" y="{y}" width="{size}" height="{size}" fill="#5e1810" mask="{mask}"/>')
        else:
            fill = "#6b5638" if state == "disabled" else f"url(#{fl['gold']})"
            out.append(f'<g filter="url(#{fl["gild"]})"><rect x="{x}" y="{y}" width="{size}" height="{size}" fill="{fill}" mask="{mask}"/></g>')
        # Three gilt dots under the arch's point, a temple's finial.
        out.append(f'<circle cx="{cx}" cy="{y + arch * 0.3 - arch * 0.3 + 1}" r="{size * 0.028}" fill="{frame}" stroke="#3a1a08" stroke-width="0.8"/>')
        return "".join(out)

    # ------------------------------------------------------------ pages
    def building_name(self, b):
        return REALM_NAMES[self.dd["key"]].get(b, G.BUILDINGS[b][0])

    def build_icons(self):
        realm = self.dd["key"]
        parts = self.hud_parts(keep_style=True) + [self.dim(0.55)]
        x, y, w, h = 70, 40, W - 140, H - 80
        parts.append(self.box(x, y, w, h))
        parts.append(self.section_title(x + 40, y + 58, "Build palette · icons"))
        parts.append(self.t(x + w - 40, y + 58, f"{self.dd['realm']}: {self.style_line}", 19, self.panel_sub, "end", 600, italic=True))
        size, gap = 88, 112
        bx, by = x + 40, y + 100
        col_w = (w - 80) / 2
        blocks = [s for s in SECTORS if s and s[3]]
        # Two columns of sector blocks.
        cols = [blocks[0:1] + blocks[2:3] + blocks[3:4], blocks[1:2] + blocks[4:6]]
        for ci, col in enumerate(cols):
            yy = by
            for key, title, sub, items in col:
                xx = bx + ci * col_w
                parts.append(self.t(xx, yy, title, 20, self.accent, "start", 700))
                per_row = 8
                for k, b in enumerate(items):
                    r, c = divmod(k, per_row)
                    ix, iy = xx + c * gap, yy + 18 + r * 152
                    parts.append(self.icon_tile(ix, iy, size, G.glyph(b, realm)))
                    for j, line in enumerate(self.wrap(self.building_name(b), gap - 8, 15, 0.48)[:2]):
                        parts.append(self.t(ix + size / 2, iy + size + 22 + j * 18, line, 15, self.panel_text, "middle", 600))
                rows = (len(items) + per_row - 1) // per_row
                yy += 18 + rows * 152 + 34
        # The sector column's own icons, and the four states.
        sy = y + h - 190
        parts.append(self.t(bx, sy, "Sector buttons", 20, self.accent, "start", 700))
        ssz, sg = 62, 74
        for k, sct in enumerate(s for s in SECTORS if s):
            ix = bx + k * sg
            parts.append(self.icon_tile(ix, sy + 18, ssz, sector_glyph(sct[0], realm)))
            for j, line in enumerate([SHORT.get(sct[0], sct[1])]):
                parts.append(self.t(ix + ssz / 2, sy + ssz + 38 + j * 16, line, 14, self.panel_text, "middle", 600))
        sx = bx + col_w + 60
        parts.append(self.t(sx, sy, "States", 20, self.accent, "start", 700))
        for k, st in enumerate(["idle", "hover", "pressed", "disabled"]):
            parts.append(self.icon_tile(sx + k * gap, sy + 18, size, G.glyph("temple", realm), st))
            parts.append(self.t(sx + k * gap + size / 2, sy + size + 40, {"hover": "hovered"}.get(st, st), 15, self.panel_text, "middle", 600))
        self.page(f"build-icons-{realm}", parts)

    def build_palette(self, open_key="industry", hover_b="distillery", disabled=("mine",)):
        realm = self.dd["key"]
        parts = self.hud_parts(keep_style=True)
        bar, _ = self.menu_buttons()
        parts.append(bar)
        # The sector column, right: the realm's icons, separators between the groups.
        size, step, sep = 62, 70, 16
        cw = size + 28
        cx0 = W - 12 - cw
        n = sum(1 for s in SECTORS if s)
        ch = 26 + n * step + 2 * sep
        parts.append(self.box(cx0, 10, cw, ch))
        yy = 24
        open_y = None
        for s in SECTORS:
            if s is None:
                parts.append(f'<line x1="{cx0 + 18}" y1="{yy + sep / 2 - 3}" x2="{cx0 + cw - 18}" y2="{yy + sep / 2 - 3}" stroke="{self.panel_rule}" stroke-width="2"/>')
                yy += sep
                continue
            key = s[0]
            st = "pressed" if key == open_key else ("disabled" if not s[3] and key == "trade" else "idle")
            parts.append(self.icon_tile(cx0 + 14, yy, size, sector_glyph(key, realm), st))
            if key == open_key:
                open_y = yy
            yy += step
        # The open sector's buildings: a grid left of the column, its top row level with the button.
        key, title, sub, items = next(s for s in SECTORS if s and s[0] == open_key)
        isz, ig, per_row = 84, 12, 4
        rows = (len(items) + per_row - 1) // per_row
        pw = 2 * 22 + per_row * isz + (per_row - 1) * ig
        ph = 64 + rows * isz + (rows - 1) * ig + 22
        px = cx0 - 16 - pw
        py = open_y - 52
        parts.append(self.box(px, py, pw, ph))
        # A notch pointing at the open sector's button.
        parts.append(f'<path d="M{px + pw - 1},{open_y + size / 2 - 12} l14,12 l-14,12Z" fill="{self.notch_fill}" stroke="{self.panel_rule}" stroke-width="1.5"/>')
        parts.append(self.section_title(px + 22, py + 40, title))
        hover_xy = None
        for k, b in enumerate(items):
            r, c = divmod(k, per_row)
            ix, iy = px + 22 + c * (isz + ig), py + 60 + r * (isz + ig)
            st = "hover" if b == hover_b else ("disabled" if b in disabled else "idle")
            parts.append(self.icon_tile(ix, iy, isz, G.glyph(b, realm), st))
            if b == hover_b:
                hover_xy = (ix, iy)
        # The hovered building's tooltip: its name and cost, under the icon.
        cost, mats, fp = COSTS[hover_b]
        tw = 340
        rows_t = [("Cost", f"{cost} gold", True)] + [(m, f"{n}  ·  {STOCK[m]} in storage", STOCK[m] >= n) for m, n in mats.items()]
        th = 70 + len(rows_t) * 32 + 46
        tx = hover_xy[0] + isz / 2 - tw / 2
        ty = py + ph + 16
        parts.append(self.box(tx, ty, tw, th))
        parts.append(f'<path d="M{hover_xy[0] + isz / 2 - 12},{ty + 1} l12,-14 l12,14Z" fill="{self.notch_fill}" stroke="{self.panel_rule}" stroke-width="1.5"/>')
        parts.append(T(tx + 22, ty + 42, self.building_name(hover_b), self.title_font, 25, self.panel_text, "start", 700))
        parts.append(self.t(tx + tw - 22, ty + 42, fp, 17, self.panel_sub, "end", 600))
        parts.append(self.rule(tx + 22, tx + tw - 22, ty + 60))
        for k, (lab, v, ok) in enumerate(rows_t):
            ry = ty + 92 + k * 32
            if lab == "Cost":
                parts.append(self.coin_small(tx + 32, ry - 6))
                parts.append(self.t(tx + 50, ry, lab, 18, self.panel_sub, "start", 600))
            else:
                parts.append(self.t(tx + 50, ry, lab, 18, self.panel_sub, "start", 600))
            parts.append(self.t(tx + tw - 22, ry, v, 18, self.panel_text if ok else (self.short_col or self.accent), "end", 700))
        short = [m for m, n in mats.items() if STOCK[m] < n]
        note = "Not enough bricks in storage" if short else "Click, then place it on the map"
        parts.append(self.t(tx + 22, ty + th - 22, note, 16, (self.short_col or self.accent) if short else self.panel_sub, "start", 600, italic=True))
        self.page(f"build-palette-{realm}", parts)


class DaiLongPalette(Palette, D.DaiLongDialogs):
    style_line = "seal stamps in vermilion ink on paper"
    notch_fill = "#efe0bc"


class KambaraPalette(Palette, D.KambaraDialogs):
    style_line = "reliefs carved in sandstone"
    notch_fill = "#c4b796"


class TampadesaPalette(Palette, D.TampadesaDialogs):
    style_line = "gold leaf on red lacquer, in arched niches"
    notch_fill = "#5b3d24"
    short_col = "#f08a5a"


REALMS = [(DaiLongPalette, DC.DAILONG), (KambaraPalette, DC.KAMBARA), (TampadesaPalette, DC.TAMPADESA)]
SCREENS = ["build_icons", "build_palette"]

if __name__ == "__main__":
    import sys
    which = sys.argv[1:] or SCREENS
    for cls, data in REALMS:
        for screen in which:
            getattr(cls(data), screen)()
