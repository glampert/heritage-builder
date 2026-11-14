"""The HUD's frames, shared by every dialog (decided with the user on 2026-11-14).

Mixed in front of each realm's Menus class, so the menus, the dialogs, the city
book and the HUD all draw their panels with the same carving:

- Đại Long: carved red wood round parchment, its ornaments outside the
  parchment (dailong_carving.py); dark text on the parchment, as before;
- Kambara: carved sandstone blocks round dark cracked stone, moss and vines,
  a Bayon face over large panels (kambara_carving.py); light text, moss-green
  accents;
- Tampadesa: weathered brick round dark mudbrick, gilt flame scrolls, stupa
  piers and nagas on large panels (tampadesa_brick.py); light text and gold.

`box` is a panel at its rect (the HUD's panels, and the large panels of the
full-screen screens); `mpanel` is a dialog, its frame inside the rect given so
the dialogs keep their layouts.
"""
from world_map import T, f
import kambara_carving as KC
import dailong_carving as DW
import tampadesa_brick as TB


class Frames:
    frame_colours = {}

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        # As instance values, so no class later in a page's MRO overrides them.
        self.crest_space = False   # The crests sit above the panels now, outside the text's room.
        for k, v in self.frame_colours.items():
            setattr(self, k, v)

    def pid(self, base):
        self._pid = getattr(self, "_pid", 0) + 1
        return f"pal-{base}-{self._pid}"

    def woodwork(self):
        if getattr(self, "_woodwork", None) is None:
            self._woodwork = DW.Woodwork(self.defs, self.pid)
        return self._woodwork

    def carving(self):
        if getattr(self, "_carving", None) is None:
            self._carving = KC.Carving(self.defs, self.pid)
        return self._carving

    def brickwork(self):
        if getattr(self, "_brickwork", None) is None:
            self._brickwork = TB.Brickwork(self.defs, self.pid, self.carving())
        return self._brickwork

    @staticmethod
    def is_big(x, y, w, h, big):
        """The HUD's rule: large panels below the top bar get the full ornaments."""
        if big is None:
            return w >= 300 and h >= 220 and y > 120
        return big

    def shadow(self, x, y, w, h, op=0.45):
        return f'<rect x="{f(x + 8)}" y="{f(y + 10)}" width="{f(w)}" height="{f(h)}" fill="#000" opacity="{op}"/>'


# ====================================================================== Đại Long
class DaiLongFrames(Frames):
    def box(self, x, y, w, h, big=None):
        return self.woodwork().panel(x, y, w, h, big=self.is_big(x, y, w, h, big))

    def mpanel(self, x, y, w, h, crest=False):
        px, py, pw, ph = x + 24, y + 34, w - 48, h - 60
        return self.shadow(px - 20, py - 30, pw + 40, ph + 54) + self.woodwork().panel(px, py, pw, ph, big=crest or h >= 560)


# ====================================================================== Kambara
class KambaraFrames(Frames):
    # Light text on the carved frames' dark stone; moss and stone for the accents.
    frame_colours = {
        "panel_text": "#efe3c6",
        "panel_sub": "#c9b690",
        "panel_rule": "#5f6152",
        "accent": "#98ad7a",
        "btn_fill": "#4a4538",
        "btn_fill_hover": "#5a5444",
        "field_fill": "#2a2620",
        "field_stroke": "#77705a",
        "knob_fill": "#d9cdb0",
        "on_mark": "#1f2418",
        "note_fill": "#2e2a22",
        "good_col": "#98ad7a",
        "on_icon": "#1f2418",
    }

    def box(self, x, y, w, h, big=None):
        big = self.is_big(x, y, w, h, big)
        thick = h >= 400
        t = 20 if thick else (11 if h < 120 else 13)
        crest = (72 if thick else 46) if big else None
        return (self.shadow(x, y, w, h, 0.4) if thick else "") + self.carving().frame(
            x, y, w, h, t=t, crest=crest, seed=int(x * 7 + y) % 89, vines=True, small=False)

    def mpanel(self, x, y, w, h, crest=False):
        big = crest or h >= 560
        return self.shadow(x, y, w, h) + self.carving().frame(x, y, w, h, t=24, crest=(88 if big else None),
                                                              seed=int(x * 7 + y) % 89, vines=True)

    def light(self, x, y, s, font, size, anchor, weight=700, ls=None, fill=None):
        # Carved into dark stone: the letters lit, a dark cut under them.
        return self.engrave(x, y, s, font, size, anchor, weight, ls=ls, fill=fill or self.panel_text, dark=False)

    def heading(self, x, y, s, size):
        return self.light(x, y, s, self.title_font, size, "start")

    def heading_center(self, x, y, s, size):
        return self.light(x, y, s, self.title_font, size, "middle")

    def dialog_title(self, cx, y, s):
        return self.light(cx, y, s, self.title_font, 44, "middle")

    def panel_title(self, x, y, s):
        return self.light(x, y, s, self.title_font, 42, "middle")

    def section_title(self, x, y, s):
        return self.light(x, y, s.upper(), self.title_font, 17, "start", ls=2, fill=self.accent)

    def divider(self, x, y, w):
        return (f'<line x1="{x}" y1="{y}" x2="{x + w}" y2="{y}" stroke="#12100c" stroke-width="1.6"/>'
                f'<line x1="{x}" y1="{y + 1.6}" x2="{x + w}" y2="{y + 1.6}" stroke="#8c7e60" stroke-width="1" opacity="0.6"/>'
                + self.leaf(x + w / 2 - 34, y, 1.0, 0, "#7d9a58") + self.leaf(x + w / 2 + 34, y, 1.0, 180, "#5f7f3a"))

    def button(self, x, y, w, h, label, primary):
        base = "#5f7a40" if primary else "#4a4538"
        out = [f'<rect x="{x + 3}" y="{y + 5}" width="{w}" height="{h}" fill="#000" opacity="0.45"/>',
               self.stone(x, y, w, h, base, 30 + int(y) % 7),
               f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="none" stroke="#12100c" stroke-width="2"/>',
               f'<path d="M{x + 2},{y + h - 2} V{y + 2} H{x + w - 2}" fill="none" stroke="#c9b690" stroke-width="1.5" opacity="0.45"/>',
               self.light(x + w / 2, y + h / 2 + 8, label, self.title_font, 22, "middle", fill="#f6eedc" if primary else self.panel_text)]
        return "".join(out)

    def meter(self, x, y, w, v):
        out = []
        sw = (w - 4 * 8) / 5
        for i in range(5):
            on = i < v
            out.append(f'<rect x="{f(x + i * (sw + 8))}" y="{y}" width="{f(sw)}" height="14" fill="{self.accent if on else "#3a362c"}" stroke="#12100c" stroke-width="1.2"/>')
        return "".join(out)

    def request_box(self, x, y, w, h, req):
        return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{self.note_fill}" stroke="#12100c" stroke-width="1.5"/>'
                f'<rect x="{x + 4}" y="{y + 4}" width="{w - 8}" height="{h - 8}" fill="none" stroke="{self.accent}" stroke-width="1"/>'
                + self.light(x + 18, y + 30, "REQUEST FROM THEIR COURT", self.title_font, 15, "start", ls=1.5, fill=self.accent)
                + T(x + 18, y + 62, f"{req[0]}  ·  {req[1]}", self.body_font, 20, self.panel_text, "start", 700))


# ====================================================================== Tampadesa
class TampadesaFrames(Frames):
    # Light text and gold on the dark mudbrick (the HUD's colours).
    frame_colours = {
        "box_style": "lacquer",
        "panel_text": "#f3e2bf",
        "panel_sub": "#d9b98a",
        "panel_rule": "#8a6a48",
        "accent": "#e9c977",
        "btn_fill": "#4e3826",
        "btn_fill_hover": "#634831",
        "field_fill": "#3a1e14",
        "field_stroke": "#8a6a48",
        "knob_fill": "#f6e3bc",
        "on_mark": "#3b2412",
        "note_fill": "#3a1e14",
        "good_col": "#b5c46a",
        "on_icon": "#3b2412",
    }

    def box(self, x, y, w, h, big=None):
        return self.brickwork().panel(x, y, w, h, big=self.is_big(x, y, w, h, big))

    def mpanel(self, x, y, w, h, crest=False):
        px, py, pw, ph = x + 20, y + 28, w - 40, h - 48
        return self.shadow(px - 20, py - 26, pw + 40, ph + 46) + self.brickwork().panel(px, py, pw, ph, big=crest or h >= 560)

    def heading(self, x, y, s, size):
        return T(x, y, s, self.title_font, size, self.panel_text, "start", 400)

    def heading_center(self, x, y, s, size):
        return T(x, y, s, self.title_font, size, self.panel_text, "middle", 400)

    def dialog_title(self, cx, y, s):
        return T(cx, y, s, self.title_font, 44, self.panel_text, "middle", 400)

    def panel_title(self, x, y, s):
        return T(x, y, s, self.title_font, 42, self.panel_text, "middle", 400)

    def section_title(self, x, y, s):
        return T(x, y, s.upper(), self.label_font, 17, self.accent, "start", 700, ls=2)

    def meter(self, x, y, w, v):
        out = []
        sw = (w - 4 * 8) / 5
        for i in range(5):
            on = i < v
            out.append(f'<rect x="{f(x + i * (sw + 8))}" y="{y}" width="{f(sw)}" height="14" fill="{self.gold() if on else "#4e3020"}" stroke="#1e1008" stroke-width="1.2" rx="2"/>')
        return "".join(out)
