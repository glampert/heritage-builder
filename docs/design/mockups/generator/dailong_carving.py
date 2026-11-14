"""Đại Long's richer look, after the user's references of 2026-11-14: parchment
inside frames of carved red lacquered wood and gilt bamboo, lashed at the corners;
tiles crowned with a small curved roof and a gilt lotus; large panels under
upturned eaves with red lanterns, a gilt cloud-scroll medallion over the top, gilt
key-pattern corners and a gilt lotus at the foot. The icons stay seal stamps in
vermilion ink. Used by palette.py for the build palette's tiles and the HUD's panels.

Everything is SVG: grain, bevels and light come from turbulence and lighting filters.
"""
import math

from palette_glyphs import f

GOLD_STOPS = [(0, "#fbe08a"), (0.35, "#e7b13e"), (0.6, "#b77d1c"), (1, "#7a4c0e")]


class Woodwork:
    def __init__(self, defs, uid):
        self.defs = defs
        self.uid = uid
        self.ids = {}

    # ------------------------------------------------------------ filters and paints
    def filt(self, name):
        if name in self.ids:
            return self.ids[name]
        fid = self.uid(f"d{name}")
        self.ids[name] = fid
        if name in ("wood", "wood_small"):
            # Lacquered wood: grain running along, a bevel, a warm sheen.
            bevel, scale = (2.6, 7.0) if name == "wood" else (1.2, 3.5)
            self.defs.append(
                f'<filter id="{fid}" x="-10%" y="-10%" width="120%" height="120%" color-interpolation-filters="sRGB">'
                f'<feTurbulence type="fractalNoise" baseFrequency="0.012 0.22" numOctaves="3" seed="4" result="n"/>'
                f'<feColorMatrix in="n" type="matrix" values="0.55 0 0 0 0.62  0.55 0 0 0 0.6  0.55 0 0 0 0.58  0 0 0 0 1" result="grain"/>'
                f'<feComposite in="SourceGraphic" in2="grain" operator="arithmetic" k1="1.05" result="g"/>'
                f'<feGaussianBlur in="SourceAlpha" stdDeviation="{bevel}" result="b"/>'
                f'<feDiffuseLighting in="b" surfaceScale="{scale}" diffuseConstant="1.3" lighting-color="#ffffff" result="d"><feDistantLight azimuth="235" elevation="40"/></feDiffuseLighting>'
                f'<feSpecularLighting in="b" surfaceScale="{scale}" specularConstant="0.7" specularExponent="18" lighting-color="#ffd9a0" result="s"><feDistantLight azimuth="235" elevation="40"/></feSpecularLighting>'
                f'<feComposite in="d" in2="g" operator="arithmetic" k1="1.35" result="lit"/>'
                f'<feComposite in="s" in2="SourceAlpha" operator="in" result="si"/>'
                f'<feComposite in="lit" in2="si" operator="arithmetic" k2="1" k3="0.5" result="all"/>'
                f'<feComposite in="all" in2="SourceAlpha" operator="in" result="body"/>'
                f'<feOffset in="b" dx="1.5" dy="2.5" result="o"/><feFlood flood-color="#120603" flood-opacity="0.7"/><feComposite in2="o" operator="in" result="sh"/>'
                f'<feMerge><feMergeNode in="sh"/><feMergeNode in="body"/></feMerge></filter>')
        elif name == "gilt":
            # Gilt carving: a bevel and a bright shine over the gold.
            self.defs.append(
                f'<filter id="{fid}" x="-15%" y="-15%" width="130%" height="130%" color-interpolation-filters="sRGB">'
                f'<feGaussianBlur in="SourceAlpha" stdDeviation="1.1" result="b"/>'
                f'<feDiffuseLighting in="b" surfaceScale="4" diffuseConstant="1.2" lighting-color="#ffffff" result="d"><feDistantLight azimuth="235" elevation="45"/></feDiffuseLighting>'
                f'<feSpecularLighting in="b" surfaceScale="4" specularConstant="1.0" specularExponent="20" lighting-color="#fff4c8" result="s"><feDistantLight azimuth="235" elevation="45"/></feSpecularLighting>'
                f'<feComposite in="d" in2="SourceGraphic" operator="arithmetic" k1="1.3" result="lit"/>'
                f'<feComposite in="s" in2="SourceAlpha" operator="in" result="si"/>'
                f'<feComposite in="lit" in2="si" operator="arithmetic" k2="1" k3="0.8" result="all"/>'
                f'<feComposite in="all" in2="SourceAlpha" operator="in" result="body"/>'
                f'<feOffset in="b" dx="1" dy="1.6" result="o"/><feFlood flood-color="#1a0a02" flood-opacity="0.65"/><feComposite in2="o" operator="in" result="sh"/>'
                f'<feMerge><feMergeNode in="sh"/><feMergeNode in="body"/></feMerge></filter>')
        elif name == "paper":
            # Parchment: warm, blotched, darker at the edges.
            self.defs.append(
                f'<filter id="{fid}" x="0" y="0" width="100%" height="100%" color-interpolation-filters="sRGB">'
                f'<feTurbulence type="fractalNoise" baseFrequency="0.035" numOctaves="4" seed="8" result="n"/>'
                f'<feColorMatrix in="n" type="matrix" values="0 0 0 0 0.6  0 0 0 0 0.38  0 0 0 0 0.14  0.9 0 0 0 -0.45" result="blot"/>'
                f'<feTurbulence type="fractalNoise" baseFrequency="0.7" numOctaves="2" seed="2" result="f"/>'
                f'<feColorMatrix in="f" type="matrix" values="0 0 0 0 0.5  0 0 0 0 0.35  0 0 0 0 0.2  0.5 0 0 0 -0.2" result="fib"/>'
                f'<feMerge result="m"><feMergeNode in="SourceGraphic"/><feMergeNode in="blot"/><feMergeNode in="fib"/></feMerge>'
                f'<feComposite in="m" in2="SourceAlpha" operator="in"/></filter>')
        elif name == "glow":
            self.defs.append(
                f'<filter id="{fid}" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur in="SourceAlpha" stdDeviation="4" result="b"/>'
                f'<feFlood flood-color="#ffcf5a" flood-opacity="1"/><feComposite in2="b" operator="in" result="g"/>'
                f'<feMerge><feMergeNode in="g"/><feMergeNode in="SourceGraphic"/></feMerge></filter>')
        elif name == "grey":
            self.defs.append(f'<filter id="{fid}"><feColorMatrix type="saturate" values="0.2"/>'
                             f'<feComponentTransfer><feFuncR type="linear" slope="0.8"/><feFuncG type="linear" slope="0.8"/><feFuncB type="linear" slope="0.8"/></feComponentTransfer></filter>')
        elif name == "lantern_glow":
            self.defs.append(f'<radialGradient id="{fid}" cx="0.4" cy="0.4" r="0.7"><stop offset="0" stop-color="#ffb070"/>'
                             f'<stop offset="0.45" stop-color="#e2321c"/><stop offset="1" stop-color="#7a0e08"/></radialGradient>')
        elif name.startswith("bamboo"):
            # Gilt bamboo, lit across its round: vertical or horizontal.
            x2, y2 = ("1", "0") if name == "bamboo_v" else ("0", "1")
            stops = [(0, "#7a4c10"), (0.18, "#e9b443"), (0.35, "#fde58e"), (0.6, "#d79a2a"), (1, "#6e420c")]
            self.defs.append(f'<linearGradient id="{fid}" x1="0" y1="0" x2="{x2}" y2="{y2}">'
                             + "".join(f'<stop offset="{o}" stop-color="{c}"/>' for o, c in stops) + "</linearGradient>")
        elif name == "goldgrad":
            self.defs.append(f'<linearGradient id="{fid}" x1="0" y1="0" x2="0.3" y2="1">'
                             + "".join(f'<stop offset="{o}" stop-color="{c}"/>' for o, c in GOLD_STOPS) + "</linearGradient>")
        return fid

    # ------------------------------------------------------------ parts
    def bamboo(self, x, y, length, th, vertical, nodes=None):
        """A gilt bamboo rod from (x, y), `th` thick, with node rings."""
        g = self.filt("bamboo_v" if vertical else "bamboo_h")
        if vertical:
            out = [f'<rect x="{f(x)}" y="{f(y)}" width="{f(th)}" height="{f(length)}" rx="{f(th * 0.45)}" fill="url(#{g})" stroke="#4a2c06" stroke-width="{f(max(0.5, th * 0.07))}"/>']
        else:
            out = [f'<rect x="{f(x)}" y="{f(y)}" width="{f(length)}" height="{f(th)}" rx="{f(th * 0.45)}" fill="url(#{g})" stroke="#4a2c06" stroke-width="{f(max(0.5, th * 0.07))}"/>']
        n = nodes if nodes is not None else max(1, int(length / (th * 4.5)))
        for k in range(1, n + 1):
            t = k / (n + 1)
            if vertical:
                ny = y + length * t
                out.append(f'<rect x="{f(x - th * 0.06)}" y="{f(ny - th * 0.12)}" width="{f(th * 1.12)}" height="{f(th * 0.24)}" rx="{f(th * 0.1)}" fill="#8a5a12" stroke="#4a2c06" stroke-width="{f(max(0.4, th * 0.05))}"/>'
                           f'<line x1="{f(x + th * 0.15)}" y1="{f(ny - th * 0.2)}" x2="{f(x + th * 0.85)}" y2="{f(ny - th * 0.2)}" stroke="#fff2b8" stroke-width="{f(max(0.4, th * 0.08))}" opacity="0.7"/>')
            else:
                nx = x + length * t
                out.append(f'<rect x="{f(nx - th * 0.12)}" y="{f(y - th * 0.06)}" width="{f(th * 0.24)}" height="{f(th * 1.12)}" rx="{f(th * 0.1)}" fill="#8a5a12" stroke="#4a2c06" stroke-width="{f(max(0.4, th * 0.05))}"/>'
                           f'<line x1="{f(nx - th * 0.2)}" y1="{f(y + th * 0.15)}" x2="{f(nx - th * 0.2)}" y2="{f(y + th * 0.85)}" stroke="#fff2b8" stroke-width="{f(max(0.4, th * 0.08))}" opacity="0.7"/>')
        return "".join(out)

    def lashing(self, cx, cy, s):
        """Cord lashed in an X where two rods cross."""
        return (f'<g stroke="#5a3608" stroke-width="{f(s * 0.34)}" stroke-linecap="round">'
                f'<line x1="{f(cx - s)}" y1="{f(cy - s)}" x2="{f(cx + s)}" y2="{f(cy + s)}"/><line x1="{f(cx + s)}" y1="{f(cy - s)}" x2="{f(cx - s)}" y2="{f(cy + s)}"/></g>'
                f'<g stroke="#f4cf6a" stroke-width="{f(s * 0.22)}" stroke-linecap="round">'
                f'<line x1="{f(cx - s)}" y1="{f(cy - s)}" x2="{f(cx + s)}" y2="{f(cy + s)}"/><line x1="{f(cx + s)}" y1="{f(cy - s)}" x2="{f(cx - s)}" y2="{f(cy + s)}"/></g>')

    def bamboo_frame(self, x, y, w, h, th):
        """Four rods, their ends running past the corners, lashed where they cross."""
        o = th * 0.6
        out = [self.bamboo(x - o, y, w + 2 * o, th, False), self.bamboo(x - o, y + h - th, w + 2 * o, th, False),
               self.bamboo(x, y - o, h + 2 * o, th, True), self.bamboo(x + w - th, y - o, h + 2 * o, th, True)]
        for cx, cy in ((x + th / 2, y + th / 2), (x + w - th / 2, y + th / 2), (x + th / 2, y + h - th / 2), (x + w - th / 2, y + h - th / 2)):
            out.append(self.lashing(cx, cy, th * 0.62))
        return "".join(out)

    def roof(self, cx, y, w, h, gilt_top=True):
        """A small curved roof whose eaves span w at y, rising h: red tiles, a gilt ridge
        ornament (a lotus between two cloud curls) and upturned ends."""
        xl, xr = cx - w / 2, cx + w / 2
        d = (f"M{f(xl - w * 0.06)},{f(y - h * 0.55)} Q{f(xl)},{f(y + h * 0.02)} {f(xl + w * 0.12)},{f(y)} H{f(xr - w * 0.12)} "
             f"Q{f(xr)},{f(y + h * 0.02)} {f(xr + w * 0.06)},{f(y - h * 0.55)} Q{f(cx + w * 0.34)},{f(y - h * 0.62)} {f(cx + w * 0.3)},{f(y - h)} "
             f"H{f(cx - w * 0.3)} Q{f(cx - w * 0.34)},{f(y - h * 0.62)} {f(xl - w * 0.06)},{f(y - h * 0.55)}Z")
        rows = "".join(f'<path d="M{f(xl + w * 0.08 + k * w * 0.01)},{f(y - h * (0.2 + 0.25 * k))} H{f(xr - w * 0.08 - k * w * 0.01)}" stroke="#4a1208" stroke-width="{f(max(0.5, h * 0.05))}" opacity="0.6"/>'
                       for k in range(3))
        ribs = "".join(f'<path d="M{f(cx + (k - 6) * w * 0.07)},{f(y - h * 0.95)} L{f(cx + (k - 6) * w * 0.08)},{f(y - h * 0.05)}" stroke="#5a1a0c" stroke-width="{f(max(0.4, h * 0.035))}" opacity="0.45"/>'
                       for k in range(13))
        ridge = (f'<rect x="{f(cx - w * 0.33)}" y="{f(y - h * 1.08)}" width="{f(w * 0.66)}" height="{f(h * 0.16)}" rx="{f(h * 0.06)}" fill="#7c2a14"/>')
        out = [f'<g filter="url(#{self.filt("wood")})"><path d="{d}" fill="#a3361d"/>{ridge}</g>', rows, ribs]
        # The eave board under the tiles, gilt.
        out.append(f'<g filter="url(#{self.filt("gilt")})"><rect x="{f(xl + w * 0.06)}" y="{f(y - h * 0.05)}" width="{f(w * 0.88)}" height="{f(h * 0.14)}" rx="{f(h * 0.05)}" fill="url(#{self.filt("goldgrad")})"/>')
        if gilt_top:
            out.append(self.lotus(cx, y - h * 1.02, h * 0.55) + self.cloud(cx - h * 0.55, y - h * 1.0, h * 0.42, -1) + self.cloud(cx + h * 0.55, y - h * 1.0, h * 0.42, 1))
        out.append("</g>")
        return "".join(out)

    def lotus(self, cx, by, s):
        """A gilt lotus standing on (cx, by), s high."""
        g = self.filt("goldgrad")
        d = (f"M{f(cx)},{f(by)} C{f(cx - s * 0.25)},{f(by - s * 0.4)} {f(cx - s * 0.18)},{f(by - s * 0.8)} {f(cx)},{f(by - s)} "
             f"C{f(cx + s * 0.18)},{f(by - s * 0.8)} {f(cx + s * 0.25)},{f(by - s * 0.4)} {f(cx)},{f(by)}Z "
             f"M{f(cx - s * 0.05)},{f(by)} C{f(cx - s * 0.5)},{f(by - s * 0.1)} {f(cx - s * 0.62)},{f(by - s * 0.45)} {f(cx - s * 0.6)},{f(by - s * 0.65)} "
             f"C{f(cx - s * 0.35)},{f(by - s * 0.55)} {f(cx - s * 0.15)},{f(by - s * 0.35)} {f(cx - s * 0.05)},{f(by)}Z "
             f"M{f(cx + s * 0.05)},{f(by)} C{f(cx + s * 0.5)},{f(by - s * 0.1)} {f(cx + s * 0.62)},{f(by - s * 0.45)} {f(cx + s * 0.6)},{f(by - s * 0.65)} "
             f"C{f(cx + s * 0.35)},{f(by - s * 0.55)} {f(cx + s * 0.15)},{f(by - s * 0.35)} {f(cx + s * 0.05)},{f(by)}Z")
        return f'<path d="{d}" fill="url(#{g})" stroke="#5a3606" stroke-width="{f(max(0.4, s * 0.04))}"/>'

    def cloud(self, x, y, s, side):
        """A gilt cloud curl lying along y, s long, curling outwards on `side`."""
        g = self.filt("goldgrad")
        pts = []
        for k in range(36):
            t = k / 35
            a = t * 2.2 * math.pi
            r = s * 0.42 * (1 - 0.7 * t)
            pts.append((x + side * (s * 0.6 * t + r * math.sin(a) * 0.6), y - r * (1 - math.cos(a)) * 0.6))
        d = "M" + " L".join(f"{f(a_)},{f(b_)}" for a_, b_ in pts)
        return (f'<path d="M{f(x)},{f(y)} q{f(side * s * 0.3)},{f(-s * 0.2)} {f(side * s * 0.6)},0" fill="none" stroke="url(#{g})" stroke-width="{f(max(1.2, s * 0.09))}" stroke-linecap="round"/>'
                f'<path d="{d}" fill="none" stroke="url(#{g})" stroke-width="{f(max(1.0, s * 0.075))}" stroke-linecap="round"/>')

    def fret(self, x, y, s, sx, sy):
        """A gilt key-pattern corner at (x, y), its arms running sx, sy."""
        g = self.filt("goldgrad")
        u = s / 6
        d = (f"M{f(x)},{f(y + sy * 6 * u)} V{f(y)} H{f(x + sx * 6 * u)} M{f(x + sx * 2 * u)},{f(y + sy * 4 * u)} V{f(y + sy * 2 * u)} "
             f"H{f(x + sx * 4 * u)} V{f(y + sy * 3.2 * u)}")
        return f'<path d="{d}" fill="none" stroke="url(#{g})" stroke-width="{f(max(1, u * 0.9))}" stroke-linecap="square"/>'

    def lantern(self, x, y, s):
        """A red lantern hanging from (x, y), s tall with its tassel."""
        lg = self.filt("lantern_glow")
        return (f'<line x1="{f(x)}" y1="{f(y)}" x2="{f(x)}" y2="{f(y + s * 0.14)}" stroke="#3a1a08" stroke-width="{f(s * 0.02)}"/>'
                f'<rect x="{f(x - s * 0.12)}" y="{f(y + s * 0.13)}" width="{f(s * 0.24)}" height="{f(s * 0.05)}" fill="#d8a23a"/>'
                f'<ellipse cx="{f(x)}" cy="{f(y + s * 0.4)}" rx="{f(s * 0.24)}" ry="{f(s * 0.23)}" fill="url(#{lg})" stroke="#5a0c06" stroke-width="{f(s * 0.012)}"/>'
                + "".join(f'<path d="M{f(x + k * s * 0.08)},{f(y + s * 0.18)} Q{f(x + k * s * 0.13)},{f(y + s * 0.4)} {f(x + k * s * 0.08)},{f(y + s * 0.62)}" fill="none" stroke="#8a1a0e" stroke-width="{f(s * 0.01)}" opacity="0.7"/>'
                          for k in (-2, -1, 1, 2))
                + f'<rect x="{f(x - s * 0.12)}" y="{f(y + s * 0.61)}" width="{f(s * 0.24)}" height="{f(s * 0.05)}" fill="#d8a23a"/>'
                f'<path d="M{f(x - s * 0.04)},{f(y + s * 0.66)} L{f(x - s * 0.05)},{f(y + s)} L{f(x + s * 0.05)},{f(y + s)} L{f(x + s * 0.04)},{f(y + s * 0.66)}Z" fill="#c4231a"/>'
                f'<circle cx="{f(x - s * 0.09)}" cy="{f(y + s * 0.34)}" r="{f(s * 0.03)}" fill="#ffd9a0" opacity="0.8"/>')

    def medallion(self, cx, cy, r):
        """The crest over a large panel: a wooden half-disc with a gilt gate, cloud scrolls either side."""
        g = self.filt("goldgrad")
        disc = f'<path d="M{f(cx - r)},{f(cy)} A{f(r)},{f(r)} 0 0 1 {f(cx + r)},{f(cy)}Z" fill="#8c2c16"/>'
        gate = (f'<rect x="{f(cx - r * 0.42)}" y="{f(cy - r * 0.42)}" width="{f(r * 0.84)}" height="{f(r * 0.42)}" fill="url(#{g})"/>'
                f'<path d="M{f(cx - r * 0.12)},{f(cy)} V{f(cy - r * 0.2)} A{f(r * 0.12)},{f(r * 0.12)} 0 0 1 {f(cx + r * 0.12)},{f(cy - r * 0.2)} V{f(cy)}Z" fill="#5a1a0c"/>'
                f'<path d="M{f(cx - r * 0.6)},{f(cy - r * 0.4)} Q{f(cx)},{f(cy - r * 0.62)} {f(cx + r * 0.6)},{f(cy - r * 0.4)} L{f(cx + r * 0.42)},{f(cy - r * 0.36)} H{f(cx - r * 0.42)}Z" fill="url(#{g})"/>'
                f'<rect x="{f(cx - r * 0.24)}" y="{f(cy - r * 0.74)}" width="{f(r * 0.48)}" height="{f(r * 0.2)}" fill="url(#{g})"/>'
                f'<path d="M{f(cx - r * 0.4)},{f(cy - r * 0.72)} Q{f(cx)},{f(cy - r * 0.9)} {f(cx + r * 0.4)},{f(cy - r * 0.72)}Z" fill="url(#{g})"/>')
        return (f'<g filter="url(#{self.filt("wood")})">{disc}</g>'
                f'<g filter="url(#{self.filt("gilt")})">{gate}'
                f'<path d="M{f(cx - r)},{f(cy)} A{f(r)},{f(r)} 0 0 1 {f(cx + r)},{f(cy)}" fill="none" stroke="url(#{g})" stroke-width="{f(r * 0.08)}"/>'
                + self.cloud(cx - r * 1.05, cy - r * 0.15, r * 1.9, -1) + self.cloud(cx + r * 1.05, cy - r * 0.15, r * 1.9, 1) + "</g>")

    # ------------------------------------------------------------ a panel
    def panel(self, x, y, w, h, big=False, lanterns=None):
        """A carved wooden frame round parchment: gilt trim inside, key-pattern corners,
        and for a large panel eaves over its top corners, lanterns and a medallion."""
        t = 18 if h >= 120 else 10
        out = []
        wood = (f"M{f(x - t)},{f(y - t)} H{f(x + w + t)} V{f(y + h + t)} H{f(x - t)}Z "
                f"M{f(x)},{f(y)} V{f(y + h)} H{f(x + w)} V{f(y)}Z")
        out.append(f'<rect x="{f(x)}" y="{f(y)}" width="{f(w)}" height="{f(h)}" fill="#efd7a4" filter="url(#{self.filt("paper")})"/>')
        ig = self.uid("dig")
        self.defs.append(f'<radialGradient id="{ig}" cx="0.5" cy="0.5" r="0.75"><stop offset="0.6" stop-color="#5a2a08" stop-opacity="0"/>'
                         f'<stop offset="1" stop-color="#5a2a08" stop-opacity="0.35"/></radialGradient>')
        out.append(f'<rect x="{f(x)}" y="{f(y)}" width="{f(w)}" height="{f(h)}" fill="url(#{ig})"/>')
        out.append(f'<g filter="url(#{self.filt("wood")})"><path d="{wood}" fill="#86301a" fill-rule="evenodd"/>'
                   + "".join(f'<rect x="{f(cx - t * 0.75)}" y="{f(cy - t * 0.75)}" width="{f(t * 1.5)}" height="{f(t * 1.5)}" rx="{f(t * 0.2)}" fill="#7a2614"/>'
                             for cx, cy in ((x - t / 2, y - t / 2), (x + w + t / 2, y - t / 2), (x - t / 2, y + h + t / 2), (x + w + t / 2, y + h + t / 2)))
                   + "</g>")
        out.append(f'<rect x="{f(x - t * 0.5)}" y="{f(y - t * 0.5)}" width="{f(w + t)}" height="{f(h + t)}" fill="none" stroke="#3a0e04" stroke-width="{f(t * 0.12)}" opacity="0.55"/>'
                   f'<rect x="{f(x - t * 0.5 + 1)}" y="{f(y - t * 0.5 + 1)}" width="{f(w + t)}" height="{f(h + t)}" fill="none" stroke="#e8885a" stroke-width="{f(t * 0.06)}" opacity="0.35"/>')
        out.append(f'<g filter="url(#{self.filt("gilt")})">' + "".join(
            f'<circle cx="{f(cx)}" cy="{f(cy)}" r="{f(t * 0.3)}" fill="url(#{self.filt("goldgrad")})"/>'
            for cx, cy in ((x - t / 2, y - t / 2), (x + w + t / 2, y - t / 2), (x - t / 2, y + h + t / 2), (x + w + t / 2, y + h + t / 2))) + "</g>")
        g = self.filt("goldgrad")
        inset = 3 if t < 12 else 5
        out.append(f'<rect x="{f(x + inset)}" y="{f(y + inset)}" width="{f(w - 2 * inset)}" height="{f(h - 2 * inset)}" fill="none" stroke="url(#{g})" stroke-width="{2 if t < 12 else 3}"/>')
        fs = 12 if t < 12 else 22
        if h >= 60:
            fi = inset + 6
            out.append(f'<g filter="url(#{self.filt("gilt")})">'
                       + self.fret(x + fi, y + fi, fs, 1, 1) + self.fret(x + w - fi, y + fi, fs, -1, 1)
                       + self.fret(x + fi, y + h - fi, fs, 1, -1) + self.fret(x + w - fi, y + h - fi, fs, -1, -1) + "</g>")
        if big:
            ew = min(150, w * 0.32)
            out.append(self.roof(x - t + ew * 0.42, y - t * 0.2, ew, ew * 0.26, gilt_top=False))
            out.append(self.roof(x + w + t - ew * 0.42, y - t * 0.2, ew, ew * 0.26, gilt_top=False))
            out.append(self.medallion(x + w / 2, y - t * 0.3, min(44, w * 0.09)))
            out.append(f'<g filter="url(#{self.filt("gilt")})">' + self.lotus(x + w / 2, y + h + t * 0.9, 26)
                       + self.cloud(x + w / 2 - 18, y + h + t * 0.5, 40, -1) + self.cloud(x + w / 2 + 18, y + h + t * 0.5, 40, 1) + "</g>")
            if lanterns:
                ls = lanterns
                out.append(self.lantern(x - t - 2, y + 4, ls) + self.lantern(x + w + t + 2, y + 4, ls))
        return "".join(out)

    # ------------------------------------------------------------ an icon tile
    def tile(self, x, y, w, ink_markup, state="idle", small=False, crest=None):
        """A palette tile w wide at (x, y): parchment in a gilt bamboo frame, under a small roof
        (crest high, above y) unless small. `ink_markup` is the stamped icon, already drawn."""
        h = w * (1.0 if small else 1.04)
        th = max(3.2, w * (0.085 if small else 0.09))
        paper = {"idle": "#f0d9a6", "hover": "#f8e6bc", "pressed": "#e8c88c", "disabled": "#d9c7a6"}[state]
        body = [f'<rect x="{f(x + th * 0.5)}" y="{f(y + th * 0.5)}" width="{f(w - th)}" height="{f(h - th)}" fill="{paper}" filter="url(#{self.filt("paper")})"/>']
        if not small:
            body.append(f'<rect x="{f(x + th)}" y="{f(y + th)}" width="{f(w - 2 * th)}" height="{f(h - 2 * th)}" fill="none" stroke="#7a3c10" stroke-width="0.8" opacity="0.5"/>')
        body.append(self.bamboo_frame(x, y, w, h, th))
        if crest:
            body.append(self.roof(x + w / 2, y + th * 0.4, w * 0.98, crest))
        frame = "".join(body)
        if state == "hover":
            frame = f'<g filter="url(#{self.filt("glow")})">{frame}</g>'
        elif state == "disabled":
            frame = f'<g filter="url(#{self.filt("grey")})">{frame}</g>'
        return frame + ink_markup, h
