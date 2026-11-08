"""Renders the City book mockups for each realm into build/: one dialog with
side tabs over the blurred city, and a page per area (overview, population,
goods, finances, ratings, labour, food). The realm looks come from menus.py
(panels and form widgets), screens.py (icons, bars) and world_map.py (titles,
dividers, buttons). The numbers are in pages_content.py.
"""
import math

from world_map import T, f, BUILD
from menus import DaiLongMenus, KambaraMenus, TampadesaMenus, jpeg_b64, W, H
import menus_content as MC
import pages_content as PC

EXTRA_ICONS = {
    "jar": "M-8,-15 H8 M-5,-15 V-10 Q-14,-6 -12,4 Q-10,15 0,15 Q10,15 12,4 Q14,-6 5,-10 V-15 M-11,0 H11",
    "star": "M0,-15 L4.4,-5 L15,-4.6 L7,2.6 L9.3,13 L0,7.6 L-9.3,13 L-7,2.6 L-15,-4.6 L-4.4,-5Z",
    "close": "M-9,-9 L9,9 M9,-9 L-9,9",
    "minus": "M-8,0 H8",
    "plus": "M-8,0 H8 M0,-8 V8",
    "up": "M-7,4 L0,-5 L7,4",
    "down": "M-7,-4 L0,5 L7,-4",
}


def num(v, sign=False):
    s = f"{v:+,}" if sign else f"{v:,}"
    return s.replace("-", "−")


class Pages:
    """The City book: layout and widgets shared by the three realms."""

    X, Y, PW, PH = 150, 60, 1620, 960
    good_col = "#4f6f2a"

    # ------------------------------------------------------------ frame
    def city_backdrop(self):
        img = jpeg_b64(BUILD / f"city_{self.pd['key']}.jpg", (W // 2, H // 2))
        fid = self.nid("blur")
        self.defs.append(f'<filter id="{fid}" x="0" y="0" width="100%" height="100%"><feGaussianBlur stdDeviation="3"/></filter>')
        return (f'<image x="-10" y="-10" width="{W + 20}" height="{H + 20}" xlink:href="data:image/jpeg;base64,{img}" preserveAspectRatio="xMidYMid slice" filter="url(#{fid})"/>'
                f'<rect width="{W}" height="{H}" fill="#120b05" opacity="0.5"/>')

    def book(self, page):
        """The dialog's frame, side tabs and page title. Returns the parts and the content box."""
        d = self.pd
        X, Y, PW, PH = self.X, self.Y, self.PW, self.PH
        parts = [self.city_backdrop(), self.mpanel(X, Y, PW, PH)]
        tx, tw = X + 76, 270
        parts.append(self.heading(tx, Y + 128, d["city"], 38))
        parts.append(self.t(tx, Y + 162, d["date"], 18, self.panel_sub, italic=True))
        parts.append(self.t(tx, Y + 188, f"Month {d['month']} · {d['season']}", 18, self.panel_sub, italic=True))
        for k, (key, label, ic) in enumerate(PC.PAGES):
            ty = Y + 226 + k * 70
            on = key == page
            if on:
                parts.append(f'<rect x="{tx - 12}" y="{ty}" width="{tw + 12}" height="58" rx="6" fill="{self.accent}" stroke="{self.field_stroke}" stroke-width="1.5"/>')
            else:
                parts.append(f'<line x1="{tx}" y1="{ty + 64}" x2="{tx + tw}" y2="{ty + 64}" stroke="{self.panel_rule}" stroke-width="1"/>')
            col = self.on_mark if on else self.accent
            parts.append(self.glyph(ic, tx + 22, ty + 29, 0.85, col))
            parts.append(T(tx + 56, ty + 37, label, self.title_font, 24, self.on_mark if on else self.panel_text, "start", 700))
        parts.append(self.button(tx, Y + PH - 132, tw, 56, "Close", False))
        parts.append(f'<line x1="{X + 380}" y1="{Y + 96}" x2="{X + 380}" y2="{Y + PH - 96}" stroke="{self.panel_rule}" stroke-width="1.5"/>')
        x0, x1 = X + 420, X + PW - 84
        title = dict((k, l) for k, l, _ in PC.PAGES)[page]
        parts.append(self.heading(x0, Y + 128, title, 44))
        parts.append(self.t(x1, Y + 126, self.page_note(page), 19, self.panel_sub, "end", italic=True))
        parts.append(self.divider(x0, Y + 156, x1 - x0))
        return parts, (x0, Y + 186, x1, Y + PH - 84)

    def page_note(self, page):
        return {"overview": "Updated each month", "population": "The last twelve months",
                "goods": "In storage yards and granaries", "finances": f"Year {self.pd['date'].split('year ')[-1]} · month {self.pd['month']}"
                if "year " in self.pd["date"] else f"Month {self.pd['month']}",
                "ratings": "What raises and lowers each rating", "labour": "Workers go to the highest priority first",
                "food": "Granaries and what the houses eat",
                "heritage": "The monument, the patrons and the festivals"}[page]

    # ------------------------------------------------------------ widgets
    def glyph(self, kind, x, y, s, c, sw=2.4):
        if kind not in EXTRA_ICONS:
            return self.icon(kind, x, y, s, c)
        return (f'<path d="{EXTRA_ICONS[kind]}" transform="translate({f(x)},{f(y)}) scale({s})" fill="none" stroke="{c}" '
                f'stroke-width="{sw}" stroke-linecap="round" stroke-linejoin="round"/>')

    def card(self, x, y, w, h, title=None, note=None):
        out = [f'<rect x="{f(x)}" y="{f(y)}" width="{f(w)}" height="{f(h)}" rx="6" fill="{self.field_fill}" fill-opacity="0.55" stroke="{self.panel_rule}" stroke-width="1.5"/>']
        if title:
            out.append(self.section_title(x + 22, y + 36, title))
        if note:
            out.append(self.t(x + w - 22, y + 36, note, 17, self.panel_sub, "end", italic=True))
        return "".join(out)

    def rule(self, x0, x1, y):
        return f'<line x1="{f(x0)}" y1="{f(y)}" x2="{f(x1)}" y2="{f(y)}" stroke="{self.panel_rule}" stroke-width="1"/>'

    def change(self, x, y, v, size=18, unit=""):
        """A monthly change, right-aligned at x: an arrow and the signed number."""
        if v == 0:
            return self.t(x, y, "steady", size, self.panel_sub, "end", italic=True)
        col = self.good_col if v > 0 else self.accent
        w = len(num(v, True) + unit) * size * 0.52
        return (self.glyph("up" if v > 0 else "down", x - w - 12, y - size * 0.32, 0.7, col, 3)
                + self.t(x, y, num(v, True) + unit, size, col, "end", 700))

    def ring(self, cx, cy, r, v, goal, sw=9, label_size=30):
        a = v / 100 * 2 * math.pi
        ex, ey = cx + r * math.sin(a), cy - r * math.cos(a)
        out = [f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="none" stroke="{self.panel_rule}" stroke-width="{sw}" opacity="0.7"/>',
               f'<path d="M{cx},{cy - r} A{r},{r} 0 {1 if v > 50 else 0} 1 {f(ex)},{f(ey)}" fill="none" stroke="{self.accent}" stroke-width="{sw}" stroke-linecap="round"/>']
        if goal is not None:
            g = goal / 100 * 2 * math.pi
            r0, r1 = r - sw, r + sw
            out.append(f'<line x1="{f(cx + r0 * math.sin(g))}" y1="{f(cy - r0 * math.cos(g))}" x2="{f(cx + r1 * math.sin(g))}" y2="{f(cy - r1 * math.cos(g))}" '
                       f'stroke="{self.panel_text}" stroke-width="3" stroke-linecap="round"/>')
        out.append(T(cx, cy + label_size * 0.36, str(v), self.title_font, label_size, self.panel_text, "middle", 700))
        return "".join(out)

    def chart(self, x, y, w, h, values, labels=True, axis=True, area=True, sw=3, last_label=False):
        lo, hi = min(values), max(values)
        pad = (hi - lo) * 0.15 or 1
        lo, hi = lo - pad, hi + pad
        step = w / (len(values) - 1)
        pts = [(x + i * step, y + h - (v - lo) / (hi - lo) * h) for i, v in enumerate(values)]
        out = []
        if axis:
            for k in range(3):
                gy = y + h * k / 2
                out.append(f'<line x1="{f(x)}" y1="{f(gy)}" x2="{f(x + w)}" y2="{f(gy)}" stroke="{self.panel_rule}" stroke-width="1" stroke-dasharray="4 5"/>')
                val = hi - (hi - lo) * k / 2
                out.append(self.t(x - 10, gy + 6, num(int(round(val, -1))), 15, self.panel_sub, "end", 600))
        line = " ".join(f"{f(px)},{f(py)}" for px, py in pts)
        if area:
            out.append(f'<path d="M{f(x)},{f(y + h)} L{line} L{f(x + w)},{f(y + h)}Z" fill="{self.accent}" opacity="0.14"/>')
        out.append(f'<polyline points="{line}" fill="none" stroke="{self.accent}" stroke-width="{sw}" stroke-linejoin="round" stroke-linecap="round"/>')
        px, py = pts[-1]
        out.append(f'<circle cx="{f(px)}" cy="{f(py)}" r="{sw + 2}" fill="{self.accent}" stroke="{self.knob_fill}" stroke-width="2"/>')
        if labels:
            month = self.pd["month"]
            for i in range(len(values)):
                m = (month - len(values) + i) % 12
                out.append(self.t(x + i * step, y + h + 26, PC.MONTHS[m], 15, self.panel_sub, "middle", 700 if i == len(values) - 1 else 600))
        return "".join(out)

    def stepper(self, x, y, value, w=150):
        """− value + buttons, left edge at x, centred on y."""
        b = 36
        out = []
        for bx, ic in ((x, "minus"), (x + w - b, "plus")):
            out.append(f'<rect x="{bx}" y="{y - b / 2}" width="{b}" height="{b}" rx="5" fill="{self.btn_fill}" stroke="{self.field_stroke}" stroke-width="1.5"/>'
                       + self.glyph(ic, bx + b / 2, y, 0.9, self.panel_text, 3))
        out.append(T(x + w / 2, y + 8, value, self.title_font, 23, self.panel_text, "middle", 700))
        return "".join(out)

    def segments(self, x, y, w, labels, sel):
        n = len(labels)
        sw = (w - (n - 1) * 6) / n
        out = []
        for k, lab in enumerate(labels):
            sx = x + k * (sw + 6)
            on = k == sel
            out.append(f'<rect x="{f(sx)}" y="{y}" width="{f(sw)}" height="46" rx="5" fill="{self.accent if on else self.field_fill}" stroke="{self.field_stroke}" stroke-width="1.5"/>')
            out.append(T(sx + sw / 2, y + 30, lab, self.title_font, 19, self.on_mark if on else self.panel_text, "middle", 700))
        return "".join(out)

    def stat(self, x, y, w, ic, label, value, sub, extra=""):
        out = [self.card(x, y, w, 184),
               self.glyph(ic, x + 34, y + 40, 0.85, self.accent),
               self.t(x + 62, y + 47, label, 19, self.panel_sub, "start", 700),
               T(x + 24, y + 106, value, self.title_font, 40, self.panel_text, "start", 700),
               self.t(x + 24, y + 136, sub, 17, self.panel_sub, italic=True), extra]
        return "".join(out)

    # ------------------------------------------------------------ data
    def ledger_rows(self):
        rows = []
        accounts = [self.pd["ledger"]["last"], self.pd["ledger"]["this"]]
        get = lambda i: [a[i] for a in accounts]
        rows.append(("Taxes", get(0), "row"))
        rows.append(("Income", get(0), "total"))
        for i, name in ((1, "Construction"), (2, "Wages"), (3, "Upkeep")):
            rows.append((name, get(i), "row"))
        rows.append(("Spending", [a[1] + a[2] + a[3] for a in accounts], "total"))
        rows.append(("Net", [a[0] + a[1] + a[2] + a[3] for a in accounts], "net"))
        rows.append(("Balance at the start of the year", get(4), "note"))
        rows.append(("Balance at the end of the year", [a[4] + a[0] + a[1] + a[2] + a[3] for a in accounts], "note"))
        return rows

    def treasury(self):
        a = self.pd["ledger"]["this"]
        return sum(a)

    def food_totals(self):
        food = self.pd["food"]
        stored = sum(r[1] for r in food)
        made = sum(r[2] for r in food)
        eaten = sum(r[3] for r in food)
        return stored, made, eaten

    # ------------------------------------------------------------ pages
    def overview(self):
        d = self.pd
        parts, (x0, y0, x1, y1) = self.book("overview")
        cw = (x1 - x0 - 3 * 20) / 4
        wf = d["workforce"]
        stored, made, eaten = self.food_totals()
        months = stored / eaten
        employment = wf["employed"] / (wf["employed"] + wf["unemployed"])
        cards = [
            ("population", "Population", num(d["population"]), f"{num(d['population_change'], True)} this month",
             lambda x: self.chart(x + 24, y0 + 146, cw - 48, 24, d["pop_trend"], labels=False, axis=False, sw=2.5)),
            ("coin", "Treasury", num(self.treasury()), f"{num(sum(d['ledger']['this'][:4]), True)} so far this year",
             lambda x: self.chart(x + 24, y0 + 146, cw - 48, 24, d["balance_trend"], labels=False, axis=False, sw=2.5)),
            ("food", "Food", f"{round(months)} months", f"{num(stored)} in {d['granaries'][0]} granaries",
             lambda x: self.bar(x + 24, y0 + 154, cw - 48, 10, months / 12)),
            ("population", "Employment", f"{round(employment * 100)}%", f"{wf['unemployed']} looking for work",
             lambda x: self.bar(x + 24, y0 + 154, cw - 48, 10, employment)),
        ]
        for k, (ic, lab, val, sub, extra) in enumerate(cards):
            cx = x0 + k * (cw + 20)
            parts.append(self.stat(cx, y0, cw, ic, lab, val, sub, extra(cx)))
        # Ratings and goals.
        ry = y0 + 184 + 20
        rw = 620
        parts.append(self.card(x0, ry, rw, 252, "Ratings", "the line marks the goal"))
        for k, (name, v, goal) in enumerate(d["ratings"]):
            cx = x0 + 88 + k * 148
            parts.append(self.ring(cx, ry + 122, 48, v, goal))
            parts.append(self.t(cx, ry + 202, name, 18, self.panel_text, "middle", 700))
            parts.append(self.t(cx, ry + 228, f"goal {goal}" if goal is not None else "no goal", 16, self.panel_sub, "middle", italic=True))
        gx, gw = x0 + rw + 20, x1 - x0 - rw - 20
        parts.append(self.card(gx, ry, gw, 252, "Goals", "Mission 5"))
        for k, (name, v, txt) in enumerate(d["goals"]):
            gy = ry + 88 + k * 56
            parts.append(self.check(gx + 34, gy - 7, True) if v >= 1 else f'<circle cx="{gx + 34}" cy="{gy - 7}" r="11" fill="none" stroke="{self.accent}" stroke-width="2"/>')
            parts.append(self.t(gx + 58, gy, name, 20, self.panel_text, "start", 700))
            parts.append(self.t(gx + gw - 24, gy, txt, 18, self.panel_sub, "end", 600))
            parts.append(self.bar(gx + 58, gy + 14, gw - 82, 8, v))
        # Warnings.
        wy = ry + 252 + 20
        parts.append(self.card(x0, wy, x1 - x0, y1 - wy, "Needs attention"))
        for k, (ic, text, link) in enumerate(d["warnings"]):
            ly = wy + 80 + k * 46
            parts.append(self.glyph(ic, x0 + 40, ly - 7, 0.75, self.accent))
            parts.append(self.t(x0 + 70, ly, text, 20, self.panel_text, "start", 700 if k == 0 else 600))
            parts.append(self.t(x1 - 24, ly, link + " ›", 18, self.accent, "end", 700))
            if k < len(d["warnings"]) - 1:
                parts.append(self.rule(x0 + 24, x1 - 24, ly + 19))
        self.page(f"city-overview-{d['key']}", parts)

    def population(self):
        d = self.pd
        parts, (x0, y0, x1, y1) = self.book("population")
        lw = 690
        parts.append(self.card(x0, y0, lw, 340, "Population"))
        parts.append(T(x0 + 24, y0 + 98, num(d["population"]), self.title_font, 46, self.panel_text, "start", 700))
        parts.append(self.change(x0 + 236, y0 + 96, d["population_change"], 20))
        parts.append(self.t(x0 + 246, y0 + 96, "this month", 18, self.panel_sub, italic=True))
        parts.append(self.chart(x0 + 84, y0 + 130, lw - 112, 158, d["pop_trend"]))
        # Workforce.
        wf = d["workforce"]
        wx, ww = x0 + lw + 20, x1 - x0 - lw - 20
        parts.append(self.card(wx, y0, ww, 340, "Workforce"))
        total = wf["employed"] + wf["unemployed"]
        parts.append(T(wx + 24, y0 + 98, f"{round(wf['employed'] / total * 100)}%", self.title_font, 46, self.panel_text, "start", 700))
        parts.append(self.t(wx + 150, y0 + 96, "of workers employed", 18, self.panel_sub, italic=True))
        bx, bw, by = wx + 24, ww - 48, y0 + 122
        ew = bw * wf["employed"] / total
        parts.append(f'<rect x="{f(bx)}" y="{by}" width="{f(bw)}" height="20" rx="4" fill="{self.panel_sub}" opacity="0.55"/>'
                     f'<rect x="{f(bx)}" y="{by}" width="{f(ew)}" height="20" rx="4" fill="{self.accent}"/>')
        rows = [("Employed", wf["employed"], self.accent), ("Looking for work", wf["unemployed"], self.panel_sub),
                ("Jobs not filled", wf["vacancies"], None), ("Too young or old to work", d["population"] - total, None)]
        for k, (lab, v, col) in enumerate(rows):
            ry = y0 + 186 + k * 38
            if col:
                parts.append(f'<rect x="{bx}" y="{ry - 14}" width="16" height="16" rx="3" fill="{col}" opacity="{1 if col == self.accent else 0.55}"/>')
            else:
                parts.append(f'<rect x="{bx + 1}" y="{ry - 13}" width="14" height="14" rx="3" fill="none" stroke="{self.panel_sub}" stroke-width="1.5"/>')
            parts.append(self.t(bx + 28, ry, lab, 19, self.panel_text, "start", 600))
            parts.append(self.t(wx + ww - 24, ry, num(v), 19, self.panel_text, "end", 700))
            if k < len(rows) - 1:
                parts.append(self.rule(bx, wx + ww - 24, ry + 13))
        # Housing.
        hy = y0 + 340 + 22
        parts.append(self.card(x0, hy, x1 - x0, y1 - hy, "Housing"))
        cols = [(x0 + 24, "Level", "start"), (x0 + 96, "House", "start"), (x0 + 400, "Houses", "end"), (x0 + 540, "Residents", "end"),
                (x0 + 580, "Share", "start"), (x0 + 800, "Next level needs", "start")]
        th = hy + 74
        for cx, lab, anchor in cols:
            parts.append(self.t(cx, th, lab, 17, self.panel_sub, anchor, 700))
        total_res = sum(d["house_residents"])
        for k in range(5):
            ry = th + 46 + k * 46
            parts.append(self.rule(x0 + 24, x1 - 24, ry - 32))
            parts.append(self.realm_level(x0 + 40, ry - 7, k + 1))
            parts.append(self.t(x0 + 96, ry, d["houses"][k], 20, self.panel_text, "start", 700))
            parts.append(self.t(x0 + 400, ry, num(d["house_counts"][k]), 20, self.panel_text, "end", 600))
            parts.append(self.t(x0 + 540, ry, num(d["house_residents"][k]), 20, self.panel_text, "end", 600))
            share = d["house_residents"][k] / total_res
            parts.append(self.bar(x0 + 580, ry - 12, 170, 10, share / 0.4))
            parts.append(self.t(x0 + 760, ry, f"{round(share * 100)}%", 16, self.panel_sub, "start", 600))
            parts.append(self.t(x0 + 800, ry, d["house_needs"][k], 19, self.panel_sub if k == 4 else self.panel_text, "start", 600, italic=k == 4))
        self.page(f"city-population-{d['key']}", parts)

    def realm_level(self, x, y, k):
        return (f'<circle cx="{x}" cy="{y}" r="15" fill="{self.field_fill}" stroke="{self.field_stroke}" stroke-width="1.5"/>'
                + T(x, y + 6, str(k), self.body_font, 17, self.accent, "middle", 800))

    def goods_rows(self, x, y, w, rows):
        out = []
        for k, (name, stored, cap, ch) in enumerate(rows):
            ry = y + k * 56
            out.append(self.good_bullet(x + 30, ry - 7))
            out.append(self.t(x + 48, ry, name, 20, self.panel_text, "start", 700))
            bx, bw = x + 190, w - 420
            out.append(self.bar(bx, ry - 12, bw, 10, stored / cap))
            out.append(self.t(bx + bw + 100, ry, f"{num(stored)} / {num(cap)}", 18, self.panel_text, "end", 600))
            out.append(self.change(x + w - 24, ry, ch, 18))
            if k < len(rows) - 1:
                out.append(self.rule(x + 24, x + w - 24, ry + 24))
        return "".join(out)

    def goods(self):
        d = self.pd
        parts, (x0, y0, x1, y1) = self.book("goods")
        cw = (x1 - x0 - 20) / 2
        share = {"Rice": 0.5, "Fish": 0.3, "Meat": 0.2}
        foods = [(n, s, round(d["granaries"][1] * share[n]), made - eaten) for n, s, made, eaten in d["food"]]
        groups = [("Foods", foods)] + PC.GOODS
        # Left: foods and consumer goods; right: raw materials, then the request.
        ly = y0
        for name, rows in groups[:2]:
            h = 82 + len(rows) * 56
            parts.append(self.card(x0, ly, cw, h, name, "stored / room · a month"))
            parts.append(self.goods_rows(x0, ly + 86, cw, rows))
            ly += h + 20
        sy = ly
        parts.append(self.card(x0, sy, cw, y1 - sy, "Storage"))
        stored_goods = sum(r[1] for _, rows in PC.GOODS for r in rows)
        stored_food = sum(r[1] for r in d["food"])
        for k, (lab, (n, cap), used) in enumerate([("Storage yards", d["yards"], stored_goods), ("Granaries", d["granaries"], stored_food)]):
            ry = sy + 82 + k * 64
            parts.append(self.t(x0 + 24, ry, lab, 20, self.panel_text, "start", 700))
            parts.append(self.t(x0 + 24, ry + 24, f"{n} buildings", 16, self.panel_sub, "start", 600, italic=True))
            parts.append(self.bar(x0 + 190, ry - 12, cw - 370, 10, used / cap))
            parts.append(self.t(x0 + cw - 24, ry, f"{num(used)} / {num(cap)}", 18, self.panel_text, "end", 600))
        rx = x0 + cw + 20
        rows = groups[2][1]
        h = 82 + len(rows) * 56
        parts.append(self.card(rx, y0, cw, h, groups[2][0], "stored / room · a month"))
        parts.append(self.goods_rows(rx, y0 + 86, cw, rows))
        qy = y0 + h + 20
        name, have, need, when = d["request"]
        parts.append(self.card(rx, qy, cw, y1 - qy, "Requests"))
        parts.append(self.glyph("request", rx + 40, qy + 82, 0.8, self.accent))
        parts.append(self.t(rx + 70, qy + 90, name, 21, self.panel_text, "start", 700))
        parts.append(self.t(rx + cw - 24, qy + 90, when, 18, self.panel_sub, "end", 600, italic=True))
        parts.append(self.bar(rx + 70, qy + 110, cw - 210, 12, have / need))
        parts.append(self.t(rx + cw - 24, qy + 122, f"{num(have)} / {num(need)}", 18, self.panel_text, "end", 700))
        parts.append(self.button(rx + cw - 244, y1 - 80, 220, 52, "Send now", True))
        self.page(f"city-goods-{d['key']}", parts)

    def finances(self):
        d = self.pd
        parts, (x0, y0, x1, y1) = self.book("finances")
        lw = 640
        parts.append(self.card(x0, y0, lw, y1 - y0, "Accounts"))
        c1, c2 = x0 + lw - 214, x0 + lw - 28
        th = y0 + 84
        parts.append(self.t(c1, th, "Last year", 18, self.panel_sub, "end", 700))
        parts.append(self.t(c2, th, "This year", 18, self.panel_sub, "end", 700))
        ry = th + 52
        for name, (last, this), kind in self.ledger_rows():
            if kind in ("total", "net"):
                parts.append(f'<line x1="{x0 + 24}" y1="{ry - 34}" x2="{x0 + lw - 24}" y2="{ry - 34}" stroke="{self.panel_text}" stroke-width="1.5"/>')
            if kind == "note" and name.endswith("start of the year"):
                ry += 26
            bold = kind in ("total", "net")
            size = 22 if kind == "net" else 20
            indent = 24 if kind in ("total", "net", "note") else 48
            parts.append(self.t(x0 + indent, ry, name, size, self.panel_sub if kind == "note" else self.panel_text, "start", 700 if bold else 600,
                                italic=kind == "note"))
            for cx, v in ((c1, last), (c2, this)):
                col = (self.good_col if v >= 0 else self.accent) if kind == "net" else self.panel_text
                parts.append(self.t(cx, ry, num(v, kind == "net"), size, col, "end", 700 if bold else 600))
            if kind == "row":
                parts.append(self.rule(x0 + 48, x0 + lw - 24, ry + 16))
            ry += 50
        parts.append(self.t(x0 + 24, y1 - 30, f"This year covers months 1 to {d['month']}.", 17, self.panel_sub, italic=True))
        # Treasury.
        rx, rw = x0 + lw + 20, x1 - x0 - lw - 20
        parts.append(self.card(rx, y0, rw, 250, "Treasury"))
        parts.append(self.coin(rx + 40, y0 + 86))
        parts.append(T(rx + 68, y0 + 100, num(self.treasury()), self.title_font, 44, self.panel_text, "start", 700))
        parts.append(self.check(rx + rw - 140, y0 + 84, True))
        parts.append(self.t(rx + rw - 120, y0 + 91, "Not in debt", 19, self.panel_text, "start", 700))
        parts.append(self.t(rx + rw - 24, y0 + 117, f"0 of {PC.DEBT_LIMIT_MONTHS} months in debt", 16, self.panel_sub, "end", italic=True))
        parts.append(self.chart(rx + 80, y0 + 140, rw - 104, 64, d["balance_trend"], labels=True, axis=True, sw=2.5))
        # Taxes and wages.
        ty = y0 + 270
        parts.append(self.card(rx, ty, rw, y1 - ty, "Taxes and wages"))
        lvl = d["tax_level"]
        parts.append(self.t(rx + 24, ty + 84, "Tax level", 21, self.panel_text, "start", 700))
        parts.append(self.t(rx + rw - 24, ty + 84, f"houses pay {PC.TAX_PERCENT[lvl]}% of their base tax", 17, self.panel_sub, "end", italic=True))
        parts.append(self.segments(rx + 24, ty + 104, rw - 48, PC.TAX_LEVELS, lvl))
        parts.append(self.t(rx + 24, ty + 192, "Not yet collected", 19, self.panel_sub, "start", 600))
        parts.append(self.t(rx + rw - 24, ty + 192, f"{num(d['owed'])} gold", 19, self.panel_text, "end", 700))
        parts.append(self.rule(rx + 24, rx + rw - 24, ty + 216))
        parts.append(self.t(rx + 24, ty + 266, "Wage", 21, self.panel_text, "start", 700))
        parts.append(self.t(rx + 92, ty + 266, "gold per worker a year", 17, self.panel_sub, "start", italic=True))
        parts.append(self.stepper(rx + rw - 174, ty + 259, str(d["wage"])))
        emp = d["workforce"]["employed"]
        rows = [("Workers paid", num(emp)), ("Wages a month", num(round(emp * d["wage"] / 12))), ("Wages a year", num(emp * d["wage"]))]
        for k, (lab, v) in enumerate(rows):
            yy = ty + 318 + k * 38
            parts.append(self.t(rx + 24, yy, lab, 19, self.panel_sub, "start", 600))
            parts.append(self.t(rx + rw - 24, yy, v, 19, self.panel_text, "end", 700))
        self.page(f"city-finances-{d['key']}", parts)

    def ratings(self):
        d = self.pd
        parts, (x0, y0, x1, y1) = self.book("ratings")
        cw, ch = (x1 - x0 - 20) / 2, (y1 - y0 - 20) / 2
        for k, ((name, v, goal), drivers) in enumerate(zip(d["ratings"], d["rating_drivers"])):
            cx, cy = x0 + (k % 2) * (cw + 20), y0 + (k // 2) * (ch + 20)
            parts.append(self.card(cx, cy, cw, ch, name))
            parts.append(self.ring(cx + 112, cy + 168, 66, v, goal, sw=12, label_size=40))
            gtxt = f"Goal {goal}: met" if goal is not None and v >= goal else f"Goal {goal}: {goal - v} to go" if goal is not None else "No goal in this mission"
            parts.append(self.t(cx + 112, cy + 274, gtxt, 17, self.panel_sub, "middle", 600, italic=True))
            dx0, dx1 = cx + 232, cx + cw - 24
            for j, (lab, dv) in enumerate(drivers):
                ry = cy + 92 + j * 44
                parts.append(self.t(dx0, ry, lab, 19, self.panel_text, "start", 600))
                parts.append(self.t(dx1, ry, num(dv, True), 19, self.good_col if dv > 0 else self.accent, "end", 700))
                parts.append(self.rule(dx0, dx1, ry + 15))
            ty = cy + 92 + len(drivers) * 44 + 6
            parts.append(self.t(dx0, ty, "Rating", 19, self.panel_text, "start", 800))
            parts.append(self.t(dx1, ty, str(v), 19, self.panel_text, "end", 800))
        self.page(f"city-ratings-{d['key']}", parts)

    def labour(self):
        d = self.pd
        parts, (x0, y0, x1, y1) = self.book("labour")
        wf = d["workforce"]
        cw = (x1 - x0 - 3 * 20) / 4
        stats = [("Workers", num(wf["employed"] + wf["unemployed"])), ("Employed", num(wf["employed"])),
                 ("Looking for work", num(wf["unemployed"])), ("Jobs not filled", num(wf["vacancies"]))]
        for k, (lab, v) in enumerate(stats):
            sx = x0 + k * (cw + 20)
            parts.append(self.card(sx, y0, cw, 118))
            parts.append(self.t(sx + 24, y0 + 42, lab, 19, self.panel_sub, "start", 700))
            parts.append(T(sx + 24, y0 + 92, v, self.title_font, 36, self.panel_text, "start", 700))
        ty = y0 + 138
        parts.append(self.card(x0, ty, x1 - x0, y1 - ty, "Sectors"))
        cols = [(x0 + 24, "Priority", "start"), (x0 + 200, "Sector", "start"), (x0 + 560, "Needed", "end"), (x0 + 690, "Employed", "end"),
                (x0 + 730, "", "start"), (x1 - 24, "", "end")]
        hy = ty + 76
        for cx, lab, anchor in cols:
            parts.append(self.t(cx, hy, lab, 17, self.panel_sub, anchor, 700))
        rows = sorted(zip(PC.LABOUR_SECTORS, d["labour"]), key=lambda r: r[1][2])
        for k, (sector, (need, emp, prio)) in enumerate(rows):
            ry = hy + 60 + k * 64
            parts.append(self.rule(x0 + 24, x1 - 24, ry - 38))
            sel = k == 1
            if sel:
                parts.append(f'<rect x="{x0 + 14}" y="{ry - 36}" width="{x1 - x0 - 28}" height="62" rx="6" fill="{self.field_fill}" stroke="{self.accent}" stroke-width="2"/>')
            parts.append(self.stepper(x0 + 24, ry - 6, str(prio), 130))
            parts.append(self.t(x0 + 200, ry, sector, 21, self.panel_text, "start", 700))
            parts.append(self.t(x0 + 560, ry, num(need), 20, self.panel_text, "end", 600))
            parts.append(self.t(x0 + 690, ry, num(emp), 20, self.panel_text, "end", 600))
            parts.append(self.bar(x0 + 730, ry - 13, x1 - x0 - 930, 12, emp / need))
            parts.append(self.t(x1 - 24, ry, "Full" if emp >= need else f"{need - emp} short", 19, self.panel_sub if emp >= need else self.accent, "end", 700))
        parts.append(self.t(x0 + 24, y1 - 28, "Priority 1 is filled first. Sectors short of workers run slower; unfilled jobs show on the map.", 17, self.panel_sub, italic=True))
        self.page(f"city-labour-{d['key']}", parts)

    def food(self):
        d = self.pd
        parts, (x0, y0, x1, y1) = self.book("food")
        stored, made, eaten = self.food_totals()
        cw = (x1 - x0 - 3 * 20) / 4
        stats = [("food", "In granaries", num(stored)), ("farm", "Grown a month", num(made)), ("population", "Eaten a month", num(eaten)),
                 ("food", "Lasts without harvests", f"{round(stored / eaten)} months")]
        for k, (ic, lab, v) in enumerate(stats):
            sx = x0 + k * (cw + 20)
            parts.append(self.card(sx, y0, cw, 118))
            parts.append(self.glyph(ic, sx + 34, y0 + 36, 0.75, self.accent))
            parts.append(self.t(sx + 58, y0 + 43, lab, 19, self.panel_sub, "start", 700))
            parts.append(T(sx + 24, y0 + 94, v, self.title_font, 36, self.panel_text, "start", 700))
        ty = y0 + 138
        th = 82 + len(d["food"]) * 60 + 10
        parts.append(self.card(x0, ty, x1 - x0, th, "By food"))
        cols = [(x0 + 48, "Food", "start"), (x0 + 330, "Stored", "end"), (x0 + 480, "Grown", "end"), (x0 + 620, "Eaten", "end"),
                (x0 + 760, "Change", "end"), (x0 + 800, "Lasts without harvests", "start")]
        hy = ty + 74
        for cx, lab, anchor in cols:
            parts.append(self.t(cx, hy, lab, 17, self.panel_sub, anchor, 700))
        for k, (name, s, m, e) in enumerate(d["food"]):
            ry = hy + 52 + k * 60
            parts.append(self.rule(x0 + 24, x1 - 24, ry - 34))
            parts.append(self.good_bullet(x0 + 32, ry - 7))
            parts.append(self.t(x0 + 48, ry, name, 21, self.panel_text, "start", 700))
            parts.append(self.t(x0 + 330, ry, num(s), 20, self.panel_text, "end", 600))
            parts.append(self.t(x0 + 480, ry, num(m), 20, self.panel_text, "end", 600))
            parts.append(self.t(x0 + 620, ry, num(e), 20, self.panel_text, "end", 600))
            parts.append(self.change(x0 + 760, ry, m - e, 19))
            lasts = s / e
            short = lasts < 3
            parts.append(self.bar(x0 + 800, ry - 13, x1 - x0 - 1000, 12, lasts / 12))
            parts.append(self.t(x1 - 24, ry, f"{round(lasts)} months", 19, self.accent if short else self.panel_text, "end", 700))
        cy = ty + th + 20
        parts.append(self.card(x0, cy, x1 - x0, y1 - cy, "Food in granaries", "the last twelve months"))
        parts.append(self.chart(x0 + 84, cy + 66, x1 - x0 - 120, y1 - cy - 130, d["food_trend"]))
        self.page(f"city-food-{d['key']}", parts)


    # ------------------------------------------------------------ heritage
    def heritage_book(self):
        """The book with the Heritage tab open, in the monument mission's city."""
        hd = PC.HERITAGE[self.pd["key"]]
        saved = self.pd
        self.pd = dict(saved, city=hd["city"], date=hd["date"], month=hd["month"], season=hd["season"])
        parts, box = self.book("heritage")
        self.pd = saved
        return hd, parts, box

    def stage_mark(self, x, y, v):
        if v >= 1:
            return self.check(x, y, True)
        if v > 0:
            return (f'<circle cx="{x}" cy="{y}" r="12" fill="{self.field_fill}" stroke="{self.accent}" stroke-width="3"/>'
                    f'<path d="M{x},{y - 12} A12,12 0 {1 if v > 0.5 else 0} 1 {f(x + 12 * math.sin(v * 2 * math.pi))},{f(y - 12 * math.cos(v * 2 * math.pi))} L{x},{y}Z" fill="{self.accent}"/>')
        return f'<circle cx="{x}" cy="{y}" r="11" fill="none" stroke="{self.panel_sub}" stroke-width="1.5" opacity="0.8"/>'

    def monument_card(self, hd, x, y, w, h):
        out = [self.card(x, y, w, h, "Monument", hd["mission"])]
        out.append(T(x + 24, y + 86, hd["monument"], self.title_font, 32, self.panel_text, "start", 700))
        out.append(self.t(x + 24, y + 114, hd["site"], 17, self.panel_sub, italic=True))
        stages = hd["stages"]
        done = sum(1 for _, v, _ in stages if v >= 1)
        out.append(self.t(x + w - 24, y + 86, f"Stage {done + 1} of {len(stages)}", 19, self.panel_text, "end", 700))
        out.append(self.t(x + w - 24, y + 114, hd["eta"], 17, self.panel_sub, "end", italic=True))
        sy = y + 152
        for k, (name, v, needs) in enumerate(stages):
            current = 0 < v < 1
            out.append(self.rule(x + 24, x + w - 24, sy - 22))
            out.append(self.stage_mark(x + 40, sy, v))
            out.append(self.t(x + 64, sy + 7, f"{k + 1}. {name}", 20, self.panel_text if v > 0 else self.panel_sub, "start", 700 if current else 600))
            out.append(self.t(x + w - 24, sy + 7, "built" if v >= 1 else f"{round(v * 100)}%" if current else "to come", 18,
                              self.good_col if v >= 1 else self.accent if current else self.panel_sub, "end", 700, italic=v == 0))
            sy += 40
            if current:
                out.append(f'<rect x="{x + 52}" y="{sy - 18}" width="{w - 76}" height="{len(needs) * 34 + 18}" rx="6" fill="{self.field_fill}" stroke="{self.accent}" stroke-width="1.5"/>')
                for j, (good, have, need) in enumerate(needs):
                    ny = sy + 10 + j * 34
                    worker = good.endswith("s") and good not in ("Bricks", "Roof Tiles")
                    out.append(self.glyph("population", x + 78, ny - 6, 0.55, self.accent) if worker else self.good_bullet(x + 78, ny - 6))
                    out.append(self.t(x + 96, ny, good, 18, self.panel_text, "start", 600))
                    out.append(self.bar(x + 250, ny - 11, w - 420, 9, have / need))
                    out.append(self.t(x + w - 40, ny, f"{have} / {need}", 17, self.panel_text, "end", 600))
                sy += len(needs) * 34 + 28
        by = y + h - 124
        out.append(self.rule(x + 24, x + w - 24, by - 12))
        out.append(self.section_title(x + 24, by + 18, "When it is finished"))
        for k, b in enumerate(hd["bonuses"]):
            bx = x + 24 + (k % 2) * (w - 48) / 2
            out.append(self.glyph("star", bx + 10, by + 48 + (k // 2) * 30 - 6, 0.45, self.accent, 3))
            out.append(self.t(bx + 28, by + 48 + (k // 2) * 30, b, 18, self.panel_text, "start", 600))
        return "".join(out)

    def patron_row(self, x, y, w, patron, compact=False):
        name, domain, ic, favour, mood, effect = patron
        low = favour < 40
        out = [f'<circle cx="{x + 30}" cy="{y + 2}" r="22" fill="{self.field_fill}" stroke="{self.accent if not low else self.panel_sub}" stroke-width="2"/>',
               self.glyph(ic, x + 30, y + 2, 0.8, self.accent),
               T(x + 66, y - 2, name, self.title_font, 20, self.panel_text, "start", 700),
               self.t(x + 66, y + 22, domain, 16, self.panel_sub, italic=True)]
        out.append(self.t(x + w - 24, y - 2, f"{mood} · {favour}", 17, self.accent if low else self.good_col, "end", 700))
        bw = 120 if compact else 200
        out.append(self.bar(x + w - 24 - bw, y + 12, bw, 9, favour / 100))
        if not compact:
            out.append(self.t(x + w - 44 - bw, y + 22, ("Below 30: " if low else "Above 70: ") + effect.lower(), 15, self.panel_sub, "end", 600, italic=True))
        return "".join(out)

    def festival_planner(self, hd, x, y, w, h, title="Next festival", detail=False, button="Hold festival"):
        fe = hd["festival"]
        out = [self.card(x, y, w, h, title, f"month {fe['month']}")]
        out.append(T(x + 24, y + 84, fe["name"], self.title_font, 28, self.panel_text, "start", 700))
        cy = y + 104
        if detail:
            out.append(self.t(x + 24, y + 110, fe["venue"], 17, self.panel_sub, italic=True))
            lines = self.wrap(fe["text"], w - 48, 18, 0.48)
            for k, line in enumerate(lines):
                out.append(self.t(x + 24, y + 144 + k * 26, line, 18, self.panel_text, "start", 600, italic=True))
            cy = y + 144 + len(lines) * 26
        bw = 230
        out.append(self.segments(x + 24, cy, w - 72 - bw, fe["scales"], fe["scale"]))
        out.append(self.button(x + w - 24 - bw, cy - 3, bw, 52, button, True))
        cy += 86
        out.append(self.t(x + 24, cy, "Costs", 17, self.panel_sub, "start", 700))
        out.append(self.t(x + w / 2 + 10, cy, "Effects", 17, self.panel_sub, "start", 700))
        for k, (good, n, have) in enumerate(fe["cost"]):
            ry = cy + 30 + k * 28
            short = have < n
            out.append(self.t(x + 24, ry, good, 18, self.panel_text, "start", 600))
            out.append(self.t(x + w / 2 - 20, ry, f"{n}" + (f"  ·  {have} held" if short else ""), 18, self.accent if short else self.panel_text, "end", 700))
        for k, (what, v) in enumerate(fe["effects"]):
            ry = cy + 30 + k * 28
            out.append(self.t(x + w / 2 + 10, ry, what, 18, self.panel_text, "start", 600))
            out.append(self.t(x + w - 24, ry, v, 18, self.good_col, "end", 700, italic=not v.startswith("+")))
        return "".join(out), cy + 30 + max(len(fe["cost"]), len(fe["effects"])) * 28

    def heritage(self):
        """Sub-tabs for Monument, Patrons and Festivals; the Festivals tab is open. monument_card and
        patron_row draw the other two tabs' contents, for when they get mockups of their own."""
        hd, parts, (x0, y0, x1, y1) = self.heritage_book()
        tw = 200
        for k, lab in enumerate(["Monument", "Patrons", "Festivals"]):
            parts.append(self.tab(x0 + k * (tw + 12), y0 - 6, tw, lab, lab == "Festivals"))
        y0 += 64
        # The year's festivals on a month strip.
        sh = 172
        parts.append(self.card(x0, y0, x1 - x0, sh, "The festival year", "dots mark festivals"))
        sx0, sx1 = x0 + 40, x1 - 40
        step = (sx1 - sx0) / 11
        ly = y0 + 84
        parts.append(f'<line x1="{sx0}" y1="{ly}" x2="{sx1}" y2="{ly}" stroke="{self.panel_rule}" stroke-width="3"/>')
        month = hd["month"]
        for m in range(12):
            mx = sx0 + m * step
            parts.append(f'<line x1="{f(mx)}" y1="{ly - 6}" x2="{f(mx)}" y2="{ly + 6}" stroke="{self.panel_sub}" stroke-width="1.5"/>')
            parts.append(self.t(mx, ly + 30, str(m + 1), 15, self.panel_text if m + 1 == month else self.panel_sub, "middle", 800 if m + 1 == month else 600))
        nx = sx0 + (month - 1) * step
        parts.append(f'<path d="M{f(nx)},{ly - 14} l-7,-10 h14z" fill="{self.panel_text}"/>')
        parts.append(self.t(nx, ly - 30, "now", 14, self.panel_text, "middle", 700, italic=True))
        for n, (name, m, patron, state) in enumerate(hd["festivals"]):
            mx = sx0 + (m - 1) * step
            on = state == "next"
            parts.append(f'<circle cx="{f(mx)}" cy="{ly}" r="{11 if on else 8}" fill="{self.accent if state != "past" else self.panel_sub}" stroke="{self.knob_fill}" stroke-width="2"/>')
            # Labels near the strip's ends align to the dot's outer side so they stay in the card.
            lx, anchor = (mx - 12, "start") if m <= 1 else (mx + 12, "end") if m >= 12 else (mx, "middle")
            parts.append(self.t(lx, ly + 54 + (n % 2) * 22, name, 15, self.accent if on else self.panel_text, anchor, 700 if on else 600))
        # Left: the festivals, with their patron and when last held.
        cy = y0 + sh + 20
        lw = 470
        parts.append(self.card(x0, cy, lw, y1 - cy, "Festivals"))
        for k, (name, m, patron, state) in enumerate(hd["festivals"]):
            ry = cy + 90 + k * 76
            on = state == "next"
            if on:
                parts.append(f'<rect x="{x0 + 12}" y="{ry - 34}" width="{lw - 24}" height="66" rx="6" fill="{self.field_fill}" stroke="{self.accent}" stroke-width="2"/>')
            parts.append(T(x0 + 24, ry, name, self.title_font, 21, self.panel_text, "start", 700))
            parts.append(self.t(x0 + lw - 24, ry, f"month {m}", 17, self.panel_sub, "end", 600))
            parts.append(self.t(x0 + 24, ry + 22, f"For {patron[0].lower() + patron[1:] if patron.startswith('every') else patron}", 16, self.panel_sub, "start", 600, italic=True))
            parts.append(self.t(x0 + lw - 24, ry + 22, {"past": "held this year", "next": "next", "later": "to come"}[state], 16,
                                self.accent if on else self.panel_sub, "end", 700, italic=True))
        # Right: the planner for the selected one.
        rx, rw = x0 + lw + 20, x1 - x0 - lw - 20
        planner, end = self.festival_planner(hd, rx, cy, rw, y1 - cy, "Plan the festival", detail=True,
                                             button=f"Hold in month {hd['festival']['month']}")
        parts.append(planner)
        parts.append(self.t(x0 + 24, y1 - 48, "Last held:", 15, self.panel_sub, "start", 700))
        parts.append(self.t(x0 + 24, y1 - 24, hd["festival"]["last"], 15, self.panel_sub, italic=True))
        self.page(f"city-heritage-{self.pd['key']}", parts)


class DaiLongPages(Pages, DaiLongMenus):
    good_col = "#4f6f2a"

    def __init__(self, pd):
        DaiLongMenus.__init__(self, MC.DAILONG)
        self.pd = pd


class KambaraPages(Pages, KambaraMenus):
    good_col = "#3f6a2a"

    def __init__(self, pd):
        KambaraMenus.__init__(self, MC.KAMBARA)
        self.pd = pd


class TampadesaPages(Pages, TampadesaMenus):
    good_col = "#4f6420"

    def __init__(self, pd):
        TampadesaMenus.__init__(self, MC.TAMPADESA)
        self.pd = pd


REALMS = [(DaiLongPages, PC.DAILONG), (KambaraPages, PC.KAMBARA), (TampadesaPages, PC.TAMPADESA)]
CITY_PAGES = [k for k, _, _ in PC.PAGES]

if __name__ == "__main__":
    import sys
    which = sys.argv[1:] or CITY_PAGES
    for cls, data in REALMS:
        for screen in which:
            getattr(cls(data), screen)()
