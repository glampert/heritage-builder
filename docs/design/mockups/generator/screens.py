"""Renders the intro, intermission and in-game HUD mockups for each realm into build/.

The three realm styles come from world_map.py; this file adds the screens'
layouts, the story panels, the steles and the HUD.
"""
import base64
import math
import random
import textwrap

from world_map import DaiLong, Kambara, Tampadesa, T, f, FONTS_CSS, BUILD
import screens_content as SC
from minimap import Minimap, widget_layout, widget_controls

W, H = 1920, 1080


class Screens:
    """Layouts shared by the three realms. The realm classes supply the look."""

    caption_pos = (110, 100)
    stele_k = 0.47
    tooltip_w = 330
    box_style = "panel"
    on_icon = "#fff4dc"
    caption_w = 760
    bubble = (1000, 470, 560, (880, 640))   # x, y, width, tail tip

    # ------------------------------------------------------------ helpers
    def page(self, name, parts):
        body = "".join(parts)
        svg = (f'<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{W}" height="{H}" viewBox="0 0 {W} {H}">'
               f'<defs>{"".join(self.defs)}</defs>{body}</svg>')
        html = (f'<!doctype html><html><head><meta charset="utf-8"><title>{name}</title>'
                f'<style>{FONTS_CSS} html,body{{margin:0;padding:0;background:#000}} svg{{display:block;font-synthesis:style}}</style>'
                f'</head><body>{svg}</body></html>')
        BUILD.mkdir(exist_ok=True)
        (BUILD / f"{name}.html").write_text(html)
        print("wrote", name)

    def wrap(self, text, width, size, k=0.5):
        return textwrap.wrap(text, max(8, int(width / (size * k))))

    def lines(self, x, y, lines, font, size, fill, lh, anchor="start", weight=500, style=""):
        return "".join(T(x, y + i * lh, s, font, size, fill, anchor, weight, style=style) for i, s in enumerate(lines))

    def clip(self, x, y, w, h, rx=0):
        cid = self.nid("clip")
        self.defs.append(f'<clipPath id="{cid}"><rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}"/></clipPath>')
        return f'clip-path="url(#{cid})"'

    # ------------------------------------------------------------ intro
    def intro(self):
        d = self.sd
        i = d["intro"]
        X, Y, PW, PH = 60, 56, 1800, 870
        parts = [self.background(), self.frame_back_at(X, Y, PW, PH)]
        parts.append(f'<g {self.clip(X, Y, PW, PH)}>{self.intro_scene(X, Y, PW, PH)}</g>')
        parts.append(self.frame_front_at(X, Y, PW, PH))
        cx, cy = self.caption_pos
        parts.append(self.caption(X + cx - 60, Y + cy - 56, self.caption_w, i["narration"]))
        bx, by, bw, (tx, ty) = self.bubble
        parts.append(self.speech(X + bx - 60, Y + by - 56, bw, i["speaker"], i["speech"], (X + tx - 60, Y + ty - 56)))
        # Bottom bar.
        parts.append(self.bar_bg(40, 958, 1840, 96))
        parts.append(T(80, 996, f'{d["campaign"]} · {d["campaign_name"]}'.upper(), self.title_font, 18, self.bar_sub, "start", 700, ls=3))
        parts.append(T(80, 1034, i["sequence"], self.title_font, 32, self.bar_text, "start", 700))
        n, cur = i["panels"], i["panel"]
        x0 = 960 - (n - 1) * 17
        for k in range(n):
            on = k + 1 == cur
            done = k + 1 < cur
            parts.append(self.dot(x0 + k * 34, 1006, on, done))
        parts.append(T(960, 1044, f"Panel {cur} of {n}", self.body_font, 18, self.bar_sub, "middle", 600))
        parts.append(self.button(1300, 978, 220, 56, "Skip", False))
        parts.append(self.button(1540, 978, 300, 56, "Next", True))
        self.page(f"intro-{d['key']}", parts)

    def dot(self, x, y, on, done):
        c = self.accent
        if on:
            return f'<circle cx="{x}" cy="{y}" r="9" fill="{c}" stroke="{self.bar_text}" stroke-width="2"/>'
        return f'<circle cx="{x}" cy="{y}" r="6" fill="{self.bar_sub if done else "none"}" stroke="{self.bar_sub}" stroke-width="1.5"/>'

    # ------------------------------------------------------------ intermission
    def intermission(self):
        d = self.sd
        b = d["debrief"]
        parts = [self.background(), self.stele_backdrop()]
        parts.append(self.stele(140, 70, 640, 950, d["stele_title"], d["stele"]))
        x, y, w, h = 860, 60, 1020, 960
        parts.append(self.box(x, y, w, h, big=True))
        ix, cy = x + 56, y + 78
        parts.append(T(ix, cy, f'{b["mission"]} complete'.upper(), self.title_font, 20, self.accent, "start", 700, ls=3))
        cy += 62
        parts.append(self.heading(ix, cy, b["name"], 54))
        cy += 40
        parts.append(T(ix, cy, f'{b["city"]} · {b["year"]}', self.body_font, 22, self.panel_sub, "start", 600, style="font-style:italic"))
        cy += 30
        parts.append(self.divider(ix, cy, w - 112))
        cy += 52
        parts.append(self.section_title(ix, cy, "Goals"))
        parts.append(T(x + w - 260, cy, "TARGET", self.title_font, 15, self.panel_sub, "end", 700, ls=2))
        parts.append(T(x + w - 56, cy, "ACHIEVED", self.title_font, 15, self.panel_sub, "end", 700, ls=2))
        for k, (g, target, got, ok) in enumerate(b["goals"]):
            ry = cy + 46 + k * 46
            parts.append(self.check(ix + 12, ry - 8, ok))
            parts.append(T(ix + 40, ry, g, self.body_font, 23, self.panel_text, "start", 600))
            parts.append(T(x + w - 260, ry, target, self.body_font, 23, self.panel_sub, "end", 600))
            parts.append(T(x + w - 56, ry, got, self.body_font, 23, self.panel_text, "end", 700))
            parts.append(f'<line x1="{ix}" y1="{ry + 16}" x2="{x + w - 56}" y2="{ry + 16}" stroke="{self.panel_rule}" stroke-width="1"/>')
        cy += 46 * 3 + 54
        parts.append(self.section_title(ix, cy, "Distinctions"))
        parts.append(T(x + w - 56, cy, "each one adds a line to the stele", self.body_font, 17, self.panel_sub, "end", 500, style="font-style:italic"))
        for k, (g, ok) in enumerate(b["optional"]):
            ry = cy + 44 + k * 42
            parts.append(self.check(ix + 12, ry - 8, ok))
            parts.append(T(ix + 40, ry, g, self.body_font, 22, self.panel_text if ok else self.panel_sub, "start", 600))
        cy += 44 * 2 + 50
        parts.append(self.section_title(ix, cy, "Ratings"))
        colw = (w - 112) / 4
        for k, (name, v) in enumerate(b["ratings"]):
            rx = ix + k * colw
            parts.append(T(rx, cy + 40, name, self.body_font, 19, self.panel_sub, "start", 600))
            parts.append(T(rx + colw - 24, cy + 40, str(v), self.title_font, 22, self.panel_text, "end", 700))
            parts.append(self.bar(rx, cy + 54, colw - 24, 12, v / 100))
        cy += 100
        for k, (name, v) in enumerate(b["stats"]):
            rx = ix + k * (w - 112) / 3
            parts.append(T(rx, cy, name, self.body_font, 19, self.panel_sub, "start", 600))
            parts.append(T(rx, cy + 38, v, self.title_font, 32, self.panel_text, "start", 700))
        cy += 70
        parts.append(self.note_box(ix, cy, w - 112, 64, b["joins"]))
        parts.append(self.button(ix, y + h - 98, 300, 58, "Keep playing", False))
        parts.append(self.button(x + w - 56 - 420, y + h - 98, 420, 58, "Continue to the world map", True))
        self.page(f"intermission-{d['key']}", parts)

    def check(self, x, y, ok):
        if ok:
            return (f'<circle cx="{x}" cy="{y}" r="12" fill="{self.accent}"/>'
                    f'<path d="M{x - 6},{y} l4,5 l8,-10" fill="none" stroke="#fff4dc" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>')
        return (f'<circle cx="{x}" cy="{y}" r="11" fill="none" stroke="{self.panel_sub}" stroke-width="2"/>'
                f'<path d="M{x - 5},{y - 5} l10,10 M{x + 5},{y - 5} l-10,10" stroke="{self.panel_sub}" stroke-width="2" stroke-linecap="round"/>')

    def bar(self, x, y, w, h, v, track=None):
        track = track or self.panel_rule
        return (f'<rect x="{f(x)}" y="{f(y)}" width="{f(w)}" height="{h}" rx="{h / 2}" fill="{track}" opacity="0.6"/>'
                f'<rect x="{f(x)}" y="{f(y)}" width="{f(w * max(0.04, min(1, v)))}" height="{h}" rx="{h / 2}" fill="{self.accent}"/>')

    def note_box(self, x, y, w, h, text):
        return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{self.note_fill}" stroke="{self.accent}" stroke-width="1.5" stroke-dasharray="6 3"/>'
                + self.icon("map", x + 36, y + h / 2, 1.0, self.accent)
                + T(x + 68, y + h / 2 + 8, text, self.body_font, 22, self.panel_text, "start", 600))

    def stele_lines(self, x, y, w, title, lines, title_size, size, lh, color, new_color, font=None, title_font=None):
        out = []
        tl = self.wrap(title, w, title_size, 0.55)
        for k, s in enumerate(tl):
            out.append(self.carve(x + w / 2, y + k * (title_size + 8), s, title_font or self.title_font, title_size, color, "middle"))
        cy = y + len(tl) * (title_size + 8) + 34
        out.append(self.carve_rule(x + 40, cy - 20, w - 80, color))
        for n, line in enumerate(lines):
            new = n == len(lines) - 1
            wl = self.wrap(line, w, size, self.stele_k)
            out.append(self.carve(x, cy, "·" if not new else "", font or self.body_font, size, color, "start"))
            for k, s in enumerate(wl):
                out.append(self.carve(x, cy + k * lh, s, font or self.body_font, size, new_color if new else color, "start", new))
            if new:
                out.append(self.fresh_mark(x - 30, cy - size * 0.35))
            cy += len(wl) * lh + 16
        # Empty rows for the missions to come.
        for k in range(3):
            out.append(f'<line x1="{x}" y1="{cy + k * 34}" x2="{x + w}" y2="{cy + k * 34}" stroke="{color}" stroke-width="1" stroke-dasharray="2 8" opacity="0.4"/>')
        return "".join(out)

    def carve(self, x, y, s, font, size, color, anchor, new=False):
        return T(x, y, s, font, size, color, anchor, 700)

    def carve_rule(self, x, y, w, color):
        return f'<line x1="{x}" y1="{y}" x2="{x + w}" y2="{y}" stroke="{color}" stroke-width="1.5" opacity="0.7"/>'

    def fresh_mark(self, x, y):
        return f'<circle cx="{x}" cy="{y}" r="6" fill="{self.accent}"/>'

    # ------------------------------------------------------------ HUD
    def hud(self):
        d = self.sd
        u = d["hud"]
        img = base64.b64encode((BUILD / f"city_{d['key']}.jpg").read_bytes()).decode()
        parts = [f'<image x="0" y="0" width="{W}" height="{H}" xlink:href="data:image/jpeg;base64,{img}" preserveAspectRatio="xMidYMid slice"/>']
        # Speed and date, top left.
        parts.append(self.box(12, 10, 470, 74))
        for k, ic in enumerate(["pause", "play", "fast", "faster"]):
            on = (k == u["speed"] - 1) or (k == 1 and u["speed"] == 2)
            on = k == u["speed"]
            parts.append(self.icon_button(42 + k * 50, 47, ic, on))
            pass
        parts.append(T(250, 43, u["date"], self.body_font, 18, self.panel_text, "start", 700))
        parts.append(T(250, 67, u["season"], self.body_font, 17, self.panel_sub, "start", 600, style="font-style:italic"))
        # Resources and ratings, top centre.
        x0, w0 = 560, 820
        parts.append(self.box(x0, 10, w0, 74))
        items = [("population", u["population"]), ("coin", u["treasury"]), ("food", u["food"])]
        for k, (ic, v) in enumerate(items):
            ix = x0 + 40 + k * 150
            parts.append(self.icon(ic, ix, 47, 1.0, self.accent))
            parts.append(T(ix + 22, 56, v, self.title_font, 24, self.panel_text, "start", 700))
        sx = x0 + 490
        parts.append(f'<line x1="{sx - 20}" y1="26" x2="{sx - 20}" y2="68" stroke="{self.panel_rule}" stroke-width="2"/>')
        for k, (lab, v) in enumerate(zip(["P", "C", "S", u["legitimacy"][0]], u["ratings"])):
            rx = sx + k * 80
            parts.append(self.rating_badge(rx + 18, 47, lab, v))
        # Goals, left.
        gx, gy, gw = 12, 100, 430
        gh = 64 + len(u["goals"]) * 66 + 10
        parts.append(self.box(gx, gy, gw, gh))
        parts.append(self.section_title(gx + 26, gy + 42, "Goals"))
        parts.append(T(gx + gw - 26, gy + 42, u["goals_title"], self.body_font, 19, self.panel_sub, "end", 600, style="font-style:italic"))
        for k, (g, txt, v) in enumerate(u["goals"]):
            ry = gy + 86 + k * 66
            parts.append(self.check(gx + 38, ry - 7, v >= 1) if v >= 1 else f'<circle cx="{gx + 38}" cy="{ry - 7}" r="11" fill="none" stroke="{self.accent}" stroke-width="2"/>')
            parts.append(T(gx + 62, ry, g, self.body_font, 20, self.panel_text, "start", 700))
            parts.append(T(gx + gw - 26, ry, txt, self.body_font, 18, self.panel_sub, "end", 600))
            parts.append(self.bar(gx + 62, ry + 14, gw - 88, 8, v))
        # Build bar, right.
        bw = 96
        bx = W - bw - 12
        n = len(u["build"])
        bh = 30 + n * 84 + 96
        parts.append(self.box(bx, 10, bw, bh))
        for k, ic in enumerate(u["build"]):
            iy = 40 + k * 84
            parts.append(self.build_button(bx + bw / 2, iy + 34, ic, k == 5))
        parts.append(self.build_button(bx + bw / 2, 40 + n * 84 + 34, "undo", False))
        # Tooltip for the hovered build button.
        ty = 40 + 5 * 84 + 34
        parts.append(self.tooltip(bx - self.tooltip_w - 20, ty - 52, self.tooltip_w, 104, "Temples and shrines", "Faith, festivals and the patron spirits"))
        # Minimap, bottom left: concept A from minimap.py in the realm's palette, with
        # its lens icons across the top and its tools down the right (minimap-lenses).
        btn, mw_ = 28, 300
        mm_w, mm_h, *_ = widget_layout(0, 0, mw_, btn, 8, 12)
        bx0, by0 = 12, 1068 - mm_h
        mm_w, mm_h, (mmx, mmy), lens_pos, tool_pos = widget_layout(bx0, by0, mw_, btn, 8, 12)
        parts.append(self.box(bx0, by0, mm_w, mm_h))
        mm = Minimap(self.defs, prefix="hudmm")
        parts.append(mm.painted(mmx, mmy, mw_, palette=self.mm_palette, k=0.62))
        colors = dict(fill=self.btn_fill, on_fill=self.accent, stroke=self.panel_rule, ink=self.panel_text,
                      on_ink=self.on_icon, alert=self.accent, badge="#c4321f")
        parts.append(widget_controls(lens_pos, tool_pos, btn, colors, "city", u.get("alerts", 2), icon_scale=0.62))
        # Messages, bottom centre.
        mx, mw = bx0 + mm_w + 14, 1256 - (bx0 + mm_w + 14)
        parts.append(self.box(mx, 950, mw, 118))
        for k, (ic, msg) in enumerate(u["messages"]):
            my = 984 + k * 32
            parts.append(self.icon(ic, mx + 34, my - 6, 0.7, self.accent))
            parts.append(T(mx + 58, my, msg, self.body_font, 19, self.panel_text if k == 0 else self.panel_sub, "start", 700 if k == 0 else 600))
        parts.append(T(mx + mw - 24, 984, "now", self.body_font, 16, self.panel_sub, "end", 500, style="font-style:italic"))
        self.page(f"hud-{d['key']}", parts)

    def rating_badge(self, x, y, lab, v):
        r = 22
        a = v / 100 * 2 * math.pi
        ex, ey = x + r * math.sin(a), y - r * math.cos(a)
        large = 1 if v > 50 else 0
        return (f'<circle cx="{x}" cy="{y}" r="{r}" fill="none" stroke="{self.panel_rule}" stroke-width="5"/>'
                f'<path d="M{x},{y - r} A{r},{r} 0 {large} 1 {f(ex)},{f(ey)}" fill="none" stroke="{self.accent}" stroke-width="5" stroke-linecap="round"/>'
                + T(x, y + 7, lab, self.title_font, 18, self.panel_text, "middle", 700)
                + T(x + 30, y + 7, str(v), self.body_font, 18, self.panel_sub, "start", 700))

    def icon_button(self, x, y, ic, on):
        fill = self.accent if on else "none"
        col = self.on_icon if on else self.panel_text
        return (f'<rect x="{x - 20}" y="{y - 20}" width="40" height="40" rx="6" fill="{fill}" stroke="{self.panel_rule}" stroke-width="1.5"/>'
                + self.icon(ic, x, y, 0.8, col))

    def build_button(self, x, y, ic, hover):
        return (f'<rect x="{x - 34}" y="{y - 34}" width="68" height="68" rx="6" fill="{self.btn_fill_hover if hover else self.btn_fill}" stroke="{self.accent if hover else self.panel_rule}" stroke-width="{3 if hover else 1.5}"/>'
                + self.icon(ic, x, y, 1.35, self.accent))

    def tooltip(self, x, y, w, h, title, sub):
        return (self.box(x, y, w, h)
                + T(x + 24, y + 44, title, self.title_font, 21, self.panel_text, "start", 700)
                + T(x + 24, y + 76, sub, self.body_font, 17, self.panel_sub, "start", 600, style="font-style:italic"))

    def minimap(self, cx, cy, r):
        rnd = random.Random(4)
        out = [f'<rect x="{cx - r - 10}" y="{cy - r * 0.62 - 6}" width="{2 * r + 20}" height="{r * 1.36}" fill="{self.mini_water}"/>']
        n = 14
        s = r / n
        for i in range(n):
            for j in range(n):
                px = cx + (i - j) * s
                py = cy - r * 0.5 + (i + j) * s * 0.5
                dist = math.hypot(i - n / 2 + 0.5, j - n / 2 + 0.5)
                if dist > n / 2 + rnd.uniform(-1, 0.6):
                    continue
                c = rnd.choice(self.mini_colors)
                out.append(f'<path d="M{f(px)},{f(py)} l{f(s)},{f(s * 0.5)} l{f(-s)},{f(s * 0.5)} l{f(-s)},{f(-s * 0.5)}z" fill="{c}"/>')
        out.append(f'<rect x="{cx - 34}" y="{cy - 10}" width="68" height="42" fill="none" stroke="#fff" stroke-width="2"/>')
        return "".join(out)

    def icon(self, kind, x, y, s, c):
        """Simple line icons, drawn around (x, y)."""
        sw = 2.4
        g = f'transform="translate({f(x)},{f(y)}) scale({s})" fill="none" stroke="{c}" stroke-width="{sw}" stroke-linecap="round" stroke-linejoin="round"'
        p = {
            "house": "M-14,0 L0,-12 L14,0 M-10,-2 V12 H10 V-2 M-3,12 V4 H3 V12",
            "water": "M0,-14 C8,-2 12,4 12,8 C12,15 6,18 0,18 C-6,18 -12,15 -12,8 C-12,4 -8,-2 0,-14Z",
            "farm": "M0,16 V-12 M0,-6 Q-10,-10 -12,-18 M0,-2 Q10,-6 12,-14 M0,4 Q-9,0 -12,-6 M0,10 Q9,6 12,0",
            "industry": "M-12,14 L6,-4 M2,-12 L14,0 L8,6 L-4,-6Z",
            "trade": "M-16,4 Q0,14 16,4 L12,-1 H-12Z M-2,-1 V-18 L12,-4",
            "temple": "M-16,-4 Q0,-12 16,-4 M-12,-10 Q0,-18 12,-10 M-10,-4 V14 M10,-4 V14 M-14,14 H14 M0,-16 V-22",
            "learning": "M-12,-12 H10 Q14,-12 14,-8 V12 H-8 Q-12,12 -12,8Z M-6,-4 H8 M-6,2 H8",
            "defence": "M0,-16 L13,-10 V0 Q13,10 0,17 Q-13,10 -13,0 V-10Z",
            "monument": "M-14,16 H14 M-10,16 V10 H10 V16 M-7,10 Q-7,-2 0,-8 Q7,-2 7,10 M0,-8 V-20",
            "undo": "M-10,-6 H6 Q14,-6 14,4 Q14,14 4,14 H-6 M-10,-6 L-4,-12 M-10,-6 L-4,0",
            "population": "M0,-14 A6,6 0 1 1 0,-2 A6,6 0 1 1 0,-14 M-11,15 Q-11,2 0,2 Q11,2 11,15Z",
            "coin": "M0,-14 A14,14 0 1 1 0,14 A14,14 0 1 1 0,-14Z M-5,-5 H5 V5 H-5Z",
            "food": "M-14,-2 H14 Q12,14 0,14 Q-12,14 -14,-2Z M-6,-6 Q-2,-14 4,-8 Q8,-14 10,-6",
            "pause": "M-5,-10 V10 M5,-10 V10",
            "play": "M-6,-10 L8,0 L-6,10Z",
            "fast": "M-11,-9 L0,0 L-11,9Z M0,-9 L11,0 L0,9Z",
            "faster": "M-14,-9 L-5,0 L-14,9Z M-5,-9 L4,0 L-5,9Z M4,-9 L13,0 L4,9Z",
            "request": "M-12,-12 H12 V12 H-12Z M-6,-4 H6 M-6,3 H6",
            "warn": "M0,-14 L14,12 H-14Z M0,-4 V3 M0,8 V8.5",
            "map": "M-16,-10 L-6,-14 L6,-10 L16,-14 V10 L6,14 L-6,10 L-16,14Z M-6,-14 V10 M6,-10 V14",
        }[kind]
        return f'<path d="{p}" {g}/>'


# ====================================================================== Đại Long
class DaiLongScreens(Screens, DaiLong):
    accent = "#a8261c"
    note_fill = "#f2e4c0"
    btn_fill = "#efe0b8"
    btn_fill_hover = "#f7ebcc"
    mini_water = "#6f8f92"
    mm_palette = {}   # concept A's own colours: a green delta
    mini_colors = ["#8a9a52", "#7a8a48", "#a8261c", "#6b4426", "#c9a76a", "#5a7a3a"]
    bubble = (1010, 360, 560, (858, 610))
    stele_k = 0.42

    def __init__(self, sd):
        DaiLong.__init__(self, {"key": sd["key"]})
        self.sd = sd

    def frame_back_at(self, x, y, w, h):
        return (self.wood(x - 22, y - 22, w + 44, h + 44, "#3a2213", 7)
                + f'<rect x="{x - 22}" y="{y - 22}" width="{w + 44}" height="{h + 44}" fill="none" stroke="#1d0f07" stroke-width="3"/>')

    def frame_front_at(self, x, y, w, h):
        out = [f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="none" stroke="#1d0f07" stroke-width="4"/>',
               f'<rect x="{x - 12}" y="{y - 12}" width="{w + 24}" height="{h + 24}" fill="none" stroke="#a87545" stroke-width="1.5"/>']
        for (cx, cy, sx, sy) in [(x - 14, y - 14, 1, 1), (x + w + 14, y - 14, -1, 1), (x - 14, y + h + 14, 1, -1), (x + w + 14, y + h + 14, -1, -1)]:
            out.append(self.cloud_scroll(cx, cy, sx, sy))
        return "".join(out)

    def box(self, x, y, w, h, big=False):
        return (f'<rect x="{x - 6}" y="{y - 6}" width="{w + 12}" height="{h + 12}" fill="#2a170c" rx="5"/>'
                f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="#ecdcb4"/>'
                + self.noise(int(x), int(y), int(w), int(h), "0.008", 4, (0.55, 0.38, 0.15), 1.0, -0.45, 31)
                + f'<rect x="{x + 5}" y="{y + 5}" width="{w - 10}" height="{h - 10}" fill="none" stroke="{self.ink}" stroke-width="1" opacity="0.5"/>')

    def heading(self, x, y, s, size):
        return T(x, y, s, self.title_font, size, self.ink, "start", 800)

    def caption(self, x, y, w, text):
        lines = self.wrap(text, w - 70, 30, 0.45)
        h = 50 + len(lines) * 40
        # An "ink cloud" caption: paper with a scalloped ink edge.
        return (f'<rect x="{x + 6}" y="{y + 8}" width="{w}" height="{h}" rx="18" fill="#000" opacity="0.25"/>'
                f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="18" fill="#f6ead0" stroke="{self.ink}" stroke-width="3"/>'
                + self.cloud_scroll(x + w - 50, y + h - 8, 0.5, -0.5)
                + self.lines(x + 36, y + 54, lines, self.title_font, 30, self.ink, 40, weight=600, style="font-style:italic"))

    def speech(self, x, y, w, who, text, tip):
        lines = self.wrap(text, w - 70, 30, 0.45)
        h = 78 + len(lines) * 40
        tx, ty = tip
        return (f'<path d="M{x + 120},{y + h - 4} L{tx},{ty} L{x + 210},{y + h - 4}" fill="#fffaf0" stroke="{self.ink}" stroke-width="3" stroke-linejoin="round"/>'
                f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="40" fill="#fffaf0" stroke="{self.ink}" stroke-width="3.5"/>'
                f'<path d="M{x + 118},{y + h - 1.5} L{x + 212},{y + h - 1.5}" stroke="#fffaf0" stroke-width="4"/>'
                + T(x + 40, y + 44, who.upper(), self.title_font, 18, self.red, "start", 800, ls=2)
                + self.lines(x + 40, y + 84, lines, self.title_font, 30, self.ink, 40, weight=800))

    # -- story panel: woodblock print ------------------------------------
    def intro_scene(self, X, Y, PW, PH):
        ink = "#1f1712"
        out = [f'<rect x="{X}" y="{Y}" width="{PW}" height="{PH}" fill="#efdcb0"/>',
               self.noise(X, Y, PW, PH, "0.7", 2, (0.45, 0.3, 0.12), 0.35, -0.08, 2),
               self.noise(X, Y, PW, PH, "0.006", 3, (0.6, 0.4, 0.15), 1.0, -0.45, 3)]
        # Sun.
        sx, sy = X + 1330, Y + 250
        out.append(f'<circle cx="{sx}" cy="{sy}" r="150" fill="#c8402a" stroke="{ink}" stroke-width="4"/>')
        out.append(f'<circle cx="{sx}" cy="{sy}" r="122" fill="none" stroke="#e0703a" stroke-width="6" opacity="0.7"/>')
        # Far and near karst.
        rnd = random.Random(12)

        def karst_row(base, color, n, hmin, hmax, wmin, wmax, seed):
            r = random.Random(seed)
            o = []
            x = X - 60
            while x < X + PW + 60:
                w, h = r.uniform(wmin, wmax), r.uniform(hmin, hmax)
                p = (f"M{f(x)},{f(base)} C{f(x - 6)},{f(base - h * 0.7)} {f(x + w * 0.1)},{f(base - h)} {f(x + w * 0.5)},{f(base - h)} "
                     f"C{f(x + w * 0.9)},{f(base - h)} {f(x + w + 6)},{f(base - h * 0.6)} {f(x + w)},{f(base)}Z")
                o.append(f'<path d="{p}" fill="{color}" stroke="{ink}" stroke-width="3.5"/>')
                o.append(f'<path d="M{f(x + w * 0.3)},{f(base - h * 0.85)} q-10,{f(h * 0.3)} 0,{f(h * 0.6)}" fill="none" stroke="{ink}" stroke-width="2" opacity="0.6"/>')
                x += w * r.uniform(0.55, 0.9)
            return "".join(o)

        out.append(karst_row(Y + 470, "#8fa9a2", 0, 160, 300, 90, 160, 5))
        out.append(self.cloud_band(X - 40, Y + 400, PW + 80, 9, ink))
        out.append(karst_row(Y + 590, "#4f7f62", 0, 140, 330, 120, 200, 9))
        # River with woodblock waves.
        river = f"M{X},{Y + 560} Q{X + 600},{Y + 530} {X + 1100},{Y + 575} T{X + PW},{Y + 555} V{Y + 790} H{X}Z"
        out.append(f'<path d="{river}" fill="#2f5878" stroke="{ink}" stroke-width="3.5"/>')
        for row in range(4):
            for k in range(14):
                wx = X + 40 + k * 130 + (row % 2) * 65 + rnd.uniform(-10, 10)
                wy = Y + 610 + row * 42
                out.append(f'<path d="M{f(wx)},{f(wy)} q18,-16 36,0 q-12,-4 -14,6" fill="none" stroke="#eadfc4" stroke-width="2.5" stroke-linecap="round"/>')
        # A leaping carp.
        fx, fy = X + 1240, Y + 620
        out.append(f'<g transform="translate({fx},{fy}) rotate(-30)"><path d="M-40,0 Q0,-26 40,0 Q0,22 -40,0Z" fill="#d9662e" stroke="{ink}" stroke-width="3"/>'
                   f'<path d="M-40,0 L-64,-18 L-60,0 L-64,18Z" fill="#d9662e" stroke="{ink}" stroke-width="3"/>'
                   f'<circle cx="24" cy="-4" r="3.5" fill="{ink}"/><path d="M-8,-14 q6,14 0,26" fill="none" stroke="{ink}" stroke-width="2"/></g>')
        # The near bank.
        bank = f"M{X - 10},{Y + 740} Q{X + 400},{Y + 700} {X + 900},{Y + 735} T{X + PW + 10},{Y + 720} V{Y + PH + 10} H{X - 10}Z"
        out.append(f'<path d="{bank}" fill="#c99a55" stroke="{ink}" stroke-width="4"/>')
        for k in range(40):
            hx, hy = X + rnd.uniform(20, PW - 20), Y + rnd.uniform(770, PH - 10)
            out.append(f'<path d="M{f(hx)},{f(hy)} l14,-3" stroke="{ink}" stroke-width="1.6" opacity="0.5"/>')
        for k in range(22):
            gx = X + rnd.uniform(0, PW)
            gy = Y + 745 + rnd.uniform(-8, 14)
            out.append(f'<path d="M{f(gx)},{f(gy)} q-4,-18 -10,-24 M{f(gx)},{f(gy)} q2,-20 6,-28 M{f(gx)},{f(gy)} q6,-12 14,-18" fill="none" stroke="#3f6a2e" stroke-width="3" stroke-linecap="round"/>')
        # Cranes.
        for (cx, cy, sc, flip) in [(X + 980, Y + 120, 1.0, 1), (X + 1120, Y + 200, 0.8, 1), (X + 1540, Y + 120, 0.9, -1), (X + 820, Y + 230, 0.7, 1), (X + 1650, Y + 300, 0.6, -1)]:
            out.append(self.crane(cx, cy, sc, flip, ink))
        # Settlers.
        for k, (px, sc) in enumerate([(X + 230, 0.82), (X + 360, 0.86), (X + 480, 0.8), (X + 590, 0.84)]):
            out.append(self.figure(px, Y + 820 - k * 4, sc, ["#7a3b26", "#4f6a3a", "#2f4a6e", "#8a5a2e"][k], hat=True, pole=k % 2 == 0, ink=ink))
        # Khai Sơn driving the stake.
        out.append(self.figure(X + 790, Y + 860, 1.45, "#2f4a6e", stake=True, ink=ink))
        return "".join(out)

    def cloud_band(self, x, y, w, n, ink):
        out = []
        step = w / n
        for k in range(n):
            cx = x + k * step + step / 2
            p = (f"M{f(cx - step * 0.6)},{f(y)} q{f(step * 0.15)},-34 {f(step * 0.32)},-12 q{f(step * 0.12)},-40 {f(step * 0.36)},-14 "
                 f"q{f(step * 0.2)},-28 {f(step * 0.34)},6 q{f(step * 0.1)},20 -{f(step * 0.1)},20 H{f(cx - step * 0.6)}Z")
            out.append(f'<path d="{p}" fill="#f6ead0" stroke="{ink}" stroke-width="3"/>')
            out.append(f'<path d="M{f(cx - step * 0.2)},{f(y - 14)} q10,-10 20,0" fill="none" stroke="{ink}" stroke-width="2"/>')
        return "".join(out)

    def crane(self, x, y, s, flip, ink):
        return (f'<g transform="translate({x},{y}) scale({s * flip},{s})">'
                f'<path d="M-50,10 Q-10,0 40,-6 L70,-14" fill="none" stroke="{ink}" stroke-width="4" stroke-linecap="round"/>'
                f'<path d="M-30,6 Q-60,-50 -110,-70 Q-60,-20 -20,4Z" fill="#f8f1e0" stroke="{ink}" stroke-width="3"/>'
                f'<path d="M-90,-62 Q-104,-68 -110,-70 Q-100,-58 -86,-52Z" fill="{ink}"/>'
                f'<path d="M-10,4 Q10,-56 50,-80 Q30,-30 6,6Z" fill="#f8f1e0" stroke="{ink}" stroke-width="3"/>'
                f'<path d="M40,-72 Q46,-78 50,-80 Q46,-68 38,-62Z" fill="{ink}"/>'
                f'<ellipse cx="-4" cy="6" rx="34" ry="11" fill="#f8f1e0" stroke="{ink}" stroke-width="3"/>'
                f'<path d="M-36,8 L-90,22 M-36,10 L-88,28" stroke="{ink}" stroke-width="2.5"/>'
                f'<circle cx="62" cy="-13" r="4" fill="#c8402a"/></g>')

    def figure(self, x, base, s, robe, hat=False, pole=False, stake=False, ink="#1f1712"):
        skin = "#e7b98a"
        o = [f'<g transform="translate({x},{base}) scale({s})">']
        o.append(f'<path d="M-16,-62 L-18,0 H-4 L-2,-62Z M2,-62 L4,0 H18 L16,-62Z" fill="#f0e6d0" stroke="{ink}" stroke-width="3"/>')
        o.append(f'<path d="M-32,-58 L-24,-150 Q0,-160 24,-150 L32,-58Z" fill="{robe}" stroke="{ink}" stroke-width="3.5" stroke-linejoin="round"/>')
        o.append(f'<path d="M-26,-110 H26 L25,-100 H-25Z" fill="#c8402a" stroke="{ink}" stroke-width="2"/>')
        o.append(f'<path d="M0,-150 L-10,-120 M0,-150 L10,-120" stroke="{ink}" stroke-width="2"/>')
        if stake:
            o.append(f'<path d="M84,-150 L96,20" stroke="#6b4426" stroke-width="16" stroke-linecap="round"/>'
                     f'<path d="M84,-150 L96,20" stroke="{ink}" stroke-width="2" fill="none"/>'
                     f'<path d="M-20,-146 Q30,-150 80,-146 M20,-146 Q50,-140 82,-134" fill="none" stroke="{robe}" stroke-width="16" stroke-linecap="round"/>'
                     f'<path d="M-20,-146 Q30,-150 80,-146 M20,-146 Q50,-140 82,-134" fill="none" stroke="{ink}" stroke-width="2"/>'
                     f'<circle cx="84" cy="-142" r="9" fill="{skin}" stroke="{ink}" stroke-width="2.5"/>')
        elif pole:
            o.append(f'<path d="M-90,-150 Q0,-160 90,-150" stroke="#6b4426" stroke-width="6" fill="none"/>'
                     f'<path d="M-84,-150 L-96,-100 M-84,-150 L-72,-100 M84,-150 L72,-100 M84,-150 L96,-100" stroke="{ink}" stroke-width="2"/>'
                     f'<path d="M-104,-100 H-64 L-70,-72 H-98Z M64,-100 H104 L98,-72 H70Z" fill="#c9a24e" stroke="{ink}" stroke-width="3"/>'
                     f'<path d="M-24,-146 L-46,-150 M24,-146 L40,-152" stroke="{robe}" stroke-width="12" stroke-linecap="round"/>')
        else:
            o.append(f'<path d="M-24,-144 L-34,-90 M24,-144 L34,-90" stroke="{robe}" stroke-width="12" stroke-linecap="round"/>')
        o.append(f'<circle cx="0" cy="-174" r="18" fill="{skin}" stroke="{ink}" stroke-width="3"/>')
        if hat:
            o.append(f'<path d="M-46,-172 L0,-212 L46,-172 Q0,-160 -46,-172Z" fill="#e8d19a" stroke="{ink}" stroke-width="3"/>'
                     f'<path d="M-20,-188 L0,-206 L20,-188" fill="none" stroke="{ink}" stroke-width="1.5" opacity="0.6"/>')
        else:
            o.append(f'<path d="M-18,-178 Q-18,-196 0,-196 Q18,-196 18,-178 Q8,-186 -18,-178Z" fill="{ink}"/>'
                     f'<circle cx="0" cy="-200" r="8" fill="{ink}"/><path d="M-18,-180 H18" stroke="#c8402a" stroke-width="3"/>')
            o.append(f'<circle cx="7" cy="-172" r="2" fill="{ink}"/>')
        o.append("</g>")
        return "".join(o)

    # -- stele ------------------------------------------------------------
    def stele_backdrop(self):
        # A red lacquer niche behind the stele.
        x, y, w, h = 110, 40, 700, 1010
        g = self.lin([(0, "#8e1d17"), (1, "#5e110c")])
        return (f'<path d="M{x},{y + h} V{y + 120} Q{x + w / 2},{y - 40} {x + w},{y + 120} V{y + h}Z" fill="{g}" stroke="#d6a65c" stroke-width="4"/>'
                f'<path d="M{x + 18},{y + h} V{y + 132} Q{x + w / 2},{y - 16} {x + w - 18},{y + 132} V{y + h}" fill="none" stroke="#d6a65c" stroke-width="1.5"/>')

    def stele(self, x, y, w, h, title, lines):
        ink = "#2b2620"
        sx, sw = x + 70, w - 140
        top, bot = y + 40, y + h - 170
        stone = self.lin([(0, "#a7aca4"), (1, "#8a8f88")], 0, 0, 1, 0)
        out = []
        # Tortoise base.
        tx, ty = x + w / 2, y + h - 70
        out.append(f'<ellipse cx="{tx}" cy="{ty + 40}" rx="300" ry="26" fill="#000" opacity="0.35"/>')
        out.append(f'<path d="M{tx - 290},{ty + 30} Q{tx - 280},{ty - 110} {tx},{ty - 120} Q{tx + 280},{ty - 110} {tx + 290},{ty + 30}Z" fill="#7f857e" stroke="{ink}" stroke-width="3"/>')
        for k in range(-3, 4):
            out.append(f'<path d="M{tx + k * 76 - 30},{ty - 40 + abs(k) * 12} l30,-22 l30,22 l-6,40 h-48z" fill="none" stroke="{ink}" stroke-width="2" opacity="0.6"/>')
        out.append(f'<path d="M{tx - 40},{ty + 30} Q{tx - 44},{ty + 70} {tx},{ty + 78} Q{tx + 44},{ty + 70} {tx + 40},{ty + 30}Z" fill="#8a9089" stroke="{ink}" stroke-width="3"/>'
                   f'<circle cx="{tx - 16}" cy="{ty + 52}" r="4" fill="{ink}"/><circle cx="{tx + 16}" cy="{ty + 52}" r="4" fill="{ink}"/>')
        # Slab.
        slab = f"M{sx},{bot} V{top + 120} Q{sx},{top} {sx + sw / 2},{top} Q{sx + sw},{top} {sx + sw},{top + 120} V{bot}Z"
        out.append(f'<path d="{slab}" fill="{stone}" stroke="{ink}" stroke-width="3"/>')
        cid = self.nid("slab")
        self.defs.append(f'<clipPath id="{cid}"><path d="{slab}"/></clipPath>')
        out.append(f'<g clip-path="url(#{cid})">' + self.noise(sx, top, sw, bot - top, "0.05", 4, (0.2, 0.2, 0.18), 1.2, -0.5, 61)
                   + self.noise(sx, top, sw, bot - top, "0.9", 2, (1, 1, 0.95), 0.5, -0.22, 62) + "</g>")
        # Crest: a sun disc between clouds.
        out.append(f'<circle cx="{sx + sw / 2}" cy="{top + 70}" r="30" fill="none" stroke="{ink}" stroke-width="3"/>'
                   f'<circle cx="{sx + sw / 2}" cy="{top + 70}" r="18" fill="{self.red}" opacity="0.85"/>')
        for d in (-1, 1):
            out.append(f'<path d="M{sx + sw / 2 + d * 44},{top + 76} q{d * 30},-30 {d * 60},-6 q{d * 20},16 {d * 6},28 q{d * -14},6 {d * -18},-8" fill="none" stroke="{ink}" stroke-width="3" stroke-linecap="round"/>')
        out.append(f'<path d="M{sx + 30},{top + 130} H{sx + sw - 30} M{sx + 30},{bot - 30} H{sx + sw - 30}" stroke="{ink}" stroke-width="2" opacity="0.7"/>')
        out.append(self.stele_lines(sx + 54, top + 178, sw - 100, title, lines, 30, 23, 32, ink, self.red))
        return "".join(out)

    def carve(self, x, y, s, font, size, color, anchor, new=False):
        return (T(x + 1, y + 1, s, font, size, "#d8dcd4", anchor, 700, extra='opacity="0.5"') + T(x, y, s, font, size, color, anchor, 700))

    def fresh_mark(self, x, y):
        return self.seal(x - 4, y, 22, "")


# ====================================================================== Kambara
class KambaraScreens(Screens, Kambara):
    accent = "#9a4a2e"
    note_fill = "#c9bc9c"
    btn_fill = "#c4b796"
    btn_fill_hover = "#d8ccae"
    mini_water = "#3f6a62"
    mm_palette = dict(land_low="#8fb565", land_high="#a9b874", forest="#3f7a3c", shallow="#5fb0ae", deep="#2a6a72")   # lush lake country
    mini_colors = ["#6f8f44", "#5f7f3a", "#8d8169", "#9a4a2e", "#c4b696", "#4f6f30"]
    caption_pos = (110, 92)
    bubble = (1150, 404, 660, (1786, 720))
    stele_k = 0.58
    tooltip_w = 410
    note_fill = "#c9bc9c"

    def __init__(self, sd):
        Kambara.__init__(self, {"key": sd["key"]})
        self.sd = sd

    def frame_back_at(self, x, y, w, h):
        x0, y0, w0, h0 = x - 26, y - 26, w + 52, h + 52
        pid = self.nid("petal")
        self.defs.append(f'<pattern id="{pid}" width="22" height="16" patternUnits="userSpaceOnUse">'
                         f'<path d="M1,16 Q1,3 11,1 Q21,3 21,16" fill="#8d8169" stroke="#3a352a" stroke-width="1.2"/></pattern>')
        return (f'<rect x="{x0 + 6}" y="{y0 + 8}" width="{w0}" height="{h0}" fill="#000" opacity="0.4"/>'
                + self.stone(x0, y0, w0, h0, "#a89c80", 5)
                + f'<rect x="{x0 + 6}" y="{y0 + 5}" width="{w0 - 12}" height="16" fill="url(#{pid})"/>'
                + self.moss_patch(x0, y0, w0, h0, 12, 0.3))

    def frame_front_at(self, x, y, w, h):
        out = [f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="none" stroke="#2c2a22" stroke-width="3"/>']
        out.append(self.vine([(x - 30, y - 20), (x + 60, y - 8), (x + 160, y - 18), (x + 260, y - 6)], 1, 16))
        out.append(self.vine([(x + w - 300, y - 16), (x + w - 160, y - 6), (x + w - 40, y - 18), (x + w + 24, y + 60), (x + w + 16, y + 180)], 3, 22))
        out.append(self.vine([(x - 22, y + h - 200), (x - 14, y + h - 60), (x + 90, y + h + 14)], 4, 12))
        return "".join(out)

    def box(self, x, y, w, h, big=False):
        out = [f'<rect x="{x + 4}" y="{y + 6}" width="{w}" height="{h}" fill="#000" opacity="0.4"/>',
               self.stone(int(x), int(y), int(w), int(h), "#b3a789", 41 + int(x) % 5),
               self.moss_patch(int(x), int(y), int(w), int(h), 42, 0.8),
               f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="none" stroke="#1c1a14" stroke-width="2.5"/>',
               f'<rect x="{x + 6}" y="{y + 6}" width="{w - 12}" height="{h - 12}" fill="none" stroke="#5d5646" stroke-width="1"/>']
        if big:
            out.append(self.vine([(x + w - 160, y - 12), (x + w - 40, y - 4), (x + w + 10, y + 70), (x + w + 4, y + 170)], 9, 14))
        return "".join(out)

    def heading(self, x, y, s, size):
        return self.engrave(x, y, s, self.title_font, size, "start", 700)

    def caption(self, x, y, w, text):
        lines = self.wrap(text, w - 70, 27, 0.55)
        h = 52 + len(lines) * 38
        return (f'<rect x="{x + 6}" y="{y + 8}" width="{w}" height="{h}" fill="#000" opacity="0.35"/>'
                + self.stone(int(x), int(y), w, h, "#c4b796", 71)
                + f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="none" stroke="#2c2a22" stroke-width="2.5"/>'
                f'<rect x="{x + 6}" y="{y + 6}" width="{w - 12}" height="{h - 12}" fill="none" stroke="{self.laterite}" stroke-width="1.2"/>'
                + "".join(self.engrave(x + 34, y + 54 + k * 38, s, self.title_font, 27, "start", 700) for k, s in enumerate(lines)))

    def speech(self, x, y, w, who, text, tip):
        lines = self.wrap(text, w - 70, 25, 0.55)
        h = 80 + len(lines) * 36
        tx, ty = tip
        return (f'<path d="M{x + 40},{y + h - 4} L{tx},{ty} L{x + 120},{y + h - 4}" fill="#efe6cf" stroke="#2c2a22" stroke-width="3" stroke-linejoin="round"/>'
                f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="30" fill="#efe6cf" stroke="#2c2a22" stroke-width="3"/>'
                f'<path d="M{x + 38},{y + h - 1.5} L{x + 122},{y + h - 1.5}" stroke="#efe6cf" stroke-width="4"/>'
                + T(x + 36, y + 42, who.upper(), self.title_font, 18, self.laterite, "start", 700, ls=2)
                + self.lines(x + 36, y + 80, lines, self.label_font, 25, self.ink, 36, weight=600))

    # -- story panel: temple bas-relief -----------------------------------
    def relief_filter(self):
        fid = self.nid("relief")
        self.defs.append(
            f'<filter id="{fid}" x="-5%" y="-5%" width="110%" height="110%">'
            f'<feGaussianBlur in="SourceAlpha" stdDeviation="2.2" result="b"/>'
            f'<feDiffuseLighting in="b" surfaceScale="6" diffuseConstant="1.15" lighting-color="#fff6e6" result="l"><feDistantLight azimuth="230" elevation="38"/></feDiffuseLighting>'
            f'<feComposite in="l" in2="SourceAlpha" operator="in" result="lit"/>'
            f'<feBlend in="SourceGraphic" in2="lit" mode="multiply" result="shaded"/>'
            f'<feOffset in="b" dx="4" dy="5" result="o"/><feFlood flood-color="#1c1810" flood-opacity="0.55"/><feComposite in2="o" operator="in" result="shadow"/>'
            f'<feMerge><feMergeNode in="shadow"/><feMergeNode in="shaded"/></feMerge></filter>')
        return f"url(#{fid})"

    def intro_scene(self, X, Y, PW, PH):
        fill = "#ddd0b2"
        out = [self.stone(X, Y, PW, PH, "#b9ac8d", 81)]
        rel = []
        cx = X + PW / 2
        # Upper register: the temple-mountain under the sun and moon.
        for k, (tw, th) in enumerate([(900, 56), (720, 54), (540, 52)]):
            ty = Y + 372 - sum(t for _, t in [(900, 56), (720, 54), (540, 52)][:k + 1])
            rel.append(f'<rect x="{cx - tw / 2}" y="{ty}" width="{tw}" height="{th - 6}" fill="{fill}"/>')
            for c in range(int(tw / 46)):
                rel.append(f'<rect x="{cx - tw / 2 + 12 + c * 46}" y="{ty + 14}" width="18" height="{th - 26}" fill="#b9ac8d"/>')
        base = Y + 372 - 162
        for dx, s in [(-260, 2.3), (260, 2.3), (-130, 2.8), (130, 2.8), (0, 3.9)]:
            rel.append(self.prang(cx + dx, base + (8 if s < 3 else 4), s, fill, fill, 0.1))
        rel.append(f'<circle cx="{X + PW - 420}" cy="{Y + 170}" r="58" fill="{fill}"/>')
        rel.append(f'<path d="M{X + PW - 200},{Y + 112} A58,58 0 1 1 {X + PW - 200},{Y + 228} A44,58 0 1 0 {X + PW - 200},{Y + 112}Z" fill="{fill}"/>')
        for k in range(30):
            rx = X + 60 + k * (PW - 120) / 29
            if abs(rx - cx) < 560 and k % 1 == 0 and Y + 40 > 0 and abs(rx - cx) < 560:
                pass
            rel.append(f'<circle cx="{f(rx)}" cy="{Y + 30}" r="9" fill="{fill}"/>')
        # Lotus-petal bands.
        for by in (Y + 384, Y + 552):
            for k in range(int(PW / 40) + 1):
                px = X + k * 40
                rel.append(f'<path d="M{px},{by + 24} Q{px},{by} {px + 20},{by - 2} Q{px + 40},{by} {px + 40},{by + 24}Z" fill="{fill}"/>')
        # Middle register: the lake, fish and a boat.
        for row in range(3):
            for k in range(int(PW / 70) + 2):
                wx = X + k * 70 + (row % 2) * 35
                wy = Y + 446 + row * 38
                rel.append(f'<path d="M{wx},{wy} q17,-18 35,0 q17,18 35,0" fill="none" stroke="{fill}" stroke-width="7"/>')
        for (fx, fy, d) in [(X + 300, Y + 500, 1), (X + 1350, Y + 470, -1), (X + 1500, Y + 520, 1), (X + 520, Y + 455, -1)]:
            rel.append(f'<g transform="translate({fx},{fy}) scale({d},1)"><path d="M-34,0 Q0,-20 34,0 Q0,20 -34,0Z M-34,0 L-54,-14 L-50,0 L-54,14Z" fill="{fill}"/></g>')
        bx, by = X + 860, Y + 540
        rel.append(f'<path d="M{bx - 300},{by - 54} Q{bx - 250},{by} {bx - 150},{by} H{bx + 150} Q{bx + 250},{by} {bx + 300},{by - 54} Q{bx + 200},{by - 32} {bx},{by - 32} Q{bx - 200},{by - 32} {bx - 300},{by - 54}Z" fill="{fill}"/>')
        for k in range(8):
            rel.append(self.relief_figure(bx - 210 + k * 60, by - 30, 0.48, fill, oar=True))
        # Lower register: a procession with parasols and an elephant.
        gy = Y + PH - 24
        for k in range(5):
            rel.append(self.relief_figure(X + 120 + k * 120, gy, 1.0, fill, parasol=k % 2 == 0))
        rel.append(self.elephant(X + 880, gy, 1.5, fill))
        rel.append(self.relief_figure(X + 886, gy - 150, 0.62, fill, crown=True))
        rel.append(f'<path d="M{X + 900},{gy - 246} V{gy - 150}" stroke="{fill}" stroke-width="6"/><ellipse cx="{X + 900}" cy="{gy - 248}" rx="52" ry="12" fill="{fill}"/><ellipse cx="{X + 900}" cy="{gy - 264}" rx="34" ry="9" fill="{fill}"/>')
        for k in range(5):
            rel.append(self.relief_figure(X + 1080 + k * 120, gy, 1.0, fill, parasol=k % 2 == 1))
        rel.append(self.relief_figure(X + 1720, gy, 1.08, fill, rod=True))
        out.append(f'<g filter="{self.relief_filter()}">{"".join(rel)}</g>')
        # Register lines cut into the stone.
        for ly in (Y + 378, Y + 414, Y + 546, Y + 582):
            out.append(f'<line x1="{X}" y1="{ly}" x2="{X + PW}" y2="{ly}" stroke="#2c2a22" stroke-width="2" opacity="0.5"/>')
        out.append(self.moss_patch(X, Y, PW, PH, 83, 0.5))
        return "".join(out)

    def relief_figure(self, x, base, s, fill, parasol=False, rod=False, oar=False, crown=False):
        p = ("M-6,-122 L18,-122 L12,-82 L-4,-82Z M-12,-82 L18,-82 L24,-46 L-16,-46Z "
             "M-10,-46 L-2,-46 L-8,0 L-20,0Z M6,-46 L14,-46 L24,0 L12,0Z "
             "M12,-118 L34,-100 L30,-94 L8,-108Z M-4,-118 L-20,-96 L-14,-92 L0,-108Z")
        o = [f'<g transform="translate({f(x)},{f(base)}) scale({s})"><path d="{p}" fill="{fill}"/>'
             f'<circle cx="6" cy="-136" r="13" fill="{fill}"/><path d="M-4,-144 L6,{-176 if crown else -164} L16,-144Z" fill="{fill}"/>']
        if parasol:
            o.append(f'<path d="M32,-98 L35,-200" stroke="{fill}" stroke-width="5"/><ellipse cx="35" cy="-202" rx="32" ry="8" fill="{fill}"/><ellipse cx="35" cy="-214" rx="20" ry="6" fill="{fill}"/>')
        if rod:
            o.append(f'<path d="M32,-98 L40,-196" stroke="{fill}" stroke-width="5"/><path d="M-6,-150 Q6,-148 18,-150" stroke="{fill}" stroke-width="5"/>')
        if oar:
            o.append(f'<path d="M30,-96 L-20,30" stroke="{fill}" stroke-width="6"/>')
        o.append("</g>")
        return "".join(o)

    def elephant(self, x, base, s, fill):
        return (f'<g transform="translate({x},{base}) scale({s})" fill="{fill}">'
                f'<ellipse cx="0" cy="-60" rx="52" ry="36"/><circle cx="46" cy="-68" r="24"/>'
                f'<path d="M62,-64 Q78,-40 70,-6 L62,-4 Q66,-36 52,-52Z"/>'
                f'<path d="M-44,-36 h14 v36 h-14Z M-22,-36 h14 v36 h-14Z M14,-36 h14 v36 h-14Z M32,-40 h14 v40 h-14Z"/>'
                f'<path d="M-52,-66 L-60,-30 L-56,-30 L-48,-60Z"/>'
                f'<path d="M-30,-96 H30 V-84 H-30Z"/></g>')

    # -- stele ------------------------------------------------------------
    def stele_backdrop(self):
        return self.vine([(60, 1060), (90, 700), (70, 380), (120, 80)], 21, 26) + self.vine([(840, 1060), (820, 640), (850, 300)], 22, 18)

    def stele(self, x, y, w, h, title, lines):
        ink = "#2c2a22"
        sx, sw = x + 60, w - 120
        top, bot = y + 30, y + h - 150
        out = []
        # Lotus pedestal.
        px, py = x + w / 2, y + h - 150
        out.append(f'<ellipse cx="{px}" cy="{y + h - 10}" rx="320" ry="22" fill="#000" opacity="0.4"/>')
        out.append(self.stone(int(x + 10), int(py), int(w - 20), 140, "#9d9176", 91))
        out.append(f'<rect x="{x + 10}" y="{py}" width="{w - 20}" height="140" fill="none" stroke="{ink}" stroke-width="2.5"/>')
        for k in range(14):
            lx = x + 26 + k * (w - 52) / 13
            out.append(f'<path d="M{f(lx - 20)},{py + 70} Q{f(lx - 20)},{py + 30} {f(lx)},{py + 22} Q{f(lx + 20)},{py + 30} {f(lx + 20)},{py + 70}" fill="#b3a789" stroke="{ink}" stroke-width="1.8"/>')
            out.append(f'<path d="M{f(lx - 20)},{py + 80} Q{f(lx - 20)},{py + 120} {f(lx)},{py + 128} Q{f(lx + 20)},{py + 120} {f(lx + 20)},{py + 80}" fill="#b3a789" stroke="{ink}" stroke-width="1.8"/>')
        # Slab with a pointed top.
        slab = f"M{sx},{bot} V{top + 110} Q{sx + 10},{top + 40} {sx + sw / 2},{top} Q{sx + sw - 10},{top + 40} {sx + sw},{top + 110} V{bot}Z"
        out.append(f'<path d="{slab}" fill="#000" opacity="0.4" transform="translate(6,8)"/>')
        cid = self.nid("slab")
        self.defs.append(f'<clipPath id="{cid}"><path d="{slab}"/></clipPath>')
        out.append(f'<g clip-path="url(#{cid})">' + self.stone(sx, top, sw, bot - top, "#c2b596", 92) + self.moss_patch(sx, top, sw, bot - top, 93, 0.6) + "</g>")
        out.append(f'<path d="{slab}" fill="none" stroke="{ink}" stroke-width="3"/>')
        out.append(f'<path d="M{sx + 16},{bot - 16} V{top + 116} Q{sx + 26},{top + 54} {sx + sw / 2},{top + 18} Q{sx + sw - 26},{top + 54} {sx + sw - 16},{top + 116} V{bot - 16}Z" fill="none" stroke="{ink}" stroke-width="1.5" opacity="0.6"/>')
        out.append(self.prang(sx + sw / 2, top + 120, 1.3, "#b3a789"))
        out.append(self.stele_lines(sx + 50, top + 180, sw - 90, title, lines, 26, 21, 31, ink, self.laterite, font=self.label_font))
        out.append(self.vine([(sx - 24, bot + 10), (sx - 16, bot - 120), (sx - 8, bot - 200)], 31, 8))
        return "".join(out)

    def carve(self, x, y, s, font, size, color, anchor, new=False):
        return self.engrave(x, y, s, font, size, anchor, 700, fill=color)


# ====================================================================== Tampadesa
class TampadesaScreens(Screens, Tampadesa):
    accent = "#8a2216"
    note_fill = "#f3dfb4"
    btn_fill = "#e8cfa0"
    btn_fill_hover = "#f6e3b8"
    mini_water = "#6f8f86"
    mm_palette = dict(land_low="#cdb27a", land_high="#dcc28c", forest="#7f9a4e", rocks="#b0906a", shallow="#79aeb0", deep="#3b7882",
                      field="#c9c46a", field_row="#a3a64a")   # dry plain
    mini_colors = ["#d8b47a", "#c99a55", "#9b4527", "#8a6418", "#7a8a48", "#e3bf86"]
    caption_pos = (110, 742)
    bubble = (900, 84, 560, (1585, 236))
    stele_k = 0.5

    def __init__(self, sd):
        Tampadesa.__init__(self, {"key": sd["key"]})
        self.sd = sd

    def frame_back_at(self, x, y, w, h):
        x0, y0, w0, h0 = x - 24, y - 24, w + 48, h + 48
        return (f'<rect x="{x0 + 6}" y="{y0 + 8}" width="{w0}" height="{h0}" fill="#000" opacity="0.4"/>'
                + self.teak(x0, y0, w0, h0, 4)
                + f'<rect x="{x0 + 6}" y="{y0 + 6}" width="{w0 - 12}" height="{h0 - 12}" fill="none" stroke="{self.gold()}" stroke-width="5"/>')

    def frame_front_at(self, x, y, w, h):
        out = [f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="none" stroke="#2a1608" stroke-width="3"/>']
        for cx, cy in [(x - 10, y + 22), (x + w + 10, y + 22), (x - 10, y + h + 26), (x + w + 10, y + h + 26)]:
            out.append(self.stupa(cx, cy, 1.15))
        return "".join(out)

    def hud(self):
        # The HUD uses earthen brown panels with gilt trim, and light text.
        self.box_style = "lacquer"
        self.panel_text, self.panel_sub, self.panel_rule = "#f3e2bf", "#d9b98a", "#8a6a48"
        self.accent, self.btn_fill, self.btn_fill_hover = "#e9c977", "#4e3826", "#634831"
        self.on_icon = "#3b2412"
        Screens.hud(self)

    def section_title(self, x, y, s):
        return T(x, y, s.upper(), self.label_font, 17, self.accent if self.box_style == "lacquer" else "#7a1d12", "start", 700, ls=2)

    def box(self, x, y, w, h, big=False):
        if self.box_style == "lacquer":
            g = self.lin([(0, "#6e5038"), (1, "#4f3825")])
            return (f'<rect x="{x + 4}" y="{y + 6}" width="{w}" height="{h}" fill="#000" opacity="0.45" rx="4"/>'
                    f'<rect x="{x - 4}" y="{y - 4}" width="{w + 8}" height="{h + 8}" fill="#2a1c10" rx="5"/>'
                    f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{g}" stroke="{self.gold()}" stroke-width="2.5" rx="3"/>'
                    f'<rect x="{x + 6}" y="{y + 6}" width="{w - 12}" height="{h - 12}" fill="none" stroke="#d9b98a" stroke-width="0.8" opacity="0.5" rx="2"/>'
                    + self.noise(int(x), int(y), int(w), int(h), "0.004 0.08", 3, (0.2, 0.12, 0.05), 0.9, -0.3, 131))
        return (f'<rect x="{x + 4}" y="{y + 6}" width="{w}" height="{h}" fill="#000" opacity="0.4"/>'
                + self.teak(int(x - 6), int(y - 6), int(w + 12), int(h + 12), 9)
                + f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="#ecd3a3"/>'
                + self.noise(int(x), int(y), int(w), int(h), "0.01", 4, (0.62, 0.36, 0.16), 1.4, -0.7, 51)
                + f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="none" stroke="{self.gold()}" stroke-width="3"/>')

    def heading(self, x, y, s, size):
        return T(x, y, s, self.title_font, size, self.ink, "start", 400)

    def caption(self, x, y, w, text):
        lines = self.wrap(text, w - 70, 30, 0.5)
        h = 50 + len(lines) * 40
        return (f'<rect x="{x + 6}" y="{y + 8}" width="{w}" height="{h}" fill="#000" opacity="0.3"/>'
                f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="#7a1d12" stroke="{self.gold()}" stroke-width="4"/>'
                + self.lines(x + 34, y + 54, lines, self.title_font, 30, "#f7e3b0", 40, weight=400))

    def speech(self, x, y, w, who, text, tip):
        lines = self.wrap(text, w - 70, 27, 0.48)
        h = 78 + len(lines) * 38
        tx, ty = tip
        return (f'<path d="M{x + w - 160},{y + h - 4} L{tx},{ty} L{x + w - 80},{y + h - 4}" fill="#fbf1dc" stroke="#3b2412" stroke-width="3" stroke-linejoin="round"/>'
                f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="34" fill="#fbf1dc" stroke="#3b2412" stroke-width="3"/>'
                f'<path d="M{x + w - 162},{y + h - 1.5} L{x + w - 78},{y + h - 1.5}" stroke="#fbf1dc" stroke-width="4"/>'
                + T(x + 36, y + 42, who.upper(), self.label_font, 18, "#8a2216", "start", 700, ls=2)
                + self.lines(x + 36, y + 80, lines, self.body_font, 27, self.ink, 38, weight=700))

    # -- story panel: temple mural ----------------------------------------
    def intro_scene(self, X, Y, PW, PH):
        ink = "#3b2412"
        out = []
        regs = [(Y, 330, "#a83a22"), (Y + 350, 340, "#d9a24e"), (Y + 710, PH - 710, "#efe4cc")]
        for ry, rh, c in regs:
            out.append(f'<rect x="{X}" y="{ry}" width="{PW}" height="{rh}" fill="{c}"/>')
        out.append(self.noise(X, Y, PW, PH, "0.02", 3, (0.95, 0.9, 0.8), 1.0, -0.55, 101))
        out.append(self.noise(X, Y, PW, PH, "0.6", 2, (0.3, 0.15, 0.05), 0.4, -0.1, 102))
        for by in (Y + 330, Y + 690):
            out.append(f'<rect x="{X}" y="{by}" width="{PW}" height="20" fill="#3b2412"/>')
            for k in range(int(PW / 24)):
                out.append(f'<circle cx="{X + 12 + k * 24}" cy="{by + 10}" r="4" fill="{"#e8c15a" if k % 2 else "#f4ead2"}"/>')
        rnd = random.Random(5)
        # Top register: white elephants carrying the scriptures towards the king.
        for k, tx in enumerate([X + 120, X + 560, X + 1460]):
            out.append(self.mural_tree(tx + 230 if k < 2 else tx - 120, Y + 318, ink))
        for k, ex in enumerate([X + 260, X + 700, X + 1140]):
            out.append(self.mural_elephant(ex, Y + 318, 1.2, ink))
        # The king, seated under a white parasol.
        kx, ky = X + 1585, Y + 318
        out.append(f'<rect x="{kx - 90}" y="{ky - 40}" width="180" height="40" fill="#e8c15a" stroke="{ink}" stroke-width="2.5"/>')
        out.append(f'<path d="M{kx},{ky - 40} V{ky - 268}" stroke="{ink}" stroke-width="4"/>')
        for k, (rw, rh) in enumerate([(130, 20), (100, 18), (70, 16)]):
            out.append(f'<path d="M{kx - rw / 2},{ky - 230 - k * 22} Q{kx},{ky - 250 - k * 22 - rh} {kx + rw / 2},{ky - 230 - k * 22}Z" fill="#f4ead2" stroke="{ink}" stroke-width="2.5"/>')
        out.append(self.mural_figure(kx, ky - 40, 1.6, "#4f6a2e", ink, seated=True, crown=True))
        out.append(f'<path d="M{kx - 22},{ky - 152} Q{kx},{ky - 136} {kx + 22},{ky - 152}" fill="none" stroke="#e8c15a" stroke-width="7"/>')
        for k in (-1, 1):
            out.append(self.mural_figure(kx + k * 130, ky, 1.0, "#f4ead2", ink, fan=True))
        # Middle register: scholars, monks and captives walking north.
        robes = ["#f4ead2", "#c8541e", "#f4ead2", "#7a3b26", "#c8541e", "#f4ead2"]
        for row, base in enumerate([Y + 510, Y + 676]):
            for k in range(15):
                fx = X + 90 + k * 110 + (row * 55)
                if fx > X + PW - 60:
                    continue
                monk = robes[(k + row) % 6] == "#c8541e"
                out.append(self.mural_figure(fx, base, 0.95, robes[(k + row * 2) % 6], ink, monk=monk, book=(k % 3 == 0 and not monk)))
        # Thiha, picked out in gold.
        out.append(self.mural_figure(X + 860, Y + 510, 1.05, "#f4ead2", ink, book=True, gold=True))
        # Bottom register: the river and boats.
        for row in range(3):
            for k in range(int(PW / 60) + 1):
                wx = X + k * 60 + (row % 2) * 30
                wy = Y + 760 + row * 30
                out.append(f'<path d="M{wx},{wy} q15,-12 30,0 q15,12 30,0" fill="none" stroke="#4f8a80" stroke-width="3"/>')
        for bx in (X + 1000, X + 1420):
            out.append(f'<path d="M{bx - 110},{Y + 800} Q{bx},{Y + 840} {bx + 110},{Y + 800} L{bx + 130},{Y + 780} H{bx - 130}Z" fill="#7a3b26" stroke="{ink}" stroke-width="2.5"/>'
                       f'<path d="M{bx},{Y + 780} V{Y + 720} L{bx + 50},{Y + 770}Z" fill="#f4ead2" stroke="{ink}" stroke-width="2"/>')
        return "".join(out)

    def mural_tree(self, x, base, ink):
        return (f'<path d="M{x},{base} V{base - 120}" stroke="{ink}" stroke-width="6"/>'
                f'<circle cx="{x}" cy="{base - 170}" r="62" fill="#4f6a2e" stroke="{ink}" stroke-width="2.5"/>'
                + "".join(f'<circle cx="{x + dx}" cy="{base - 170 + dy}" r="4" fill="#f4ead2"/>' for dx, dy in [(-30, -20), (0, -36), (28, -14), (-12, 6), (20, 22), (-36, 24), (36, 10), (6, -10)]))

    def mural_elephant(self, x, base, s, ink):
        return (f'<g transform="translate({x},{base}) scale({s})">'
                f'<path d="M-44,-36 h16 v36 h-16Z M-22,-36 h16 v36 h-16Z M14,-36 h16 v36 h-16Z M34,-40 h16 v40 h-16Z" fill="#f4ead2" stroke="{ink}" stroke-width="2.5"/>'
                f'<ellipse cx="0" cy="-64" rx="58" ry="38" fill="#f4ead2" stroke="{ink}" stroke-width="2.5"/>'
                f'<circle cx="52" cy="-72" r="26" fill="#f4ead2" stroke="{ink}" stroke-width="2.5"/>'
                f'<path d="M70,-66 Q88,-40 80,-6 L70,-4 Q74,-36 58,-52Z" fill="#f4ead2" stroke="{ink}" stroke-width="2.5"/>'
                f'<ellipse cx="40" cy="-70" rx="12" ry="18" fill="#f4ead2" stroke="{ink}" stroke-width="2"/><circle cx="58" cy="-78" r="3" fill="{ink}"/>'
                f'<path d="M-40,-96 H40 L34,-46 H-34Z" fill="#a83a22" stroke="{ink}" stroke-width="2.5"/>'
                f'<path d="M-40,-96 H40 M-36,-60 H36" stroke="#e8c15a" stroke-width="4"/>'
                f'<rect x="-22" y="-132" width="44" height="36" fill="#e8c15a" stroke="{ink}" stroke-width="2.5"/>'
                f'<path d="M-22,-132 L0,-150 L22,-132" fill="#e8c15a" stroke="{ink}" stroke-width="2.5"/>'
                f'<path d="M0,-150 V-200" stroke="{ink}" stroke-width="3"/><path d="M-36,-196 Q0,-226 36,-196Z" fill="#e8c15a" stroke="{ink}" stroke-width="2.5"/></g>')

    def mural_figure(self, x, base, s, robe, ink, monk=False, book=False, gold=False, seated=False, crown=False, fan=False):
        skin = "#c98a52"
        o = [f'<g transform="translate({x},{base}) scale({s})">']
        if seated:
            o.append(f'<path d="M-34,0 Q-30,-30 -16,-44 L-14,-80 Q0,-90 14,-80 L16,-44 Q30,-30 34,0Z" fill="{robe}" stroke="{ink}" stroke-width="2.5"/>')
            top = -80
        else:
            o.append(f'<path d="M-16,0 L-12,-70 Q0,-80 12,-70 L16,0Z" fill="{robe}" stroke="{ink}" stroke-width="2.5"/>')
            o.append(f'<path d="M-8,0 v6 M8,0 v6" stroke="{ink}" stroke-width="3"/>')
            top = -70
        if monk:
            o.append(f'<path d="M-12,{top + 4} L12,{top + 40}" stroke="#8a3a12" stroke-width="5"/>')
        o.append(f'<circle cx="0" cy="{top - 14}" r="13" fill="{skin}" stroke="{ink}" stroke-width="2.5"/>')
        if not monk:
            o.append(f'<path d="M-13,{top - 16} Q0,{top - 34} 13,{top - 16} Q0,{top - 24} -13,{top - 16}Z" fill="{ink}"/>')
        if crown:
            o.append(f'<path d="M-10,{top - 26} L0,{top - 56} L10,{top - 26}Z" fill="#e8c15a" stroke="{ink}" stroke-width="2"/>')
        if book:
            o.append(f'<rect x="8" y="{top + 14}" width="26" height="8" fill="#e8c15a" stroke="{ink}" stroke-width="1.5"/>')
        if fan:
            o.append(f'<path d="M14,{top + 10} L28,{top - 70}" stroke="{ink}" stroke-width="3"/><ellipse cx="30" cy="{top - 82}" rx="16" ry="22" fill="#e8c15a" stroke="{ink}" stroke-width="2"/>')
        if gold:
            o.append(f'<path d="M-20,4 L-15,{top - 2} Q0,{top - 46} 15,{top - 2} L20,4Z" fill="none" stroke="#e8c15a" stroke-width="4"/>')
        o.append("</g>")
        return "".join(o)

    # -- stele ------------------------------------------------------------
    def stele_backdrop(self):
        # A whitewashed brick niche.
        x, y, w, h = 100, 30, 720, 1030
        return (f'<path d="M{x},{y + h} V{y + 200} Q{x + w / 2},{y - 60} {x + w},{y + 200} V{y + h}Z" fill="#efe4cc" stroke="{self.gold()}" stroke-width="6"/>'
                + f'<g clip-path="url(#{self._niche(x, y, w, h)})">' + self.noise(x, y, w, h, "0.02", 3, (0.6, 0.5, 0.35), 1.2, -0.7, 111) + "</g>")

    def _niche(self, x, y, w, h):
        cid = self.nid("niche")
        self.defs.append(f'<clipPath id="{cid}"><path d="M{x},{y + h} V{y + 200} Q{x + w / 2},{y - 60} {x + w},{y + 200} V{y + h}Z"/></clipPath>')
        return cid

    def stele(self, x, y, w, h, title, lines):
        ink = "#3b2412"
        sx, sw = x + 60, w - 120
        top, bot = y + 90, y + h - 130
        out = []
        # Brick plinth.
        out.append(self.bricks(int(x + 10), int(bot), int(w - 20), int(y + h - bot), 121))
        out.append(f'<rect x="{x + 10}" y="{bot}" width="{w - 20}" height="{y + h - bot}" fill="none" stroke="{ink}" stroke-width="2.5"/>')
        out.append(f'<rect x="{x}" y="{bot - 8}" width="{w}" height="16" fill="{self.gold()}" stroke="{ink}" stroke-width="2"/>')
        # Slab with a rounded top and a gilded band.
        slab = f"M{sx},{bot} V{top + sw / 2} A{sw / 2},{sw / 2 * 0.5} 0 0 1 {sx + sw},{top + sw / 2} V{bot}Z"
        slab = f"M{sx},{bot} V{top + 140} Q{sx},{top} {sx + sw / 2},{top} Q{sx + sw},{top} {sx + sw},{top + 140} V{bot}Z"
        out.append(f'<path d="{slab}" fill="#000" opacity="0.3" transform="translate(6,8)"/>')
        out.append(f'<path d="{slab}" fill="#8e8a80" stroke="{ink}" stroke-width="3"/>')
        cid = self.nid("slab")
        self.defs.append(f'<clipPath id="{cid}"><path d="{slab}"/></clipPath>')
        out.append(f'<g clip-path="url(#{cid})">' + self.noise(sx, top, sw, bot - top, "0.05", 4, (0.2, 0.18, 0.15), 1.2, -0.5, 122)
                   + f'<path d="{slab}" fill="none" stroke="{self.gold()}" stroke-width="28"/></g>')
        out.append(f'<path d="{slab}" fill="none" stroke="{ink}" stroke-width="3"/>')
        # Gilded umbrella finial above.
        cx = sx + sw / 2
        out.append(f'<path d="M{cx},{top + 4} V{top - 70}" stroke="{ink}" stroke-width="4"/>')
        for k, rw in enumerate([90, 66, 44]):
            out.append(f'<path d="M{cx - rw / 2},{top - 30 - k * 18} Q{cx},{top - 52 - k * 18} {cx + rw / 2},{top - 30 - k * 18}Z" fill="{self.gold()}" stroke="{ink}" stroke-width="2"/>')
        out.append(self.stele_lines(sx + 50, top + 150, sw - 96, title, lines, 30, 23, 32, "#efe4cc", "#f3d77c"))
        return "".join(out)

    def carve(self, x, y, s, font, size, color, anchor, new=False):
        return (T(x + 1, y + 1.5, s, font, size, "#1e1a14", anchor, 700, extra='opacity="0.7"') + T(x, y, s, font, size, color, anchor, 700))

    def fresh_mark(self, x, y):
        return f'<circle cx="{x}" cy="{y}" r="7" fill="{self.gold()}" stroke="#3b2412"/>'


REALMS = [(DaiLongScreens, SC.DAILONG), (KambaraScreens, SC.KAMBARA), (TampadesaScreens, SC.TAMPADESA)]

if __name__ == "__main__":
    import sys
    which = sys.argv[1:] or ["intro", "intermission", "hud"]
    for cls, data in REALMS:
        for screen in which:
            getattr(cls(data), screen)()
