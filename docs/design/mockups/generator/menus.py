"""Renders the main menu mockups for each realm into build/: home, load / save,
settings, custom game and about. The realm styles come from world_map.py and
screens.py; this file adds the menu panels, the form widgets and the home
backdrops for Kambara and Tampadesa (Đại Long uses the game's own painting).
"""
import base64
import io
import math
import pathlib
import random

from PIL import Image

from world_map import T, f, BUILD, Kambara, Tampadesa
from screens import DaiLongScreens, KambaraScreens, TampadesaScreens
from frames import DaiLongFrames, KambaraFrames, TampadesaFrames
import menus_content as MC

W, H = 1920, 1080
HERE = pathlib.Path(__file__).parent
ASSETS = HERE / "../../../../assets"


def jpeg_b64(path, size=None):
    im = Image.open(path).convert("RGB")
    if size:
        im = im.resize(size, Image.LANCZOS)
    buf = io.BytesIO()
    im.save(buf, "JPEG", quality=88)
    return base64.b64encode(buf.getvalue()).decode()


class Menus:
    """Menu layouts and widgets shared by the three realms."""

    sub_size = 28
    title_size = 52

    # ------------------------------------------------------------ backdrop
    def backdrop(self, dim):
        scene = self.home_scene()
        if not dim:
            return scene
        fid = self.nid("blur")
        self.defs.append(f'<filter id="{fid}" x="0" y="0" width="100%" height="100%"><feGaussianBlur stdDeviation="4"/></filter>')
        return f'<g filter="url(#{fid})">{scene}</g><rect width="{W}" height="{H}" fill="#120b05" opacity="0.42"/>'

    # ------------------------------------------------------------ widgets
    def t(self, x, y, s, size, color=None, anchor="start", weight=600, italic=False, font=None, ls=None):
        return T(x, y, s, font or self.body_font, size, color or self.panel_text, anchor, weight, ls=ls,
                 style="font-style:italic" if italic else "")

    def heading_c(self, x, y, s, size):
        return self.heading_center(x, y, s, size)

    def heading_center(self, x, y, s, size):
        return T(x, y, s, self.title_font, size, self.panel_text, "middle", 800)

    def checkbox(self, x, y, on):
        out = [f'<rect x="{x}" y="{y - 14}" width="28" height="28" rx="4" fill="{self.field_fill}" stroke="{self.field_stroke}" stroke-width="2"/>']
        if on:
            out.append(f'<rect x="{x + 4}" y="{y - 10}" width="20" height="20" rx="3" fill="{self.accent}"/>'
                       f'<path d="M{x + 8},{y} l4,5 l8,-10" fill="none" stroke="{self.on_mark}" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>')
        return "".join(out)

    def toggle(self, x, y, on):
        """A switch, right-aligned at x."""
        w, h = 62, 30
        x0 = x - w
        knob = x0 + (w - h / 2) if on else x0 + h / 2
        return (f'<rect x="{x0}" y="{y - h / 2}" width="{w}" height="{h}" rx="{h / 2}" fill="{self.accent if on else self.field_fill}" stroke="{self.field_stroke}" stroke-width="2"/>'
                f'<circle cx="{knob}" cy="{y}" r="{h / 2 - 4}" fill="{self.knob_fill}" stroke="{self.field_stroke}" stroke-width="1.5"/>'
                + self.t(x0 - 16, y + 7, "On" if on else "Off", 19, self.panel_sub, "end", 600))

    def slider(self, x, y, w, v, label):
        """A slider, right-aligned at x, with its value to its left."""
        x0 = x - w
        kx = x0 + w * v
        return (f'<rect x="{x0}" y="{y - 5}" width="{w}" height="10" rx="5" fill="{self.field_fill}" stroke="{self.field_stroke}" stroke-width="1.5"/>'
                f'<rect x="{x0}" y="{y - 5}" width="{f(w * v)}" height="10" rx="5" fill="{self.accent}"/>'
                f'<circle cx="{f(kx)}" cy="{y}" r="13" fill="{self.knob_fill}" stroke="{self.field_stroke}" stroke-width="2"/>'
                f'<circle cx="{f(kx)}" cy="{y}" r="5" fill="{self.accent}"/>'
                + self.t(x0 - 18, y + 7, label, 19, self.panel_sub, "end", 700))

    def dropdown(self, x, y, w, value, right=True):
        x0 = x - w if right else x
        return (f'<rect x="{x0}" y="{y - 22}" width="{w}" height="44" rx="5" fill="{self.field_fill}" stroke="{self.field_stroke}" stroke-width="2"/>'
                + self.t(x0 + 16, y + 7, value, 20, self.panel_text, "start", 600)
                + f'<path d="M{x0 + w - 30},{y - 4} l8,8 l8,-8" fill="none" stroke="{self.accent}" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/>')

    def text_input(self, x, y, w, value, focus=True):
        return (f'<rect x="{x}" y="{y - 24}" width="{w}" height="48" rx="5" fill="{self.field_fill}" stroke="{self.accent if focus else self.field_stroke}" stroke-width="{2.5 if focus else 2}"/>'
                + self.t(x + 16, y + 8, value, 21, self.panel_text, "start", 600)
                + (f'<line x1="{x + 22 + len(value) * 10.2}" y1="{y - 13}" x2="{x + 22 + len(value) * 10.2}" y2="{y + 13}" stroke="{self.panel_text}" stroke-width="2"/>' if focus else ""))

    def tab(self, x, y, w, label, on):
        h = 50
        out = [f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="6" fill="{self.accent if on else self.field_fill}" stroke="{self.field_stroke}" stroke-width="1.5" opacity="{1 if on else 0.85}"/>']
        out.append(T(x + w / 2, y + 33, label, self.title_font, 21, self.on_mark if on else self.panel_text, "middle", 700))
        return "".join(out)

    def row_bg(self, x, y, w, h, selected):
        if selected:
            return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="6" fill="{self.field_fill}" stroke="{self.accent}" stroke-width="2.5"/>'
        return f'<line x1="{x + 8}" y1="{y + h}" x2="{x + w - 8}" y2="{y + h}" stroke="{self.panel_rule}" stroke-width="1"/>'

    def realm_emblem(self, key, x, y, r, locked=False, current=False):
        out = [f'<circle cx="{x}" cy="{y}" r="{r}" fill="{self.field_fill}" stroke="{self.accent if current else self.field_stroke}" stroke-width="{3.5 if current else 2}"/>']
        s = r / 40
        if key == "dailong":
            out.append(f'<circle cx="{x}" cy="{y - 4 * s}" r="{14 * s}" fill="#b3261e" stroke="#2b2118" stroke-width="{2 * s}"/>')
            for d in (-1, 1):
                out.append(f'<path d="M{x + d * 8 * s},{y + 14 * s} q{d * 12 * s},{-12 * s} {d * 24 * s},{-2 * s} q{d * 6 * s},{8 * s} {d * -2 * s},{12 * s}" fill="none" stroke="#2b2118" stroke-width="{2.4 * s}" stroke-linecap="round"/>')
        elif key == "kambara":
            out.append(Kambara.prang(self, x, y + 24 * s, 1.0 * s, "#b8ab8c", "#2c2a22", 1.4 * s))
        else:
            out.append(Tampadesa.stupa(self, x, y + 24 * s, 1.0 * s, "#d9b04e", "#4a2c08", 1.4 * s))
        if locked:
            out.append(f'<circle cx="{x}" cy="{y}" r="{r}" fill="{self.field_fill}" opacity="0.7"/>')
            out.append(f'<rect x="{x - 11 * s}" y="{y - 2 * s}" width="{22 * s}" height="{17 * s}" rx="{3 * s}" fill="{self.panel_sub}"/>'
                       f'<path d="M{x - 7 * s},{y - 2 * s} v{-6 * s} a{7 * s},{7 * s} 0 0 1 {14 * s},0 v{6 * s}" fill="none" stroke="{self.panel_sub}" stroke-width="{3 * s}"/>')
        return "".join(out)

    def thumb(self, x, y, w, h):
        img = jpeg_b64(BUILD / f"city_{self.sd['key']}.jpg", (w * 2, h * 2))
        return (f'<image x="{x}" y="{y}" width="{w}" height="{h}" xlink:href="data:image/jpeg;base64,{img}" preserveAspectRatio="xMidYMid slice" {self.clip(x, y, w, h, 4)}/>'
                f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="4" fill="none" stroke="{self.field_stroke}" stroke-width="3"/>')

    def dialog_title(self, cx, y, s):
        return T(cx, y, s, self.title_font, 44, self.panel_text, "middle", 800)

    # ------------------------------------------------------------ home
    def home(self):
        d = self.sd
        parts = [self.backdrop(False)]
        x, y, w, h = 90, 60, 560, 960
        parts.append(self.mpanel(x, y, w, h, crest=True))
        cx = x + w / 2
        parts.append(self.heading_center(cx, y + 200, "Heritage Builder", self.title_size))
        parts.append(self.t(cx, y + 246, d["subtitle"], self.sub_size, self.accent, "middle", 600, italic=True))
        parts.append(self.divider(x + 70, y + 282, w - 140))
        # Campaign progress: the realms reached so far.
        for k, (key, name, _, num) in enumerate(MC.REALMS):
            ex = cx + (k - 1) * 140
            reached = k < d["progress"]
            parts.append(self.realm_emblem(key, ex, y + 342, 36, locked=not reached, current=k == d["progress"] - 1))
            parts.append(self.t(ex, y + 402, name if reached else "Locked", 18, self.panel_text if reached else self.panel_sub, "middle", 700))
        items = [("Continue", d["continue_sub"]), ("Campaign", d["campaign_sub"]), ("Custom Game", None), ("Load Game", None),
                 ("Settings", None), ("About", None), ("Quit Game", None)]
        iy = y + 436
        for label, sub in items:
            hover = label == d["hover"]
            ih = 72 if sub else 54
            if hover:
                parts.append(f'<rect x="{x + 50}" y="{iy - 6}" width="{w - 100}" height="{ih}" rx="8" fill="{self.field_fill}" stroke="{self.accent}" stroke-width="2"/>')
                parts.append(self.item_marker(x + 80, iy + (ih - 12) / 2))
            parts.append(T(cx, iy + 37, label, self.title_font, 31, self.accent if hover else self.panel_text, "middle", 700))
            if sub:
                parts.append(self.t(cx, iy + 61, sub, 18, self.panel_sub, "middle", 600, italic=True))
            iy += ih + 8
        parts.append(self.t(cx, y + h - 60, MC.VERSION, 17, self.panel_sub, "middle", 600))
        self.page(f"menu-home-{d['key']}", parts)

    # ------------------------------------------------------------ load / save
    def load_save(self):
        d = self.sd
        parts = [self.backdrop(True)]
        x, y, w, h = 190, 70, 1540, 940
        parts.append(self.mpanel(x, y, w, h))
        parts.append(self.dialog_title(x + w / 2, y + 112, "Load / Save"))
        lx, lw = x + 70, 900
        ty = y + 170
        parts.append(self.section_title(lx + 80, ty, "Saved game"))
        parts.append(self.section_title(lx + 560, ty, "Saved"))
        ry = ty + 18
        for k, (name, mission, where, gdate, saved, pop, gold, played) in enumerate(d["saves"]):
            sel = k == d["selected_save"]
            parts.append(self.row_bg(lx, ry, lw, 78, sel))
            realm = "kambara" if "Campaign II " in where or "Suryadeva" in gdate or "Jayadeva" in gdate else "tampadesa" if "Pugara" in gdate else "dailong"
            parts.append(self.realm_emblem(realm, lx + 40, ry + 39, 26))
            parts.append(self.t(lx + 80, ry + 34, name, 22, self.panel_text, "start", 700))
            parts.append(self.t(lx + 80, ry + 62, f"{mission} · {where}", 18, self.panel_sub, "start", 600))
            parts.append(self.t(lx + 560, ry + 34, saved, 20, self.panel_text, "start", 600))
            parts.append(self.t(lx + 560, ry + 62, gdate, 18, self.panel_sub, "start", 600, italic=True))
            ry += 86
        # Preview of the selected save.
        name, mission, where, gdate, saved, pop, gold, played = d["saves"][d["selected_save"]]
        px, pw = x + 1030, 440
        parts.append(self.thumb(px, ty - 6, pw, 248))
        info = [("Mission", mission), ("Campaign", where), ("Date", gdate), ("Population", pop), ("Treasury", gold), ("Time played", played)]
        for k, (lab, val) in enumerate(info):
            iy = ty + 290 + k * 42
            parts.append(self.t(px, iy, lab, 19, self.panel_sub, "start", 600))
            parts.append(self.t(px + pw, iy, val, 19, self.panel_text, "end", 700))
            parts.append(f'<line x1="{px}" y1="{iy + 13}" x2="{px + pw}" y2="{iy + 13}" stroke="{self.panel_rule}" stroke-width="1"/>')
        # Save name and buttons.
        by = y + h - 176
        parts.append(self.section_title(lx, by - 30, "Save as"))
        parts.append(self.text_input(lx, by + 6, lw, d["save_name"]))
        bb = y + h - 116
        parts.append(self.button(lx, bb, 200, 56, "Delete", False))
        parts.append(self.button(px - 120, bb, 180, 56, "Cancel", False))
        parts.append(self.button(px + 78, bb, 180, 56, "Save", False))
        parts.append(self.button(px + 276, bb, 164, 56, "Load", True))
        self.page(f"menu-load-save-{d['key']}", parts)

    # ------------------------------------------------------------ settings
    def settings(self):
        d = self.sd
        parts = [self.backdrop(True)]
        x, y, w, h = 360, 80, 1200, 920
        parts.append(self.mpanel(x, y, w, h))
        parts.append(self.dialog_title(x + w / 2, y + 112, "Settings"))
        tabs = ["Game", "Sound", "Graphics", "Controls"]
        tw = 200
        tx = x + w / 2 - (len(tabs) * tw + (len(tabs) - 1) * 14) / 2
        for k, lab in enumerate(tabs):
            parts.append(self.tab(tx + k * (tw + 14), y + 150, tw, lab, lab == d["settings_tab"]))
        rx0, rx1 = x + 90, x + w - 90
        ry = y + 266
        step = min(66, 540 / len(MC.SETTINGS[d["settings_tab"]]))
        for opt in MC.SETTINGS[d["settings_tab"]]:
            label, kind = opt[0], opt[1]
            parts.append(self.t(rx0, ry + 8, label, 23, self.panel_text, "start", 600))
            if kind == "toggle":
                parts.append(self.toggle(rx1, ry, opt[2]))
                if len(opt) > 3:
                    parts.append(self.dropdown(rx1 - 140, ry, 230, opt[3][0]))
            elif kind == "dropdown":
                parts.append(self.dropdown(rx1, ry, 300, opt[2]))
            else:
                parts.append(self.slider(rx1, ry, 340, opt[2], opt[3]))
            parts.append(f'<line x1="{rx0}" y1="{ry + step / 2}" x2="{rx1}" y2="{ry + step / 2}" stroke="{self.panel_rule}" stroke-width="1"/>')
            ry += step
        bb = y + h - 100
        parts.append(self.button(rx0, bb, 300, 56, "Restore defaults", False))
        parts.append(self.button(rx1 - 420, bb, 200, 56, "Cancel", False))
        parts.append(self.button(rx1 - 200, bb, 200, 56, "Ok", True))
        self.page(f"menu-settings-{d['key']}", parts)

    # ------------------------------------------------------------ custom game
    def custom_game(self):
        d = self.sd
        parts = [self.backdrop(True)]
        x, y, w, h = 110, 60, 1700, 960
        parts.append(self.mpanel(x, y, w, h))
        parts.append(self.dialog_title(x + w / 2, y + 112, "Custom Game"))
        # Realm.
        lx, lw = x + 70, 500
        cy = y + 180
        parts.append(self.section_title(lx, cy, "Realm"))
        for k, (key, name, sub, num) in enumerate(MC.REALMS):
            ry = cy + 18 + k * 96
            unlocked = k < d["progress"]
            sel = key == d["key"]
            parts.append(self.row_bg(lx, ry, lw, 86, sel))
            parts.append(self.realm_emblem(key, lx + 46, ry + 43, 32, locked=not unlocked, current=sel))
            parts.append(self.t(lx + 96, ry + 38, name, 24, self.panel_text if unlocked else self.panel_sub, "start", 700))
            parts.append(self.t(lx + 96, ry + 66, sub if unlocked else f"Start Campaign {num} to unlock", 18, self.panel_sub, "start", 600, italic=True))
        # Map.
        my = cy + 18 + 3 * 96 + 52
        parts.append(self.section_title(lx, my, "Map"))
        for k, (name, size) in enumerate(d["maps"]):
            ry = my + 16 + k * 62
            parts.append(self.row_bg(lx, ry, lw, 54, k == 0))
            parts.append(self.t(lx + 20, ry + 36, name, 22, self.panel_text, "start", 700 if k == 0 else 600))
            parts.append(self.t(lx + lw - 20, ry + 36, size, 19, self.panel_sub, "end", 600))
        # Preview.
        px, pw = x + 640, 470
        parts.append(self.section_title(px, cy, "Preview"))
        parts.append(f'<rect x="{px}" y="{cy + 18}" width="{pw}" height="330" rx="6" fill="{self.mini_water}" stroke="{self.field_stroke}" stroke-width="3"/>')
        parts.append(self.minimap_big(px + pw / 2, cy + 18 + 165, 200))
        stats = [("Size", d["maps"][0][1]), ("Rivers", "1 large, 3 streams"), ("Fertile land", "High"), ("Trade partners", "4"), ("Hazards", "Floods, fire")]
        for k, (lab, val) in enumerate(stats):
            sy = cy + 400 + k * 42
            parts.append(self.t(px, sy, lab, 19, self.panel_sub, "start", 600))
            parts.append(self.t(px + pw, sy, val, 19, self.panel_text, "end", 700))
            parts.append(f'<line x1="{px}" y1="{sy + 13}" x2="{px + pw}" y2="{sy + 13}" stroke="{self.panel_rule}" stroke-width="1"/>')
        # Options.
        ox, ow = x + 1180, 450
        parts.append(self.section_title(ox, cy, "Options"))
        oy = cy + 50
        for lab, val in [("Starting date", d["era"]), ("Goals", "Sandbox: no goals"), ("Starting treasury", "5,000 gold")]:
            parts.append(self.t(ox, oy, lab, 20, self.panel_sub, "start", 600))
            parts.append(self.dropdown(ox, oy + 36, ow, val, right=False))
            oy += 98
        parts.append(self.t(ox, oy, "Difficulty", 20, self.panel_sub, "start", 600))
        parts.append(self.t(ox + ow, oy, MC.DIFFICULTY[2], 20, self.panel_text, "end", 700))
        for k in range(5):
            bw = (ow - 4 * 8) / 5
            parts.append(f'<rect x="{f(ox + k * (bw + 8))}" y="{oy + 16}" width="{f(bw)}" height="16" rx="4" fill="{self.accent if k <= 2 else self.field_fill}" stroke="{self.field_stroke}" stroke-width="1.5"/>')
        oy += 82
        for lab, on in [("Seasons and floods", True), ("Fires and collapses", True), ("Raids and invasions", False), ("Fast construction", False)]:
            parts.append(self.t(ox, oy + 7, lab, 21, self.panel_text, "start", 600))
            parts.append(self.toggle(ox + ow, oy, on))
            oy += 56
        bb = y + h - 100
        parts.append(self.button(ox + ow - 470, bb, 180, 56, "Cancel", False))
        parts.append(self.button(ox + ow - 270, bb, 270, 56, "Start", True))
        self.page(f"menu-custom-game-{d['key']}", parts)

    def minimap_big(self, cx, cy, r):
        rnd = random.Random(len(self.sd["key"]))
        out = []
        n = 22
        s = r / n
        for i in range(n):
            for j in range(n):
                px = cx + (i - j) * s
                py = cy - r * 0.5 + (i + j) * s * 0.5
                dist = math.hypot(i - n / 2 + 0.5, j - n / 2 + 0.5)
                if dist > n / 2 + rnd.uniform(-1.5, 0.8):
                    continue
                river = abs(i - 11 - 3 * math.sin(j / 3.2)) < 0.9
                c = self.mini_water if river else rnd.choice(self.mini_land)
                out.append(f'<path d="M{f(px)},{f(py)} l{f(s)},{f(s * 0.5)} l{f(-s)},{f(s * 0.5)} l{f(-s)},{f(-s * 0.5)}z" fill="{c}"/>')
        return "".join(out)

    # ------------------------------------------------------------ about
    def about(self):
        d = self.sd
        parts = [self.backdrop(True)]
        x, y, w, h = 460, 60, 1000, 960
        parts.append(self.mpanel(x, y, w, h, crest=True))
        cx = x + w / 2
        parts.append(self.heading_center(cx, y + 190, "Heritage Builder", self.title_size))
        parts.append(self.t(cx, y + 236, d["subtitle"], self.sub_size, self.accent, "middle", 600, italic=True))
        parts.append(self.t(cx, y + 282, "A City Builder by Core System Games", 22, self.panel_text, "middle", 600))
        parts.append(self.divider(x + 140, y + 314, w - 280))
        cy = y + 362
        for role, names in MC.CREDITS:
            parts.append(T(cx, cy, role.upper(), self.title_font, 16, self.accent, "middle", 700, ls=2))
            for k, n in enumerate(names):
                parts.append(self.t(cx, cy + 31 + k * 28, n, 21, self.panel_text, "middle", 600))
            cy += 31 + len(names) * 28 + 22
        parts.append(self.t(cx, cy + 14, "“They built for those who came after them.”", 24, self.panel_sub, "middle", 600, italic=True))
        parts.append(self.t(cx, y + h - 116, f"{MC.VERSION} · Copyright © 2026. All Rights Reserved", 17, self.panel_sub, "middle", 600))
        parts.append(self.button(cx - 120, y + h - 98, 240, 56, "Back", True))
        self.page(f"menu-about-{d['key']}", parts)


# ====================================================================== Đại Long
class DaiLongMenus(DaiLongFrames, Menus, DaiLongScreens):
    field_fill = "#f7ead0"
    field_stroke = "#6b4f33"
    knob_fill = "#fff8ea"
    on_mark = "#fff4dc"
    mini_land = ["#8a9a52", "#7a8a48", "#9aa860", "#c9a76a", "#5a7a3a"]
    mini_water = "#6f8f92"

    def home_scene(self):
        img = jpeg_b64(ASSETS / "ui/misc/home_menu_static_bg.png")
        return f'<image x="0" y="0" width="{W}" height="{H}" xlink:href="data:image/jpeg;base64,{img}" preserveAspectRatio="xMidYMid slice"/>'

    def roller(self, x, y, w):
        g = self.lin([(0, "#3a1c0c"), (0.35, "#a0612e"), (0.55, "#7a4420"), (1, "#3a1c0c")])
        out = [f'<rect x="{x + 22}" y="{y - 20}" width="{w - 44}" height="40" rx="8" fill="{g}" stroke="#2a170c" stroke-width="2.5"/>']
        for ex, d in ((x, 1), (x + w, -1)):
            out.append(f'<rect x="{ex - (0 if d > 0 else 30)}" y="{y - 26}" width="30" height="52" rx="6" fill="#8e1d17" stroke="#2a170c" stroke-width="2.5"/>'
                       f'<circle cx="{ex - d * 12}" cy="{y}" r="13" fill="#d6a65c" stroke="#2a170c" stroke-width="2.5"/>')
        return "".join(out)

    def item_marker(self, x, y):
        return self.seal(x, y, 24, "")


# ====================================================================== Kambara
class KambaraMenus(KambaraFrames, Menus, KambaraScreens):
    field_fill = "#d6cbb0"
    field_stroke = "#4a4436"
    knob_fill = "#efe6cf"
    on_mark = "#f6eedc"
    mini_land = ["#6f8f44", "#5f7f3a", "#8d8169", "#c4b696", "#4f6f30"]
    mini_water = "#3f6a62"
    title_size = 46

    def item_marker(self, x, y):
        return self.leaf(x - 14, y, 0.9, 0, "#6e9a48")

    def home_scene(self):
        ink = "#2f3a2a"
        out = []
        sky = self.lin([(0, "#78c2e6"), (0.75, "#cdeef3"), (1, "#f2efd8")])
        out.append(f'<rect width="{W}" height="660" fill="{sky}"/>')
        out.append(self.toon_clouds([(1640, 130, 1.0), (930, 150, 0.75), (1980, 300, 0.8)], "#ffffff", ink))
        # Jungle on the horizon.
        out.append(self.blobs([(x, 600 + 18 * math.sin(x / 90), 50 + 20 * math.sin(x / 47)) for x in range(-40, 1980, 70)], "#4f8a4a", ink))
        out.append(self.blobs([(x, 640 + 12 * math.sin(x / 60), 34 + 12 * math.cos(x / 37)) for x in range(-20, 1980, 55)], "#66a052", ink))
        temple = self.toon_temple_mountain(1270, 650, ink)
        out.append(temple)
        # The baray, with the temple's reflection.
        water = self.lin([(0, "#4cb6cc"), (1, "#2a86a0")])
        out.append(f'<rect y="650" width="{W}" height="{H - 650}" fill="{water}"/>')
        out.append(f'<g opacity="0.28" transform="translate(0,1300) scale(1,-1)" {self.clip(0, 650, W, 430)}>{temple}</g>')
        out.append(f'<line x1="0" y1="650" x2="{W}" y2="650" stroke="{ink}" stroke-width="3"/>')
        rnd = random.Random(7)
        for _ in range(60):
            wx, wy = rnd.uniform(0, W), rnd.uniform(680, 1060)
            out.append(f'<path d="M{f(wx)},{f(wy)} q14,-8 28,0 q14,8 28,0" fill="none" stroke="#e8f8fb" stroke-width="2.5" opacity="0.65"/>')
        # Causeway with balustrades.
        out.append(f'<path d="M1150,1080 L1240,650 H1300 L1390,1080Z" fill="#c9bfa5" stroke="{ink}" stroke-width="3"/>')
        for k in range(9):
            t = k / 8
            yy = 650 + 430 * t * t
            for side in (-1, 1):
                xx = 1270 + side * (30 + 90 * t * t)
                out.append(f'<rect x="{f(xx - 4 - 4 * t)}" y="{f(yy - 10 - 26 * t)}" width="{f(8 + 8 * t)}" height="{f(10 + 26 * t)}" fill="#b3aa92" stroke="{ink}" stroke-width="1.5"/>')
        # Stilt house, palms and lotuses.
        out.append(self.stilt_house(1830, 662, ink))
        for (px, py, s) in [(1680, 700, 1.2), (1910, 720, 1.4), (760, 690, 1.0), (900, 680, 0.75)]:
            out.append(self.sugar_palm(px, py, s, ink))
        for (lx, ly, s) in [(760, 980, 1.3), (900, 1040, 1.0), (1560, 990, 1.2), (1760, 1050, 1.4), (1450, 1060, 0.9), (680, 860, 0.8)]:
            out.append(self.lotus(lx, ly, s, ink, flower=(lx * 7) % 3 != 0))
        return "".join(out)

    def toon_clouds(self, clouds, fill, ink):
        out = []
        for (cx, cy, s) in clouds:
            parts = [(-90, 10, 48), (-40, -20, 60), (20, -30, 66), (80, -6, 52), (130, 14, 38), (10, 20, 50)]
            out.append("".join(f'<circle cx="{cx + dx * s}" cy="{cy + dy * s}" r="{r * s}" fill="{ink}" stroke="{ink}" stroke-width="7"/>' for dx, dy, r in parts))
            out.append("".join(f'<circle cx="{cx + dx * s}" cy="{cy + dy * s}" r="{r * s}" fill="{fill}"/>' for dx, dy, r in parts))
        return "".join(out)

    def blobs(self, items, fill, ink):
        return ("".join(f'<circle cx="{f(x)}" cy="{f(y)}" r="{f(r)}" fill="{ink}" stroke="{ink}" stroke-width="6"/>' for x, y, r in items)
                + "".join(f'<circle cx="{f(x)}" cy="{f(y)}" r="{f(r)}" fill="{fill}"/>' for x, y, r in items))

    def toon_temple_mountain(self, cx, base, ink):
        stone, shade = "#b3aa92", "#8f8670"
        out = []
        # Gallery wall in front, terraces behind.
        for (tw, ty0, th) in [(640, base - 160, 56), (480, base - 214, 54), (330, base - 264, 50)]:
            out.append(f'<rect x="{cx - tw / 2}" y="{ty0}" width="{tw}" height="{th}" fill="{stone}" stroke="{ink}" stroke-width="3"/>'
                       f'<rect x="{cx + tw / 2 - 50}" y="{ty0}" width="50" height="{th}" fill="{shade}" opacity="0.6"/>')
            for c in range(int(tw / 40)):
                out.append(f'<rect x="{cx - tw / 2 + 14 + c * 40}" y="{ty0 + 14}" width="12" height="{th - 26}" fill="{shade}"/>')
        for (dx, s, b) in [(-180, 2.9, base - 214), (180, 2.9, base - 214), (-110, 3.4, base - 264), (110, 3.4, base - 264), (0, 4.8, base - 264)]:
            out.append(self.prang(cx + dx, b, s, stone, ink, 3))
            out.append(f'<circle cx="{cx + dx + 4 * s}" cy="{b - 20 * s}" r="{3 * s}" fill="#5f7f3a" opacity="0.8"/>')
        out.append(f'<rect x="{cx - 450}" y="{base - 104}" width="900" height="104" fill="{stone}" stroke="{ink}" stroke-width="3"/>'
                   f'<rect x="{cx - 450}" y="{base - 104}" width="900" height="22" fill="{shade}" stroke="{ink}" stroke-width="2"/>')
        for c in range(22):
            out.append(f'<rect x="{cx - 430 + c * 40}" y="{base - 74}" width="14" height="66" fill="{shade}"/>')
        out.append(f'<path d="M{cx - 70},{base} V{base - 120} Q{cx},{base - 190} {cx + 70},{base - 120} V{base}Z" fill="{stone}" stroke="{ink}" stroke-width="3"/>'
                   f'<path d="M{cx - 30},{base} V{base - 90} Q{cx},{base - 120} {cx + 30},{base - 90} V{base}Z" fill="#3a3a30"/>')
        return "".join(out)

    def stilt_house(self, x, base, ink):
        return (f'<path d="M{x - 70},{base} V{base - 40} M{x - 30},{base} V{base - 40} M{x + 30},{base} V{base - 40} M{x + 70},{base} V{base - 40}" stroke="{ink}" stroke-width="5"/>'
                f'<rect x="{x - 80}" y="{base - 100}" width="160" height="62" fill="#a9733f" stroke="{ink}" stroke-width="3"/>'
                f'<path d="M{x - 110},{base - 96} L{x},{base - 170} L{x + 110},{base - 96}Z" fill="#c9a258" stroke="{ink}" stroke-width="3"/>'
                f'<rect x="{x - 20}" y="{base - 86}" width="40" height="48" fill="#5a3a1c" stroke="{ink}" stroke-width="2"/>')

    def sugar_palm(self, x, base, s, ink):
        top = base - 260 * s
        fr = "".join(f'<path d="M{x},{f(top)} L{f(x + 70 * s * math.cos(a))},{f(top + 70 * s * math.sin(a))}" stroke="#3f7a32" stroke-width="{f(9 * s)}" stroke-linecap="round"/>'
                     for a in [math.radians(v) for v in range(180, 361, 20)])
        return (f'<path d="M{x},{base} Q{x + 8 * s},{f(base - 130 * s)} {x},{f(top)}" fill="none" stroke="{ink}" stroke-width="{f(13 * s)}"/>'
                f'<path d="M{x},{base} Q{x + 8 * s},{f(base - 130 * s)} {x},{f(top)}" fill="none" stroke="#8a6a44" stroke-width="{f(8 * s)}"/>'
                + fr.replace('#3f7a32', ink).replace(f'stroke-width="{f(9 * s)}"', f'stroke-width="{f(14 * s)}"') + fr
                + f'<circle cx="{x}" cy="{f(top)}" r="{f(14 * s)}" fill="#3f7a32" stroke="{ink}" stroke-width="2"/>')

    def lotus(self, x, y, s, ink, flower=True):
        out = [f'<path d="M{x},{y} m{-50 * s},0 a{50 * s},{18 * s} 0 1 0 {100 * s},0 a{50 * s},{18 * s} 0 1 0 {-100 * s},0Z" fill="#4f9a44" stroke="{ink}" stroke-width="2.5"/>'
               f'<path d="M{x},{y} L{x + 46 * s},{y - 6 * s} L{x + 40 * s},{y + 10 * s}Z" fill="#2a86a0"/>']
        if flower:
            for a in (-40, -15, 15, 40):
                out.append(f'<path d="M{x},{y - 6 * s} q{a * s * 0.6},{-30 * s} {a * s * 0.2},{-52 * s} q{-a * s * 0.5},{20 * s} {-a * s * 0.2},{52 * s}Z" fill="#f2a6b8" stroke="{ink}" stroke-width="2"/>')
            out.append(f'<path d="M{x},{y - 6 * s} q{-14 * s},{-34 * s} 0,{-62 * s} q{14 * s},{28 * s} 0,{62 * s}Z" fill="#f7c3cf" stroke="{ink}" stroke-width="2"/>')
        return "".join(out)


# ====================================================================== Tampadesa
class TampadesaMenus(TampadesaFrames, Menus, TampadesaScreens):
    field_fill = "#f6e3bc"
    field_stroke = "#6b4a2a"
    knob_fill = "#fff6e2"
    on_mark = "#fff4dc"
    mini_land = ["#d8b47a", "#c99a55", "#9b4527", "#7a8a48", "#e3bf86"]
    mini_water = "#6f8f86"
    sub_size = 25

    def item_marker(self, x, y):
        return self.stupa(x, y + 14, 0.5)

    def home_scene(self):
        ink = "#3b2412"
        out = []
        sky = self.lin([(0, "#9ccbe0"), (0.55, "#f4dcb2"), (1, "#f0be82")])
        out.append(f'<rect width="{W}" height="700" fill="{sky}"/>')
        out.append(f'<circle cx="1560" cy="330" r="190" fill="#fff3d0" opacity="0.5"/><circle cx="1560" cy="330" r="80" fill="#fff6dc"/>')
        out.append(KambaraMenus.toon_clouds(self, [(1180, 150, 0.9), (1700, 110, 0.7)], "#fffaf0", ink))
        # Far hills and the plain.
        out.append(f'<path d="M0,610 Q300,540 620,590 T1240,570 T1920,580 V700 H0Z" fill="#b9a7a8" opacity="0.8"/>')
        ground = self.lin([(0, "#e0b46c"), (1, "#c98e48")])
        out.append(f'<rect y="620" width="{W}" height="{H - 620}" fill="{ground}"/>')
        out.append(self.noise(0, 620, W, H - 620, "0.01", 3, (0.55, 0.32, 0.12), 1.3, -0.7, 141))
        rnd = random.Random(11)
        # Far temples: a plain of small silhouettes.
        far = []
        for _ in range(46):
            tx, ty = rnd.uniform(-20, 1940), rnd.uniform(612, 660)
            s = 0.25 + (ty - 612) / 160
            far.append((ty, tx, s))
        for ty, tx, s in sorted(far):
            out.append(self.brick_temple(tx, ty, s, ink, far=True, gilded=rnd.random() < 0.3))
        # Middle distance.
        mids = [(760, 760, 0.8, False), (930, 730, 0.6, True), (1640, 760, 0.85, True), (1820, 740, 0.6, False), (1060, 790, 0.5, False)]
        for tx, ty, s, g in sorted(mids, key=lambda m: m[1]):
            out.append(self.brick_temple(tx, ty, s, ink, gilded=g))
        out.append(self.white_stupa(880, 820, 1.0, ink))
        out.append(self.great_temple(1270, 820, ink))
        # Palms, the river and a boat.
        for (px, py, s) in [(700, 880, 1.1), (1500, 860, 0.9), (1800, 900, 1.3), (1080, 900, 0.8), (640, 760, 0.6), (1950, 820, 1.0)]:
            out.append(self.toddy_palm(px, py, s, ink))
        river = f"M0,960 Q480,920 960,950 T1920,930 V1080 H0Z"
        out.append(f'<path d="{river}" fill="#6fa39a" stroke="{ink}" stroke-width="3"/>')
        for _ in range(30):
            wx, wy = rnd.uniform(0, W), rnd.uniform(975, 1070)
            out.append(f'<path d="M{f(wx)},{f(wy)} q12,-7 24,0 q12,7 24,0" fill="none" stroke="#e6f2ec" stroke-width="2.5" opacity="0.7"/>')
        out.append(f'<path d="M1180,1012 Q1290,1050 1400,1012 L1420,996 H1160Z" fill="#7a3b26" stroke="{ink}" stroke-width="3"/>'
                   f'<path d="M1290,996 V900 L1370,990Z" fill="#f4ead2" stroke="{ink}" stroke-width="3"/>')
        return "".join(out)

    def brick_temple(self, x, base, s, ink, far=False, gilded=False):
        brick, dark = ("#b8705a", "#a0604c") if far else ("#b5583a", "#8e4029")
        sw = 1.2 if far else 2.5
        spire = self.gold() if gilded else brick
        return (f'<g transform="translate({f(x)},{f(base)}) scale({f(s)})">'
                f'<rect x="-70" y="-60" width="140" height="60" fill="{brick}" stroke="{ink}" stroke-width="{sw / s if far else 2.5}"/>'
                f'<rect x="-50" y="-100" width="100" height="40" fill="{brick}" stroke="{ink}" stroke-width="{sw / s if far else 2.5}"/>'
                f'<path d="M-34,-100 Q-30,-170 0,-210 Q30,-170 34,-100Z" fill="{spire}" stroke="{ink}" stroke-width="{sw / s if far else 2.5}"/>'
                f'<path d="M0,-210 V-236" stroke="{ink}" stroke-width="3"/>'
                + ("" if far else f'<rect x="-14" y="-44" width="28" height="44" fill="#3b2412"/><path d="M-70,-60 H70 M-50,-100 H50" stroke="#f4ead2" stroke-width="3"/>'
                   f'<rect x="40" y="-60" width="30" height="60" fill="{dark}" opacity="0.6"/>')
                + "</g>")

    def white_stupa(self, x, base, s, ink):
        return (f'<g transform="translate({x},{base}) scale({s})">'
                f'<rect x="-80" y="-30" width="160" height="30" fill="#f4ead2" stroke="{ink}" stroke-width="2.5"/>'
                f'<rect x="-64" y="-56" width="128" height="26" fill="#f4ead2" stroke="{ink}" stroke-width="2.5"/>'
                f'<path d="M-52,-56 C-52,-120 -16,-140 -10,-160 L0,-230 L10,-160 C16,-140 52,-120 52,-56Z" fill="#f4ead2" stroke="{ink}" stroke-width="2.5"/>'
                f'<path d="M-12,-160 L0,-230 L12,-160Z" fill="{self.gold()}" stroke="{ink}" stroke-width="2"/>'
                f'<path d="M-14,-232 Q0,-246 14,-232Z" fill="{self.gold()}" stroke="{ink}" stroke-width="2"/></g>')

    def great_temple(self, cx, base, ink):
        brick, dark = "#b5583a", "#8e4029"
        out = []
        for (tw, th, ty) in [(560, 90, base - 90), (420, 80, base - 170), (300, 60, base - 230)]:
            out.append(f'<rect x="{cx - tw / 2}" y="{ty}" width="{tw}" height="{th}" fill="{brick}" stroke="{ink}" stroke-width="3"/>'
                       f'<path d="M{cx - tw / 2},{ty + 8} H{cx + tw / 2}" stroke="#f4ead2" stroke-width="5"/>'
                       f'<rect x="{cx + tw / 2 - 60}" y="{ty}" width="60" height="{th}" fill="{dark}" opacity="0.5"/>')
        for dx in (-240, 240, -170, 170):
            b = base - 90 if abs(dx) > 200 else base - 170
            out.append(f'<path d="M{cx + dx - 18},{b} Q{cx + dx - 14},{b - 50} {cx + dx},{b - 76} Q{cx + dx + 14},{b - 50} {cx + dx + 18},{b}Z" fill="{self.gold()}" stroke="{ink}" stroke-width="2.5"/>')
        out.append(f'<path d="M{cx - 100},{base - 230} C{cx - 100},{base - 340} {cx - 34},{base - 410} {cx},{base - 470} C{cx + 34},{base - 410} {cx + 100},{base - 340} {cx + 100},{base - 230}Z" fill="{self.gold()}" stroke="{ink}" stroke-width="3"/>')
        for k in range(1, 5):
            yy = base - 230 - k * 44
            ww = 100 * (1 - k * 0.17)
            out.append(f'<path d="M{cx - ww},{yy} H{cx + ww}" stroke="#8a6418" stroke-width="2.5"/>')
        out.append(f'<path d="M{cx},{base - 470} V{base - 520}" stroke="{ink}" stroke-width="4"/>'
                   f'<path d="M{cx - 20},{base - 506} Q{cx},{base - 526} {cx + 20},{base - 506}Z" fill="{self.gold()}" stroke="{ink}" stroke-width="2"/>')
        for k, dx in enumerate((-140, 0, 140)):
            out.append(f'<path d="M{cx + dx - 26},{base} V{base - 50} Q{cx + dx},{base - 80} {cx + dx + 26},{base - 50} V{base}Z" fill="#3b2412" stroke="#f4ead2" stroke-width="3"/>')
        return "".join(out)

    def toddy_palm(self, x, base, s, ink):
        top = base - 230 * s
        leaves = []
        for a in range(0, 360, 30):
            r = math.radians(a)
            ex, ey = x + 62 * s * math.cos(r), top + 44 * s * math.sin(r)
            leaves.append(f'<path d="M{x},{f(top)} L{f(ex - 10 * s * math.sin(r))},{f(ey + 6 * s * math.cos(r))} L{f(ex + 10 * s * math.sin(r))},{f(ey - 6 * s * math.cos(r))}Z" fill="#5c7a2e" stroke="{ink}" stroke-width="1.6"/>')
        return (f'<path d="M{x},{base} V{f(top)}" stroke="{ink}" stroke-width="{f(12 * s)}"/>'
                f'<path d="M{x},{base} V{f(top)}" stroke="#7a5a3a" stroke-width="{f(7 * s)}"/>' + "".join(leaves))


REALMS = [(DaiLongMenus, MC.DAILONG), (KambaraMenus, MC.KAMBARA), (TampadesaMenus, MC.TAMPADESA)]

if __name__ == "__main__":
    import sys
    which = sys.argv[1:] or ["home", "load_save", "settings", "custom_game", "about"]
    for cls, data in REALMS:
        for screen in which:
            getattr(cls(data), screen)()
