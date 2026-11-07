"""Minimap concepts: the same sample city drawn four ways, with the minimap's
real projection (an isometric diamond clipped to the playable rectangle, as
in crates/game/src/tile/minimap/), and the widget's toolbar. The icon paths
come from crates/tools/ui-icons/icons.json, which also makes the game's sprites.

Writes build/minimap-concepts.html and build/minimap-lenses.html.
Needs numpy and Pillow.
"""
import base64
import io
import json
import math
import pathlib

import numpy as np
from PIL import Image, ImageFilter

from world_map import T, f, FONTS_CSS, BUILD

# The icons' SVG paths, shared with the game (crates/tools/ui-icons/make_icons.py).
ICONS = pathlib.Path(__file__).resolve().parents[4] / "crates" / "tools" / "ui-icons" / "icons.json"
GLYPHS = json.loads(ICONS.read_text())["glyphs"]

N = 80                       # map size in tiles
RNG = np.random.default_rng(7)

# Sectors, as TileSector in the game.
HOUSING, FOOD, INDUSTRY, SERVICES, INFRA, CULTURE, TRADE, BEAUTY = range(8)
SECTOR_NAMES = ["Housing", "Food and farming", "Industry", "Services", "Infrastructure", "Culture and religion", "Trade", "Beautification"]
# The game's current colours (MinimapTileColor).
GAME_COLORS = ["#e1c378", "#9baa28", "#aa2323", "#508cff", "#646464", "#aa3cbe", "#e6b928", "#3cc86e"]


# ====================================================================== sample city
def smooth_noise(cells, seed, size=N):
    r = np.random.default_rng(seed).random((cells, cells))
    im = Image.fromarray((r * 255).astype(np.uint8)).resize((size, size), Image.BICUBIC)
    return np.asarray(im, dtype=float) / 255


def build_city():
    ii, jj = np.meshgrid(np.arange(N), np.arange(N), indexing="ij")
    height = 0.6 * smooth_noise(6, 1) + 0.4 * smooth_noise(12, 2)
    height += np.clip((ii - 52) / 30, 0, 1) * np.clip((24 - jj) / 24, 0, 1) * 0.9   # hills to the north-east
    height = (height - height.min()) / (height.max() - height.min())
    # Sea along the west, a river from the hills down to it, and a pond.
    coast = 18 + 5 * np.sin(ii / 9) + 3 * smooth_noise(8, 3) * 3
    water = (jj - ii) > coast
    pts = [(62, 8), (52, 18), (44, 22), (36, 30), (34, 40), (28, 48), (20, 56), (10, 68)]
    river = np.zeros((N, N), bool)
    for (a, b), (c, d) in zip(pts, pts[1:]):
        for t in np.linspace(0, 1, 60):
            x, y = a + (c - a) * t, b + (d - b) * t
            river |= (ii - x) ** 2 + (jj - y) ** 2 < 2.1 ** 2
    water |= river
    water |= (ii - 60) ** 2 / 9 + (jj - 50) ** 2 / 5 < 1
    rocks = (height > 0.8) & ~water
    veg = (smooth_noise(10, 4) > 0.58) & ~water & ~rocks

    roads = np.zeros((N, N), bool)
    for r in (38, 46, 54, 62):
        roads[r, 30:60] = True
    for c in (30, 38, 46, 54):
        roads[38:63, c] = True
    roads[46, 12:31] = True                      # road west, over the river
    roads[30:39, 54] = True                      # road north, to the hills
    bridge = roads & water

    buildings = []                               # (i, j, w, h, sector, name)
    taken = roads | water | rocks

    def put(i, j, w, h, sector, name=""):
        if i + w > N or j + h > N or taken[i:i + w, j:j + h].any():
            return False
        taken[i:i + w, j:j + h] = True
        buildings.append((i, j, w, h, sector, name))
        return True

    # Blocks between the roads.
    blocks = [(39, 31), (39, 39), (39, 47), (47, 31), (47, 39), (47, 47), (55, 31), (55, 39), (55, 47)]
    kinds = ["houses", "houses", "market", "temple", "palace", "houses", "industry", "houses", "granary"]
    for (i0, j0), kind in zip(blocks, kinds):
        if kind == "houses":
            for di in (0, 3, 5):
                for dj in (0, 2, 4):
                    put(i0 + di, j0 + dj + (1 if di == 5 else 0), 2, 2, HOUSING)
            put(i0 + 3, j0 + 6, 1, 1, SERVICES, "well")
            put(i0 + 6, j0 + 6, 1, 1, BEAUTY)
        elif kind == "market":
            put(i0 + 1, j0 + 1, 3, 3, TRADE, "market")
            put(i0 + 5, j0 + 1, 2, 2, SERVICES, "clinic")
            put(i0 + 1, j0 + 5, 2, 2, HOUSING)
            put(i0 + 4, j0 + 5, 2, 2, HOUSING)
        elif kind == "temple":
            put(i0 + 2, j0 + 2, 3, 3, CULTURE, "temple")
            for d in ((0, 0), (0, 6), (6, 0), (6, 6)):
                put(i0 + d[0], j0 + d[1], 1, 1, BEAUTY)
            put(i0 + 5, j0 + 5, 2, 2, HOUSING)
        elif kind == "palace":
            put(i0 + 1, j0 + 1, 5, 5, CULTURE, "palace")
        elif kind == "industry":
            put(i0, j0, 3, 3, INFRA, "storage")
            put(i0 + 4, j0, 2, 2, INDUSTRY, "kiln")
            put(i0 + 4, j0 + 3, 2, 2, INDUSTRY, "kiln")
            put(i0, j0 + 4, 3, 3, INDUSTRY, "brickworks")
        elif kind == "granary":
            put(i0 + 1, j0 + 1, 3, 3, INFRA, "granary")
            put(i0 + 4, j0 + 4, 3, 3, TRADE, "trade post")
    # Paddies along the river, and a few hamlets.
    fields = []
    for (i0, j0) in [(24, 40), (24, 44), (20, 46), (20, 50), (40, 22), (44, 24), (30, 22), (32, 18), (50, 60), (44, 62)]:
        if put(i0, j0, 3, 3, FOOD, "paddy"):
            fields.append((i0, j0))
    put(28, 34, 2, 2, HOUSING); put(25, 36, 2, 2, HOUSING); put(16, 44, 2, 2, HOUSING)
    put(64, 34, 3, 3, INDUSTRY, "quarry"); put(56, 26, 2, 2, INDUSTRY, "lumber")
    veg &= ~taken
    return dict(height=height, water=water, rocks=rocks, veg=veg, roads=roads, bridge=bridge, buildings=buildings)


CITY = build_city()


# ====================================================================== raster helpers
def png_uri(arr):
    im = Image.fromarray(np.clip(arr, 0, 255).astype(np.uint8))
    buf = io.BytesIO()
    im.save(buf, "PNG")
    return "data:image/png;base64," + base64.b64encode(buf.getvalue()).decode()


def hexrgb(h):
    h = h.lstrip("#")
    return np.array([int(h[k:k + 2], 16) for k in (0, 2, 4)], float)


def up(mask, px, blur):
    im = Image.fromarray((mask.astype(float) * 255).astype(np.uint8)).resize((N * px, N * px), Image.BICUBIC)
    if blur:
        im = im.filter(ImageFilter.GaussianBlur(blur))
    return np.asarray(im, float) / 255


def mix(base, color, w):
    return base * (1 - w[..., None]) + hexrgb(color) * w[..., None]


# ====================================================================== the widget
class Minimap:
    """Draws one concept into a rect: the diamond map in tile space under the
    isometric matrix, clipped to the playable rectangle."""

    def __init__(self, defs, prefix=""):
        self.defs = defs
        self.prefix = prefix
        self.n = 0

    def nid(self, p):
        self.n += 1
        return f"{self.prefix}{p}{self.n}"

    def frame(self, x, y, w):
        """Map area at (x, y) with width w (height w / 2). Returns (open, close, to_screen)."""
        s = w / N
        ox, oy = x + w / 2, y - w / 4
        cid = self.nid("mm")
        self.defs.append(f'<clipPath id="{cid}"><rect x="{f(x)}" y="{f(y)}" width="{f(w)}" height="{f(w / 2)}" rx="6"/></clipPath>')

        def to_screen(i, j):
            return ox + (i - j) * s, oy + (i + j) * s / 2
        start = f'<g clip-path="url(#{cid})"><g transform="matrix({f(s)},{f(s / 2)},{f(-s)},{f(s / 2)},{f(ox)},{f(oy)})">'
        return start, "</g>", "</g>", to_screen

    def image(self, uri, pixelated=False):
        st = ' style="image-rendering:pixelated"' if pixelated else ""
        return f'<image x="0" y="0" width="{N}" height="{N}" href="{uri}" preserveAspectRatio="none"{st}/>'

    def road_paths(self, roads):
        segs = []
        for i in range(N):
            for j in range(N):
                if not roads[i, j]:
                    continue
                if i + 1 < N and roads[i + 1, j]:
                    segs.append(f"M{i + .5},{j + .5}H{i + 1.5}")
                if j + 1 < N and roads[i, j + 1]:
                    segs.append(f"M{i + .5},{j + .5}V{j + 1.5}")
        return "".join(segs)

    def camera(self, to_screen, style, k=1.0):
        cx, cy = to_screen(48, 40)
        w, h = 150 * k, 84 * k
        t, sw = 14 * k, max(2.5, 4 * k)
        if style == "ink":
            return (f'<rect x="{f(cx - w / 2)}" y="{f(cy - h / 2)}" width="{w}" height="{h}" fill="none" stroke="#a8261c" stroke-width="3" stroke-dasharray="14 5"/>')
        return (f'<rect x="{f(cx - w / 2)}" y="{f(cy - h / 2)}" width="{w}" height="{h}" rx="5" fill="#ffffff" fill-opacity="0.12" stroke="#fffaf0" stroke-width="2.5"/>'
                + "".join(f'<path d="M{f(px)},{f(py + dy * t)} V{f(py)} H{f(px + dx * t)}" fill="none" stroke="#fffaf0" stroke-width="{f(sw)}" stroke-linecap="round"/>'
                          for px, py, dx, dy in [(cx - w / 2, cy - h / 2, 1, 1), (cx + w / 2, cy - h / 2, -1, 1), (cx - w / 2, cy + h / 2, 1, -1), (cx + w / 2, cy + h / 2, -1, -1)]))

    def ping(self, to_screen, i, j, color="#e8432f", k=1.0):
        x, y = to_screen(i, j)
        return (f'<circle cx="{f(x)}" cy="{f(y)}" r="{f(22 * k)}" fill="none" stroke="{color}" stroke-width="2" opacity="0.45"/>'
                f'<circle cx="{f(x)}" cy="{f(y)}" r="{f(13 * k)}" fill="none" stroke="{color}" stroke-width="{f(max(2, 3 * k))}"/>'
                f'<circle cx="{f(x)}" cy="{f(y - 30 * k)}" r="{f(13 * k)}" fill="{color}" stroke="#fff" stroke-width="{f(max(1.2, 2 * k))}"/>'
                + T(x, y - 24.5 * k, "!", "sans-serif", f(17 * k), "#fff", "middle", 800))

    # ------------------------------------------------------------ concept 0: today
    def current(self, x, y, w):
        c = CITY
        img = np.zeros((N, N, 3))
        rnd = np.random.default_rng(1).random((N, N)) < 0.5
        img[:] = np.where(rnd[..., None], hexrgb("#707d37"), hexrgb("#647832"))
        img[c["veg"]] = np.where(rnd[c["veg"]][..., None], hexrgb("#0a7319"), hexrgb("#197d28"))
        img[c["rocks"]] = np.where(rnd[c["rocks"]][..., None], hexrgb("#5a554b"), hexrgb("#504b41"))
        img[c["water"]] = hexrgb("#1e6473")
        img[c["roads"]] = hexrgb("#a57a51")
        for i, j, bw, bh, sec, _ in c["buildings"]:
            img[i:i + bw, j:j + bh] = hexrgb(GAME_COLORS[sec])
        img = img.transpose(1, 0, 2)          # rows are j in the image
        a, b, e, ts = self.frame(x, y, w)
        return (f'<rect x="{x}" y="{y}" width="{w}" height="{w / 2}" rx="6" fill="#000"/>' + a + self.image(png_uri(img), True) + b
                + f'<rect x="{f(ts(48, 40)[0] - 75)}" y="{f(ts(48, 40)[1] - 42)}" width="150" height="84" fill="none" stroke="#fff" stroke-width="2"/>'
                + e)

    # ------------------------------------------------------------ concept A: painted relief
    PAINT = ["#e7c98b", "#a9b85a", "#b8553f", "#5b8fc9", "#8d8a83", "#9a5aa8", "#e0a93a", "#6fbf7a"]

    # Concept A's colours; a realm can override any of them (see screens.py's HUD).
    PALETTE = dict(land_low="#a7bd72", land_high="#cdbb83", forest="#5d8c48", rocks="#a19584",
                   shallow="#6db3c2", deep="#2f6f8c", foam="#f3f0dc", field="#b7c560", field_row="#8fa843")

    def painted(self, x, y, w, lens=None, palette=None, k=1.0):
        """Concept A. `k` scales the camera frame, pings and icons for a smaller widget."""
        c = CITY
        pal = {**self.PALETTE, **(palette or {})}
        px = 8
        water = up(c["water"], px, 2.2)
        deep = up(c["water"], px, 9)
        veg = up(c["veg"], px, 3)
        rocks = up(c["rocks"], px, 3)
        hgt = up(c["height"], px, 6)
        tex = up(np.random.default_rng(5).random((N, N)) > 0.5, px, 1.2)
        land = mix(np.zeros(water.shape + (3,)) + hexrgb(pal["land_low"]), pal["land_high"], np.clip((hgt - 0.35) * 1.6, 0, 1))
        land = mix(land, pal["forest"], np.clip(veg * 1.4, 0, 1) * (0.8 + 0.2 * tex))
        land = mix(land, pal["rocks"], np.clip(rocks * 1.5, 0, 1))
        gy, gx = np.gradient(hgt)
        shade = np.clip(1 + 14 * (gx * -1 + gy * -1), 0.75, 1.2)
        land = land * shade[..., None]
        sea = mix(np.zeros(water.shape + (3,)) + hexrgb(pal["shallow"]), pal["deep"], np.clip((deep - 0.5) * 2.2, 0, 1))
        img = mix(land, "#000000", np.zeros_like(water))
        img = img * (1 - water[..., None]) + sea * water[..., None]
        foam = np.exp(-((water - 0.5) / 0.09) ** 2)
        img = mix(img, pal["foam"], foam * 0.85)
        if lens:
            gray = img.mean(axis=2, keepdims=True)
            img = img * 0.35 + (gray * 0.85 + 30) * 0.65
            img = mix(img, lens["color"], lens["field"](px) * lens.get("alpha", 0.6))
        img = img.transpose(1, 0, 2)
        a, b, e, ts = self.frame(x, y, w)
        out = [f'<rect x="{x}" y="{y}" width="{w}" height="{w / 2}" rx="6" fill="{pal["deep"]}"/>', a, self.image(png_uri(img))]
        rp = self.road_paths(c["roads"])
        out.append(f'<path d="{rp}" stroke="#6b5a40" stroke-width="0.75" stroke-linecap="round" fill="none" opacity="0.8"/>'
                   f'<path d="{rp}" stroke="#f4e8c8" stroke-width="0.45" stroke-linecap="round" fill="none"/>')
        pid = self.nid("paddy")
        self.defs.append(f'<pattern id="{pid}" width="1" height="0.6" patternUnits="userSpaceOnUse"><rect width="1" height="0.6" fill="{pal["field"]}"/><rect width="1" height="0.22" fill="{pal["field_row"]}"/></pattern>')
        for i, j, bw, bh, sec, name in sorted(c["buildings"], key=lambda q: q[0] + q[1]):
            col = (lens["building"](sec, name, i, j) if lens else self.PAINT[sec])
            if col is None:
                col = "#d9d3c4"
            if sec == FOOD and not lens:
                out.append(f'<rect x="{i + .08}" y="{j + .08}" width="{bw - .16}" height="{bh - .16}" rx=".2" fill="url(#{pid})" stroke="#7b8a3a" stroke-width=".08"/>')
                continue
            ins = 0.14
            out.append(f'<rect x="{i + ins + .32}" y="{j + ins + .32}" width="{bw - 2 * ins}" height="{bh - 2 * ins}" rx=".18" fill="#1f1a12" opacity=".35"/>'
                       f'<rect x="{i + ins}" y="{j + ins}" width="{bw - 2 * ins}" height="{bh - 2 * ins}" rx=".18" fill="{col}" stroke="#2b2418" stroke-opacity=".55" stroke-width=".1"/>')
        out.append(b)
        if not lens:
            for i, j, bw, bh, sec, name in c["buildings"]:
                if name in ("palace", "temple", "market"):
                    sx, sy = ts(i + bw / 2, j + bh / 2)
                    out.append(self.landmark(sx, sy, name, k))
        out.append(self.camera(ts, "modern", k))
        if not lens:
            out.append(self.ping(ts, 56.5, 32.5, k=k))
        out.append(e)
        return "".join(out)

    def landmark(self, x, y, name, k=1.0):
        glyph = GLYPHS[name]
        return (f'<circle cx="{f(x)}" cy="{f(y)}" r="{f(12 * k)}" fill="#fffaf0" stroke="#2b2418" stroke-width="1.5"/>'
                f'<path d="{glyph}" transform="translate({f(x)},{f(y)}) scale({f(k)})" fill="none" stroke="#2b2418" stroke-width="1.6" stroke-linejoin="round" stroke-linecap="round"/>')

    # ------------------------------------------------------------ concept B: ink map
    def ink(self, x, y, w):
        c = CITY
        px = 8
        water = up(c["water"], px, 2.4)
        far = up(c["water"], px, 7)
        paper = np.zeros(water.shape + (3,)) + hexrgb("#ecdcb4")
        grain = up(np.random.default_rng(9).random((N, N)), px, 4)
        paper = mix(paper, "#c9ab72", grain * 0.35)
        img = mix(paper, "#b9c4b4", np.clip((water - 0.5) * 6, 0, 1))
        line = np.exp(-((water - 0.5) / 0.07) ** 2)
        img = mix(img, "#2b2118", line * 0.9)
        for lvl, a in ((0.72, 0.45), (0.9, 0.28)):
            img = mix(img, "#2b2118", np.exp(-((far - lvl) / 0.035) ** 2) * a * (water > 0.5))
        img = img.transpose(1, 0, 2)
        a, b, e, ts = self.frame(x, y, w)
        out = [f'<rect x="{x}" y="{y}" width="{w}" height="{w / 2}" rx="6" fill="#b9c4b4"/>', a, self.image(png_uri(img))]
        out.append(f'<path d="{self.road_paths(c["roads"])}" stroke="#4a3725" stroke-width="0.22" stroke-linecap="round" fill="none"/>')
        hid = self.nid("hatch")
        self.defs.append(f'<pattern id="{hid}" width=".5" height=".5" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><rect width=".5" height=".5" fill="#e9cf9c"/><line x1="0" y1="0" x2="0" y2=".5" stroke="#8a5a34" stroke-width=".1"/></pattern>')
        tint = {HOUSING: f"url(#{hid})", FOOD: "#cfd29a", INDUSTRY: "#d79a82", SERVICES: "#b9cbd8", INFRA: "#cdc4b4", CULTURE: "#d3a9c0", TRADE: "#e7c779", BEAUTY: "#b9d3a6"}
        for i, j, bw, bh, sec, name in c["buildings"]:
            out.append(f'<rect x="{i + .1}" y="{j + .1}" width="{bw - .2}" height="{bh - .2}" fill="{tint[sec]}" stroke="#2b2118" stroke-width=".13"/>')
        out.append(b)
        rnd = np.random.default_rng(3)
        for i in range(0, N, 2):
            for j in range(0, N, 2):
                sx, sy = ts(i + rnd.random(), j + rnd.random())
                if c["veg"][i, j]:
                    out.append(f'<path d="M{f(sx)},{f(sy)} v4" stroke="#2b2118" stroke-width="1.2"/><circle cx="{f(sx)}" cy="{f(sy - 2)}" r="3.4" fill="#9fae78" stroke="#2b2118" stroke-width="1.1"/>')
                elif c["rocks"][i, j]:
                    out.append(f'<path d="M{f(sx - 6)},{f(sy + 3)} L{f(sx)},{f(sy - 6)} L{f(sx + 6)},{f(sy + 3)}" fill="#e6d5ad" stroke="#2b2118" stroke-width="1.2"/><path d="M{f(sx)},{f(sy - 6)} l2,5" stroke="#2b2118" stroke-width=".8"/>')
        for i, j, bw, bh, sec, name in c["buildings"]:
            if name in ("palace", "temple", "market"):
                sx, sy = ts(i + bw / 2, j + bh / 2)
                letter = {"palace": "H", "temple": "T", "market": "C"}[name]
                out.append(f'<g transform="rotate(-4 {f(sx)} {f(sy)})"><rect x="{f(sx - 10)}" y="{f(sy - 10)}" width="20" height="20" fill="#a8261c" rx="2"/>'
                           f'<rect x="{f(sx - 7.5)}" y="{f(sy - 7.5)}" width="15" height="15" fill="none" stroke="#f1dfb8" stroke-width="1"/></g>'
                           + T(sx, sy + 5, letter, "Protest Revolution", 14, "#f6e6c4", "middle"))
        sx, sy = ts(57, 33)
        out.append(f'<circle cx="{f(sx)}" cy="{f(sy)}" r="15" fill="none" stroke="#a8261c" stroke-width="2.5" stroke-dasharray="4 3"/>')
        out.append(self.camera(ts, "ink"))
        out.append(e)
        return "".join(out)

    # ------------------------------------------------------------ concept C: lenses
    def lens(self, kind):
        c = CITY
        bl = c["buildings"]

        def field_from(points, radius):
            m = np.zeros((N, N))
            for (i, j) in points:
                m[int(i), int(j)] = 1

            def fld(px):
                im = Image.fromarray((m * 255).astype(np.uint8)).resize((N * px, N * px), Image.NEAREST)
                im = im.filter(ImageFilter.GaussianBlur(radius * px))
                a = np.asarray(im, float)
                return np.clip(a / max(a.max(), 1) * 1.6, 0, 1)
            return fld

        if kind == "sectors":
            return dict(color="#ffffff", alpha=0, field=lambda px: np.zeros((N * px, N * px)),
                        building=lambda s, n, i, j: self.LENS_SECTOR[s])
        if kind == "water":
            wells = [(i + .5, j + .5) for i, j, bw, bh, s, n in bl if n == "well"]
            river = [(i, j) for i in range(0, N, 3) for j in range(0, N, 3) if c["water"][i, j]]
            fld = field_from(wells + river, 2.6)
            near = lambda i, j: any((i - a) ** 2 + (j - b) ** 2 < 40 for a, b in wells + river)
            return dict(color="#3d8fd1", alpha=0.55, field=fld,
                        building=lambda s, n, i, j: ("#2f78c4" if near(i, j) else "#e0893a") if s == HOUSING else None)
        if kind == "fire":
            risky = [(i + bw / 2, j + bh / 2) for i, j, bw, bh, s, n in bl if s in (INDUSTRY, HOUSING) and i > 44]
            fld = field_from(risky, 2.2)
            return dict(color="#e2492f", alpha=0.6, field=fld,
                        building=lambda s, n, i, j: ("#c4321f" if s == INDUSTRY else "#e7925a" if (s == HOUSING and i > 44) else None))
        if kind == "religion":
            temples = [(i + bw / 2, j + bh / 2) for i, j, bw, bh, s, n in bl if n in ("temple", "palace")]
            fld = field_from(temples, 4.5)
            near = lambda i, j: any((i - a) ** 2 + (j - b) ** 2 < 90 for a, b in temples)
            return dict(color="#9a5aa8", alpha=0.45, field=fld,
                        building=lambda s, n, i, j: ("#7f3f93" if s == CULTURE else "#59a35f" if (s == HOUSING and near(i, j)) else "#d6643c" if s == HOUSING else None))

    LENS_SECTOR = ["#f2c45a", "#9cc24a", "#d4463a", "#3f8fe0", "#7d8794", "#9b59c2", "#f08c2a", "#3dbf8a"]


# ====================================================================== chrome
def icon(kind, x, y, s, c):
    p = GLYPHS[kind]
    return f'<path d="{p}" transform="translate({f(x)},{f(y)}) scale({s})" fill="none" stroke="{c}" stroke-width="{2.2 / s}" stroke-linecap="round" stroke-linejoin="round"/>'


LENSES = [("city", "City"), ("sectors", "Sectors"), ("water", "Water"), ("fire", "Risks"), ("religion", "Faith")]
TOOLS = [("plus", "Zoom in"), ("minus", "Zoom out"), ("alert", "Next alert"), ("home", "Centre on the palace"), ("world", "World map"), ("collapse", "Hide the map")]


def widget_layout(x, y, map_w, btn, gap, pad):
    """The widget's layout: lens icons in a row across the top, the map under them, the
    tools in a column on the right. Returns the total size and where everything goes."""
    map_x, map_y = x + pad, y + pad + btn + gap
    col_x = map_x + map_w + gap
    inner_h = btn + gap + map_w / 2
    W = pad * 2 + map_w + gap + btn
    H = pad * 2 + inner_h
    lens_pos = [(map_x + k * (btn + gap), y + pad) for k in range(len(LENSES))]
    step = (inner_h - btn) / (len(TOOLS) - 1)
    tool_pos = [(col_x, y + pad + k * step) for k in range(len(TOOLS))]
    return W, H, (map_x, map_y), lens_pos, tool_pos


def widget_controls(lens_pos, tool_pos, btn, colors, lens_on="city", alerts=2, icon_scale=None):
    """Icon-only buttons: one per lens, then the tools. `colors`: fill, on_fill, stroke, ink, on_ink, alert, badge."""
    c = colors
    s = icon_scale or btn / 44
    out = []
    for (bx, by), (key, _) in zip(lens_pos, LENSES):
        on = key == lens_on
        out.append(f'<rect x="{f(bx)}" y="{f(by)}" width="{btn}" height="{btn}" rx="{btn * 0.16:.1f}" fill="{c["on_fill"] if on else c["fill"]}" stroke="{c["stroke"]}" stroke-width="1.2"/>'
                   + icon(key, bx + btn / 2, by + btn / 2, s, c["on_ink"] if on else c["ink"]))
    for (bx, by), (key, _) in zip(tool_pos, TOOLS):
        out.append(f'<rect x="{f(bx)}" y="{f(by)}" width="{btn}" height="{btn}" rx="{btn * 0.16:.1f}" fill="{c["fill"]}" stroke="{c["stroke"]}" stroke-width="1.2"/>'
                   + icon(key, bx + btn / 2, by + btn / 2, s, c["alert"] if key == "alert" else c["ink"]))
        if key == "alert" and alerts:
            r = btn * 0.25
            out.append(f'<circle cx="{f(bx + btn - 2)}" cy="{f(by + 2)}" r="{f(r)}" fill="{c["badge"]}" stroke="#fff4dc" stroke-width="1.2"/>'
                       + T(bx + btn - 2, by + 2 + r * 0.42, str(alerts), "sans-serif", f(r * 1.25), "#fff", "middle", 800))
    return "".join(out)


DAILONG_CONTROLS = dict(fill="#f6ead0", on_fill="#a8261c", stroke="#2b211880", ink="#2b2118", on_ink="#fff4dc", alert="#a8261c", badge="#a8261c")


def widget(mm, x, y, w, body_fn, lens_on="city", alerts=2, hover=None):
    """The minimap widget in Đại Long's HUD chrome, large, for the widget sheet."""
    btn, gap, pad = 44, 10, 14
    W, H, (mx, my), lens_pos, tool_pos = widget_layout(x, y, w, btn, gap, pad)
    out = [f'<rect x="{x - 6}" y="{y - 6}" width="{W + 12}" height="{H + 12}" rx="8" fill="#2a170c"/>',
           f'<rect x="{x}" y="{y}" width="{W}" height="{H}" rx="5" fill="#ecdcb4"/>',
           body_fn(mx, my, w),
           widget_controls(lens_pos, tool_pos, btn, DAILONG_CONTROLS, lens_on, alerts, icon_scale=1.0)]
    if hover is not None:
        (bx, by), (_, label) = lens_pos[hover], LENSES[hover]
        tw = 18 + len(label) * 11
        out.append(f'<rect x="{f(bx)}" y="{f(by + btn + 6)}" width="{tw}" height="32" rx="5" fill="#2a170c" opacity=".92"/>'
                   + T(bx + 10, by + btn + 28, label, "Protest Revolution", 19, "#f6ead0"))
    return "".join(out), W, H


def page(name, title, sub, body, defs, h=1080):
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="{h}" viewBox="0 0 1920 {h}"><defs>{"".join(defs)}</defs>'
           f'<rect width="1920" height="{h}" fill="#1d1813"/>'
           + T(48, 62, title, "sans-serif", 30, "#f3e6c8", "start", 700)
           + T(48, 94, sub, "sans-serif", 18, "#b9a98b", "start", 500) + body + "</svg>")
    html = (f'<!doctype html><html><head><meta charset="utf-8"><title>{name}</title><style>{FONTS_CSS} html,body{{margin:0;background:#1d1813}} svg{{display:block}}</style></head>'
            f'<body>{svg}</body></html>')
    BUILD.mkdir(exist_ok=True)
    (BUILD / f"{name}.html").write_text(html)
    print("wrote", name)


def caption(x, y, title, lines, w=860):
    out = [T(x, y, title, "sans-serif", 22, "#f3e6c8", "start", 700)]
    for k, s in enumerate(lines):
        out.append(T(x, y + 28 + k * 23, s, "sans-serif", 16, "#cbbd9f", "start", 500))
    return "".join(out)


def concepts():
    defs = []
    mm = Minimap(defs)
    body = []
    MW = 640
    cells = [
        (60, 140, "Today", mm.current, ["One pixel per tile in nine sector colours.",
                                         "Accurate, but flat and noisy: hard to read at a glance."]),
        (780, 140, "A · Painted relief", mm.painted, ["Terrain painted smoothly: hill shading, water depth, foam at the shore.",
                                                         "Buildings as soft-shadowed blocks in muted sector colours; roads as pale lines;",
                                                         "landmarks as icons. Modern, readable, and works for every realm."]),
        (60, 600, "B · Ink map", mm.ink, ["Matches the world map: parchment, ink coastline with contour lines,",
                                          "tree and hill glyphs, hatched housing, red seals for landmarks.",
                                          "Most characterful; can be restyled per realm like the world map."]),
        (780, 600, "C · Lens: Sectors", lambda x, y, w: mm.painted(x, y, w, mm.lens("sectors")),
         ["The same painted map, desaturated, with one data layer on top: here, sectors.",
          "Lenses (water, risks, faith...) reuse the game's overlays on the minimap."]),
    ]
    for x, y, title, fn, lines in cells:
        body.append(f'<rect x="{x}" y="{y}" width="{MW + 20}" height="{MW / 2 + 20}" rx="10" fill="#2a2219"/>')
        body.append(fn(x + 10, y + 10, MW))
        body.append(caption(x + MW + 44 if False else x, y + MW / 2 + 60, title, lines))
    # Right column notes.
    notes = [("Why not sprites", ["At 2–6 px per tile, building", "sprites turn into noise."]),
             ("The approach", ["Draw the data with care:", "smooth painted terrain,", "clear building shapes, and", "icons only for what matters:", "landmarks and alerts."]),
             ("A and C combine", ["Lenses are A with one data", "layer on top: the default", "view is A, and a lens bar", "switches layers."]),
             ("B per realm", ["The ink map fits Đại Long", "best; Kambara and Tampadesa", "would need their own take,", "as on the world map."])]
    yy = 160
    for t, lines in notes:
        body.append(T(1520, yy, t, "sans-serif", 19, "#f3e6c8", "start", 700))
        for k, s in enumerate(lines):
            body.append(T(1520, yy + 26 + k * 22, s, "sans-serif", 15, "#cbbd9f", "start", 500))
        yy += 26 + len(lines) * 22 + 34
    page("minimap-concepts", "Minimap concepts", "The same 80 × 80 sample city in the minimap's isometric projection, clipped to the playable area as in the game.",
         "".join(body), defs)


def lenses():
    defs = []
    mm = Minimap(defs)
    body = []
    # The full widget with tools, in Đại Long's chrome.
    w = 760
    inner, W, H = widget(mm, 60, 130, w, lambda x, y, ww: mm.painted(x, y, ww), "city", hover=2)
    body.append(inner)
    notes = [
        ("Lens icons (top)", "City, Sectors, Water, Risks, Faith; the name shows as a tooltip. Picks what the minimap shows."),
        ("+ / −", "Zoom the minimap in around the camera: big maps (256 × 256) get too dense at one zoom."),
        ("Alerts", "Counts fires, collapses, raids and requests; each press jumps the camera to the next one."),
        ("Home", "Centres the camera on the palace or the governor's house."),
        ("World map", "Opens the world map, for trade routes and requests."),
        ("Collapse", "Folds the widget down to its frame, as the current open / close button does."),
        ("On the map", "Camera frame with corner ticks, pulsing pings for alerts, icons for landmarks only."),
    ]
    nx = 60 + W + 60
    for k, (t, s) in enumerate(notes):
        yy = 170 + k * 62
        body.append(T(nx, yy, t, "sans-serif", 19, "#f3e6c8", "start", 700))
        body.append(T(nx, yy + 25, s, "sans-serif", 15, "#cbbd9f", "start", 500))
    # The four lenses, small.
    LW = 420
    for k, (key, title, legend) in enumerate([
            ("sectors", "Sectors", [(c, n) for c, n in zip(Minimap.LENS_SECTOR, SECTOR_NAMES)]),
            ("water", "Water", [("#2f78c4", "House has water"), ("#e0893a", "No water"), ("#3d8fd1", "Coverage")]),
            ("fire", "Risks: fire", [("#c4321f", "High risk"), ("#e7925a", "At risk"), ("#e2492f", "Risk heat")]),
            ("religion", "Faith", [("#7f3f93", "Temple"), ("#59a35f", "Covered"), ("#d6643c", "Not covered")])]):
        x = 60 + k * (LW + 40)
        y = 640
        body.append(f'<rect x="{x}" y="{y}" width="{LW + 16}" height="{LW / 2 + 16}" rx="8" fill="#2a2219"/>')
        body.append(mm.painted(x + 8, y + 8, LW, mm.lens(key)))
        body.append(T(x, y + LW / 2 + 52, title, "sans-serif", 20, "#f3e6c8", "start", 700))
        for n, (col, lab) in enumerate(legend):
            lx = x + (n % 2) * 220
            ly = y + LW / 2 + 82 + (n // 2) * 24
            body.append(f'<rect x="{lx}" y="{ly - 12}" width="14" height="14" rx="3" fill="{col}"/>' + T(lx + 22, ly, lab, "sans-serif", 15, "#cbbd9f", "start", 500))
    page("minimap-lenses", "Minimap widget and lenses", "Concept A in Đại Long's HUD chrome: lens icons across the top, tools down the right, and the four lenses below.",
         "".join(body), defs)


if __name__ == "__main__":
    concepts()
    lenses()
