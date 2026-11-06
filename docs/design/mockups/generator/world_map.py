"""Renders the three world map mockups as HTML pages (one SVG each) into build/."""
import math
import pathlib
import random
from xml.sax.saxutils import escape

import common as C
from common import P, f, smooth, roughen, mp
import content

HERE = pathlib.Path(__file__).parent
BUILD = HERE / "build"
FONTS_CSS = (BUILD / "fonts.css").read_text()


def T(x, y, s, font, size, fill, anchor="start", weight=None, ls=None, extra="", style=""):
    a = [f'x="{f(x)}" y="{f(y)}"', f'font-family="{font}"', f'font-size="{size}"', f'fill="{fill}"']
    if anchor != "start":
        a.append(f'text-anchor="{anchor}"')
    if weight:
        a.append(f'font-weight="{weight}"')
    if ls is not None:
        a.append(f'letter-spacing="{ls}"')
    if style:
        a.append(f'style="{style}"')
    return f"<text {' '.join(a)} {extra}>{escape(s)}</text>"


class Theme:
    title_font = "serif"
    label_font = "serif"
    body_font = "serif"
    ink = "#222"
    goods_size = 19
    sub_size = 24
    label_pos = {}

    def __init__(self, d):
        self.d = d
        self.defs = []
        self.n = 0

    # ------------------------------------------------------------ helpers
    def nid(self, p):
        self.n += 1
        return f"{p}{self.n}"

    def noise(self, x, y, w, h, freq, octaves, rgb, ak, ab, seed=1, extra="", typ="fractalNoise"):
        fid = self.nid("nz")
        r, g, b = rgb
        self.defs.append(
            f'<filter id="{fid}" filterUnits="userSpaceOnUse" x="{x}" y="{y}" width="{w}" height="{h}">'
            f'<feTurbulence type="{typ}" baseFrequency="{freq}" numOctaves="{octaves}" seed="{seed}"/>'
            f'<feColorMatrix type="matrix" values="0 0 0 0 {r} 0 0 0 0 {g} 0 0 0 0 {b} {ak} 0 0 0 {ab}"/>'
            f"</filter>")
        return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" filter="url(#{fid})" {extra}/>'

    def lin(self, stops, x1=0, y1=0, x2=0, y2=1):
        gid = self.nid("lg")
        s = "".join(f'<stop offset="{o}" stop-color="{c}"/>' for o, c in stops)
        self.defs.append(f'<linearGradient id="{gid}" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}">{s}</linearGradient>')
        return f"url(#{gid})"

    def rad(self, stops, cx=0.5, cy=0.5, r=0.5):
        gid = self.nid("rg")
        s = "".join(f'<stop offset="{o}" stop-color="{c}" stop-opacity="{a}"/>' for o, c, a in stops)
        self.defs.append(f'<radialGradient id="{gid}" cx="{cx}" cy="{cy}" r="{r}">{s}</radialGradient>')
        return f"url(#{gid})"

    # ------------------------------------------------------------ geometry
    def land_path(self):
        return smooth(mp(roughen(C.LAND, 3.5, 11, sub=3)), closed=True, tension=0.6)

    def island_paths(self):
        return [smooth(mp(roughen(i, 1.2, 5 + k, sub=2)), closed=True) for k, i in enumerate(C.ISLANDS)]

    def lake_path(self):
        L = C.LAKE
        pts = []
        rnd = random.Random(3)
        a0 = math.radians(L["rot"])
        for i in range(18):
            t = i / 18 * 2 * math.pi
            rx, ry = L["rx"] * rnd.uniform(0.92, 1.06), L["ry"] * rnd.uniform(0.9, 1.1)
            x, y = rx * math.cos(t), ry * math.sin(t)
            pts.append(P(L["cx"] + x * math.cos(a0) - y * math.sin(a0), L["cy"] + x * math.sin(a0) + y * math.cos(a0)))
        return smooth(pts, closed=True)

    def river_path(self, key):
        return smooth(mp(roughen(C.RIVERS[key], 2.0, len(key), sub=2, closed=False)))

    # ------------------------------------------------------------ page
    def render(self):
        body = []
        body.append(self.background())
        body.append(self.frame_back())
        body.append(f'<g clip-path="url(#mapclip)">{self.map_layer()}</g>')
        body.append(self.frame_front())
        body.append(self.topbar())
        body.append(self.panel())
        body.append(self.legend())
        self.defs.append(f'<clipPath id="mapclip"><rect x="{C.MAP_X}" y="{C.MAP_Y}" width="{f(C.MAP_W)}" height="{f(C.MAP_H)}"/></clipPath>')
        svg = (f'<svg xmlns="http://www.w3.org/2000/svg" width="{C.W}" height="{C.H}" viewBox="0 0 {C.W} {C.H}">'
               f'<defs>{"".join(self.defs)}</defs>{"".join(body)}</svg>')
        return (f'<!doctype html><html><head><meta charset="utf-8"><title>World map: {self.d["key"]}</title>'
                f'<style>{FONTS_CSS} html,body{{margin:0;padding:0;background:#000}} svg{{display:block;font-synthesis:style}}</style>'
                f'</head><body>{svg}</body></html>')

    def map_layer(self):
        d = self.d
        out = [self.sea()]
        land = self.land_path()
        isl = self.island_paths()
        out.append(self.land(land, isl))
        out.append(self.terrain_decor())
        if d.get("unknown"):
            out.append(self.unknown_area(*d["unknown"]))
        for k in C.RIVERS:
            w = 4.5 if k in ("son", "great", "shwemyit") else 3
            out.append(self.river(self.river_path(k), w))
        out.append(self.lake(self.lake_path()))
        if d["key"] == "kambara":
            out.append(self.barays())
        mts = [("m", *m) for m in C.MOUNTAINS] + [("k", x, y, 1) for x, y in C.KARST] + [("h", x, y, 1) for x, y in C.HOLY_HILLS]
        mts.sort(key=lambda m: m[2])
        for kind, x, y, s in mts:
            sx, sy = P(x, y)
            out.append(self.mountain(sx, sy, s * C.MAP_S) if kind == "m" else self.karst(sx, sy) if kind == "k" else self.hill(sx, sy))
        for r in d["regions"]:
            text, x, y = C.REGIONS[r]
            sx, sy = P(x, y)
            big = r in ("dailong", "kambara", "tampadesa")
            out.append(self.region_label(text, sx, sy, big, r == d["key"]))
        if d.get("threat"):
            out.append(self.threat(*d["threat"]))
        for kind, state, pts in d["routes"]:
            out.append(self.route(kind, state, smooth(mp(pts))))
        out.append(self.sea_traders())
        for name, x, y, realm in d["foreign"]:
            sx, sy = P(x, y)
            out.append(self.foreign_city(sx, sy, name, realm, name == d["selected"]))
        for name, x, y, kind in d["player"]:
            sx, sy = P(x, y)
            out.append(self.player_city(sx, sy, name, kind, name == d["selected"]))
        for name, x, y, tag in d["choice"]:
            sx, sy = P(x, y)
            out.append(self.choice_site(sx, sy, name, tag))
        out.append(self.choice_box(*P(*d["choice_box"]), *d["choice_text"], [P(x, y) for _, x, y, _ in d["choice"]]))
        out.append(self.compass(*P(958, 186)))
        return "".join(out)

    # Defaults that themes can override ----------------------------------
    def terrain_decor(self):
        return ""

    def barays(self):
        out = []
        for (x, y, w, h) in [(486, 392, 34, 9), (566, 384, 26, 8)]:
            sx, sy = P(x, y)
            out.append(f'<rect x="{f(sx)}" y="{f(sy)}" width="{f(w*C.MAP_S)}" height="{f(h*C.MAP_S)}" '
                       f'fill="{self.water}" stroke="{self.water_edge}" stroke-width="1.5" rx="1"/>')
        return "".join(out)

    def threat(self, title, sub):
        return ""

    def sea_traders(self):
        return ""

    def label(self, x, y, name, size=17, anchor="middle", fill=None, font=None, weight=600, italic=False):
        st = "font-style:italic" if italic else ""
        halo = self.halo
        return (T(x, y, name, font or self.label_font, size, halo, anchor, weight, extra=f'stroke="{halo}" stroke-width="4" stroke-linejoin="round" opacity="0.85"', style=st)
                + T(x, y, name, font or self.label_font, size, fill or self.ink, anchor, weight, style=st))

    def place_label(self, x, y, name, size, dy, key=None, **kw):
        pos = self.label_pos.get(key or name, "below")
        if pos == "below":
            return self.label(x, y + dy + size * 0.8, name, size, "middle", **kw)
        if pos == "above":
            return self.label(x, y - dy - 4, name, size, "middle", **kw)
        if pos == "right":
            return self.label(x + dy + 4, y + size * 0.35, name, size, "start", **kw)
        return self.label(x - dy - 4, y + size * 0.35, name, size, "end", **kw)

    def compass(self, x, y):
        c = self.ink
        pts = []
        for i in range(8):
            a = -math.pi / 2 + i * math.pi / 4
            r = 38 if i % 2 == 0 else 20
            pts.append((x + r * math.cos(a), y + r * math.sin(a)))
            a2 = a + math.pi / 8
            pts.append((x + 7 * math.cos(a2), y + 7 * math.sin(a2)))
        poly = " ".join(f"{f(px)},{f(py)}" for px, py in pts)
        return (f'<g opacity="0.85"><circle cx="{f(x)}" cy="{f(y)}" r="28" fill="none" stroke="{c}" stroke-width="1.2"/>'
                f'<polygon points="{poly}" fill="{self.compass_fill}" stroke="{c}" stroke-width="1.2"/>'
                + T(x, y - 46, "N", self.title_font, 18, c, "middle", 700) + "</g>")

    # ------------------------------------------------------------ UI
    def topbar(self):
        d = self.d
        out = [self.bar_bg(30, 20, C.W - 60, 92)]
        out.append(self.title_text(64, 76, d["title"]))
        out.append(T(64 + self.title_w, 76, d["subtitle"], self.body_font, self.sub_size, self.bar_sub, "start", 500, style="font-style:italic"))
        x = 1150
        out.append(T(x, 58, d["date"], self.body_font, 22, self.bar_text, "end", 600))
        out.append(T(x, 86, d["season"], self.body_font, 18, self.bar_sub, "end", 500, style="font-style:italic"))
        out.append(self.coin(1206, 66))
        out.append(T(1230, 75, d["treasury"], self.title_font, 26, self.bar_text, "start", 700))
        out.append(self.button(C.PANEL_X, 38, C.PANEL_W, 56, "Return to the city", True))
        return "".join(out)

    def panel(self):
        d = self.d
        p = d["panel"]
        x, y, w, h = C.PANEL_X, 132, C.PANEL_W, int(C.MAP_H)
        out = [self.panel_bg(x, y, w, h)]
        ix = x + 32
        cy = y + 40
        out.append(self.panel_emblem(x + w / 2, cy + 46))
        cy += 148
        out.append(self.panel_title(x + w / 2, cy, p["name"]))
        cy += 32
        out.append(T(x + w / 2, cy, f'{p["sub"]} · {p["role"]}', self.body_font, 19, self.panel_sub, "middle", 500, style="font-style:italic"))
        cy += 28
        out.append(self.divider(x + 40, cy, w - 80))
        cy += 40
        out.append(T(ix, cy, "Relations", self.body_font, 19, self.panel_sub, "start", 600))
        out.append(T(x + w - 32, cy, p["relation"][0], self.body_font, 19, self.panel_text, "end", 700))
        cy += 14
        out.append(self.meter(ix, cy, w - 64, p["relation"][1]))
        cy += 46
        out.append(T(ix, cy, p["route"][0], self.body_font, 20, self.panel_text, "start", 700))
        cy += 26
        out.append(T(ix, cy, p["route"][1], self.body_font, 18, self.panel_sub, "start", 500))
        cy += 22
        out.append(self.divider(x + 40, cy, w - 80))
        cy += 40
        col2 = x + w / 2 + 12
        out.append(self.section_title(ix, cy, "They sell"))
        out.append(self.section_title(col2, cy, "They buy"))
        cy += 12
        for i in range(3):
            ry = cy + 26 + i * 40
            for (gx, (g, price)) in ((ix, p["sells"][i]), (col2, p["buys"][i])):
                out.append(self.good_bullet(gx + 8, ry - 6))
                out.append(T(gx + 24, ry, g, self.body_font, self.goods_size, self.panel_text, "start", 500))
                out.append(T(gx + w / 2 - 44, ry, price, self.body_font, self.goods_size, self.panel_text, "end", 700))
            out.append(f'<line x1="{ix}" y1="{ry + 14}" x2="{x + w - 32}" y2="{ry + 14}" stroke="{self.panel_rule}" stroke-width="1"/>')
        cy += 26 + 3 * 40 + 18
        out.append(self.request_box(ix, cy, w - 64, 92, p["request"]))
        cy += 92 + 26
        for i, b in enumerate(p["buttons"]):
            out.append(self.button(ix, cy + i * 66, w - 64, 54, b, False))
        return "".join(out)

    def legend(self):
        y = 1060
        x = 70
        out = [f'<rect x="40" y="1034" width="{f(C.MAP_W + 26)}" height="40" rx="6" fill="#000" opacity="0.45"/>']
        for kind, text in content.LEGEND:
            out.append(self.legend_glyph(kind, x, y - 6))
            out.append(T(x + 28, y, text, self.body_font, 18, self.legend_text, "start", 600))
            x += 34 + len(text) * 9.6 + 26
        return "".join(out)

    def legend_glyph(self, kind, x, y):
        if kind == "player":
            return self.player_city(x + 8, y, "", "city", False, mini=True)
        if kind == "foreign":
            return self.foreign_city(x + 8, y, "", "", False, mini=True)
        if kind == "choice":
            return self.choice_site(x + 8, y, "", "", mini=True)
        state = "closed" if kind == "closed" else "open"
        k = "land" if kind == "closed" else kind
        return self.route(k, state, f"M{x - 4},{y} L{x + 22},{y}", legend=True)


# ====================================================================== Đại Long
class DaiLong(Theme):
    title_font = C.REALM_FONTS["dailong"]["title"]
    label_font = C.REALM_FONTS["dailong"]["label"]
    body_font = C.REALM_FONTS["dailong"]["body"]
    ink = "#2b2118"
    halo = "#ead9b0"
    water = "#c9c2a2"
    water_edge = "#2b2118"
    compass_fill = "#b3261e"
    red = "#a8261c"
    title_w = 228
    bar_text = "#f3e3bd"
    bar_sub = "#d9b98a"
    panel_text = "#2b2118"
    panel_sub = "#6b4f33"
    panel_rule = "#c9b083"
    legend_text = "#f0dcb4"
    label_pos = {"Đê Son": "right", "Bến Gốm": "right", "Phù Sa": "above", "Chợ Đèo": "left",
                 "Hoa Thành": "left", "Thần Khê": "below", "Hội Thủy": "left", "Vân Cảng": "below",
                 "Gate of the North": "left"}

    def rough(self, scale=2.2, freq=0.05):
        fid = self.nid("rough")
        self.defs.append(f'<filter id="{fid}" x="-5%" y="-5%" width="110%" height="110%"><feTurbulence type="fractalNoise" baseFrequency="{freq}" numOctaves="2" seed="4"/>'
                         f'<feDisplacementMap in="SourceGraphic" scale="{scale}" xChannelSelector="R" yChannelSelector="G"/></filter>')
        return f"url(#{fid})"

    def wood(self, x, y, w, h, base="#4a2c18", seed=2):
        g = self.lin([(0, "#5b3820"), (0.5, base), (1, "#3a2213")], 0, 0, 1, 0)
        return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{g}"/>'
                + self.noise(x, y, w, h, "0.004 0.09", 3, (0.12, 0.06, 0.02), 1.1, -0.35, seed)
                + self.noise(x, y, w, h, "0.01 0.25", 2, (0.85, 0.6, 0.35), 0.5, -0.32, seed + 3))

    def background(self):
        return self.wood(0, 0, C.W, C.H, "#43281a", 1)

    def frame_back(self):
        x, y, w, h = C.MAP_X - 22, C.MAP_Y - 22, C.MAP_W + 44, C.MAP_H + 44
        return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="#2a170c" rx="6"/>'
                f'<rect x="{x + 4}" y="{y + 4}" width="{w - 8}" height="{h - 8}" fill="none" stroke="#8a5a34" stroke-width="2" rx="4"/>')

    def cloud_scroll(self, x, y, sx, sy):
        # A carved cloud-scroll corner ornament.
        p = (f"M0,0 C30,0 52,10 52,34 C52,50 40,58 30,58 C18,58 12,48 16,40 C20,32 32,34 32,42 "
             f"M52,34 C62,18 84,16 92,30 C98,42 88,52 80,48 C72,44 76,36 82,38 "
             f"M0,0 C0,30 10,52 34,52")
        t = f"translate({x},{y}) scale({sx},{sy})"
        return (f'<path d="{p}" transform="{t}" fill="none" stroke="#1d0f07" stroke-width="7" stroke-linecap="round"/>'
                f'<path d="{p}" transform="{t}" fill="none" stroke="#a87545" stroke-width="3" stroke-linecap="round"/>')

    def frame_front(self):
        x0, y0, x1, y1 = C.MAP_X, C.MAP_Y, C.MAP_X + C.MAP_W, C.MAP_Y + C.MAP_H
        out = [f'<rect x="{x0}" y="{y0}" width="{f(C.MAP_W)}" height="{f(C.MAP_H)}" fill="none" stroke="#1d0f07" stroke-width="3"/>',
               f'<rect x="{x0 + 8}" y="{y0 + 8}" width="{f(C.MAP_W - 16)}" height="{f(C.MAP_H - 16)}" fill="none" stroke="{self.ink}" stroke-width="1.2" opacity="0.7"/>']
        for (cx, cy, sx, sy) in [(x0 - 14, y0 - 14, 1, 1), (x1 + 14, y0 - 14, -1, 1), (x0 - 14, y1 + 14, 1, -1), (x1 + 14, y1 + 14, -1, -1)]:
            out.append(self.cloud_scroll(cx, cy, sx, sy))
        return "".join(out)

    def sea(self):
        x, y, w, h = C.MAP_X, C.MAP_Y, C.MAP_W, C.MAP_H
        out = [f'<rect x="{x}" y="{y}" width="{f(w)}" height="{f(h)}" fill="#ddcca1"/>',
               self.noise(x, y, int(w), int(h), "0.012", 3, (0.35, 0.25, 0.12), 0.9, -0.38, 9)]
        # Wave pattern: rows of ink arcs.
        pid = self.nid("wave")
        self.defs.append(f'<pattern id="{pid}" width="44" height="22" patternUnits="userSpaceOnUse">'
                         f'<path d="M0,16 Q11,4 22,16 Q33,4 44,16" fill="none" stroke="#3b4a4a" stroke-width="1.1" opacity="0.45"/>'
                         f'<path d="M22,5 Q33,-7 44,5 M0,5 Q-11,-7 -22,5" fill="none" stroke="#3b4a4a" stroke-width="1.1" opacity="0.3"/></pattern>')
        out.append(f'<rect x="{x}" y="{y}" width="{f(w)}" height="{f(h)}" fill="url(#{pid})"/>')
        out.append(f'<rect x="{x}" y="{y}" width="{f(w)}" height="{f(h)}" fill="#55707a" opacity="0.13"/>')
        return "".join(out)

    def land(self, path, islands):
        x, y, w, h = C.MAP_X, C.MAP_Y, int(C.MAP_W), int(C.MAP_H)
        cid = self.nid("landclip")
        allp = path + " " + " ".join(islands)
        self.defs.append(f'<clipPath id="{cid}"><path d="{allp}"/></clipPath>')
        rough = self.rough(2.4)
        burn = self.rad([(0.55, "#000", 0), (1, "#5a3a14", 0.55)], r=0.75)
        return (f'<path d="{allp}" fill="#ecdcb4"/>'
                f'<g clip-path="url(#{cid})">'
                + self.noise(x, y, w, h, "0.9", 2, (0.4, 0.3, 0.15), 0.35, -0.08, 3)
                + self.noise(x, y, w, h, "0.006", 4, (0.55, 0.38, 0.15), 1.0, -0.42, 21)
                + "</g>"
                f'<path d="{allp}" fill="none" stroke="#6b8a8a" stroke-width="9" opacity="0.18"/>'
                f'<path d="{allp}" fill="none" stroke="{self.ink}" stroke-width="2.4" filter="{rough}"/>'
                f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{burn}"/>')

    def terrain_decor(self):
        # Rice paddies on the Sông Son delta: small ink grids.
        out = []
        rnd = random.Random(5)
        for (x, y) in [(708, 150), (742, 170), (760, 206), (724, 196), (680, 140), (794, 214)]:
            sx, sy = P(x, y)
            for i in range(3):
                for j in range(2):
                    px, py = sx + i * 9 + rnd.uniform(-1, 1), sy + j * 6
                    out.append(f'<path d="M{f(px)},{f(py)} l8,-2 l2,5 l-8,2z" fill="none" stroke="{self.ink}" stroke-width="0.7" opacity="0.45"/>')
        # Forests: little ink trees in clumps.
        for cx, cy in [(470, 340), (500, 470), (640, 300), (660, 430), (420, 540), (520, 250), (600, 520), (560, 200), (380, 420)]:
            for i in range(7):
                x, y = P(cx + rnd.uniform(-22, 22), cy + rnd.uniform(-12, 12))
                r = rnd.uniform(4, 6)
                out.append(f'<path d="M{f(x - r)},{f(y)} q{f(r * 0.2)},{f(-r * 1.6)} {f(r)},{f(-r * 1.4)} q{f(r * 0.9)},{f(r * 0.1)} {f(r)},{f(r * 1.4)}" fill="#cbb98f" stroke="{self.ink}" stroke-width="1"/>'
                           f'<path d="M{f(x)},{f(y)} v4" stroke="{self.ink}" stroke-width="1"/>')
        return "".join(out)

    def unknown_area(self, title, sub):
        cid = self.nid("fog")
        x0, y0 = P(-20, -20)
        pts = mp([(-20, -20), (300, -20), (330, 120), (318, 260), (340, 420), (330, 560), (300, 700), (-20, 700)])
        path = smooth(pts, closed=True)
        hid = self.nid("hatch")
        self.defs.append(f'<pattern id="{hid}" width="12" height="12" patternUnits="userSpaceOnUse" patternTransform="rotate(35)">'
                         f'<line x1="0" y1="0" x2="0" y2="12" stroke="{self.ink}" stroke-width="0.8" opacity="0.28"/></pattern>')
        cx, cy = P(150, 300)
        return (f'<path d="{path}" fill="#d9c69c" opacity="0.75"/><path d="{path}" fill="url(#{hid})"/>'
                f'<path d="{path}" fill="none" stroke="{self.ink}" stroke-width="1.4" stroke-dasharray="2 7" stroke-linecap="round"/>'
                + T(cx, cy, title, self.title_font, 24, self.ink, "middle", 600, ls=5, extra='opacity="0.55"', style="font-style:italic")
                + T(cx, cy + 30, sub, self.body_font, 19, self.ink, "middle", 600, extra='opacity="0.6"', style="font-style:italic"))

    def river(self, path, w):
        rough = self.rough(1.5, 0.08)
        return (f'<path d="{path}" fill="none" stroke="#6f8f92" stroke-width="{w + 5}" stroke-linecap="round" opacity="0.35"/>'
                f'<path d="{path}" fill="none" stroke="{self.ink}" stroke-width="{w * 0.45}" stroke-linecap="round" filter="{rough}"/>')

    def lake(self, path):
        return (f'<path d="{path}" fill="#b9bfa5"/><path d="{path}" fill="#55707a" opacity="0.2"/>'
                f'<path d="{path}" fill="none" stroke="{self.ink}" stroke-width="2"/>')

    def mountain(self, x, y, s):
        w, h = s * 1.15, s * 1.25
        wash = self.lin([(0, "#2b2118"), (1, "#2b2118")], 0, 0, 0, 1) if False else None
        p = f"M{f(x - w)},{f(y)} Q{f(x - w * 0.35)},{f(y - h * 0.7)} {f(x)},{f(y - h)} Q{f(x + w * 0.3)},{f(y - h * 0.65)} {f(x + w)},{f(y)}"
        inner = f"M{f(x - w * 0.2)},{f(y - h * 0.62)} Q{f(x - w * 0.1)},{f(y - h * 0.3)} {f(x - w * 0.42)},{f(y - h * 0.05)}"
        return (f'<path d="{p} Z" fill="#7d7563" opacity="0.28"/>'
                f'<path d="{p} Z" fill="#ecdcb4" opacity="0.6" transform="translate({f(w*0.18)},0)"/>'
                f'<path d="{p}" fill="none" stroke="{self.ink}" stroke-width="1.8" stroke-linecap="round"/>'
                f'<path d="{inner}" fill="none" stroke="{self.ink}" stroke-width="1" opacity="0.7"/>')

    def karst(self, x, y):
        h, w = 30, 8
        p = f"M{f(x - w)},{f(y)} C{f(x - w - 2)},{f(y - h * 0.6)} {f(x - w * 0.6)},{f(y - h)} {f(x)},{f(y - h)} C{f(x + w * 0.7)},{f(y - h)} {f(x + w + 1)},{f(y - h * 0.5)} {f(x + w)},{f(y)}"
        return (f'<path d="{p}Z" fill="#cbb98f"/>'
                f'<path d="{p}" fill="none" stroke="{self.ink}" stroke-width="1.6"/>'
                f'<path d="M{f(x - 2)},{f(y - h * 0.8)} q-3,10 0,22" fill="none" stroke="{self.ink}" stroke-width="0.9" opacity="0.7"/>')

    def hill(self, x, y):
        return self.mountain(x, y, 11)

    def region_label(self, text, x, y, big, own):
        size = (40 if own else 32) if big else 20
        fill = self.red if own else self.ink
        op = 0.8 if own else 0.55
        return T(x, y, text, self.title_font, size, fill, "middle", 600 if big else 600, ls=10 if big else 4,
                 extra=f'opacity="{op}"', style="font-style:italic")

    def route(self, kind, state, path, legend=False):
        if state == "closed":
            return f'<path d="{path}" fill="none" stroke="#7a6a55" stroke-width="2.4" stroke-dasharray="3 7" stroke-linecap="round"/>'
        if kind == "sea":
            return (f'<path d="{path}" fill="none" stroke="{self.red}" stroke-width="3" stroke-dasharray="0.1 9" stroke-linecap="round"/>')
        if kind == "river":
            return f'<path d="{path}" fill="none" stroke="#2f5d6a" stroke-width="3" stroke-dasharray="12 5" stroke-linecap="round"/>'
        return f'<path d="{path}" fill="none" stroke="{self.red}" stroke-width="2.6" stroke-dasharray="14 6" stroke-linecap="round"/>'

    def seal(self, x, y, s, letter, color=None):
        color = color or self.red
        rough = self.rough(1.6, 0.3)
        return (f'<g transform="rotate(-4 {f(x)} {f(y)})" filter="{rough}">'
                f'<rect x="{f(x - s / 2)}" y="{f(y - s / 2)}" width="{f(s)}" height="{f(s)}" fill="{color}" rx="2"/>'
                f'<rect x="{f(x - s / 2 + 3)}" y="{f(y - s / 2 + 3)}" width="{f(s - 6)}" height="{f(s - 6)}" fill="none" stroke="#f1dfb8" stroke-width="1.4"/>'
                + T(x, y + s * 0.2, letter, self.title_font, s * 0.55, "#f6e6c4", "middle", 800) + "</g>")

    def player_city(self, x, y, name, kind, sel, mini=False):
        if mini:
            return self.seal(x, y, 18, "")
        s = 34 if kind == "capital" else 24
        out = []
        if kind == "monument":
            out.append(self.seal(x, y, 20, "", "#6d1a13"))
        else:
            if kind == "capital":
                out.append(f'<rect x="{f(x - s / 2 - 6)}" y="{f(y - s / 2 - 6)}" width="{f(s + 12)}" height="{f(s + 12)}" fill="none" stroke="{self.red}" stroke-width="1.5" transform="rotate(-4 {f(x)} {f(y)})"/>')
            out.append(self.seal(x, y, s, name[0]))
        out.append(self.place_label(x, y, name, 21 if kind == "capital" else 18, s / 2 + 2, weight=700))
        return "".join(out)

    def foreign_city(self, x, y, name, realm, sel, mini=False):
        r = 7 if mini else 10
        out = []
        if sel:
            out.append(f'<circle cx="{f(x)}" cy="{f(y)}" r="26" fill="none" stroke="{self.red}" stroke-width="2.5"/>'
                       f'<circle cx="{f(x)}" cy="{f(y)}" r="32" fill="none" stroke="{self.red}" stroke-width="1" stroke-dasharray="3 4"/>')
        out.append(f'<circle cx="{f(x)}" cy="{f(y)}" r="{r}" fill="#ecdcb4" stroke="{self.ink}" stroke-width="2"/>'
                   f'<circle cx="{f(x)}" cy="{f(y)}" r="{r * 0.35}" fill="{self.ink}"/>')
        if not mini:
            out.append(self.place_label(x, y, name, 18, r + 3, italic=True, weight=700))
        return "".join(out)

    def choice_site(self, x, y, name, tag, mini=False):
        r = 8 if mini else 13
        out = [f'<circle cx="{f(x)}" cy="{f(y)}" r="{r}" fill="none" stroke="{self.red}" stroke-width="2.2" stroke-dasharray="4 3"/>'
               f'<circle cx="{f(x)}" cy="{f(y)}" r="{r * 0.35}" fill="{self.red}"/>']
        if not mini:
            out.append(self.place_label(x, y, f"{tag} · {name}", 17, r + 3, key=name, fill=self.red, weight=700))
        return "".join(out)

    def choice_box(self, x, y, title, sub, sites):
        w, h = 292, 74
        out = []
        for sx, sy in sites:
            out.append(f'<path d="M{f(x + 30)},{f(y)} Q{f((x + sx) / 2 + 20)},{f((y + sy) / 2)} {f(sx)},{f(sy + 14)}" fill="none" stroke="{self.red}" stroke-width="1.2" stroke-dasharray="2 4"/>')
        out.append(f'<rect x="{f(x)}" y="{f(y)}" width="{w}" height="{h}" fill="#f2e4c0" stroke="{self.ink}" stroke-width="1.5"/>'
                   f'<rect x="{f(x + 4)}" y="{f(y + 4)}" width="{w - 8}" height="{h - 8}" fill="none" stroke="{self.red}" stroke-width="1"/>')
        out.append(T(x + 18, y + 32, title, self.title_font, 19, self.red, "start", 700))
        out.append(T(x + 18, y + 58, sub, self.body_font, 18, self.ink, "start", 700, style="font-style:italic"))
        out.append(self.seal(x + w - 30, y + h - 26, 26, "⇄"))
        return "".join(out)

    def sea_traders(self):
        out = []
        for (x, y, flip) in [(924, 430, 1), (700, 646, 1)]:
            sx, sy = P(x, y)
            out.append(f'<g transform="translate({f(sx)},{f(sy)}) scale({flip},1)" opacity="0.85">'
                       f'<path d="M-22,0 Q0,10 22,0 L18,-4 L-18,-4Z" fill="#3a2a1c"/>'
                       f'<path d="M-4,-4 L-4,-34 Q8,-24 10,-6Z" fill="#b8a27a" stroke="{self.ink}" stroke-width="1.2"/>'
                       f'<path d="M-6,-4 L-6,-26 Q-16,-18 -16,-6Z" fill="#b8a27a" stroke="{self.ink}" stroke-width="1.2"/></g>')
        return "".join(out)

    # UI -----------------------------------------------------------------
    def bar_bg(self, x, y, w, h):
        return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="#2a170c" rx="6"/>'
                f'<rect x="{x + 5}" y="{y + 5}" width="{w - 10}" height="{h - 10}" fill="none" stroke="#9a6a3c" stroke-width="1.5" rx="4"/>')

    def title_text(self, x, y, s):
        return T(x, y, s, self.title_font, 40, "#f3e3bd", "start", 800)

    def coin(self, x, y):
        return (f'<circle cx="{x}" cy="{y}" r="15" fill="#c99a3c" stroke="#5a3a14" stroke-width="2"/>'
                f'<rect x="{x - 5}" y="{y - 5}" width="10" height="10" fill="#2a170c"/>')

    def button(self, x, y, w, h, label, primary):
        fill = self.lin([(0, "#b3332a"), (1, "#7a1912")]) if primary else self.lin([(0, "#6b4426"), (1, "#4a2c18")])
        return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="5" fill="#1d0f07"/>'
                f'<rect x="{x + 3}" y="{y + 3}" width="{w - 6}" height="{h - 6}" rx="4" fill="{fill}" stroke="#d6a65c" stroke-width="1.5"/>'
                + T(x + w / 2, y + h / 2 + 8, label, self.title_font, 22, "#f6e3b4", "middle", 700))

    def panel_bg(self, x, y, w, h):
        out = [f'<rect x="{x - 10}" y="{y - 10}" width="{w + 20}" height="{h + 20}" fill="#2a170c" rx="6"/>',
               f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="#ecdcb4"/>',
               self.noise(x, y, w, h, "0.008", 4, (0.55, 0.38, 0.15), 1.0, -0.45, 31),
               self.noise(x, y, w, h, "0.9", 2, (0.4, 0.3, 0.15), 0.3, -0.06, 4),
               f'<rect x="{x + 10}" y="{y + 10}" width="{w - 20}" height="{h - 20}" fill="none" stroke="{self.ink}" stroke-width="1.2" opacity="0.6"/>']
        return "".join(out)

    def panel_emblem(self, x, y):
        # Sea Kingdom emblem in ink: a junk's sail inside a ring.
        return (f'<circle cx="{x}" cy="{y}" r="58" fill="none" stroke="{self.ink}" stroke-width="2"/>'
                f'<circle cx="{x}" cy="{y}" r="51" fill="none" stroke="{self.red}" stroke-width="1"/>'
                f'<path d="M{x - 30},{y + 18} Q{x},{y + 34} {x + 30},{y + 18}Z" fill="{self.ink}"/>'
                f'<path d="M{x - 4},{y + 16} L{x - 4},{y - 36} Q{x + 22},{y - 16} {x + 22},{y + 12}Z" fill="#cdb689" stroke="{self.ink}" stroke-width="1.6"/>'
                f'<path d="M{x - 8},{y + 16} L{x - 8},{y - 22} Q{x - 26},{y - 6} {x - 24},{y + 12}Z" fill="#cdb689" stroke="{self.ink}" stroke-width="1.6"/>'
                f'<path d="M{x - 40},{y + 30} q10,-6 20,0 t20,0 t20,0 t20,0" fill="none" stroke="{self.ink}" stroke-width="1.4"/>')

    def panel_title(self, x, y, s):
        return T(x, y, s, self.title_font, 42, self.ink, "middle", 800)

    def divider(self, x, y, w):
        return (f'<path d="M{x},{y} Q{x + w / 2},{y - 4} {x + w},{y}" fill="none" stroke="{self.ink}" stroke-width="1.6"/>'
                f'<rect x="{x + w / 2 - 6}" y="{y - 8}" width="12" height="12" fill="{self.red}" transform="rotate(45 {x + w / 2} {y - 2})"/>')

    def meter(self, x, y, w, v):
        out = []
        sw = (w - 4 * 8) / 5
        for i in range(5):
            on = i < v
            out.append(f'<rect x="{f(x + i * (sw + 8))}" y="{y}" width="{f(sw)}" height="14" fill="{self.red if on else "none"}" stroke="{self.ink}" stroke-width="1.2"/>')
        return "".join(out)

    def section_title(self, x, y, s):
        return T(x, y, s.upper(), self.title_font, 17, self.red, "start", 700, ls=2)

    def good_bullet(self, x, y):
        return f'<rect x="{x - 5}" y="{y - 5}" width="10" height="10" fill="{self.red}" transform="rotate(-4 {x} {y})"/>'

    def request_box(self, x, y, w, h, req):
        return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="#f2e4c0" stroke="{self.red}" stroke-width="1.5" stroke-dasharray="6 3"/>'
                + T(x + 18, y + 30, "REQUEST FROM THEIR COURT", self.title_font, 15, self.red, "start", 700, ls=1.5)
                + T(x + 18, y + 62, f"{req[0]}  ·  {req[1]}", self.body_font, 21, self.ink, "start", 700)
                + self.seal(x + w - 34, y + h / 2, 32, "!"))


# ====================================================================== Kambara
class Kambara(Theme):
    title_font = C.REALM_FONTS["kambara"]["title"]
    label_font = C.REALM_FONTS["kambara"]["label"]
    body_font = C.REALM_FONTS["kambara"]["body"]
    ink = "#2c2a22"
    halo = "#cfc2a3"
    water = "#3f6a62"
    water_edge = "#1f2e2a"
    compass_fill = "#9a4a2e"
    laterite = "#9a4a2e"
    moss = "#5f7f3a"
    goods_size = 17
    sub_size = 19
    title_w = 276
    bar_text = "#efe6cf"
    bar_sub = "#b9c99a"
    panel_text = "#2c2a22"
    panel_sub = "#5d5646"
    panel_rule = "#a1957a"
    legend_text = "#e6dcc3"
    label_pos = {"Suryagiri": "left", "Pashchima": "below", "Silapura": "above", "Rajamarga": "right",
                 "Kampong Sla": "below", "The pepper hills": "right", "The bay port": "right",
                 "Thăng Hà": "right", "Yasonagara": "above"}

    def stone(self, x, y, w, h, base="#b3a789", seed=1, dark=False):
        return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{base}"/>'
                + self.noise(x, y, w, h, "0.035", 4, (0.2, 0.18, 0.12), 1.2, -0.45, seed)
                + self.noise(x, y, w, h, "0.6", 2, (0.98, 0.95, 0.85), 0.5, -0.2, seed + 1))

    def moss_patch(self, x, y, w, h, seed, amount=0.5):
        # Moss blotches: low-frequency noise pushed through a steep alpha curve.
        return (self.noise(x, y, w, h, "0.018", 4, (0.30, 0.40, 0.18), 5.0, -3.0 - amount, seed, 'opacity="0.8"')
                + self.noise(x, y, w, h, "0.05", 3, (0.22, 0.32, 0.12), 5.0, -3.1 - amount, seed + 7, 'opacity="0.8"'))

    def background(self):
        return (self.stone(0, 0, C.W, C.H, "#4a4a3e", 3)
                + self.moss_patch(0, 0, C.W, C.H, 4, 0.2)
                + self.blocks(0, 0, C.W, C.H, 120, 60, "#25251d", 0.5))

    def blocks(self, x, y, w, h, bw, bh, color, op):
        pid = self.nid("blk")
        self.defs.append(f'<pattern id="{pid}" x="{x}" y="{y}" width="{bw}" height="{bh * 2}" patternUnits="userSpaceOnUse">'
                         f'<path d="M0,0 H{bw} M0,{bh} H{bw} M0,0 V{bh} M{bw / 2},{bh} V{bh * 2}" stroke="{color}" stroke-width="2" fill="none" opacity="{op}"/></pattern>')
        return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="url(#{pid})"/>'

    def frame_back(self):
        x, y, w, h = C.MAP_X - 26, C.MAP_Y - 26, C.MAP_W + 52, C.MAP_H + 52
        out = [f'<rect x="{x + 6}" y="{y + 8}" width="{w}" height="{h}" fill="#000" opacity="0.4"/>',
               self.stone(x, y, w, h, "#a89c80", 5)]
        # Lotus-petal band (pattern of small arches) along the frame.
        pid = self.nid("petal")
        self.defs.append(f'<pattern id="{pid}" width="22" height="16" patternUnits="userSpaceOnUse">'
                         f'<path d="M1,16 Q1,3 11,1 Q21,3 21,16" fill="#8d8169" stroke="#3a352a" stroke-width="1.2"/>'
                         f'<path d="M6,16 Q6,8 11,6 Q16,8 16,16" fill="none" stroke="#e1d6bb" stroke-width="0.8" opacity="0.6"/></pattern>')
        out.append(f'<rect x="{x + 6}" y="{y + 5}" width="{w - 12}" height="16" fill="url(#{pid})"/>')
        out.append(f'<rect x="{x + 6}" y="{y + h - 21}" width="{w - 12}" height="16" fill="url(#{pid})" transform="rotate(180 {x + w / 2} {y + h - 13})"/>')
        out.append(self.moss_patch(x, y, w, h, 12, 0.3))
        return "".join(out)

    def prang(self, x, y, s, fill="#b8ab8c", stroke="#2c2a22", sw=1.5):
        # A lotus-bud temple tower: stepped base, tiers, bud top.
        p = (f"M{f(x - 10 * s)},{f(y)} L{f(x - 10 * s)},{f(y - 6 * s)} L{f(x - 8 * s)},{f(y - 6 * s)} L{f(x - 8 * s)},{f(y - 14 * s)} "
             f"C{f(x - 8 * s)},{f(y - 26 * s)} {f(x - 4 * s)},{f(y - 34 * s)} {f(x)},{f(y - 40 * s)} "
             f"C{f(x + 4 * s)},{f(y - 34 * s)} {f(x + 8 * s)},{f(y - 26 * s)} {f(x + 8 * s)},{f(y - 14 * s)} "
             f"L{f(x + 8 * s)},{f(y - 6 * s)} L{f(x + 10 * s)},{f(y - 6 * s)} L{f(x + 10 * s)},{f(y)} Z")
        tiers = "".join(f'<path d="M{f(x - (7.6 - i * 1.4) * s)},{f(y - (14 + i * 6) * s)} H{f(x + (7.6 - i * 1.4) * s)}" stroke="{stroke}" stroke-width="{sw * 0.7}"/>' for i in range(4))
        return (f'<path d="{p}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" stroke-linejoin="round"/>{tiers}'
                f'<path d="M{f(x)},{f(y - 40 * s)} V{f(y - 46 * s)}" stroke="{stroke}" stroke-width="{sw}"/>'
                f'<rect x="{f(x - 3 * s)}" y="{f(y - 11 * s)}" width="{f(6 * s)}" height="{f(11 * s)}" fill="{stroke}"/>')

    def leaf(self, x, y, s, a, c):
        return (f'<g transform="translate({f(x)},{f(y)}) rotate({f(a)}) scale({f(s)})">'
                f'<path d="M0,0 C8,-6 22,-6 34,0 C22,6 8,6 0,0Z" fill="{c}" stroke="#1f3016" stroke-width="0.8"/>'
                f'<path d="M2,0 H30" stroke="#1f3016" stroke-width="0.6" opacity="0.7"/></g>')

    def vine(self, pts, seed, n=16, scale=1.0):
        rnd = random.Random(seed)
        out = [f'<path d="{smooth(pts)}" fill="none" stroke="#3b4a24" stroke-width="3" stroke-linecap="round"/>']
        # Leaves along the vine path.
        segs = len(pts) - 1
        for i in range(n):
            t = i / n * segs
            k = min(int(t), segs - 1)
            u = t - k
            x = pts[k][0] + (pts[k + 1][0] - pts[k][0]) * u
            y = pts[k][1] + (pts[k + 1][1] - pts[k][1]) * u
            c = rnd.choice(["#4f7a33", "#6e9a48", "#3f6a2a", "#86ad57"])
            out.append(self.leaf(x, y, rnd.uniform(0.6, 1.1) * scale, rnd.uniform(0, 360), c))
        return "".join(out)

    def frame_front(self):
        x0, y0, x1, y1 = C.MAP_X, C.MAP_Y, C.MAP_X + C.MAP_W, C.MAP_Y + C.MAP_H
        out = [f'<rect x="{x0}" y="{y0}" width="{f(C.MAP_W)}" height="{f(C.MAP_H)}" fill="none" stroke="#2c2a22" stroke-width="3"/>',
               f'<rect x="{x0 - 2}" y="{y0 - 2}" width="{f(C.MAP_W + 4)}" height="{f(C.MAP_H + 4)}" fill="none" stroke="#e6dcc1" stroke-width="1" opacity="0.6"/>']
        for cx, cy in [(x0 - 10, y0 + 6), (x1 + 10, y0 + 6), (x0 - 10, y1 + 30), (x1 + 10, y1 + 30)]:
            out.append(self.prang(cx, cy, 1.0, "#a3967a"))
        out.append(self.vine([(x0 - 30, y0 - 20), (x0 + 60, y0 - 8), (x0 + 160, y0 - 18), (x0 + 260, y0 - 6)], 1, 16))
        out.append(self.vine([(x0 - 22, y0 - 10), (x0 - 12, y0 + 90), (x0 - 18, y0 + 200)], 2, 10))
        out.append(self.vine([(x1 - 300, y0 - 16), (x1 - 160, y0 - 6), (x1 - 40, y0 - 18), (x1 + 24, y0 + 60), (x1 + 16, y0 + 180)], 3, 22))
        out.append(self.vine([(x1 + 20, y1 - 140), (x1 + 12, y1 - 40), (x1 - 90, y1 + 14)], 4, 12))
        out.append(self.vine([(x0 + 300, y1 + 18), (x0 + 420, y1 + 8), (x0 + 520, y1 + 20)], 5, 10))
        return "".join(out)

    def sea(self):
        x, y, w, h = C.MAP_X, C.MAP_Y, int(C.MAP_W), int(C.MAP_H)
        g = self.lin([(0, "#2f5a54"), (1, "#24433f")])
        pid = self.nid("ripple")
        self.defs.append(f'<pattern id="{pid}" width="60" height="26" patternUnits="userSpaceOnUse">'
                         f'<path d="M4,13 q10,-6 20,0 M34,24 q10,-6 20,0 M30,4 q8,-4 16,0" fill="none" stroke="#a6c7b8" stroke-width="1.2" opacity="0.35"/></pattern>')
        return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{g}"/>'
                + self.noise(x, y, w, h, "0.02", 3, (0.6, 0.8, 0.7), 0.8, -0.4, 6)
                + f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="url(#{pid})"/>')

    def land(self, path, islands):
        x, y, w, h = C.MAP_X, C.MAP_Y, int(C.MAP_W), int(C.MAP_H)
        allp = path + " " + " ".join(islands)
        cid = self.nid("landclip")
        self.defs.append(f'<clipPath id="{cid}"><path d="{allp}"/></clipPath>')
        # Carved relief: a dark rim offset down-right, a light one up-left.
        return (f'<path d="{allp}" fill="#1c2a26" transform="translate(4,5)" opacity="0.7"/>'
                f'<path d="{allp}" fill="#c4b696"/>'
                f'<g clip-path="url(#{cid})">'
                + self.noise(x, y, w, h, "0.03", 4, (0.25, 0.22, 0.15), 1.2, -0.48, 8)
                + self.noise(x, y, w, h, "0.7", 2, (1, 0.97, 0.88), 0.45, -0.2, 9)
                + self.moss_patch(x, y, w, h, 14, 0.6)
                + "</g>"
                f'<path d="{allp}" fill="none" stroke="#2c2a22" stroke-width="2"/>'
                f'<path d="{allp}" fill="none" stroke="#efe6cf" stroke-width="1.2" transform="translate(-1.5,-1.5)" opacity="0.7"/>')

    def terrain_decor(self):
        # Monsoon forest: clumps of rounded canopies.
        out = []
        rnd = random.Random(8)
        clumps = [(470, 340), (500, 470), (640, 300), (660, 430), (420, 540), (520, 250), (600, 520), (740, 380), (300, 160), (160, 260)]
        for cx, cy in clumps:
            for i in range(9):
                x, y = P(cx + rnd.uniform(-22, 22), cy + rnd.uniform(-12, 12))
                r = rnd.uniform(4, 7)
                out.append(f'<circle cx="{f(x)}" cy="{f(y)}" r="{f(r)}" fill="{rnd.choice(["#5f7f3a", "#4f6f30", "#6f8f44"])}" stroke="#25361a" stroke-width="0.8"/>')
        return "".join(out)

    def unknown_area(self, title, sub):
        pts = mp([(-20, -20), (300, -20), (330, 120), (318, 260), (340, 420), (330, 560), (300, 700), (-20, 700)])
        path = smooth(pts, closed=True)
        cx, cy = P(150, 300)
        return (f'<path d="{path}" fill="#3a3a30" opacity="0.55"/>'
                + f'<path d="{path}" fill="none" stroke="#e6dcc1" stroke-width="1.5" stroke-dasharray="1 8" stroke-linecap="round" opacity="0.7"/>'
                + self.engrave(cx, cy, title, self.title_font, 24, "middle", 700, ls=5, fill="#d8ceb4", dark=False)
                + T(cx, cy + 30, sub, self.body_font, 19, "#d8ceb4", "middle", 600))

    def engrave(self, x, y, s, font, size, anchor="middle", weight=700, ls=None, fill=None, dark=True):
        # Chiselled text: a light lip below-right and a dark cut.
        fill = fill or self.ink
        lip = "#f1e8d2" if dark else "#14140f"
        return (T(x + 1.2, y + 1.4, s, font, size, lip, anchor, weight, ls, extra='opacity="0.8"')
                + T(x, y, s, font, size, fill, anchor, weight, ls))

    def river(self, path, w):
        return (f'<path d="{path}" fill="none" stroke="#1c2a26" stroke-width="{w + 3}" stroke-linecap="round" transform="translate(1,1.5)"/>'
                f'<path d="{path}" fill="none" stroke="#3f6a62" stroke-width="{w + 1}" stroke-linecap="round"/>'
                f'<path d="{path}" fill="none" stroke="#9cc0b0" stroke-width="1" stroke-linecap="round" opacity="0.6"/>')

    def lake(self, path):
        return (f'<path d="{path}" fill="#1c2a26" transform="translate(2,3)"/>'
                f'<path d="{path}" fill="#3f6a62" stroke="#2c2a22" stroke-width="1.5"/>'
                f'<path d="{path}" fill="none" stroke="#9cc0b0" stroke-width="1" transform="scale(0.97) translate({f(0.03 * P(566, 446)[0] / 0.97)},{f(0.03 * P(566, 446)[1] / 0.97)})" opacity="0.6"/>')

    def mountain(self, x, y, s):
        w, h = s * 1.1, s * 1.3
        return (f'<path d="M{f(x - w)},{f(y)} L{f(x - w * 0.1)},{f(y - h)} L{f(x)},{f(y)}Z" fill="#ddd2b6" stroke="#2c2a22" stroke-width="1.2" stroke-linejoin="round"/>'
                f'<path d="M{f(x - w * 0.1)},{f(y - h)} L{f(x + w)},{f(y)} L{f(x)},{f(y)}Z" fill="#7d725d" stroke="#2c2a22" stroke-width="1.2" stroke-linejoin="round"/>'
                f'<path d="M{f(x - w * 0.55)},{f(y - h * 0.45)} l{f(w * 0.25)},{f(h * 0.12)} M{f(x + w * 0.4)},{f(y - h * 0.4)} l{f(-w * 0.2)},{f(h * 0.15)}" stroke="#2c2a22" stroke-width="0.8"/>')

    def karst(self, x, y):
        return self.mountain(x, y, 10)

    def hill(self, x, y):
        return (self.mountain(x, y, 13)
                + f'<circle cx="{f(x + 3)}" cy="{f(y - 5)}" r="4" fill="{self.moss}" opacity="0.9"/>')

    def barays(self):
        out = []
        for (x, y, w, h) in [(432, 392, 40, 9), (562, 380, 30, 8)]:
            sx, sy = P(x, y)
            ww, hh = w * C.MAP_S, h * C.MAP_S
            out.append(f'<rect x="{f(sx + 2)}" y="{f(sy + 3)}" width="{f(ww)}" height="{f(hh)}" fill="#1c2a26"/>'
                       f'<rect x="{f(sx)}" y="{f(sy)}" width="{f(ww)}" height="{f(hh)}" fill="#3f6a62" stroke="#2c2a22" stroke-width="1.5"/>'
                       f'<line x1="{f(sx + 3)}" y1="{f(sy + 3)}" x2="{f(sx + ww - 3)}" y2="{f(sy + 3)}" stroke="#9cc0b0" stroke-width="1" opacity="0.6"/>')
        sx, sy = P(432, 410)
        out.append(T(sx, sy + 6, "Western Baray", self.body_font, 13, "#2c2a22", "start", 600, extra='opacity="0.75"'))
        return "".join(out)

    def region_label(self, text, x, y, big, own):
        size = (40 if own else 32) if big else 19
        if own:
            return self.engrave(x, y, text, self.title_font, size, "middle", 700, ls=12, fill=self.laterite)
        return self.engrave(x, y, text, self.title_font, size, "middle", 700, ls=10 if big else 4, fill="#4a4436")

    def label(self, x, y, name, size=17, anchor="middle", fill=None, font=None, weight=600, italic=False):
        fill = fill or self.ink
        return (T(x, y, name, font or self.label_font, size, "#e3d8bd", anchor, weight, extra='stroke="#e3d8bd" stroke-width="5" stroke-linejoin="round" opacity="0.75"')
                + T(x, y, name, font or self.label_font, size, fill, anchor, weight))

    def route(self, kind, state, path, legend=False):
        if state == "closed":
            return f'<path d="{path}" fill="none" stroke="{"#cfc6ae" if legend else "#5d5646"}" stroke-width="2.5" stroke-dasharray="2 8" stroke-linecap="round"/>'
        if kind == "sea":
            return (f'<path d="{path}" fill="none" stroke="#efe6cf" stroke-width="3" stroke-dasharray="0.1 9" stroke-linecap="round"/>')
        if kind == "river":
            return (f'<path d="{path}" fill="none" stroke="#efe6cf" stroke-width="3.2" stroke-dasharray="12 5" stroke-linecap="round"/>')
        return (f'<path d="{path}" fill="none" stroke="#1c1a14" stroke-width="5" stroke-linecap="round" opacity="0.35" transform="translate(1,1.5)"/>'
                f'<path d="{path}" fill="none" stroke="{self.laterite}" stroke-width="3.4" stroke-dasharray="14 6" stroke-linecap="round"/>')

    def player_city(self, x, y, name, kind, sel, mini=False):
        if mini:
            return self.prang(x, y + 9, 0.42, "#c9bc9c", sw=1)
        out = []
        if kind == "capital":
            out.append(f'<rect x="{f(x - 30)}" y="{f(y - 8)}" width="60" height="22" fill="#3f6a62" stroke="#2c2a22" stroke-width="1.5"/>')
            out.append(self.prang(x - 14, y + 8, 0.55, "#c9bc9c"))
            out.append(self.prang(x + 14, y + 8, 0.55, "#c9bc9c"))
            out.append(self.prang(x, y + 6, 0.8, "#d7cbae"))
            out.append(self.place_label(x, y - 30, name, 21, 6, weight=700))
        elif kind == "monument":
            out.append(self.prang(x, y + 8, 0.6, "#d7cbae"))
            out.append(f'<circle cx="{f(x)}" cy="{f(y - 10)}" r="22" fill="none" stroke="{self.laterite}" stroke-width="1.5" stroke-dasharray="3 3"/>')
            out.append(self.place_label(x, y - 6, name, 16, 18, weight=600))
        else:
            out.append(self.prang(x, y + 8, 0.6, "#c9bc9c"))
            out.append(self.place_label(x, y - 8, name, 18, 18, weight=700))
        return "".join(out)

    def foreign_city(self, x, y, name, realm, sel, mini=False):
        r = 7 if mini else 10
        out = []
        if sel:
            out.append(f'<circle cx="{f(x)}" cy="{f(y)}" r="27" fill="none" stroke="#1c1a14" stroke-width="4" opacity="0.4" transform="translate(1,2)"/>'
                       f'<circle cx="{f(x)}" cy="{f(y)}" r="27" fill="none" stroke="#e8c15a" stroke-width="3"/>'
                       f'<circle cx="{f(x)}" cy="{f(y)}" r="34" fill="none" stroke="#e8c15a" stroke-width="1" stroke-dasharray="3 4"/>')
        out.append(f'<circle cx="{f(x + 1)}" cy="{f(y + 1.5)}" r="{r}" fill="#1c1a14" opacity="0.6"/>'
                   f'<circle cx="{f(x)}" cy="{f(y)}" r="{r}" fill="#d7cbae" stroke="#2c2a22" stroke-width="2"/>'
                   f'<circle cx="{f(x)}" cy="{f(y)}" r="{r * 0.4}" fill="{self.laterite}"/>')
        if not mini:
            out.append(self.place_label(x, y, name, 17, r + 3, weight=700))
        return "".join(out)

    def choice_site(self, x, y, name, tag, mini=False):
        r = 8 if mini else 13
        out = [f'<circle cx="{f(x)}" cy="{f(y)}" r="{r}" fill="none" stroke="#e8c15a" stroke-width="2.4" stroke-dasharray="4 3"/>'
               f'<circle cx="{f(x)}" cy="{f(y)}" r="{r * 0.35}" fill="#e8c15a"/>']
        if not mini:
            out.append(self.place_label(x, y, f"{tag} · {name}", 17, r + 3, key=name, fill="#6e2f17", weight=700))
        return "".join(out)

    def choice_box(self, x, y, title, sub, sites):
        w, h = 330, 78
        out = []
        for sx, sy in sites:
            out.append(f'<path d="M{f(x + 20)},{f(y)} Q{f((x + sx) / 2)},{f((y + sy) / 2 + 20)} {f(sx + 6)},{f(sy + 12)}" fill="none" stroke="#e8c15a" stroke-width="1.5" stroke-dasharray="2 4"/>')
        out.append(f'<rect x="{f(x + 4)}" y="{f(y + 5)}" width="{w}" height="{h}" fill="#000" opacity="0.35"/>')
        out.append(self.stone(int(x), int(y), w, h, "#b8ac8e", 17))
        out.append(f'<rect x="{f(x)}" y="{f(y)}" width="{w}" height="{h}" fill="none" stroke="#2c2a22" stroke-width="2"/>'
                   f'<rect x="{f(x + 5)}" y="{f(y + 5)}" width="{w - 10}" height="{h - 10}" fill="none" stroke="{self.laterite}" stroke-width="1.2"/>')
        out.append(self.engrave(x + 18, y + 32, title, self.title_font, 18, "start", 700, fill=self.laterite))
        out.append(T(x + 18, y + 60, sub, self.body_font, 17, self.ink, "start", 600))
        out.append(self.leaf(x + w - 46, y + 10, 0.9, 200, "#6e9a48") + self.leaf(x + w - 30, y + 8, 0.8, 150, "#4f7a33"))
        return "".join(out)

    def sea_traders(self):
        out = []
        for (x, y) in [(924, 430), (700, 646), (120, 600)]:
            sx, sy = P(x, y)
            out.append(f'<g transform="translate({f(sx)},{f(sy)})">'
                       f'<path d="M-22,0 Q0,10 22,0 L18,-4 L-18,-4Z" fill="#2c2a22"/>'
                       f'<path d="M-4,-4 L-4,-32 L14,-8Z" fill="#d7cbae" stroke="#2c2a22" stroke-width="1.2"/></g>')
        return "".join(out)

    # UI -----------------------------------------------------------------
    def bar_bg(self, x, y, w, h):
        return (f'<rect x="{x + 4}" y="{y + 6}" width="{w}" height="{h}" fill="#000" opacity="0.4"/>'
                + self.stone(x, y, w, h, "#5a5646", 21)
                + self.moss_patch(x, y, w, h, 22, 0.6)
                + f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="none" stroke="#1c1a14" stroke-width="2"/>'
                f'<rect x="{x + 6}" y="{y + 6}" width="{w - 12}" height="{h - 12}" fill="none" stroke="#a89c80" stroke-width="1.2"/>')

    def title_text(self, x, y, s):
        return self.engrave(x, y, s, self.title_font, 38, "start", 700, fill="#efe6cf", dark=False)

    def coin(self, x, y):
        return (f'<circle cx="{x}" cy="{y}" r="15" fill="#d9b24e" stroke="#4a3a14" stroke-width="2"/>'
                f'<circle cx="{x}" cy="{y}" r="6" fill="none" stroke="#4a3a14" stroke-width="1.5"/>')

    def button(self, x, y, w, h, label, primary):
        base = "#9a4a2e" if primary else "#8d8169"
        out = [f'<rect x="{x + 3}" y="{y + 5}" width="{w}" height="{h}" fill="#000" opacity="0.4"/>',
               self.stone(x, y, w, h, base, 30 + int(y) % 7),
               f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="none" stroke="#1c1a14" stroke-width="2"/>',
               f'<path d="M{x + 2},{y + h - 2} V{y + 2} H{x + w - 2}" fill="none" stroke="#f1e8d2" stroke-width="1.5" opacity="0.6"/>',
               self.engrave(x + w / 2, y + h / 2 + 8, label, self.title_font, 22, "middle", 700,
                            fill="#f6eedc" if primary else self.ink, dark=not primary)]
        return "".join(out)

    def panel_bg(self, x, y, w, h):
        return (f'<rect x="{x + 6}" y="{y + 8}" width="{w}" height="{h}" fill="#000" opacity="0.45"/>'
                + self.stone(x, y, w, h, "#b3a789", 41)
                + self.moss_patch(x, y, w, h, 42, 0.7)
                + f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="none" stroke="#1c1a14" stroke-width="2.5"/>'
                f'<rect x="{x + 10}" y="{y + 10}" width="{w - 20}" height="{h - 20}" fill="none" stroke="#5d5646" stroke-width="1.2"/>'
                + self.vine([(x + w - 120, y - 12), (x + w - 30, y - 4), (x + w + 10, y + 60), (x + w + 4, y + 150)], 9, 12))

    def panel_emblem(self, x, y):
        # Hamsara: a sacred goose in a carved roundel.
        return (f'<circle cx="{x + 2}" cy="{y + 3}" r="58" fill="#1c1a14" opacity="0.4"/>'
                f'<circle cx="{x}" cy="{y}" r="58" fill="#a89c80" stroke="#2c2a22" stroke-width="2"/>'
                f'<circle cx="{x}" cy="{y}" r="48" fill="none" stroke="#efe6cf" stroke-width="1" opacity="0.7"/>'
                f'<path d="M{x - 30},{y + 18} C{x - 20},{y + 30} {x + 22},{y + 30} {x + 30},{y + 8} C{x + 18},{y + 14} {x + 6},{y + 4} {x + 4},{y - 10} '
                f'C{x + 2},{y - 26} {x + 14},{y - 34} {x + 22},{y - 26} L{x + 32},{y - 24} L{x + 22},{y - 20} C{x + 16},{y - 22} {x + 12},{y - 16} {x + 14},{y - 6} '
                f'C{x + 4},{y - 4} {x - 14},{y - 8} {x - 30},{y + 18}Z" fill="#d7cbae" stroke="#2c2a22" stroke-width="1.8"/>'
                f'<path d="M{x - 18},{y + 10} q10,-10 24,-6" fill="none" stroke="#2c2a22" stroke-width="1.2"/>')

    def panel_title(self, x, y, s):
        return self.engrave(x, y, s, self.title_font, 42, "middle", 700, fill=self.ink)

    def divider(self, x, y, w):
        return (f'<line x1="{x}" y1="{y}" x2="{x + w}" y2="{y}" stroke="#2c2a22" stroke-width="1.5"/>'
                f'<line x1="{x}" y1="{y + 1.5}" x2="{x + w}" y2="{y + 1.5}" stroke="#f1e8d2" stroke-width="1" opacity="0.7"/>'
                + self.leaf(x + w / 2 - 34, y, 1.0, 0, "#6e9a48") + self.leaf(x + w / 2 + 34, y, 1.0, 180, "#4f7a33"))

    def meter(self, x, y, w, v):
        out = []
        sw = (w - 4 * 8) / 5
        for i in range(5):
            on = i < v
            out.append(f'<rect x="{f(x + i * (sw + 8))}" y="{y}" width="{f(sw)}" height="14" fill="{self.moss if on else "#8d8169"}" stroke="#2c2a22" stroke-width="1.2"/>')
        return "".join(out)

    def section_title(self, x, y, s):
        return self.engrave(x, y, s.upper(), self.title_font, 17, "start", 700, ls=2, fill=self.laterite)

    def good_bullet(self, x, y):
        return f'<rect x="{x - 4.5}" y="{y - 4.5}" width="9" height="9" fill="{self.moss}" stroke="#2c2a22" transform="rotate(45 {x} {y})"/>'

    def request_box(self, x, y, w, h, req):
        return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="#c9bc9c" stroke="#2c2a22" stroke-width="1.5"/>'
                f'<rect x="{x + 4}" y="{y + 4}" width="{w - 8}" height="{h - 8}" fill="none" stroke="{self.laterite}" stroke-width="1"/>'
                + self.engrave(x + 18, y + 30, "REQUEST FROM THEIR COURT", self.title_font, 15, "start", 700, ls=1.5, fill=self.laterite)
                + T(x + 18, y + 62, f"{req[0]}  ·  {req[1]}", self.body_font, 20, self.ink, "start", 700))


# ====================================================================== Tampadesa
class Tampadesa(Theme):
    title_font = C.REALM_FONTS["tampadesa"]["title"]
    label_font = C.REALM_FONTS["tampadesa"]["label"]
    body_font = C.REALM_FONTS["tampadesa"]["body"]
    ink = "#3b2412"
    halo = "#ecd3a3"
    water = "#6f9188"
    water_edge = "#3b2412"
    compass_fill = "#c8962e"
    brick = "#9b4527"
    title_w = 226
    bar_text = "#f7e3b0"
    bar_sub = "#e2b866"
    panel_text = "#3b2412"
    panel_sub = "#7a4f2a"
    panel_rule = "#c79d64"
    legend_text = "#f3dcae"
    label_pos = {"Gilded Stupa": "left", "Okthaw": "right", "Taungzin": "below", "Pugara": "right",
                 "Kyautthin": "below", "Royal port": "right", "Upriver town": "left"}

    def gold(self):
        if not hasattr(self, "_gold"):
            self._gold = self.lin([(0, "#7a5410"), (0.3, "#f3d77c"), (0.55, "#b8872a"), (0.8, "#f6df8e"), (1, "#8a6418")], 0, 0, 1, 1)
        return self._gold

    def bricks(self, x, y, w, h, seed=1):
        pid = self.nid("brick")
        bw, bh = 56, 22
        self.defs.append(f'<pattern id="{pid}" x="{x}" y="{y}" width="{bw}" height="{bh * 2}" patternUnits="userSpaceOnUse">'
                         f'<rect width="{bw}" height="{bh * 2}" fill="#cfa77a"/>'
                         f'<rect x="1.5" y="1.5" width="{bw - 3}" height="{bh - 3}" fill="#9b4527" rx="1.5"/>'
                         f'<rect x="{-bw / 2 + 1.5}" y="{bh + 1.5}" width="{bw - 3}" height="{bh - 3}" fill="#8a3b20" rx="1.5"/>'
                         f'<rect x="{bw / 2 + 1.5}" y="{bh + 1.5}" width="{bw - 3}" height="{bh - 3}" fill="#a84d2b" rx="1.5"/></pattern>')
        return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="url(#{pid})"/>'
                + self.noise(x, y, w, h, "0.05", 4, (0.25, 0.1, 0.04), 1.2, -0.5, seed)
                + self.noise(x, y, w, h, "0.008", 3, (0.95, 0.85, 0.65), 1.2, -0.62, seed + 5))

    def teak(self, x, y, w, h, seed=3):
        g = self.lin([(0, "#5a3218"), (0.5, "#4a2812"), (1, "#3a1e0c")], 0, 0, 1, 0)
        return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{g}"/>'
                + self.noise(x, y, w, h, "0.004 0.1", 3, (0.12, 0.05, 0.01), 1.1, -0.35, seed)
                + self.noise(x, y, w, h, "0.01 0.3", 2, (0.8, 0.5, 0.25), 0.5, -0.32, seed + 2))

    def background(self):
        return self.bricks(0, 0, C.W, C.H, 2)

    def frame_back(self):
        x, y, w, h = C.MAP_X - 24, C.MAP_Y - 24, C.MAP_W + 48, C.MAP_H + 48
        return (f'<rect x="{x + 6}" y="{y + 8}" width="{w}" height="{h}" fill="#000" opacity="0.4"/>'
                + self.teak(x, y, w, h, 4)
                + f'<rect x="{x + 6}" y="{y + 6}" width="{w - 12}" height="{h - 12}" fill="none" stroke="{self.gold()}" stroke-width="5"/>'
                f'<rect x="{x + 14}" y="{y + 14}" width="{w - 28}" height="{h - 28}" fill="none" stroke="#2a1608" stroke-width="2"/>')

    def stupa(self, x, y, s, fill=None, stroke="#4a2c08", sw=1.4):
        fill = fill or self.gold()
        # Terraced base, bell, rings, spire and umbrella finial.
        p = (f"M{f(x - 14 * s)},{f(y)} H{f(x + 14 * s)} V{f(y - 4 * s)} H{f(x + 11 * s)} V{f(y - 8 * s)} H{f(x + 9 * s)} "
             f"C{f(x + 9 * s)},{f(y - 20 * s)} {f(x + 4 * s)},{f(y - 24 * s)} {f(x + 2.5 * s)},{f(y - 28 * s)} "
             f"L{f(x + 1.2 * s)},{f(y - 42 * s)} L{f(x)},{f(y - 46 * s)} L{f(x - 1.2 * s)},{f(y - 42 * s)} L{f(x - 2.5 * s)},{f(y - 28 * s)} "
             f"C{f(x - 4 * s)},{f(y - 24 * s)} {f(x - 9 * s)},{f(y - 20 * s)} {f(x - 9 * s)},{f(y - 8 * s)} H{f(x - 11 * s)} V{f(y - 4 * s)} H{f(x - 14 * s)} Z")
        rings = "".join(f'<path d="M{f(x - (2.6 - i * 0.4) * s)},{f(y - (30 + i * 3) * s)} H{f(x + (2.6 - i * 0.4) * s)}" stroke="{stroke}" stroke-width="{sw * 0.7}"/>' for i in range(3))
        return (f'<path d="{p}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" stroke-linejoin="round"/>{rings}'
                f'<path d="M{f(x - 3 * s)},{f(y - 44 * s)} Q{f(x)},{f(y - 47 * s)} {f(x + 3 * s)},{f(y - 44 * s)}" fill="none" stroke="{stroke}" stroke-width="{sw}"/>')

    def frame_front(self):
        x0, y0, x1, y1 = C.MAP_X, C.MAP_Y, C.MAP_X + C.MAP_W, C.MAP_Y + C.MAP_H
        out = [f'<rect x="{x0}" y="{y0}" width="{f(C.MAP_W)}" height="{f(C.MAP_H)}" fill="none" stroke="#2a1608" stroke-width="3"/>']
        for cx, cy in [(x0 - 10, y0 + 22), (x1 + 10, y0 + 22), (x0 - 10, y1 + 26), (x1 + 10, y1 + 26)]:
            out.append(self.stupa(cx, cy, 1.15))
        return "".join(out)

    def sea(self):
        x, y, w, h = C.MAP_X, C.MAP_Y, int(C.MAP_W), int(C.MAP_H)
        pid = self.nid("wv")
        self.defs.append(f'<pattern id="{pid}" width="36" height="18" patternUnits="userSpaceOnUse">'
                         f'<path d="M0,12 q9,-8 18,0 t18,0" fill="none" stroke="#e8d6a8" stroke-width="1.1" opacity="0.35"/></pattern>')
        return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="#6f8f86"/>'
                + self.noise(x, y, w, h, "0.015", 3, (0.85, 0.75, 0.5), 0.9, -0.4, 6)
                + f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="url(#{pid})"/>')

    def land(self, path, islands):
        x, y, w, h = C.MAP_X, C.MAP_Y, int(C.MAP_W), int(C.MAP_H)
        allp = path + " " + " ".join(islands)
        cid = self.nid("landclip")
        self.defs.append(f'<clipPath id="{cid}"><path d="{allp}"/></clipPath>')
        # Dry plain: ochre with a sparse stipple; greener valleys added as blotches.
        sid = self.nid("stip")
        self.defs.append(f'<pattern id="{sid}" width="14" height="14" patternUnits="userSpaceOnUse">'
                         f'<circle cx="3" cy="4" r="0.9" fill="#7a4a1e" opacity="0.5"/><circle cx="10" cy="11" r="0.8" fill="#7a4a1e" opacity="0.4"/></pattern>')
        return (f'<path d="{allp}" fill="#e3bf86"/>'
                f'<g clip-path="url(#{cid})">'
                + self.noise(x, y, w, h, "0.01", 4, (0.62, 0.36, 0.16), 1.6, -0.75, 12)
                + self.noise(x, y, w, h, "0.014", 3, (0.45, 0.5, 0.25), 2.2, -1.25, 15)
                + f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="url(#{sid})"/>'
                + self.noise(x, y, w, h, "0.8", 2, (0.4, 0.22, 0.08), 0.35, -0.08, 13)
                + "</g>"
                f'<path d="{allp}" fill="none" stroke="#efdcae" stroke-width="7" opacity="0.5"/>'
                f'<path d="{allp}" fill="none" stroke="{self.ink}" stroke-width="2.2"/>')

    def terrain_decor(self):
        out = []
        rnd = random.Random(4)
        # Toddy palms on the dry plain.
        for _ in range(26):
            x, y = rnd.uniform(160, 330), rnd.uniform(140, 470)
            if abs(y - 258) < 26 and abs(x - 236) < 40:
                continue
            sx, sy = P(x, y)
            out.append(self.palm(sx, sy, rnd.uniform(0.8, 1.1)))
        return "".join(out)

    def palm(self, x, y, s):
        fr = "".join(f'<path d="M{f(x)},{f(y - 16 * s)} q{f(dx * 0.5 * s)},{f(-6 * s)} {f(dx * s)},{f(dy * s)}" fill="none" stroke="#4d5a22" stroke-width="1.6" stroke-linecap="round"/>'
                     for dx, dy in [(-8, 2), (8, 2), (-6, -4), (6, -4), (0, -7)])
        return f'<path d="M{f(x)},{f(y)} Q{f(x + 1.5 * s)},{f(y - 8 * s)} {f(x)},{f(y - 16 * s)}" stroke="{self.ink}" stroke-width="1.4" fill="none"/>{fr}'

    def river(self, path, w):
        return (f'<path d="{path}" fill="none" stroke="#efdcae" stroke-width="{w + 5}" stroke-linecap="round" opacity="0.6"/>'
                f'<path d="{path}" fill="none" stroke="#5f8279" stroke-width="{w + 1}" stroke-linecap="round"/>'
                f'<path d="{path}" fill="none" stroke="{self.ink}" stroke-width="0.8" stroke-linecap="round" opacity="0.5"/>')

    def lake(self, path):
        return f'<path d="{path}" fill="#6f8f86" stroke="{self.ink}" stroke-width="1.8"/>'

    def mountain(self, x, y, s):
        w, h = s * 1.2, s * 1.05
        p = f"M{f(x - w)},{f(y)} C{f(x - w * 0.7)},{f(y - h * 0.9)} {f(x + w * 0.5)},{f(y - h * 1.2)} {f(x + w)},{f(y)} Z"
        return (f'<path d="{p}" fill="#9b7a3e" stroke="{self.ink}" stroke-width="1.4"/>'
                f'<path d="M{f(x + w * 0.1)},{f(y - h * 0.85)} C{f(x + w * 0.5)},{f(y - h * 0.5)} {f(x + w * 0.7)},{f(y - h * 0.3)} {f(x + w)},{f(y)} L{f(x + w * 0.2)},{f(y)}Z" fill="#6c4f22" opacity="0.6"/>'
                f'<circle cx="{f(x - w * 0.3)}" cy="{f(y - h * 0.35)}" r="{f(s * 0.22)}" fill="#55642a"/>'
                f'<circle cx="{f(x + w * 0.25)}" cy="{f(y - h * 0.55)}" r="{f(s * 0.2)}" fill="#4b5a24"/>')

    def karst(self, x, y):
        return self.mountain(x, y, 9)

    def hill(self, x, y):
        return self.mountain(x, y, 11)

    def region_label(self, text, x, y, big, own):
        size = (36 if own else 32) if big else 19
        if own:
            return (T(x, y, text, self.title_font, size, "#4a2c08", "middle", 400, ls=10, extra='stroke="#4a2c08" stroke-width="5" stroke-linejoin="round" opacity="0.6"')
                    + T(x, y, text, self.title_font, size, self.gold(), "middle", 400, ls=10))
        return T(x, y, text, self.label_font if not big else self.title_font, size, self.ink, "middle", 700 if not big else 400, ls=8 if big else 4, extra='opacity="0.55"')

    def threat(self, title, sub):
        x, y = P(150, 30)
        cid = self.nid("storm")
        rnd = random.Random(2)
        blobs = "".join(f'<circle cx="{f(x + rnd.uniform(-170, 230))}" cy="{f(y + rnd.uniform(-60, 10))}" r="{f(rnd.uniform(30, 60))}"/>' for _ in range(14))
        return (f'<g fill="#3a2a2a" opacity="0.55">{blobs}</g>'
                + T(x + 30, y + 10, title, self.label_font, 21, "#f2dfb3", "middle", 700, ls=4)
                + T(x + 30, y + 36, sub, self.body_font, 18, "#f2dfb3", "middle", 500, style="font-style:italic")
                + f'<path d="M{f(x + 120)},{f(y + 44)} q30,40 10,90" fill="none" stroke="#7a1d12" stroke-width="2.5" stroke-dasharray="6 5"/>'
                f'<path d="M{f(x + 122)},{f(y + 134)} l-9,-12 l14,1z" fill="#7a1d12"/>')

    def route(self, kind, state, path, legend=False):
        if state == "closed":
            return f'<path d="{path}" fill="none" stroke="{"#d9b98a" if legend else "#7a5a3a"}" stroke-width="2.5" stroke-dasharray="2 8" stroke-linecap="round"/>'
        if kind == "sea":
            return f'<path d="{path}" fill="none" stroke="#f3dca0" stroke-width="3" stroke-dasharray="0.1 9" stroke-linecap="round"/>'
        if kind == "river":
            return (f'<path d="{path}" fill="none" stroke="#2a1608" stroke-width="5" stroke-linecap="round" opacity="0.25"/>'
                    f'<path d="{path}" fill="none" stroke="#f3dca0" stroke-width="3" stroke-dasharray="12 5" stroke-linecap="round"/>')
        return (f'<path d="{path}" fill="none" stroke="#2a1608" stroke-width="5.5" stroke-linecap="round" opacity="0.3"/>'
                f'<path d="{path}" fill="none" stroke="#7a1d12" stroke-width="3.2" stroke-dasharray="14 6" stroke-linecap="round"/>')

    def player_city(self, x, y, name, kind, sel, mini=False):
        if mini:
            return self.stupa(x, y + 10, 0.42, sw=1)
        out = []
        if kind == "capital":
            out.append(f'<rect x="{f(x - 26)}" y="{f(y - 8)}" width="52" height="20" fill="{self.brick}" stroke="{self.ink}" stroke-width="1.5"/>')
            out.append(self.stupa(x - 15, y + 8, 0.45))
            out.append(self.stupa(x + 15, y + 8, 0.45))
            out.append(self.stupa(x, y + 6, 0.75))
            out.append(self.place_label(x, y - 8, name, 22, 26, weight=700))
        elif kind == "monument":
            out.append(f'<circle cx="{f(x)}" cy="{f(y - 8)}" r="22" fill="none" stroke="#b8872a" stroke-width="1.5" stroke-dasharray="3 3"/>')
            out.append(self.stupa(x, y + 8, 0.55))
            out.append(self.place_label(x, y - 8, name, 16, 22, weight=700))
        else:
            out.append(self.stupa(x, y + 8, 0.55))
            out.append(self.place_label(x, y - 8, name, 18, 16, weight=700))
        return "".join(out)

    def foreign_city(self, x, y, name, realm, sel, mini=False):
        r = 7 if mini else 10
        out = []
        if sel:
            out.append(f'<circle cx="{f(x)}" cy="{f(y)}" r="27" fill="none" stroke="{self.gold()}" stroke-width="4"/>'
                       f'<circle cx="{f(x)}" cy="{f(y)}" r="27" fill="none" stroke="{self.ink}" stroke-width="1"/>'
                       f'<circle cx="{f(x)}" cy="{f(y)}" r="34" fill="none" stroke="{self.ink}" stroke-width="1" stroke-dasharray="3 4"/>')
        out.append(f'<circle cx="{f(x)}" cy="{f(y)}" r="{r}" fill="#f0dcae" stroke="{self.ink}" stroke-width="2"/>'
                   f'<circle cx="{f(x)}" cy="{f(y)}" r="{r * 0.4}" fill="{self.brick}"/>')
        if not mini:
            out.append(self.place_label(x, y, name, 17, r + 3, weight=700))
        return "".join(out)

    def choice_site(self, x, y, name, tag, mini=False):
        r = 8 if mini else 13
        out = [f'<circle cx="{f(x)}" cy="{f(y)}" r="{r}" fill="none" stroke="#7a1d12" stroke-width="2.4" stroke-dasharray="4 3"/>'
               f'<circle cx="{f(x)}" cy="{f(y)}" r="{r * 0.35}" fill="{self.gold()}" stroke="#7a1d12"/>']
        if not mini:
            out.append(self.place_label(x, y, f"{tag} · {name}", 17, r + 3, key=name, fill="#7a1d12", weight=700))
        return "".join(out)

    def choice_box(self, x, y, title, sub, sites):
        w, h = 392, 78
        out = []
        for sx, sy in sites:
            out.append(f'<path d="M{f(x + w - 30)},{f(y)} Q{f((x + w + sx) / 2 - 20)},{f((y + sy) / 2 + 10)} {f(sx - 10)},{f(sy + 8)}" fill="none" stroke="#7a1d12" stroke-width="1.5" stroke-dasharray="2 4"/>')
        out.append(f'<rect x="{f(x + 4)}" y="{f(y + 5)}" width="{w}" height="{h}" fill="#000" opacity="0.3"/>'
                   f'<rect x="{f(x)}" y="{f(y)}" width="{w}" height="{h}" fill="#2a1608"/>'
                   f'<rect x="{f(x + 4)}" y="{f(y + 4)}" width="{w - 8}" height="{h - 8}" fill="#7a1d12" stroke="{self.gold()}" stroke-width="2"/>')
        out.append(T(x + 18, y + 33, title, self.title_font, 20, "#f6df8e", "start", 400))
        out.append(T(x + 18, y + 60, sub, self.body_font, 18, "#f7e3b0", "start", 500, style="font-style:italic"))
        return "".join(out)

    def sea_traders(self):
        out = []
        for (x, y) in [(360, 644), (924, 430), (700, 646)]:
            sx, sy = P(x, y)
            out.append(f'<g transform="translate({f(sx)},{f(sy)})">'
                       f'<path d="M-24,0 Q0,10 24,0 L20,-4 L-20,-4Z" fill="{self.ink}"/>'
                       f'<path d="M-2,-4 L-2,-34 Q14,-20 14,-6Z" fill="#f0dcae" stroke="{self.ink}" stroke-width="1.2"/></g>')
        sx, sy = P(360, 644)
        out.append(self.label(sx + 30, sy - 4, "To the far traders", 16, "start", italic=True, font=self.body_font))
        return "".join(out)

    # UI -----------------------------------------------------------------
    def bar_bg(self, x, y, w, h):
        return (f'<rect x="{x + 4}" y="{y + 6}" width="{w}" height="{h}" fill="#000" opacity="0.4"/>'
                + self.teak(x, y, w, h, 7)
                + f'<rect x="{x + 5}" y="{y + 5}" width="{w - 10}" height="{h - 10}" fill="none" stroke="{self.gold()}" stroke-width="3"/>')

    def title_text(self, x, y, s):
        return T(x, y, s, self.title_font, 42, self.gold(), "start", 400, extra='stroke="#2a1608" stroke-width="1"')

    def coin(self, x, y):
        return (f'<circle cx="{x}" cy="{y}" r="15" fill="{self.gold()}" stroke="#4a2c08" stroke-width="2"/>'
                f'<circle cx="{x}" cy="{y}" r="5" fill="#7a1d12"/>')

    def button(self, x, y, w, h, label, primary):
        return (f'<rect x="{x + 3}" y="{y + 5}" width="{w}" height="{h}" fill="#000" opacity="0.4" rx="4"/>'
                f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="#1e1008" rx="4"/>'
                f'<rect x="{x + 4}" y="{y + 4}" width="{w - 8}" height="{h - 8}" fill="{"#8a2216" if primary else "#2e1a0c"}" stroke="{self.gold()}" stroke-width="2" rx="3"/>'
                + T(x + w / 2, y + h / 2 + 8, label, self.title_font, 22, "#f6df8e", "middle", 400))

    def panel_bg(self, x, y, w, h):
        out = [f'<rect x="{x + 6}" y="{y + 8}" width="{w}" height="{h}" fill="#000" opacity="0.45"/>',
               self.teak(x - 10, y - 10, w + 20, h + 20, 9),
               f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="#ecd3a3"/>',
               self.noise(x, y, w, h, "0.01", 4, (0.62, 0.36, 0.16), 1.4, -0.7, 51),
               self.noise(x, y, w, h, "0.8", 2, (0.4, 0.22, 0.08), 0.3, -0.06, 52),
               f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="none" stroke="{self.gold()}" stroke-width="4"/>',
               f'<rect x="{x + 10}" y="{y + 10}" width="{w - 20}" height="{h - 20}" fill="none" stroke="{self.ink}" stroke-width="1" opacity="0.6"/>']
        return "".join(out)

    def panel_emblem(self, x, y):
        # Kambara as Tampadesa sees it: a lotus-bud tower drawn in their gold-on-lacquer style.
        prang = Kambara.prang(self, x, y + 36, 1.55, self.gold(), "#4a2c08", 1.8)
        return (f'<circle cx="{x}" cy="{y}" r="60" fill="#2a1608"/>'
                f'<circle cx="{x}" cy="{y}" r="55" fill="#7a1d12" stroke="{self.gold()}" stroke-width="3"/>' + prang)

    def panel_title(self, x, y, s):
        return T(x, y, s, self.title_font, 42, self.ink, "middle", 400)

    def divider(self, x, y, w):
        return (f'<rect x="{x}" y="{y - 1.5}" width="{w}" height="3" fill="{self.gold()}"/>'
                + self.stupa(x + w / 2, y + 4, 0.38))

    def meter(self, x, y, w, v):
        out = []
        sw = (w - 4 * 8) / 5
        for i in range(5):
            on = i < v
            out.append(f'<rect x="{f(x + i * (sw + 8))}" y="{y}" width="{f(sw)}" height="14" fill="{self.gold() if on else "#d9bd8a"}" stroke="{self.ink}" stroke-width="1.2" rx="2"/>')
        return "".join(out)

    def section_title(self, x, y, s):
        return T(x, y, s.upper(), self.label_font, 17, "#7a1d12", "start", 700, ls=2)

    def good_bullet(self, x, y):
        return f'<circle cx="{x}" cy="{y}" r="5" fill="{self.gold()}" stroke="#4a2c08"/>'

    def request_box(self, x, y, w, h, req):
        return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="#2a1608" rx="3"/>'
                f'<rect x="{x + 4}" y="{y + 4}" width="{w - 8}" height="{h - 8}" fill="#7a1d12" stroke="{self.gold()}" stroke-width="1.5" rx="2"/>'
                + T(x + 18, y + 32, "REQUEST FROM THEIR COURT", self.label_font, 15, "#f6df8e", "start", 700, ls=1.5)
                + T(x + 18, y + 64, f"{req[0]}  ·  {req[1]}", self.body_font, 21, "#f7e3b0", "start", 700))


THEMES = [("dailong", DaiLong, content.DAILONG), ("kambara", Kambara, content.KAMBARA), ("tampadesa", Tampadesa, content.TAMPADESA)]

if __name__ == "__main__":
    import sys
    only = sys.argv[1:]
    for key, cls, data in THEMES:
        if only and key not in only:
            continue
        BUILD.mkdir(exist_ok=True)
        (BUILD / f"world-map-{key}.html").write_text(cls(data).render())
        print("wrote", key)
