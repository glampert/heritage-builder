"""Renders the build palette's icon mockups for each realm into build/:

- build-icons-<realm>: every building's icon, by sector, and the four states
  (idle, hovered, pressed, disabled);
- build-palette-<realm>: the HUD in the realm's icon style (speed, goals and
  game-menu buttons, the sector column, the minimap's buttons), one sector open
  as a grid of icons left of the column, and the hovered icon's tooltip with its
  name and cost;
- hud-goals-<realm>: the same HUD at rest, with the goals panel open under its
  button.

Each realm treats the same pictograms its own way: Đại Long stamps them in
vermilion seal ink on paper, Kambara carves them in relief in sandstone, and
Tampadesa gilds them on red lacquer in an arched niche. The pictograms are in
palette_glyphs.py, drawn in each realm's architecture; Kambara's carving, after
the user's references, is in kambara_carving.py.
"""
import base64

from world_map import T, BUILD
import dialogs as D
import screens_content as SC
import kambara_carving as KC
from minimap import GLYPHS as MM_GLYPHS
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
def hud_glyph(key):
    """The HUD's buttons, in the pictograms' 100-unit box."""
    P, R, Ci, E = G.P, G.R, G.Ci, G.E
    if key == "pause":
        return [R(-20, -28, 14, 56, rx=3), R(6, -28, 14, 56, rx=3)]
    if key == "play":
        return [P("M-18,-30 L28,0 L-18,30Z")]
    if key == "fast":
        return [P("M-34,-26 L2,0 L-34,26Z"), P("M2,-26 L38,0 L2,26Z")]
    if key == "faster":
        return [P("M-42,-22 L-14,0 L-42,22Z"), P("M-14,-22 L14,0 L-14,22Z"), P("M14,-22 L42,0 L14,22Z")]
    if key == "goals":  # a banner on its pole, a tick cut in it
        return [P("M-26,40 V-38", True, 6), P("M-23,-36 H30 L20,-18 L30,0 H-23Z"), P("M-12,-18 L-3,-9 L14,-27", False, 5), G.Ci(-26, -40, 5)]
    if key == "menu":
        return [R(-30, -26, 60, 10, rx=4), R(-30, -5, 60, 10, rx=4), R(-30, 16, 60, 10, rx=4)]
    if key == "save":  # a lacquered chest with a lock
        return [P("M-34,-8 Q-34,-30 0,-30 Q34,-30 34,-8Z"), R(-34, -4, 68, 36, rx=3), P("M-34,-6 H34", False, 3),
                R(-7, -12, 14, 16, False, 2), R(-4, -9, 8, 9), P("M-24,6 V26 M24,6 V26", False, 2.4)]
    if key == "gear":
        teeth = "".join(f'<rect x="-6" y="-38" width="12" height="16" rx="2" fill="S" transform="rotate({a})"/>' for a in range(0, 360, 45))
        return [teeth, Ci(0, 0, 25), Ci(0, 0, 10, False)]
    if key == "book":  # the City book: a bound book with clasps
        return [R(-28, -34, 56, 68, rx=4), R(-28, -34, 9, 68, False), R(-26, -34, 5, 68), P("M-10,-18 H18 M-10,-6 H18 M-10,6 H12", False, 4),
                R(22, -8, 10, 16, rx=2)]
    if key == "briefing":  # a folded letter under a round seal
        return [R(-36, -24, 72, 50, rx=3), P("M-36,-22 L0,6 L36,-22", False, 3.4), Ci(0, 10, 12), P("M-5,6 L0,13 L6,5", False, 2.6)]
    if key == "scroll":  # rolled at both ends, lines of writing between
        return [R(-24, -26, 48, 52), E(-26, 0, 9, 30), E(26, 0, 9, 30), E(26, 0, 4, 22, False),
                P("M-14,-14 H14 M-14,-4 H14 M-14,6 H10 M-14,16 H4", False, 3.4)]
    if key in MM_GLYPHS:  # the minimap's line icons, thickened for the tiles
        return [f'<path d="{MM_GLYPHS[key]}" transform="scale(2.9)" fill="none" stroke="S" stroke-width="{9 / 2.9:.2f}" stroke-linecap="round" stroke-linejoin="round"/>']
    raise KeyError(key)


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
    if key == "services":  # an old rolled scroll, its edicts on it
        return hud_glyph("scroll")
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




HUD_DATA = {s["key"]: s["hud"] for s in (SC.DAILONG, SC.KAMBARA, SC.TAMPADESA)}
MENU_BUTTONS = [("menu", "Game"), ("save", "Load / Save"), ("gear", "Settings"), ("book", "City book"), ("briefing", "Briefing")]
# The minimap's buttons as the game lays them out: the lenses right-aligned across
# the top (the landmarks toggle first), the tools down the right.
MM_LENSES = [("pin", "Landmarks"), ("fire", "Risk"), ("water", "Water"), ("sectors", "Sectors"), ("city", "City")]
MM_TOOLS = [("plus", "Zoom in"), ("minus", "Zoom out"), ("alert", "Alerts"), ("home", "Home"), ("world", "World map"), ("collapse", "Hide")]


class Palette:
    """The icon treatments and the restyled HUD, mixed into each realm's Dialogs class."""
    short_col = None
    crest_k = 0.0          # Kambara's tiles carry a crest above their top, this much of their width
    sector_size = 62

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
        ink, ink_small, gild, shadow = (self.pid(n) for n in ("ink", "inks", "gild", "shadow"))
        self.defs.append(
            # Seal ink: speckled where the stamp missed the paper, edges a little ragged.
            f'<filter id="{ink}" x="-5%" y="-5%" width="110%" height="110%">'
            f'<feTurbulence type="fractalNoise" baseFrequency="0.9" numOctaves="2" seed="7" result="n"/>'
            f'<feDisplacementMap in="SourceGraphic" in2="n" scale="1.6" result="d"/>'
            f'<feTurbulence type="fractalNoise" baseFrequency="0.45" numOctaves="3" seed="3" result="n2"/>'
            f'<feColorMatrix in="n2" type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  -3.2 0 0 0 2.65" result="a"/>'
            f'<feComposite in="d" in2="a" operator="in"/></filter>'
            # The same, lighter, for small buttons, so their icons stay whole.
            f'<filter id="{ink_small}" x="-5%" y="-5%" width="110%" height="110%">'
            f'<feTurbulence type="fractalNoise" baseFrequency="0.7" numOctaves="2" seed="3" result="n2"/>'
            f'<feColorMatrix in="n2" type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  -2.0 0 0 0 2.05" result="a"/>'
            f'<feComposite in="SourceGraphic" in2="a" operator="in"/></filter>'
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
        self._filters = dict(ink=ink, ink_small=ink_small, gild=gild, shadow=shadow, gold=gold, lacq=lacq, goldbg=goldbg)
        return self._filters

    # ------------------------------------------------------------ one icon
    def tile_height(self, size, small=False):
        return size * 1.08 if (self.crest_k and not small) else size

    def tile_crest(self, size, small=False, crest_k=None):
        k = self.crest_k if crest_k is None else crest_k
        return 0 if small else size * k

    def icon_tile(self, x, y, size, elements, state="idle", small=False, crest_k=None):
        """An icon tile with its top-left corner at (x, y), in the realm's treatment;
        small ones (the HUD's and the minimap's buttons) keep less ornament."""
        return getattr(self, f"tile_{self.dd['key']}")(x, y, size, elements, state, small, crest_k)

    def tile_dailong(self, x, y, size, els, state, small, crest_k):
        fl = self.filters()
        s, cx, cy = size / 100 * (0.84 if small else 0.78), x + size / 2, y + size / 2 + size * 0.02
        out = []
        lift = state == "hover"
        card = "#f8eed6" if lift else "#f1e3c3"
        out.append(f'<g filter="url(#{fl["shadow"]})"><rect x="{x}" y="{y - (2 if lift else 0)}" width="{size}" height="{size}" rx="{3 if small else 5}" fill="{card}" '
                   f'stroke="{"#a8261c" if lift else "#c9b083"}" stroke-width="{2 if lift else 1.2}"/></g>')
        if lift:
            y -= 2
            cy -= 2
        ink = "#a8261c" if state != "disabled" else "#8d7b62"
        m = size * (0.07 if small else 0.08)
        sx, sy, sw = x + m, y + m, size - 2 * m
        grain = fl["ink_small" if small else "ink"]
        if state == "pressed":  # the seal cut the other way: red ground, the building left bare
            mask = self.mask_of(els, cx, cy, s * (0.9 if small else 0.98), inverse=True, box=(sx, sy, sw, sw, 3))
            out.append(f'<g filter="url(#{grain})"><rect x="{sx}" y="{sy}" width="{sw}" height="{sw}" rx="3" fill="#b3271b" mask="{mask}"/></g>')
        else:
            mask = self.mask_of(els, cx, cy, s * (0.9 if small else 0.95))
            op = 0.55 if state == "disabled" else 1
            inner = "" if small else (f'<rect x="{sx + size * 0.065}" y="{sy + size * 0.065}" width="{sw - size * 0.13}" height="{sw - size * 0.13}" rx="3" '
                                      f'fill="none" stroke="{ink}" stroke-width="{size * 0.014}"/>')
            out.append(f'<g filter="url(#{grain})" opacity="{op}">'
                       f'<rect x="{sx}" y="{sy}" width="{sw}" height="{sw}" rx="{3 if small else 6}" fill="none" stroke="{ink}" stroke-width="{max(1.6, size * 0.045)}"/>'
                       f'{inner}<rect x="{x}" y="{y}" width="{size}" height="{size}" fill="{ink}" mask="{mask}"/></g>')
        return "".join(out)

    def tile_kambara(self, x, y, size, els, state, small, crest_k):
        carve = self.carving()
        cy = y + size * (0.5 if small else 0.56)
        mask = self.mask_of(els, x + size / 2, cy, size / 100 * (0.84 if small else 0.78))
        k = self.crest_k if crest_k is None else crest_k
        markup, _ = carve.tile(x, y, size, mask, state, small=small, crest=None if small else size * k, seed=int(x + y) % 97)
        return markup

    def tile_tampadesa(self, x, y, size, els, state, small, crest_k):
        fl = self.filters()
        s, cx, cy = size / 100 * (0.8 if small else 0.74), x + size / 2, y + size / 2 + size * (0.05 if small else 0.07)
        arch = size * (0.16 if small else 0.22)

        def niche(ix, iy, w, h, a):
            return (f"M{ix},{iy + h} V{iy + a} Q{ix},{iy + a * 0.35} {ix + w * 0.32},{iy + a * 0.25} Q{ix + w / 2},{iy - a * 0.15} {ix + w / 2},{iy - a * 0.3} "
                    f"Q{ix + w / 2},{iy - a * 0.15} {ix + w * 0.68},{iy + a * 0.25} Q{ix + w},{iy + a * 0.35} {ix + w},{iy + a} V{iy + h}Z")
        outer = niche(x, y + arch * 0.3, size, size - arch * 0.3, arch)
        g = size * (0.06 if small else 0.07)
        inner = niche(x + g, y + arch * 0.3 + g * 1.2, size - 2 * g, size - arch * 0.3 - 2.2 * g, arch * 0.85)
        lift = state == "hover"
        frame = f"url(#{fl['gold']})" if state != "disabled" else "#6e5a3c"
        out = [f'<g filter="url(#{fl["shadow"]})"><path d="{outer}" fill="{frame}" stroke="#3a1a08" stroke-width="1.2"/></g>']
        ground = f"url(#{fl['goldbg']})" if state == "pressed" else (f"url(#{fl['lacq']})" if state != "disabled" else "#3e1c14")
        out.append(f'<path d="{inner}" fill="{ground}" stroke="#3a1a08" stroke-width="1"/>')
        out.append(f'<path d="{inner}" fill="none" stroke="{"#f7dc8a" if lift else "#b98432"}" stroke-width="{1.6 if lift else 0.8}"/>')
        if lift:
            out.append(f'<path d="{outer}" fill="none" stroke="#ffe8a0" stroke-width="2.4" opacity="0.9"/>')
        mask = self.mask_of(els, cx, cy, s)
        if state == "pressed":
            out.append(f'<rect x="{x}" y="{y}" width="{size}" height="{size}" fill="#5e1810" mask="{mask}"/>')
        elif state == "disabled":  # tarnished, the leaf worn off
            out.append(f'<rect x="{x}" y="{y}" width="{size}" height="{size}" fill="#6a5034" opacity="0.7" mask="{mask}"/>')
        else:
            out.append(f'<g filter="url(#{fl["gild"]})"><rect x="{x}" y="{y}" width="{size}" height="{size}" fill="url(#{fl["gold"]})" mask="{mask}"/></g>')
        out.append(f'<circle cx="{cx}" cy="{y + 1}" r="{size * 0.028}" fill="{frame}" stroke="#3a1a08" stroke-width="0.8"/>')
        return "".join(out)

    def carving(self):
        if getattr(self, "_carving", None) is None:
            self._carving = KC.Carving(self.defs, self.pid)
        return self._carving

    def badge(self, x, y, text, r=11):
        return (f'<circle cx="{x}" cy="{y}" r="{r}" fill="#b3271b" stroke="#fff4dc" stroke-width="1.6"/>'
                + T(x, y + r * 0.42, text, "sans-serif", r * 1.15, "#fff", "middle", 800))

    # ------------------------------------------------------------ the HUD
    def notch(self, x, y, d):
        """A small triangle on a panel's edge at (x, y), pointing d (up or right)."""
        p = f"M{x - 12},{y + 1} l12,-14 l12,14Z" if d == "up" else f"M{x - 1},{y - 12} l14,12 l-14,12Z"
        return f'<path d="{p}" fill="{self.notch_fill}" stroke="{self.panel_rule}" stroke-width="1.5"/>'

    def sector_column(self, open_key=None):
        """The sector column at the right edge; returns its markup, left edge and the open button's top."""
        size = self.sector_size
        crest = self.tile_crest(size, crest_k=self.crest_k * 0.7)
        step = self.tile_height(size) + crest + (8 if self.crest_k else 8)
        sep = 16
        cw = size + 28
        cx0 = W - 12 - cw
        n = sum(1 for s in SECTORS if s)
        ch = 26 + n * step + 2 * sep
        out = [self.box(cx0, 10, cw, ch)]
        yy = 26 + crest
        open_y = None
        for s in SECTORS:
            if s is None:
                out.append(f'<line x1="{cx0 + 18}" y1="{yy + sep / 2 - 6}" x2="{cx0 + cw - 18}" y2="{yy + sep / 2 - 6}" stroke="{self.panel_rule}" stroke-width="2"/>')
                yy += sep
                continue
            key = s[0]
            st = "pressed" if key == open_key else ("disabled" if key == "trade" else "idle")
            out.append(self.icon_tile(cx0 + 14, yy, size, sector_glyph(key, self.dd["key"]), st, crest_k=self.crest_k * 0.7))
            if key == open_key:
                open_y = yy
            yy += step
        return "".join(out), cx0, open_y

    def top_left(self, goals_open):
        """Speed buttons, the date, and the goals button; returns the markup and the goals button's centre."""
        u = self.hud
        x, y, w, h = 12, 10, 590, 74
        out = [self.box(x, y, w, h)]
        bs = 44
        for k, ic in enumerate(["pause", "play", "fast", "faster"]):
            out.append(self.icon_tile(x + 16 + k * 52, y + 15, bs, hud_glyph(ic), "pressed" if k == u["speed"] else "idle", small=True))
        out.append(T(x + 238, y + 33, u["date"], self.body_font, 18, self.panel_text, "start", 700))
        out.append(T(x + 238, y + 57, u["season"], self.body_font, 17, self.panel_sub, "start", 600, style="font-style:italic"))
        gx = x + w - 16 - bs
        done = sum(1 for g in u["goals"] if g[2] >= 1)
        out.append(f'<line x1="{gx - 16}" y1="{y + 16}" x2="{gx - 16}" y2="{y + h - 16}" stroke="{self.panel_rule}" stroke-width="2"/>')
        out.append(self.icon_tile(gx, y + 15, bs, hud_glyph("goals"), "pressed" if goals_open else "idle", small=True))
        out.append(self.badge(gx + bs - 2, y + 17, f"{done}/{len(u['goals'])}", 13))
        return "".join(out), (gx + bs / 2, y + 15 + bs)

    def goals_panel(self, bx, by):
        """The goals, dropped down under their button."""
        u = self.hud
        gw = 440
        gh = 64 + len(u["goals"]) * 66 + 10
        gx, gy = bx + 40 - gw, by + 22
        out = [self.box(gx, gy, gw, gh), self.notch(bx, gy, "up")]
        out.append(self.section_title(gx + 26, gy + 42, "Goals"))
        out.append(T(gx + gw - 26, gy + 42, u["goals_title"], self.body_font, 19, self.panel_sub, "end", 600, style="font-style:italic"))
        for k, (g, txt, v) in enumerate(u["goals"]):
            ry = gy + 86 + k * 66
            out.append(self.check(gx + 38, ry - 7, True) if v >= 1 else f'<circle cx="{gx + 38}" cy="{ry - 7}" r="11" fill="none" stroke="{self.accent}" stroke-width="2"/>')
            out.append(T(gx + 62, ry, g, self.body_font, 20, self.panel_text, "start", 700))
            out.append(T(gx + gw - 26, ry, txt, self.body_font, 18, self.panel_sub, "end", 600))
            out.append(self.bar(gx + 62, ry + 14, gw - 88, 8, v))
        return "".join(out)

    def top_centre(self):
        u = self.hud
        x0, w0 = 618, 790
        out = [self.box(x0, 10, w0, 74)]
        for k, (ic, v) in enumerate([("population", u["population"]), ("coin", u["treasury"]), ("food", u["food"])]):
            ix = x0 + 40 + k * 150
            out.append(self.icon(ic, ix, 47, 1.0, self.accent))
            out.append(T(ix + 22, 56, v, self.title_font, 24, self.panel_text, "start", 700))
        sx = x0 + 480
        out.append(f'<line x1="{sx - 20}" y1="26" x2="{sx - 20}" y2="68" stroke="{self.panel_rule}" stroke-width="2"/>')
        for k, (lab, v) in enumerate(zip(["P", "C", "S", u["legitimacy"][0]], u["ratings"])):
            out.append(self.rating_badge(sx + k * 78 + 18, 47, lab, v))
        return "".join(out)

    def menu_bar(self, right):
        bs = 44
        bw = 24 + len(MENU_BUTTONS) * 52 - 8
        bx = right - bw
        out = [self.box(bx, 10, bw, 74)]
        for k, (ic, _) in enumerate(MENU_BUTTONS):
            out.append(self.icon_tile(bx + 12 + k * 52, 25, bs, hud_glyph(ic), small=True))
        return "".join(out)

    def minimap_widget(self, lens_on="city", alerts=2):
        """The minimap as the game lays it out: the map, the lenses right-aligned over it, the tools down the right."""
        from minimap import Minimap
        btn, gap, pad, mw = 30, 8, 14, 300
        mh = mw / 2
        fw = pad * 2 + mw + gap + btn
        fh = pad * 2 + btn + gap + mh
        x, y = 12, H - 12 - fh
        out = [self.box(x, y, fw, fh)]
        mx, my = x + pad, y + pad + btn + gap
        mm = Minimap(self.defs, prefix=self.pid("mm"))
        out.append(mm.painted(mx, my, mw, palette=self.mm_palette, k=0.62))
        right = mx + mw
        for k, (key, _) in enumerate(reversed(MM_LENSES)):
            bx = right - btn - k * (btn + gap)
            out.append(self.icon_tile(bx, y + pad, btn, hud_glyph(key), "pressed" if key == lens_on else "idle", small=True))
        cx = right + gap
        step = (btn + gap + mh - btn) / (len(MM_TOOLS) - 1)
        for k, (key, _) in enumerate(MM_TOOLS):
            ty = y + pad + k * step
            out.append(self.icon_tile(cx, ty, btn, hud_glyph(key), small=True))
            if key == "alert" and alerts:
                out.append(self.badge(cx + btn - 1, ty + 2, str(alerts), 9))
        return "".join(out), x + fw

    def messages(self, left):
        u = self.hud
        mx, mw = left + 14, 1290 - left - 14
        out = [self.box(mx, 950, mw, 118)]
        for k, (ic, msg) in enumerate(u["messages"]):
            yy = 984 + k * 32
            out.append(self.icon(ic, mx + 34, yy - 6, 0.7, self.accent))
            out.append(T(mx + 58, yy, msg, self.body_font, 19, self.panel_text if k == 0 else self.panel_sub, "start", 700 if k == 0 else 600))
        out.append(T(mx + mw - 24, 984, "now", self.body_font, 16, self.panel_sub, "end", 500, style="font-style:italic"))
        return "".join(out)

    def hud_v2(self, open_sector=None, goals_open=False):
        """The HUD with every button in the realm's icon style. Returns parts and the sector column's left edge and open button."""
        self.hud = HUD_DATA[self.dd["key"]]
        img = base64.b64encode((BUILD / f"city_{self.dd['key']}.jpg").read_bytes()).decode()
        parts = [f'<image x="0" y="0" width="{W}" height="{H}" xlink:href="data:image/jpeg;base64,{img}" preserveAspectRatio="xMidYMid slice"/>']
        tl, goals_btn = self.top_left(goals_open)
        parts.append(tl)
        parts.append(self.top_centre())
        col, cx0, open_y = self.sector_column(open_sector)
        parts.append(self.menu_bar(cx0 - 14))
        parts.append(col)
        mm, mm_right = self.minimap_widget()
        parts.append(mm)
        parts.append(self.messages(mm_right))
        if goals_open:
            parts.append(self.goals_panel(*goals_btn))
        return parts, cx0, open_y

    # ------------------------------------------------------------ pages
    def building_name(self, b):
        return REALM_NAMES[self.dd["key"]].get(b, G.BUILDINGS[b][0])

    def build_palette(self, open_key="industry", hover_b="distillery", disabled=("mine",)):
        realm = self.dd["key"]
        parts, cx0, open_y = self.hud_v2(open_key)
        key, title, sub, items = next(s for s in SECTORS if s and s[0] == open_key)
        isz, ig, per_row = 84, 12, 4
        crest = self.tile_crest(isz)
        cell_h = self.tile_height(isz) + crest
        rows = (len(items) + per_row - 1) // per_row
        pw = 2 * 22 + per_row * isz + (per_row - 1) * ig
        ph = 64 + crest * 0.25 + rows * cell_h + (rows - 1) * ig + 22
        px = cx0 - 16 - pw
        py = open_y - 52 - crest
        parts.append(self.box(px, py, pw, ph))
        parts.append(self.notch(px + pw - 1, open_y + self.sector_size / 2, "right"))
        parts.append(self.section_title(px + 22, py + 40, title))
        hover_xy = None
        for k, b in enumerate(items):
            r, c = divmod(k, per_row)
            ix, iy = px + 22 + c * (isz + ig), py + 60 + crest * 1.25 + r * (cell_h + ig)
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
        parts.append(self.notch(hover_xy[0] + isz / 2, ty, "up"))
        parts.append(T(tx + 22, ty + 42, self.building_name(hover_b), self.title_font, 25, self.panel_text, "start", 700))
        parts.append(self.t(tx + tw - 22, ty + 42, fp, 17, self.panel_sub, "end", 600))
        parts.append(self.rule(tx + 22, tx + tw - 22, ty + 60))
        for k, (lab, v, ok) in enumerate(rows_t):
            ry = ty + 92 + k * 32
            if lab == "Cost":
                parts.append(self.coin_small(tx + 32, ry - 6))
            parts.append(self.t(tx + 50, ry, lab, 18, self.panel_sub, "start", 600))
            parts.append(self.t(tx + tw - 22, ry, v, 18, self.panel_text if ok else (self.short_col or self.accent), "end", 700))
        short = [m for m, n in mats.items() if STOCK[m] < n]
        note = "Not enough bricks in storage" if short else "Click, then place it on the map"
        parts.append(self.t(tx + 22, ty + th - 22, note, 16, (self.short_col or self.accent) if short else self.panel_sub, "start", 600, italic=True))
        self.page(f"build-palette-{realm}", parts)

    def hud_goals(self):
        parts, *_ = self.hud_v2(None, goals_open=True)
        self.page(f"hud-goals-{self.dd['key']}", parts)

    def build_icons(self):
        realm = self.dd["key"]
        parts, *_ = self.hud_v2()
        parts.append(self.dim(0.55))
        x, y, w, h = 50, 30, W - 100, H - 60
        parts.append(self.box(x, y, w, h))
        parts.append(self.section_title(x + 40, y + 52, "Build palette · icons"))
        parts.append(self.t(x + w - 40, y + 52, f"{self.dd['realm']}: {self.style_line}", 19, self.panel_sub, "end", 600, italic=True))
        size, gap = (80, 112) if self.crest_k else (84, 112)
        crest = self.tile_crest(size)
        cell = self.tile_height(size) + crest
        bx, by = x + 40, y + 84
        col_w = (w - 80) / 2
        blocks = [s for s in SECTORS if s and s[3]]
        cols = [blocks[0:1] + blocks[2:3] + blocks[3:4], blocks[1:2] + blocks[4:6]]
        for ci, col in enumerate(cols):
            yy = by
            for key, title, sub, items in col:
                xx = bx + ci * col_w
                parts.append(self.t(xx, yy, title, 20, self.accent, "start", 700))
                for k, b in enumerate(items):
                    ix, iy = xx + k * gap, yy + 14 + crest
                    parts.append(self.icon_tile(ix, iy, size, G.glyph(b, realm)))
                    for j, line in enumerate(self.wrap(self.building_name(b), gap - 8, 15, 0.48)[:2]):
                        parts.append(self.t(ix + size / 2, iy + self.tile_height(size) + 20 + j * 17, line, 15, self.panel_text, "middle", 600))
                yy += 14 + cell + 62
        # The sector column's icons, the HUD's buttons, the minimap's, and the four states.
        sy = by + 3 * (14 + cell + 62) - 8
        parts.append(self.t(bx, sy, "Sector buttons", 20, self.accent, "start", 700))
        ssz, sg = 58, 70
        sc = self.tile_crest(ssz, crest_k=self.crest_k * 0.7)
        for k, sct in enumerate(s for s in SECTORS if s):
            ix = bx + k * sg
            parts.append(self.icon_tile(ix, sy + 14 + sc, ssz, sector_glyph(sct[0], realm), crest_k=self.crest_k * 0.7))
            parts.append(self.t(ix + ssz / 2, sy + 14 + sc + self.tile_height(ssz) + 20, SHORT.get(sct[0], sct[1]), 14, self.panel_text, "middle", 600))
        hy = sy + 14 + sc + self.tile_height(ssz) + 56
        parts.append(self.t(bx, hy, "HUD buttons", 20, self.accent, "start", 700))
        hud_items = [("pause", "Pause"), ("play", "Play"), ("fast", "Fast"), ("faster", "Faster"), ("goals", "Goals")] + MENU_BUTTONS
        for k, (ic, lab) in enumerate(hud_items):
            ix = bx + k * 70
            parts.append(self.icon_tile(ix + 7, hy + 14, 44, hud_glyph(ic), small=True))
            parts.append(self.t(ix + 29, hy + 78, lab, 14, self.panel_text, "middle", 600))
        mx = bx + col_w
        parts.append(self.t(mx, hy, "Minimap buttons, at their size", 20, self.accent, "start", 700))
        for k, (ic, lab) in enumerate(MM_LENSES + MM_TOOLS):
            ix = mx + k * 52
            parts.append(self.icon_tile(ix + 11, hy + 20, 30, hud_glyph(ic), "pressed" if ic == "city" else "idle", small=True))
        stx = mx
        parts.append(self.t(stx, sy, "States", 20, self.accent, "start", 700))
        for k, st in enumerate(["idle", "hover", "pressed", "disabled"]):
            parts.append(self.icon_tile(stx + k * 100, sy + 14 + sc, ssz, G.glyph("temple", realm), st, crest_k=self.crest_k * 0.7))
            parts.append(self.t(stx + k * 100 + ssz / 2, sy + 14 + sc + self.tile_height(ssz) + 20, {"hover": "hovered"}.get(st, st), 14, self.panel_text, "middle", 600))
        for k, st in enumerate(["idle", "hover", "pressed", "disabled"]):
            parts.append(self.icon_tile(stx + 420 + k * 56, sy + 14 + sc + 6, 44, hud_glyph("gear"), st, small=True))
        self.page(f"build-icons-{realm}", parts)


class DaiLongPalette(Palette, D.DaiLongDialogs):
    style_line = "seal stamps in vermilion ink on paper"
    notch_fill = "#efe0bc"


class KambaraPalette(Palette, D.KambaraDialogs):
    style_line = "carved sandstone, moss and vines, a Bayon face over each"
    notch_fill = "#7c5f3a"
    crest_k = 0.34
    sector_size = 54

    def __init__(self, dd):
        D.KambaraDialogs.__init__(self, dd)
        # Light text on the carved frames' dark stone.
        self.panel_text, self.panel_sub, self.panel_rule, self.accent = "#efe3c6", "#c9b690", "#7e6a4c", "#e6a560"

    def box(self, x, y, w, h, big=False):
        big = w >= 300 and h >= 220 and y > 120
        return self.carving().frame(x, y, w, h, t=11 if h < 120 else 13, crest=46 if big else None, seed=int(x * 7 + y) % 89,
                                    vines=True, small=False)

    def section_title(self, x, y, s):
        return T(x, y, s.upper(), self.title_font, 20, self.accent, "start", 700, style="letter-spacing:2px")


class TampadesaPalette(Palette, D.TampadesaDialogs):
    style_line = "gold leaf on red lacquer, in arched niches"
    notch_fill = "#5b3d24"
    short_col = "#f08a5a"

    def __init__(self, dd):
        D.TampadesaDialogs.__init__(self, dd)
        # The HUD's earthen brown panels with gilt trim, and light text (screens.py).
        self.box_style = "lacquer"
        self.panel_text, self.panel_sub, self.panel_rule = "#f3e2bf", "#d9b98a", "#8a6a48"
        self.accent, self.btn_fill, self.btn_fill_hover = "#e9c977", "#4e3826", "#634831"


REALMS = [(DaiLongPalette, DC.DAILONG), (KambaraPalette, DC.KAMBARA), (TampadesaPalette, DC.TAMPADESA)]
SCREENS = ["build_icons", "build_palette", "hud_goals"]

if __name__ == "__main__":
    import sys
    which = sys.argv[1:] or SCREENS
    for cls, data in REALMS:
        for screen in which:
            getattr(cls(data), screen)()
