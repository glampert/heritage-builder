"""Tampadesa's brick and gold, after the user's references of 2026-11-14: frames of
weathered red brick with gold filigree, gilt stupa finials crowning them, singhas
guarding a large panel's corners, gilt nagas, a gold lotus at the foot, vines, and
rough dark mudbrick inside. The icons are gold leaf in relief. Used by palette.py
for the build palette's tiles and the HUD's panels.

Everything is SVG: the bricks are a pattern, their roughness and light come from
turbulence and lighting filters.
"""
import math
import random

from palette_glyphs import f

GOLD = [(0, "#ffe9a0"), (0.3, "#f0bf4c"), (0.62, "#b8801e"), (1, "#6e440a")]


class Brickwork:
    def __init__(self, defs, uid, carving):
        self.defs = defs
        self.uid = uid
        self.carving = carving      # Kambara's carving, for its leaves and vines
        self.ids = {}

    def filt(self, name):
        if name in self.ids:
            return self.ids[name]
        fid = self.uid(f"t{name}")
        self.ids[name] = fid
        if name in ("bricks", "mud"):
            # A brick pattern: courses of bricks in mortar, each brick's red a little different.
            rnd = random.Random(3 if name == "bricks" else 9)
            bw, bh = (22, 9) if name == "bricks" else (30, 12)
            reds = ["#a8442a", "#9a3a24", "#b4522e", "#8e3420", "#a24a2c", "#b85a34"] if name == "bricks" else \
                   ["#4e1e14", "#55241a", "#47190f", "#5c2a1c", "#4a2016"]
            cells = []
            for r in range(4):
                off = (bw / 2) if r % 2 else 0
                for c in range(-1, 5):
                    cells.append(f'<rect x="{f(c * bw + off + 0.8)}" y="{f(r * bh + 0.8)}" width="{f(bw - 1.6)}" height="{f(bh - 1.6)}" rx="1" fill="{rnd.choice(reds)}"/>')
            mortar = "#3e1a10" if name == "bricks" else "#2a0e08"
            self.defs.append(f'<pattern id="{fid}" width="{f(bw * 4)}" height="{f(bh * 4)}" patternUnits="userSpaceOnUse">'
                             f'<rect width="{f(bw * 4)}" height="{f(bh * 4)}" fill="{mortar}"/>{"".join(cells)}</pattern>')
        elif name in ("rough", "rough_small"):
            # Weathered brick: a bevel at the shape's edge, pits and grain, lit from the top left.
            bevel, scale = (2.0, 6.0) if name == "rough" else (1.0, 3.0)
            self.defs.append(
                f'<filter id="{fid}" x="-10%" y="-10%" width="120%" height="120%" color-interpolation-filters="sRGB">'
                f'<feTurbulence type="fractalNoise" baseFrequency="0.25" numOctaves="3" seed="6" result="n"/>'
                f'<feColorMatrix in="n" type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  1 0 0 0 0" result="na"/>'
                f'<feGaussianBlur in="SourceAlpha" stdDeviation="{bevel}" result="b"/>'
                f'<feComposite in="b" in2="na" operator="arithmetic" k2="1" k3="0.35" result="h"/>'
                f'<feDiffuseLighting in="h" surfaceScale="{scale}" diffuseConstant="1.3" lighting-color="#ffffff" result="d"><feDistantLight azimuth="225" elevation="38"/></feDiffuseLighting>'
                f'<feComposite in="d" in2="SourceGraphic" operator="arithmetic" k1="1.35" result="lit"/>'
                f'<feTurbulence type="fractalNoise" baseFrequency="0.04" numOctaves="3" seed="2" result="m"/>'
                f'<feColorMatrix in="m" type="matrix" values="0 0 0 0 0.12  0 0 0 0 0.05  0 0 0 0 0.02  2.2 0 0 0 -1.15" result="soot"/>'
                f'<feComposite in="soot" in2="lit" operator="over" result="all"/>'
                f'<feComposite in="all" in2="SourceAlpha" operator="in" result="body"/>'
                f'<feOffset in="b" dx="1.6" dy="2.4" result="o"/><feFlood flood-color="#0e0402" flood-opacity="0.75"/><feComposite in2="o" operator="in" result="sh"/>'
                f'<feMerge><feMergeNode in="sh"/><feMergeNode in="body"/></feMerge></filter>')
        elif name == "panel":
            # The inside: rough dark mudbrick, mottled, soot in its hollows.
            self.defs.append(
                f'<filter id="{fid}" x="0" y="0" width="100%" height="100%" color-interpolation-filters="sRGB">'
                f'<feTurbulence type="fractalNoise" baseFrequency="0.09" numOctaves="4" seed="11" result="n"/>'
                f'<feColorMatrix in="n" type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  1 0 0 0 0" result="na"/>'
                f'<feDiffuseLighting in="na" surfaceScale="2.2" diffuseConstant="1.2" lighting-color="#ffffff" result="d"><feDistantLight azimuth="225" elevation="50"/></feDiffuseLighting>'
                f'<feComposite in="d" in2="SourceGraphic" operator="arithmetic" k1="1.2" result="lit"/>'
                f'<feComposite in="lit" in2="SourceAlpha" operator="in"/></filter>')
        elif name == "gilt":
            self.defs.append(
                f'<filter id="{fid}" x="-15%" y="-15%" width="130%" height="130%" color-interpolation-filters="sRGB">'
                f'<feGaussianBlur in="SourceAlpha" stdDeviation="1.0" result="b"/>'
                f'<feDiffuseLighting in="b" surfaceScale="4" diffuseConstant="1.15" lighting-color="#ffffff" result="d"><feDistantLight azimuth="225" elevation="42"/></feDiffuseLighting>'
                f'<feSpecularLighting in="b" surfaceScale="4" specularConstant="1.1" specularExponent="18" lighting-color="#fff2c0" result="s"><feDistantLight azimuth="225" elevation="42"/></feSpecularLighting>'
                f'<feComposite in="d" in2="SourceGraphic" operator="arithmetic" k1="1.3" result="lit"/>'
                f'<feComposite in="s" in2="SourceAlpha" operator="in" result="si"/>'
                f'<feComposite in="lit" in2="si" operator="arithmetic" k2="1" k3="0.85" result="all"/>'
                f'<feComposite in="all" in2="SourceAlpha" operator="in" result="body"/>'
                f'<feOffset in="b" dx="1.1" dy="1.8" result="o"/><feFlood flood-color="#120402" flood-opacity="0.75"/><feComposite in2="o" operator="in" result="sh"/>'
                f'<feMerge><feMergeNode in="sh"/><feMergeNode in="body"/></feMerge></filter>')
        elif name == "gold":
            self.defs.append(f'<linearGradient id="{fid}" x1="0" y1="0" x2="0.35" y2="1">'
                             + "".join(f'<stop offset="{o}" stop-color="{c}"/>' for o, c in GOLD) + "</linearGradient>")
        elif name == "glow":
            self.defs.append(
                f'<filter id="{fid}" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur in="SourceAlpha" stdDeviation="4" result="b"/>'
                f'<feFlood flood-color="#ffd060" flood-opacity="1"/><feComposite in2="b" operator="in" result="g"/>'
                f'<feMerge><feMergeNode in="g"/><feMergeNode in="SourceGraphic"/></feMerge></filter>')
        elif name == "grey":
            self.defs.append(f'<filter id="{fid}"><feColorMatrix type="saturate" values="0.15"/>'
                             f'<feComponentTransfer><feFuncR type="linear" slope="0.7"/><feFuncG type="linear" slope="0.7"/><feFuncB type="linear" slope="0.7"/></feComponentTransfer></filter>')
        return fid

    # ------------------------------------------------------------ gold ornament
    def stupa(self, cx, by, s):
        """A gilt stupa standing on (cx, by), s tall: terraces, a bell, rings, a spire and a finial."""
        u = s / 100
        d = (f"M{f(cx - 34 * u)},{f(by)} H{f(cx + 34 * u)} V{f(by - 8 * u)} H{f(cx + 26 * u)} V{f(by - 15 * u)} H{f(cx + 20 * u)} "
             f"C{f(cx + 20 * u)},{f(by - 36 * u)} {f(cx + 10 * u)},{f(by - 44 * u)} {f(cx + 6 * u)},{f(by - 46 * u)} "
             f"L{f(cx + 2 * u)},{f(by - 92 * u)} L{f(cx)},{f(by - 100 * u)} L{f(cx - 2 * u)},{f(by - 92 * u)} L{f(cx - 6 * u)},{f(by - 46 * u)} "
             f"C{f(cx - 10 * u)},{f(by - 44 * u)} {f(cx - 20 * u)},{f(by - 36 * u)} {f(cx - 20 * u)},{f(by - 15 * u)} H{f(cx - 26 * u)} V{f(by - 8 * u)} H{f(cx - 34 * u)}Z")
        rings = "".join(f'<path d="M{f(cx - (6 - k) * u * 1.1)},{f(by - (52 + k * 7) * u)} H{f(cx + (6 - k) * u * 1.1)}" stroke="#6e440a" stroke-width="{f(max(0.4, 1.6 * u))}"/>' for k in range(5))
        band = f'<path d="M{f(cx - 19 * u)},{f(by - 26 * u)} Q{f(cx)},{f(by - 31 * u)} {f(cx + 19 * u)},{f(by - 26 * u)}" fill="none" stroke="#6e440a" stroke-width="{f(max(0.4, 1.6 * u))}"/>'
        return (f'<g filter="url(#{self.filt("gilt")})"><path d="{d}" fill="url(#{self.filt("gold")})"/></g>{rings}{band}')

    def lotus(self, cx, by, s):
        g = self.filt("gold")
        d = (f"M{f(cx)},{f(by)} C{f(cx - s * 0.25)},{f(by - s * 0.4)} {f(cx - s * 0.18)},{f(by - s * 0.8)} {f(cx)},{f(by - s)} "
             f"C{f(cx + s * 0.18)},{f(by - s * 0.8)} {f(cx + s * 0.25)},{f(by - s * 0.4)} {f(cx)},{f(by)}Z "
             + "".join(f"M{f(cx + k * s * 0.05)},{f(by)} C{f(cx + k * s * 0.5)},{f(by - s * 0.1)} {f(cx + k * s * 0.62)},{f(by - s * 0.45)} {f(cx + k * s * 0.6)},{f(by - s * 0.65)} "
                       f"C{f(cx + k * s * 0.35)},{f(by - s * 0.55)} {f(cx + k * s * 0.15)},{f(by - s * 0.35)} {f(cx + k * s * 0.05)},{f(by)}Z " for k in (-1, 1)))
        return f'<g filter="url(#{self.filt("gilt")})"><path d="{d}" fill="url(#{g})"/></g>'

    def kanok(self, x, y, s, sx=1, sy=1):
        """A gilt flame scroll (kanok) curling from (x, y), s long, mirrored by sx, sy."""
        g = self.filt("gold")
        pts = []
        for k in range(40):
            t = k / 39
            a = t * 2.0 * math.pi
            r = s * 0.32 * (1 - 0.75 * t)
            pts.append((x + sx * (s * 0.55 * t + r * math.sin(a)), y + sy * (-r * (1 - math.cos(a)) * 0.9)))
        d = "M" + " L".join(f"{f(a_)},{f(b_)}" for a_, b_ in pts)
        flame = (f"M{f(x + sx * s * 0.15)},{f(y)} Q{f(x + sx * s * 0.05)},{f(y - sy * s * 0.5)} {f(x - sx * s * 0.2)},{f(y - sy * s * 0.62)} "
                 f"Q{f(x + sx * s * 0.2)},{f(y - sy * s * 0.4)} {f(x + sx * s * 0.3)},{f(y)}Z")
        return (f'<g filter="url(#{self.filt("gilt")})"><path d="{d}" fill="none" stroke="url(#{g})" stroke-width="{f(max(1.2, s * 0.13))}" stroke-linecap="round"/>'
                f'<path d="{flame}" fill="url(#{g})"/></g>')

    def naga(self, x, y, s, side=1):
        """A gilt naga, its body undulating from (x, y) towards `side`, its crested head raised at the end."""
        g = self.filt("gold")
        u = s / 100
        pts = []
        for k in range(30):
            t = k / 29
            pts.append((x + side * t * 78 * u, y - math.sin(t * math.pi * 2) * 9 * u * (1 - t * 0.3) - t * 10 * u))
        body = "M" + " L".join(f"{f(a_)},{f(b_)}" for a_, b_ in pts)
        hx, hy = pts[-1]
        head = (f"M{f(hx - side * 4 * u)},{f(hy + 4 * u)} Q{f(hx + side * 4 * u)},{f(hy - 16 * u)} {f(hx + side * 16 * u)},{f(hy - 14 * u)} "
                f"Q{f(hx + side * 22 * u)},{f(hy - 10 * u)} {f(hx + side * 18 * u)},{f(hy - 4 * u)} Q{f(hx + side * 8 * u)},{f(hy)} {f(hx + side * 6 * u)},{f(hy + 6 * u)}Z")
        crest = "".join(f"M{f(hx + side * (2 + k * 5) * u)},{f(hy - (12 + k * 1.5) * u)} l{f(-side * 4 * u)},{f(-9 * u)} l{f(side * 7 * u)},{f(5 * u)}Z " for k in range(3))
        return (f'<g filter="url(#{self.filt("gilt")})"><path d="{body}" fill="none" stroke="url(#{g})" stroke-width="{f(9 * u)}" stroke-linecap="round" stroke-linejoin="round"/>'
                f'<path d="{head} {crest}" fill="url(#{g})"/></g>'
                f'<path d="{body}" fill="none" stroke="#6e440a" stroke-width="{f(1.2 * u)}" stroke-dasharray="{f(3 * u)} {f(4 * u)}" opacity="0.7"/>'
                f'<circle cx="{f(hx + side * 12 * u)}" cy="{f(hy - 9 * u)}" r="{f(1.5 * u)}" fill="#3a1206"/>')

    def singha(self, cx, by, s, side=1):
        """A gilt singha seated on (cx, by), s tall, facing `side`: haunches, chest, maned head, raised tail."""
        g = self.filt("gold")
        u = s / 100
        X = lambda v: cx + side * v * u
        d = (f"M{f(X(-30))},{f(by)} Q{f(X(-36))},{f(by - 30 * u)} {f(X(-22))},{f(by - 46 * u)} "
             f"Q{f(X(-8))},{f(by - 58 * u)} {f(X(4))},{f(by - 62 * u)} "
             f"Q{f(X(2))},{f(by - 80 * u)} {f(X(12))},{f(by - 92 * u)} Q{f(X(24))},{f(by - 100 * u)} {f(X(34))},{f(by - 90 * u)} "
             f"Q{f(X(40))},{f(by - 82 * u)} {f(X(38))},{f(by - 74 * u)} L{f(X(30))},{f(by - 70 * u)} Q{f(X(28))},{f(by - 56 * u)} {f(X(22))},{f(by - 46 * u)} "
             f"L{f(X(24))},{f(by - 10 * u)} L{f(X(30))},{f(by - 6 * u)} V{f(by)}Z")
        mane = "".join(f"M{f(X(8 + k * 6))},{f(by - (66 + (k % 2) * 4) * u)} q{f(side * 3 * u)},{f(8 * u)} {f(side * 6 * u)},0" for k in range(4))
        tail = f"M{f(X(-30))},{f(by - 20 * u)} Q{f(X(-48))},{f(by - 40 * u)} {f(X(-40))},{f(by - 62 * u)} q{f(side * 6 * u)},{f(-6 * u)} {f(side * 2 * u)},{f(-12 * u)}"
        return (f'<g filter="url(#{self.filt("gilt")})"><path d="{d}" fill="url(#{g})"/>'
                f'<path d="{tail}" fill="none" stroke="url(#{g})" stroke-width="{f(5 * u)}" stroke-linecap="round"/></g>'
                f'<path d="{mane}" fill="none" stroke="#6e440a" stroke-width="{f(1.6 * u)}" stroke-linecap="round"/>'
                f'<circle cx="{f(X(28))}" cy="{f(by - 84 * u)}" r="{f(2 * u)}" fill="#3a1206"/>')

    def naga_hood(self, cx, by, s, side=1):
        """A gilt naga rearing from its coils at (cx, by), s tall, its hood a fan of five heads."""
        g = self.filt("gold")
        u = s / 100
        X = lambda v: cx + side * v * u
        coils = (f'<ellipse cx="{f(X(-6))}" cy="{f(by - 8 * u)}" rx="{f(30 * u)}" ry="{f(9 * u)}"/>'
                 f'<ellipse cx="{f(X(-2))}" cy="{f(by - 20 * u)}" rx="{f(22 * u)}" ry="{f(8 * u)}"/>')
        neck = (f"M{f(X(-8))},{f(by - 22 * u)} Q{f(X(-14))},{f(by - 52 * u)} {f(X(0))},{f(by - 66 * u)} "
                f"L{f(X(12))},{f(by - 62 * u)} Q{f(X(2))},{f(by - 48 * u)} {f(X(8))},{f(by - 22 * u)}Z")
        hx, hy, R = X(4), by - 70 * u, 26 * u
        fan = (f"M{f(hx)},{f(hy + 6 * u)} L{f(hx - R)},{f(hy - R * 0.2)} A{f(R)},{f(R)} 0 0 1 {f(hx + R)},{f(hy - R * 0.2)}Z")
        heads = ""
        for k in range(5):
            a = math.radians(-64 + k * 32)
            px, py = hx + math.sin(a) * R * 1.02, hy - math.cos(a) * R * 1.02
            heads += (f'<ellipse cx="{f(px)}" cy="{f(py)}" rx="{f(5.5 * u)}" ry="{f(9 * u)}" transform="rotate({f(math.degrees(a))} {f(px)} {f(py)})"/>')
        return (f'<g filter="url(#{self.filt("gilt")})" fill="url(#{g})">{coils}<path d="{neck}"/><path d="{fan}"/>{heads}</g>'
                f'<path d="M{f(hx - R * 0.55)},{f(hy - R * 0.1)} Q{f(hx)},{f(hy - R * 0.55)} {f(hx + R * 0.55)},{f(hy - R * 0.1)}" fill="none" stroke="#6e440a" stroke-width="{f(1.5 * u)}"/>'
                f'<path d="M{f(X(-30))},{f(by - 8 * u)} Q{f(X(-6))},{f(by - 14 * u)} {f(X(22))},{f(by - 8 * u)}" fill="none" stroke="#6e440a" stroke-width="{f(1.2 * u)}" stroke-dasharray="{f(3 * u)} {f(3 * u)}"/>')

    def vines(self, x, y, s, seed, side=1):
        c = self.carving
        return (f'<g filter="url(#{c.filt("leaf")})">' + c.leaf_cluster(x, y, s, seed, 3)
                + c.vine(x + side * s * 0.2, y + s * 0.3, s * 3.2, s * 0.8, seed + 1, 1, side) + "</g>")

    # ------------------------------------------------------------ brick
    def bricks(self, d, small=False, mud=False):
        """A brick shape (path d), patterned, weathered and lit."""
        pat = self.filt("mud" if mud else "bricks")
        return f'<g filter="url(#{self.filt("rough_small" if small else "rough")})"><path d="{d}" fill="url(#{pat})" fill-rule="evenodd"/></g>'

    def panel(self, x, y, w, h, big=False):
        """A brick frame round dark mudbrick, gold filigree at its corners and lip; a large panel
        gets stupas on its top corners and a crest at the middle, singhas guarding its foot, vines."""
        thin = h < 120
        t = 10 if thin else (16 if not big else 20)
        tb = t + (0 if thin else 6)
        out = [f'<rect x="{f(x)}" y="{f(y)}" width="{f(w)}" height="{f(h)}" fill="#4a1c12" filter="url(#{self.filt("panel")})"/>',
               f'<rect x="{f(x)}" y="{f(y)}" width="{f(w)}" height="{f(h)}" fill="url(#{self.filt("mud")})" opacity="0.35"/>']
        ring = (f"M{f(x - t)},{f(y - tb)} H{f(x + w + t)} V{f(y + h + t)} H{f(x - t)}Z M{f(x)},{f(y)} V{f(y + h)} H{f(x + w)} V{f(y)}Z")
        out.append(self.bricks(ring))
        g = self.filt("gold")
        out.append(f'<g filter="url(#{self.filt("gilt")})"><rect x="{f(x - 2)}" y="{f(y - 2)}" width="{f(w + 4)}" height="{f(h + 4)}" fill="none" stroke="url(#{g})" stroke-width="{3 if thin else 4}"/></g>')
        if thin:
            for cx, cy in ((x - t / 2, y - tb / 2), (x + w + t / 2, y - tb / 2), (x - t / 2, y + h + t / 2), (x + w + t / 2, y + h + t / 2)):
                out.append(self.lotus(cx, cy + t * 0.45, t * 0.95))
            return "".join(out)
        # Gilt flame scrolls on the corners, on the brick.
        k = t * 2.2
        out.append(self.kanok(x + t * 0.2, y + t * 1.6, k, 1, 1) + self.kanok(x + w - t * 0.2, y + t * 1.6, k, -1, 1)
                   + self.kanok(x + t * 0.2, y + h - t * 0.4, k, 1, 1) + self.kanok(x + w - t * 0.2, y + h - t * 0.4, k, -1, 1))
        out.append(self.lotus(x + w / 2, y + h + t * 0.9, t * 1.5))
        if big:
            # Brick piers at the top corners, each crowned by a stupa; a crest at the top's middle.
            for px in (x - t, x + w + t - t * 2.4):
                out.append(self.bricks(f"M{f(px - t * 0.3)},{f(y + t)} V{f(y - tb - t * 1.2)} H{f(px + t * 2.7)} V{f(y + t)}Z"))
                out.append(self.stupa(px + t * 1.2, y - tb - t * 1.1, t * 3.2))
            cx = x + w / 2
            out.append(self.bricks(f"M{f(cx - t * 3)},{f(y - tb + 2)} Q{f(cx - t * 1.5)},{f(y - tb - t * 1.4)} {f(cx)},{f(y - tb - t * 1.8)} "
                                   f"Q{f(cx + t * 1.5)},{f(y - tb - t * 1.4)} {f(cx + t * 3)},{f(y - tb + 2)}Z"))
            out.append(self.stupa(cx, y - tb - t * 0.6, t * 3.6) + self.kanok(cx - t * 1.4, y - tb - t * 0.4, t * 2.6, -1, 1)
                       + self.kanok(cx + t * 1.4, y - tb - t * 0.4, t * 2.6, 1, 1))
            ns = min(w * 0.36, t * 9)
            out.append(self.naga(x + t * 0.5, y + h + t * 0.75, ns, 1) + self.naga(x + w - t * 0.5, y + h + t * 0.75, ns, -1))
            out.append(self.vines(x + t * 2.6, y - tb + 2, t * 0.9, 3, 1) + self.vines(x + w - t * 2.6, y - tb + 2, t * 0.9, 7, -1))
        return "".join(out)

    # ------------------------------------------------------------ an icon tile
    def tile(self, x, y, w, glyph_markup, state="idle", small=False, crest=None):
        """A palette tile: a brick frame w wide at (x, y), a gold arch inside its top, the
        panel of dark mudbrick, three gilt spires above (crest high) unless small, a gold lotus at the foot."""
        h = w * (1.0 if small else 1.04)
        t = max(3.5, w * (0.09 if small else 0.085))
        panel = {"idle": "#4a1c12", "hover": "#5a2416", "pressed": "#3a120a", "disabled": "#3e2a24"}[state]
        out = [f'<rect x="{f(x + t * 0.6)}" y="{f(y + t * 0.6)}" width="{f(w - t * 1.2)}" height="{f(h - t * 1.2)}" fill="{panel}" filter="url(#{self.filt("panel")})"/>']
        ring = (f"M{f(x)},{f(y)} H{f(x + w)} V{f(y + h)} H{f(x)}Z M{f(x + t)},{f(y + t)} V{f(y + h - t)} H{f(x + w - t)} V{f(y + t)}Z")
        out.append(self.bricks(ring, small=small))
        g = self.filt("gold")
        if small:
            out.append(f'<rect x="{f(x + t - 0.6)}" y="{f(y + t - 0.6)}" width="{f(w - 2 * t + 1.2)}" height="{f(h - 2 * t + 1.2)}" fill="none" stroke="url(#{g})" stroke-width="{f(max(1, w * 0.035))}"/>')
        else:
            # The gold arch: a scalloped lintel under the top of the frame.
            ax, aw, ay = x + t, w - 2 * t, y + t
            n = 5
            sc = "".join(f" Q{f(ax + aw * (k + 0.5) / n)},{f(ay + aw * 0.1)} {f(ax + aw * (k + 1) / n)},{f(ay)}" for k in range(n))
            arch = f"M{f(ax)},{f(ay)}{sc} V{f(ay - 1)} H{f(ax)}Z"
            out.append(f'<g filter="url(#{self.filt("gilt")})"><path d="{arch}" fill="url(#{g})"/>'
                       f'<rect x="{f(x + t - 1)}" y="{f(y + t - 1)}" width="{f(w - 2 * t + 2)}" height="{f(h - 2 * t + 2)}" fill="none" stroke="url(#{g})" stroke-width="{f(w * 0.025)}"/></g>')
            out.append(self.lotus(x + w / 2, y + h + t * 0.25, t * 1.6))
            if crest:
                out.append(self.stupa(x + w / 2, y + t * 0.4, crest * 1.25) + self.stupa(x + t * 0.5, y + t * 0.3, crest * 0.7)
                           + self.stupa(x + w - t * 0.5, y + t * 0.3, crest * 0.7))
                out.append(self.vines(x + t * 0.8, y + t * 0.4, t * 0.85, int(x) % 13, 1))
        frame = "".join(out)
        if state == "hover":
            frame = f'<g filter="url(#{self.filt("glow")})">{frame}</g>'
        elif state == "disabled":
            frame = f'<g filter="url(#{self.filt("grey")})">{frame}</g>'
        if state == "pressed":
            frame += f'<rect x="{f(x + t + 2)}" y="{f(y + t + 2)}" width="{f(w - 2 * t - 4)}" height="{f(h - 2 * t - 4)}" fill="none" stroke="#ffd36a" stroke-width="{f(max(1, w * 0.02))}" opacity="0.8"/>'
        return frame + glyph_markup, h
