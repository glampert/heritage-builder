"""Shared geography, geometry helpers and screen layout for the world map mockups.

Map space is 1000 x 672 units, north at the top. It is drawn into the map
viewport on a 1920 x 1080 screen.
"""
import math
import random

W, H = 1920, 1080
MAP_X, MAP_Y, MAP_S = 52, 132, 1.29          # viewport origin and scale
MAP_W, MAP_H = 1000 * MAP_S, 672 * MAP_S       # 1290 x 867
PANEL_X, PANEL_W = 1400, 480


def P(x, y):
    return MAP_X + x * MAP_S, MAP_Y + y * MAP_S


def f(v):
    return f"{v:.1f}"


def smooth(points, closed=False, tension=0.5):
    """Catmull-Rom spline through screen points, as an SVG path."""
    pts = list(points)
    n = len(pts)
    if n < 2:
        return ""
    out = [f"M{f(pts[0][0])},{f(pts[0][1])}"]
    rng = range(n) if closed else range(n - 1)
    for i in rng:
        p0 = pts[(i - 1) % n] if (closed or i > 0) else pts[i]
        p1 = pts[i]
        p2 = pts[(i + 1) % n]
        p3 = pts[(i + 2) % n] if (closed or i + 2 < n) else p2
        c1 = (p1[0] + (p2[0] - p0[0]) * tension / 3, p1[1] + (p2[1] - p0[1]) * tension / 3)
        c2 = (p2[0] - (p3[0] - p1[0]) * tension / 3, p2[1] - (p3[1] - p1[1]) * tension / 3)
        out.append(f"C{f(c1[0])},{f(c1[1])} {f(c2[0])},{f(c2[1])} {f(p2[0])},{f(p2[1])}")
    if closed:
        out.append("Z")
    return " ".join(out)


def roughen(points, amp, seed, sub=3, closed=True):
    """Subdivide a polyline and jitter the new points for a hand-drawn edge."""
    rnd = random.Random(seed)
    pts = []
    n = len(points)
    rng = range(n) if closed else range(n - 1)
    for i in rng:
        a, b = points[i], points[(i + 1) % n]
        for k in range(sub):
            t = k / sub
            x = a[0] + (b[0] - a[0]) * t
            y = a[1] + (b[1] - a[1]) * t
            if k:
                x += rnd.uniform(-amp, amp)
                y += rnd.uniform(-amp, amp)
            pts.append((x, y))
    if not closed:
        pts.append(points[-1])
    return pts


def mp(points):
    return [P(x, y) for x, y in points]


# ---------------------------------------------------------------- geography

# The land, clockwise; edges along the map border are off-screen in practice.
LAND = [
    (-20, -20), (1020, -20), (1020, 92), (960, 96), (925, 112), (890, 138), (862, 160),
    (842, 178), (822, 196), (806, 208), (794, 232), (782, 262), (772, 292), (768, 322),
    (778, 352), (796, 380), (812, 404), (818, 432), (808, 462), (788, 490), (764, 516),
    (738, 540), (712, 566), (690, 588), (664, 600), (640, 592), (618, 566), (596, 536),
    (572, 518), (546, 508), (518, 516), (494, 540), (478, 572), (468, 606), (458, 640),
    (452, 692), (418, 692), (414, 650), (404, 616), (390, 596), (368, 588), (346, 594),
    (324, 604), (300, 598), (278, 590), (252, 594), (228, 580), (200, 556), (166, 534),
    (128, 512), (92, 486), (58, 462), (24, 446), (-20, 432),
]

ISLANDS = [
    [(842, 288), (858, 280), (866, 292), (858, 306), (844, 304)],     # Vân Cảng
    [(880, 360), (892, 356), (896, 366), (886, 372)],
    [(300, 628), (314, 624), (318, 634), (306, 640)],
]

RIVERS = {
    "son": [(612, 32), (640, 62), (668, 94), (700, 124), (734, 152), (766, 178), (792, 198), (812, 206)],
    "son_b": [(766, 178), (782, 202), (794, 226)],
    "great": [(508, 0), (522, 60), (548, 120), (582, 176), (600, 232), (606, 290), (620, 350),
              (636, 410), (652, 470), (668, 520), (684, 560), (700, 584)],
    "great_b": [(668, 520), (654, 560), (650, 594)],
    "shwemyit": [(176, 0), (196, 60), (220, 120), (246, 180), (256, 228), (238, 262), (236, 300),
                 (256, 344), (278, 400), (296, 456), (306, 510), (322, 560), (336, 596)],
    "shwemyit_b": [(306, 510), (288, 552), (282, 594)],
    "lake_link": [(604, 428), (622, 432), (640, 428)],
}

LAKE = dict(cx=566, cy=446, rx=48, ry=22, rot=-28)

# Mountain glyphs: (x, y, size)
def _band(rnd, x0, y0, x1, y1, n, s0, s1, spread):
    out = []
    for i in range(n):
        t = i / max(1, n - 1)
        x = x0 + (x1 - x0) * t + rnd.uniform(-spread, spread)
        y = y0 + (y1 - y0) * t + rnd.uniform(-spread, spread)
        out.append((x, y, rnd.uniform(s0, s1)))
    return out

_r = random.Random(7)
MOUNTAINS = (
    _band(_r, 300, 70, 600, 40, 11, 13, 20, 12)            # northern range
    + _band(_r, 860, 60, 990, 70, 5, 12, 18, 8)
    + _band(_r, 350, 130, 440, 470, 13, 12, 19, 18)        # central highlands
    + _band(_r, 410, 120, 500, 300, 6, 11, 16, 14)
    + _band(_r, 700, 236, 748, 470, 9, 12, 17, 10)         # coastal range
    + _band(_r, 90, 120, 150, 420, 10, 11, 16, 14)          # western range
)
MOUNTAINS.sort(key=lambda m: m[1])
KARST = [(706, 240), (698, 262), (712, 286), (756, 250), (760, 274), (694, 286)]
HOLY_HILLS = [(570, 354), (584, 346), (598, 354)]

# Region labels: (text, x, y, size, letter-spacing)
REGIONS = {
    "dailong": ("ĐẠI LONG", 566, 142),
    "kambara": ("KAMBARA", 590, 296),
    "tampadesa": ("TAMPADESA", 160, 410),
    "vijara": ("VIJARA", 760, 440),
    "hamsara": ("HAMSARA", 384, 546),
    "north": ("THE NORTHERN THRONE", 760, 34),
    "clans": ("THE HIGHLAND CLANS", 400, 332),
}

NORTH_BORDER = [(520, 72), (600, 78), (680, 66), (760, 74), (840, 64), (920, 72), (1010, 66)]
