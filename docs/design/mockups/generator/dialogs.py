"""Renders the in-game dialogs and flow screens that had no mockup, for each
realm, into build/:

- game-menu, game-quit: the in-game Game menu, and its Quit popup;
- campaign-picker: Campaign on the home menu, once more than one is reached;
- briefing: the mission's scroll before it starts;
- campaign-complete: the finished stele;
- inspector: the building inspector;
- event: the important-event popup (a flood, a court request, a fire).

The looks come from pages.py, menus.py and screens.py; the text is in dialogs_content.py.
"""
import math

from world_map import T, f
from menus import DaiLongMenus, KambaraMenus, TampadesaMenus, W, H
from pages import Pages, EXTRA_ICONS, num
import menus_content as MC
import pages_content as PC
import screens_content as SC
import dialogs_content as DC

EXTRA_ICONS.update({
    "menu": "M-12,-8 H12 M-12,0 H12 M-12,8 H12",
    "save": "M-12,-13 H7 L13,-7 V13 H-12Z M-6,-13 V-5 H5 V-13 M-6,13 V3 H6 V13",
    "gear": "M0,-7 A7,7 0 1 1 0,7 A7,7 0 1 1 0,-7 M0,-15 V-10 M0,10 V15 M-15,0 H-10 M10,0 H15 M-10.6,-10.6 L-7,-7 M7,7 L10.6,10.6 M10.6,-10.6 L7,-7 M-7,7 L-10.6,10.6",
    "scroll": "M-10,-13 H10 Q14,-13 14,-9 Q14,-5 10,-5 M-10,-13 Q-14,-13 -14,-9 V9 Q-14,13 -10,13 H10 Q14,13 14,9 V-5 M-8,-1 H8 M-8,5 H6",
    "cart": "M-15,-6 H11 L8,6 H-12Z M-8,6 A4,4 0 1 0 -8,14 A4,4 0 1 0 -8,6 M4,6 A4,4 0 1 0 4,14 A4,4 0 1 0 4,6 M11,-6 L15,-12",
})

ATTRS = ["box_style", "panel_text", "panel_sub", "panel_rule", "accent", "btn_fill", "btn_fill_hover", "on_icon", "field_fill",
         "field_stroke", "on_mark"]


class Dialogs(Pages):
    stele_scale = 1.0
    stele_full = False

    def __init__(self, dd, menus_data, cls):
        cls.__init__(self, menus_data)
        self.dd = dd
        self.pd = next(p for p in PC.REALMS if p["key"] == dd["key"])
        self._capture = None

    # ------------------------------------------------------------ plumbing
    def page(self, name, parts):
        if self._capture is not None:
            self._capture.extend(parts)
            return
        super().page(name, parts)

    def hud_parts(self, keep_style=False):
        """The HUD of screens.py, without its build-button tooltip, as parts."""
        saved = {a: getattr(self, a) for a in ATTRS if hasattr(self, a)}
        menus_sd = self.sd
        self.sd = next(s for s in (SC.DAILONG, SC.KAMBARA, SC.TAMPADESA) if s["key"] == self.dd["key"])
        self._capture = []
        self.tooltip = lambda *a, **k: ""
        self.hud()
        parts, self._capture = self._capture, None
        del self.tooltip
        self.sd = menus_sd
        if keep_style and self.box_style == "lacquer":
            self.field_fill, self.field_stroke, self.on_mark = "#3f2d1e", "#8a6a48", "#3b2412"
        elif not keep_style:
            for a, v in saved.items():
                setattr(self, a, v)
        return parts

    def dim(self, op=0.5):
        return f'<rect width="{W}" height="{H}" fill="#120b05" opacity="{op}"/>'

    def stele_lines(self, x, y, w, title, lines, title_size, size, lh, color, new_color, font=None, title_font=None):
        s = self.stele_scale
        out = super().stele_lines(x, y, w, title, lines, title_size * min(1, s * 1.15), size * s, lh * s, color, new_color, font, title_font)
        if self.stele_full:  # No empty rows: every mission is carved.
            out = out.replace('stroke-dasharray="2 8" opacity="0.4"', 'stroke-dasharray="2 8" opacity="0"')
        return out

    # ------------------------------------------------------------ game menu
    def game_menu_panel(self, hover="Resume"):
        d = self.dd
        x, y, w, h = 660, 110, 600, 860
        parts = [self.mpanel(x, y, w, h, crest=True)]
        cx = x + w / 2
        ty = y + 236 if self.crest_space else y + 170
        parts.append(self.heading_center(cx, ty, "Game", 52))
        parts.append(self.t(cx, ty + 42, f"{d['mission_now']} · {d['city_now']}", 22, self.accent, "middle", 600, italic=True))
        parts.append(self.divider(x + 90, ty + 74, w - 180))
        iy = ty + 104
        for label in ["Resume", "Save", "Load", "Settings", "Quit to menu", "Exit game"]:
            on = label == hover
            if on:
                parts.append(f'<rect x="{x + 60}" y="{iy}" width="{w - 120}" height="62" rx="8" fill="{self.field_fill}" stroke="{self.accent}" stroke-width="2"/>')
                parts.append(self.item_marker(x + 92, iy + 25))
            parts.append(T(cx, iy + 42, label, self.title_font, 31, self.accent if on else self.panel_text, "middle", 700))
            iy += 72
        parts.append(self.t(cx, y + h - 70, d["last_saved"], 18, self.panel_sub, "middle", 600, italic=True))
        return parts

    def game_menu(self):
        parts = self.hud_parts() + [self.dim(0.45)] + self.game_menu_panel()
        self.page(f"game-menu-{self.dd['key']}", parts)

    def game_quit(self):
        parts = self.hud_parts() + [self.dim(0.45)] + self.game_menu_panel("Quit to menu") + [self.dim(0.35)]
        x, y, w, h = 520, 330, 880, 420
        parts.append(self.mpanel(x, y, w, h))
        cx = x + w / 2
        parts.append(self.dialog_title(cx, y + 124, "Quit to the menu?"))
        parts.append(self.t(cx, y + 182, "Anything since the last save will be lost.", 23, self.panel_text, "middle", 600))
        parts.append(self.t(cx, y + 218, f"{self.dd['last_saved']}.", 20, self.panel_sub, "middle", 600, italic=True))
        bw, by = 220, y + h - 132
        parts.append(self.button(cx - 1.5 * bw - 24, by, bw, 56, "Cancel", False))
        parts.append(self.button(cx - bw / 2, by, bw, 56, "Save and quit", False))
        parts.append(self.button(cx + bw / 2 + 24, by, bw, 56, "Quit", True))
        self.page(f"game-quit-{self.dd['key']}", parts)

    # ------------------------------------------------------------ campaign picker
    def campaign_picker(self):
        d = self.dd
        reached = self.sd["progress"]
        parts = [self.backdrop(True)]
        x, y, w, h = 160, 70, 1600, 940
        parts.append(self.mpanel(x, y, w, h))
        parts.append(self.dialog_title(x + w / 2, y + 116, "Campaign"))
        cw, gap = 440, 40
        x0 = x + (w - 3 * cw - 2 * gap) / 2
        cy, ch = y + 160, 620
        for k, ((key, realm, name, roman), dd) in enumerate(zip(MC.REALMS, DC.REALMS)):
            cx = x0 + k * (cw + gap)
            state = "done" if k < reached - 1 else "now" if k == reached - 1 else "locked"
            sel = state == "now"
            parts.append(f'<rect x="{cx}" y="{cy}" width="{cw}" height="{ch}" rx="8" fill="{self.field_fill}" fill-opacity="{0.9 if sel else 0.45}" '
                         f'stroke="{self.accent if sel else self.panel_rule}" stroke-width="{3 if sel else 1.5}"/>')
            mx = cx + cw / 2
            parts.append(self.realm_emblem(key, mx, cy + 96, 58, locked=state == "locked", current=sel))
            parts.append(T(mx, cy + 196, f"CAMPAIGN {roman}", self.title_font, 17, self.accent, "middle", 700, ls=3))
            col = self.panel_text if state != "locked" else self.panel_sub
            for j, line in enumerate(self.wrap(name, cw - 60, 29, 0.5)):
                parts.append(T(mx, cy + 238 + j * 36, line, self.title_font, 29, col, "middle", 700))
            parts.append(self.t(mx, cy + 316, realm, 20, self.panel_sub, "middle", 600, italic=True))
            parts.append(self.rule(cx + 40, cx + cw - 40, cy + 344))
            # One mark per mission.
            done = 10 if state == "done" else 5 if state == "now" else 0
            for m in range(10):
                px = mx + (m - 4.5) * 34
                if m < done:
                    parts.append(f'<circle cx="{px}" cy="{cy + 384}" r="10" fill="{self.accent}"/>')
                elif m == done and state == "now":
                    parts.append(f'<circle cx="{px}" cy="{cy + 384}" r="10" fill="{self.field_fill}" stroke="{self.accent}" stroke-width="3"/>')
                else:
                    parts.append(f'<circle cx="{px}" cy="{cy + 384}" r="9" fill="none" stroke="{self.panel_sub}" stroke-width="1.5" opacity="0.7"/>')
            if state == "done":
                lines = [("Finished: the stele is carved", self.panel_text, 700), (f"{dd['finished']['distinctions'][0]} of 12 distinctions", self.panel_sub, 600)]
                quote = dd["stele_more"][-1]
            elif state == "now":
                lines = [(f"Mission 6 of 10: {dd['missions'][5]}", self.panel_text, 700), ("Last played today, 23:10", self.panel_sub, 600)]
                quote = SC_STELE[key][-1]
            else:
                lines = [(f"Finish Campaign {MC.REALMS[k - 1][3]} to unlock", self.panel_sub, 700), ("", self.panel_sub, 600)]
                quote = None
            for j, (s, c, wgt) in enumerate(lines):
                parts.append(self.t(mx, cy + 440 + j * 30, s, 20 if j == 0 else 18, c, "middle", wgt, italic=j == 1))
            if quote:
                for j, line in enumerate(self.wrap(f"“{quote}”", cw - 70, 18, 0.5)[:3]):
                    parts.append(self.t(mx, cy + 518 + j * 26, line, 18, self.panel_sub, "middle", 600, italic=True))
        bb = y + h - 112
        parts.append(self.button(x0, bb, 200, 56, "Back", False))
        parts.append(self.button(x0 + 3 * cw + 2 * gap - 520, bb, 260, 56, "Replay a mission", False))
        parts.append(self.button(x0 + 3 * cw + 2 * gap - 240, bb, 240, 56, "Continue", True))
        self.page(f"campaign-picker-{d['key']}", parts)

    # ------------------------------------------------------------ briefing
    def briefing(self):
        d = self.dd
        b = d["briefing"]
        parts = [self.city_backdrop()]
        x, y, w, h = 200, 50, 1520, 980
        parts.append(self.mpanel(x, y, w, h))
        lx, rx, x1 = x + 100, x + 720, x + w - 100
        parts.append(self.section_title(lx, y + 112, f"{d['campaign']} · {b['mission']}"))
        parts.append(self.heading(lx, y + 172, b["name"], 52))
        parts.append(self.t(lx, y + 212, f"{b['city']} · {b['year']}", 22, self.panel_sub, italic=True))
        parts.append(self.divider(lx, y + 244, x1 - lx))
        # Left: the image and what's new.
        iy = y + 278
        parts.append(self.thumb(lx, iy, 560, 300))
        parts.append(self.t(lx + 280, iy + 328, b["image"], 17, self.panel_sub, "middle", 600, italic=True))
        ny = iy + 380
        parts.append(self.section_title(lx, ny, "New in this mission"))
        for k, s in enumerate(b["new"]):
            parts.append(self.good_bullet(lx + 10, ny + 38 + k * 36 - 7))
            parts.append(self.t(lx + 30, ny + 38 + k * 36, s, 20, self.panel_text, "start", 600))
        ty = ny + 38 + len(b["new"]) * 36 + 30
        parts.append(self.section_title(lx, ty, "Threats"))
        parts.append(self.glyph("warn", lx + 12, ty + 31, 0.75, self.accent))
        parts.append(self.t(lx + 36, ty + 38, b["threats"], 20, self.panel_text, "start", 600, italic=True))
        # Right: the lead-in, the story and the goals.
        ry = y + 300
        for k, line in enumerate(self.wrap(f"“{b['lead']}”", x1 - rx, 26, 0.48)):
            parts.append(self.t(rx, ry + k * 36, line, 26, self.accent, "start", 600, italic=True))
        ry += 36 * len(self.wrap(f"“{b['lead']}”", x1 - rx, 26, 0.48)) + 18
        for para in b["text"]:
            ls = self.wrap(para, x1 - rx, 21, 0.47)
            for k, line in enumerate(ls):
                parts.append(self.t(rx, ry + k * 31, line, 21, self.panel_text, "start", 600))
            ry += len(ls) * 31 + 16
        ry += 22
        parts.append(self.section_title(rx, ry, "Goals"))
        parts.append(self.t(x1, ry, "TARGET", 15, self.panel_sub, "end", 700, ls=2))
        for k, (g, target) in enumerate(b["goals"]):
            gy = ry + 44 + k * 42
            parts.append(f'<circle cx="{rx + 12}" cy="{gy - 7}" r="11" fill="none" stroke="{self.accent}" stroke-width="2"/>')
            parts.append(self.t(rx + 36, gy, g, 21, self.panel_text, "start", 700))
            parts.append(self.t(x1, gy, target, 21, self.panel_text, "end", 600))
            parts.append(self.rule(rx, x1, gy + 15))
        bb = y + h - 122
        parts.append(self.button(x1 - 520, bb, 260, 58, "Replay the story", False))
        parts.append(self.button(x1 - 240, bb, 240, 58, "Begin", True))
        self.page(f"briefing-{d['key']}", parts)

    # ------------------------------------------------------------ campaign complete
    def campaign_complete(self):
        d = self.dd
        fin = d["finished"]
        sd = next(s for s in (SC.DAILONG, SC.KAMBARA, SC.TAMPADESA) if s["key"] == d["key"])
        # A wider stele than the debrief's, for all ten lines; its niche is stretched to match.
        self.stele_scale, self.stele_full = self.final_stele_scale, True
        parts = [self.background(), f'<g transform="translate(460,0) scale(1.12,1) translate(-460,0)">{self.stele_backdrop()}</g>']
        parts.append(self.stele(80, 40, 780, 1010, sd["stele_title"], sd["stele"] + d["stele_more"]))
        self.stele_scale, self.stele_full = 1.0, False
        x, y, w, h = 900, 60, 980, 960
        parts.append(self.box(x, y, w, h, big=True))
        ix, x1 = x + 56, x + w - 56
        cy = y + 82
        parts.append(T(ix, cy, f"{d['campaign']} complete".upper(), self.title_font, 20, self.accent, "start", 700, ls=3))
        cy += 64
        for k, line in enumerate(self.wrap(d["campaign_name"], w - 112, 52, 0.52)):
            parts.append(self.heading(ix, cy + k * 58, line, 52))
        cy += 58 * (len(self.wrap(d["campaign_name"], w - 112, 52, 0.52)) - 1) + 40
        parts.append(self.t(ix, cy, fin["years"], 21, self.panel_sub, italic=True))
        cy += 30
        parts.append(self.divider(ix, cy, w - 112))
        cy += 64
        for k, line in enumerate(self.wrap(f"“{d['closing']}”", w - 160, 28, 0.5)):
            parts.append(self.t(x + w / 2, cy + k * 38, line, 28, self.accent, "middle", 600, italic=True))
        cy += 38 * len(self.wrap(f"“{d['closing']}”", w - 160, 28, 0.5)) + 30
        parts.append(self.section_title(ix, cy, "Missions"))
        parts.append(self.t(x1, cy, "each mark is a distinction", 17, self.panel_sub, "end", italic=True))
        colw = (w - 112) / 2
        marks = [2, 1, 1, 2, 1, 0, 1, 2, 1, 1]
        for k, name in enumerate(d["missions"]):
            mx = ix + (k // 5) * colw
            my = cy + 42 + (k % 5) * 36
            parts.append(self.t(mx, my, str(k + 1), 18, self.panel_sub, "start", 700))
            parts.append(self.t(mx + 30, my, name, 19, self.panel_text, "start", 600))
            for j in range(marks[k]):
                parts.append(self.glyph("star", mx + colw - 30 - j * 22, my - 6, 0.45, self.accent, 3))
        cy += 42 + 5 * 36 + 18
        stats = [("Missions", "10 of 10"), ("Distinctions", f"{fin['distinctions'][0]} of {fin['distinctions'][1]}"),
                 ("Largest city", f"{fin['largest'][0]} · {fin['largest'][1]}")]
        for k, (lab, v) in enumerate(stats):
            sx = ix + k * (w - 112) / 3
            parts.append(self.t(sx, cy, lab, 19, self.panel_sub, "start", 600))
            parts.append(T(sx, cy + 38, v, self.title_font, 28, self.panel_text, "start", 700))
        cy += 76
        nkey, ntext = fin["next"]
        parts.append(f'<rect x="{ix}" y="{cy}" width="{w - 112}" height="84" fill="{self.note_fill}" stroke="{self.accent}" stroke-width="1.5" stroke-dasharray="6 3"/>')
        if nkey:
            parts.append(self.realm_emblem(nkey, ix + 46, cy + 42, 28))
        else:
            parts.append(self.glyph("star", ix + 46, cy + 42, 1.0, self.accent))
        nl = self.wrap(ntext, w - 190, 21, 0.45)
        for k, line in enumerate(nl):
            parts.append(self.t(ix + 90, cy + 49 - (len(nl) - 1) * 14 + k * 28, line, 21, self.panel_text, "start", 600))
        parts.append(self.button(ix, y + h - 98, 300, 58, "Watch the epilogue", False))
        parts.append(self.button(x1 - 340, y + h - 98, 340, 58, "Return to the menu", True))
        self.page(f"campaign-complete-{d['key']}", parts)

    # ------------------------------------------------------------ HUD: menu buttons, build menu
    def menu_buttons(self, hover=None):
        """The game-menu buttons, in their own bar left of the build bar."""
        icons = [("menu", "Game"), ("save", "Load / Save"), ("gear", "Settings"), ("learning", "City book"), ("scroll", "Briefing")]
        bw = 30 + len(icons) * 50
        bx = W - 96 - 12 - 18 - bw
        out = [self.box(bx, 10, bw, 74)]
        for k, (ic, _) in enumerate(icons):
            x = bx + 40 + k * 50
            on = ic == hover
            fill = self.accent if on else "none"
            out.append(f'<rect x="{x - 20}" y="{27}" width="40" height="40" rx="6" fill="{fill}" stroke="{self.panel_rule}" stroke-width="1.5"/>'
                       + self.glyph(ic, x, 47, 0.8, self.on_icon if on else self.panel_text))
        return "".join(out), bx

    def coin_small(self, x, y):
        return (f'<circle cx="{x}" cy="{y}" r="9" fill="#c99a3c" stroke="#5a3a14" stroke-width="1.5"/>'
                f'<rect x="{x - 3}" y="{y - 3}" width="6" height="6" fill="#2a170c"/>')

    # ------------------------------------------------------------ inspector
    def inspector(self):
        d = self.dd
        b = DC.INSPECTOR
        parts = self.hud_parts(keep_style=True)
        bar, _ = self.menu_buttons()
        parts.append(bar)
        w, h = 500, 780
        x, y = W - 96 - 12 - 20 - w, 100
        parts.append(self.box(x, y, w, h))
        x0, x1 = x + 28, x + w - 28
        parts.append(f'<rect x="{x0}" y="{y + 28}" width="76" height="76" rx="8" fill="{self.btn_fill}" stroke="{self.panel_rule}" stroke-width="1.5"/>'
                     + self.glyph("industry", x0 + 38, y + 66, 1.4, self.accent))
        parts.append(T(x0 + 96, y + 64, b["name"], self.title_font, 30, self.panel_text, "start", 700))
        parts.append(self.t(x0 + 96, y + 94, f"{b['kind']} · {b['size']}", 18, self.panel_sub, "start", 600, italic=True))
        parts.append(f'<rect x="{x1 - 36}" y="{y + 24}" width="36" height="36" rx="5" fill="none" stroke="{self.panel_rule}" stroke-width="1.5"/>'
                     + self.glyph("close", x1 - 18, y + 42, 0.8, self.panel_text, 3))
        sy = y + 130
        parts.append(f'<circle cx="{x0 + 8}" cy="{sy}" r="7" fill="{self.good_col}"/>')
        parts.append(self.t(x0 + 24, sy + 7, b["status"], 20, self.panel_text, "start", 700))
        parts.append(self.t(x1, sy + 7, "Pottery for 18 houses", 18, self.panel_sub, "end", 600, italic=True))
        parts.append(self.rule(x0, x1, sy + 28))
        # Workers.
        wy = sy + 74
        parts.append(self.section_title(x0, wy, "Workers"))
        have, need = b["workers"]
        parts.append(self.t(x1, wy, f"{have} / {need}", 19, self.panel_text, "end", 700))
        for k in range(need):
            parts.append(self.glyph("population", x0 + 12 + k * 36, wy + 38, 0.62, self.accent if k < have else self.panel_rule))
        parts.append(self.t(x0, wy + 80, b["labour_note"], 17, self.accent, "start", 600, italic=True))
        # Production.
        py = wy + 132
        parts.append(self.section_title(x0, py, "Production"))
        parts.append(self.t(x1, py, b["batch"], 17, self.panel_sub, "end", 600, italic=True))
        rows = [("Needs", *b["inputs"][0]), ("Makes", *b["outputs"][0])]
        for k, (lab, good, n, cap) in enumerate(rows):
            ry = py + 44 + k * 44
            parts.append(self.t(x0, ry, lab, 18, self.panel_sub, "start", 600))
            parts.append(self.good_bullet(x0 + 82, ry - 6))
            parts.append(self.t(x0 + 98, ry, good, 19, self.panel_text, "start", 700))
            parts.append(self.bar(x0 + 190, ry - 12, x1 - x0 - 290, 10, n / cap))
            parts.append(self.t(x1, ry, f"{n} / {cap}", 18, self.panel_text, "end", 600))
        ry = py + 44 + 2 * 44
        parts.append(self.t(x0, ry, "Next batch", 18, self.panel_sub, "start", 600))
        parts.append(self.bar(x0 + 190, ry - 12, x1 - x0 - 290, 10, b["progress"]))
        parts.append(self.t(x1, ry, f"{round(b['progress'] * 100)}%", 18, self.panel_text, "end", 600))
        dy = ry + 34
        parts.append(self.rule(x0, x1, dy))
        parts.append(self.glyph("cart", x0 + 16, dy + 36, 0.8, self.accent))
        parts.append(self.t(x0 + 44, dy + 43, b["delivery"], 18, self.panel_text, "start", 600))
        # Costs.
        cy = dy + 104
        parts.append(self.section_title(x0, cy, "Costs"))
        wages = have * self.pd["wage"]
        for k, (lab, v) in enumerate([("Upkeep", f"{b['upkeep']} gold a year"), ("Wages", f"{have} workers · {wages} gold a year"), ("Built", b["built"])]):
            ry = cy + 40 + k * 36
            parts.append(self.t(x0, ry, lab, 18, self.panel_sub, "start", 600))
            parts.append(self.t(x1, ry, v, 18, self.panel_text, "end", 600))
        bb = y + h - 86
        parts.append(self.button(x0, bb, 210, 56, "Pause work", False))
        parts.append(self.button(x1 - 210, bb, 210, 56, "Demolish", False))
        self.page(f"inspector-{d['key']}", parts)

    # ------------------------------------------------------------ event popup
    def event(self):
        d = self.dd
        e = d["event"]
        parts = self.hud_parts() + [self.dim(0.4)]
        x, y, w, h = 500, 110, 920, 810
        parts.append(self.mpanel(x, y, w, h))
        ix, x1 = x + 90, x + w - 90
        iy = y + 76
        parts.append(self.thumb(ix, iy, x1 - ix, 230))
        parts.append(f'<rect x="{ix}" y="{iy}" width="{x1 - ix}" height="230" fill="#120b05" opacity="0.25"/>')
        cx = (ix + x1) / 2
        parts.append(f'<circle cx="{cx}" cy="{iy + 115}" r="58" fill="{self.field_fill}" stroke="{self.accent}" stroke-width="4"/>'
                     + self.glyph(e["icon"], cx, iy + 115, 2.4, self.accent, 2.6))
        ty = iy + 290
        parts.append(self.section_title(ix, ty, e["kind"]))
        parts.append(self.t(x1, ty, e["when"], 17, self.panel_sub, "end", 600, italic=True))
        parts.append(self.heading(ix, ty + 54, e["title"], 42))
        ly = ty + 102
        for k, line in enumerate(self.wrap(e["text"], x1 - ix, 21, 0.47)):
            parts.append(self.t(ix, ly + k * 31, line, 21, self.panel_text, "start", 600))
        ly += 31 * len(self.wrap(e["text"], x1 - ix, 21, 0.47)) + 14
        for k, (ic, s) in enumerate(e["effects"]):
            ey = ly + k * 42
            parts.append(self.rule(ix, x1, ey - 28))
            parts.append(self.glyph(ic, ix + 14, ey - 7, 0.7, self.accent))
            parts.append(self.t(ix + 44, ey, s, 20, self.panel_text, "start", 600))
        bb = y + h - 126
        parts.append(self.button(x1 - 520, bb, 300, 56, e["buttons"][0], False))
        parts.append(self.button(x1 - 200, bb, 200, 56, e["buttons"][1], True))
        self.page(f"event-{d['key']}", parts)


SC_STELE = {s["key"]: s["stele"] for s in (SC.DAILONG, SC.KAMBARA, SC.TAMPADESA)}


class DaiLongDialogs(Dialogs, DaiLongMenus):
    final_stele_scale = 0.74
    good_col = "#4f6f2a"
    crest_space = False
    note_fill = "#f2e4c0"

    def __init__(self, dd):
        Dialogs.__init__(self, dd, MC.DAILONG, DaiLongMenus)


class KambaraDialogs(Dialogs, KambaraMenus):
    final_stele_scale = 0.8
    good_col = "#3f6a2a"
    crest_space = True

    def __init__(self, dd):
        Dialogs.__init__(self, dd, MC.KAMBARA, KambaraMenus)


class TampadesaDialogs(Dialogs, TampadesaMenus):
    final_stele_scale = 0.68
    good_col = "#4f6420"
    crest_space = True

    def __init__(self, dd):
        Dialogs.__init__(self, dd, MC.TAMPADESA, TampadesaMenus)


REALMS = [(DaiLongDialogs, DC.DAILONG), (KambaraDialogs, DC.KAMBARA), (TampadesaDialogs, DC.TAMPADESA)]
SCREENS = ["game_menu", "game_quit", "campaign_picker", "briefing", "campaign_complete", "inspector", "event"]

if __name__ == "__main__":
    import sys
    which = sys.argv[1:] or SCREENS
    for cls, data in REALMS:
        for screen in which:
            getattr(cls(data), screen)()
