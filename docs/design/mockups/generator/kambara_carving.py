"""Kambara's carved sandstone, after the user's references of 2026-11-14 (a temple
frame of weathered blocks, corner volutes, a Bayon face crowning the top, moss
and hanging vines, a dark cracked panel inside, and reliefs lit from the top
left). Used by palette.py for the build palette's tiles and the HUD's panels.

Everything is SVG: the stone's grain, bevels and light come from turbulence and
lighting filters, so a frame stays sharp at any size.
"""
import math
import random

from palette_glyphs import f


class Carving:
    def __init__(self, defs, uid):
        self.defs = defs
        self.uid = uid
        self.ids = {}

    # ------------------------------------------------------------ filters
    def filt(self, name):
        if name in self.ids:
            return self.ids[name]
        fid = self.uid(f"k{name}")
        self.ids[name] = fid
        if name in ("stone", "relief", "relief_sunk", "stone_small"):
            # Carved sandstone: the shape's bevel plus a grain, lit from the top left
            # (the sunk relief from the bottom right), with a warm highlight and a shadow.
            bevel, grain, freq, scale, az = {
                "stone": (2.4, 0.5, 0.13, 9.0, 225), "stone_small": (1.0, 0.18, 0.2, 3.0, 225),
                "relief": (1.2, 0.22, 0.2, 8.0, 225), "relief_sunk": (1.3, 0.16, 0.16, 5.0, 45)}[name]
            sh = 0.8 if name != "relief_sunk" else 0.0
            self.defs.append(
                f'<filter id="{fid}" x="-15%" y="-15%" width="130%" height="130%" color-interpolation-filters="sRGB">'
                f'<feTurbulence type="fractalNoise" baseFrequency="{freq}" numOctaves="4" seed="5" result="n"/>'
                f'<feColorMatrix in="n" type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  1 0 0 0 0" result="na"/>'
                f'<feGaussianBlur in="SourceAlpha" stdDeviation="{bevel}" result="b"/>'
                f'<feComposite in="b" in2="na" operator="arithmetic" k2="1" k3="{grain}" result="h"/>'
                f'<feDiffuseLighting in="h" surfaceScale="{scale}" diffuseConstant="1.25" lighting-color="#ffffff" result="d">'
                f'<feDistantLight azimuth="{az}" elevation="34"/></feDiffuseLighting>'
                f'<feSpecularLighting in="h" surfaceScale="{scale}" specularConstant="0.6" specularExponent="12" lighting-color="#ffe2b0" result="s">'
                f'<feDistantLight azimuth="{az}" elevation="42"/></feSpecularLighting>'
                f'<feComposite in="d" in2="SourceGraphic" operator="arithmetic" k1="1.12" result="lit"/>'
                f'<feComposite in="s" in2="SourceAlpha" operator="in" result="si"/>'
                f'<feComposite in="lit" in2="si" operator="arithmetic" k2="1" k3="0.4" result="all"/>'
                f'<feTurbulence type="fractalNoise" baseFrequency="{freq * 0.35}" numOctaves="3" seed="21" result="mt"/>'
                f'<feColorMatrix in="mt" type="matrix" values="0.9 0 0 0 0.42  0.9 0 0 0 0.4  0.9 0 0 0 0.36  0 0 0 0 1" result="mott"/>'
                f'<feComposite in="all" in2="mott" operator="arithmetic" k1="1.15" result="allm"/>'
                f'<feComposite in="allm" in2="SourceAlpha" operator="in" result="body"/>'
                f'<feOffset in="b" dx="{1.6 if sh else 0}" dy="{2.2 if sh else 0}" result="o"/>'
                f'<feFlood flood-color="#0c0905" flood-opacity="{sh}"/><feComposite in2="o" operator="in" result="shadow"/>'
                f'<feMerge><feMergeNode in="shadow"/><feMergeNode in="body"/></feMerge></filter>')
        elif name == "panel":
            # The panel's dark stone: mottled, pitted, its edges shaded by the frame.
            self.defs.append(
                f'<filter id="{fid}" x="0" y="0" width="100%" height="100%" color-interpolation-filters="sRGB">'
                f'<feTurbulence type="fractalNoise" baseFrequency="0.035" numOctaves="4" seed="9" result="n"/>'
                f'<feColorMatrix in="n" type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  1 0 0 0 0" result="na"/>'
                f'<feDiffuseLighting in="na" surfaceScale="2.4" diffuseConstant="1.15" lighting-color="#ffffff" result="d">'
                f'<feDistantLight azimuth="225" elevation="55"/></feDiffuseLighting>'
                f'<feComposite in="d" in2="SourceGraphic" operator="arithmetic" k1="1.1" result="lit"/>'
                f'<feTurbulence type="fractalNoise" baseFrequency="0.012" numOctaves="2" seed="4" result="m"/>'
                f'<feColorMatrix in="m" type="matrix" values="0 0 0 0 0.16  0 0 0 0 0.22  0 0 0 0 0.06  1.6 0 0 0 -0.62" result="moss"/>'
                f'<feComposite in="moss" in2="lit" operator="over" result="mm"/>'
                f'<feComposite in="mm" in2="SourceAlpha" operator="in"/></filter>')
        elif name == "moss":
            # Moss in the frame's joints and on its top: speckled green where noise runs high.
            self.defs.append(
                f'<filter id="{fid}" x="0" y="0" width="100%" height="100%" color-interpolation-filters="sRGB">'
                f'<feTurbulence type="fractalNoise" baseFrequency="0.045" numOctaves="3" seed="12" result="n"/>'
                f'<feColorMatrix in="n" type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  6 0 0 0 -3.1" result="blob"/>'
                f'<feTurbulence type="fractalNoise" baseFrequency="0.5" numOctaves="2" seed="13" result="fine"/>'
                f'<feColorMatrix in="fine" type="matrix" values="0 0 0 0 0.3  0.4 0 0 0 0.26  0 0 0 0 0.08  0 0 0 0 1" result="g"/>'
                f'<feComposite in="g" in2="blob" operator="in" result="gb"/>'
                f'<feComposite in="gb" in2="SourceAlpha" operator="in"/></filter>')
        elif name == "leaf":
            self.defs.append(
                f'<filter id="{fid}" x="-30%" y="-30%" width="160%" height="160%">'
                f'<feGaussianBlur in="SourceAlpha" stdDeviation="1.1"/><feOffset dx="1.4" dy="2" result="o"/>'
                f'<feFlood flood-color="#0b0904" flood-opacity="0.65"/><feComposite in2="o" operator="in"/>'
                f'<feMerge><feMergeNode/><feMergeNode in="SourceGraphic"/></feMerge></filter>')
        elif name == "glow":
            self.defs.append(
                f'<filter id="{fid}" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur in="SourceAlpha" stdDeviation="4" result="b"/>'
                f'<feFlood flood-color="#ffc864" flood-opacity="0.95"/><feComposite in2="b" operator="in" result="g"/>'
                f'<feMerge><feMergeNode in="g"/><feMergeNode in="SourceGraphic"/></feMerge></filter>')
        elif name == "grey":
            self.defs.append(f'<filter id="{fid}"><feColorMatrix type="saturate" values="0.15"/>'
                             f'<feComponentTransfer><feFuncR type="linear" slope="0.72"/><feFuncG type="linear" slope="0.72"/><feFuncB type="linear" slope="0.72"/></feComponentTransfer></filter>')
        elif name == "leafgrad":
            self.defs.append(f'<linearGradient id="{fid}" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#b9d45c"/>'
                             f'<stop offset="0.55" stop-color="#7fa236"/><stop offset="1" stop-color="#4a6c1f"/></linearGradient>')
        elif name == "glyphgrad":
            self.defs.append(f'<linearGradient id="{fid}" x1="0" y1="0" x2="0.3" y2="1"><stop offset="0" stop-color="#e2c79c"/>'
                             f'<stop offset="1" stop-color="#a5845a"/></linearGradient>')
        return fid

    # ------------------------------------------------------------ parts
    def volute(self, x, y, r, sx=1, sy=1):
        """A spiral scroll ending at (x, y), curling inwards; sx, sy mirror it."""
        pts = []
        for k in range(60):
            t = k / 59
            a = t * 3.2 * math.pi
            rr = r * (1 - 0.82 * t)
            pts.append((x + sx * (rr * math.cos(a) - r), y + sy * (rr * math.sin(a))))
        d = "M" + " L".join(f"{f(px)},{f(py)}" for px, py in pts)
        return f'<path d="{d}" fill="none" stroke="#000" stroke-width="{f(r * 0.36)}" stroke-linecap="round"/>'

    def face(self, cx, by, s):
        """A Bayon face under a lotus crown, its chin at (cx, by), s high in all.
        Returns the solid shapes and the cuts (closed eyes, nose, smile, the crown's lines)."""
        u = s / 100

        def petal(px, py, pw, ph):
            return (f"M{f(px - pw / 2)},{f(py)} C{f(px - pw / 2)},{f(py - ph * 0.6)} {f(px - pw * 0.1)},{f(py - ph * 0.8)} {f(px)},{f(py - ph)} "
                    f"C{f(px + pw * 0.1)},{f(py - ph * 0.8)} {f(px + pw / 2)},{f(py - ph * 0.6)} {f(px + pw / 2)},{f(py)}Z ")
        crown = ""
        for k in range(-2, 3):
            crown += petal(cx + k * 11 * u, by - 60 * u, 13 * u, 16 * u)
        for k in range(-1, 2):
            crown += petal(cx + k * 11 * u, by - 72 * u, 14 * u, 17 * u)
        crown += petal(cx, by - 85 * u, 15 * u, 19 * u)
        head = (f"M{f(cx - 24 * u)},{f(by - 54 * u)} C{f(cx - 26 * u)},{f(by - 20 * u)} {f(cx - 14 * u)},{f(by)} {f(cx)},{f(by)} "
                f"C{f(cx + 14 * u)},{f(by)} {f(cx + 26 * u)},{f(by - 20 * u)} {f(cx + 24 * u)},{f(by - 54 * u)}Z")
        ears = (f"M{f(cx - 23 * u)},{f(by - 46 * u)} c{f(-9 * u)},{f(2 * u)} {f(-9 * u)},{f(22 * u)} {f(-3 * u)},{f(26 * u)} l{f(4 * u)},{f(-6 * u)}Z "
                f"M{f(cx + 23 * u)},{f(by - 46 * u)} c{f(9 * u)},{f(2 * u)} {f(9 * u)},{f(22 * u)} {f(3 * u)},{f(26 * u)} l{f(-4 * u)},{f(-6 * u)}Z")
        band = f'<rect x="{f(cx - 28 * u)}" y="{f(by - 62 * u)}" width="{f(56 * u)}" height="{f(9 * u)}" rx="{f(3 * u)}"/>'
        sw = max(0.9, 3.2 * u)
        cuts = (f"M{f(cx - 16 * u)},{f(by - 37 * u)} q{f(6 * u)},{f(4 * u)} {f(11 * u)},0 M{f(cx + 5 * u)},{f(by - 37 * u)} q{f(6 * u)},{f(4 * u)} {f(11 * u)},0 "
                f"M{f(cx - 17 * u)},{f(by - 43 * u)} q{f(7 * u)},{f(-3 * u)} {f(13 * u)},0 M{f(cx + 4 * u)},{f(by - 43 * u)} q{f(7 * u)},{f(-3 * u)} {f(13 * u)},0 "
                f"M{f(cx - 1 * u)},{f(by - 36 * u)} l{f(-3 * u)},{f(13 * u)} q{f(4 * u)},{f(3 * u)} {f(8 * u)},0 "
                f"M{f(cx - 9 * u)},{f(by - 13 * u)} q{f(9 * u)},{f(6 * u)} {f(18 * u)},0 "
                f"M{f(cx - 26 * u)},{f(by - 58 * u)} H{f(cx + 26 * u)}")
        return (f'<path d="{crown}"/>{band}<path d="{ears}"/><path d="{head}"/>',
                f'<path d="{cuts}" fill="none" stroke="#000" stroke-width="{f(sw)}" stroke-linecap="round" stroke-linejoin="round"/>')

    def leaf(self, x, y, s, a):
        """A heart-shaped leaf hanging from (x, y), s long, turned a degrees."""
        g = self.filt("leafgrad")
        d = (f"M0,0 C{f(-0.5 * s)},{f(0.1 * s)} {f(-0.55 * s)},{f(0.65 * s)} 0,{f(s)} "
             f"C{f(0.55 * s)},{f(0.65 * s)} {f(0.5 * s)},{f(0.1 * s)} 0,0Z")
        return (f'<g transform="translate({f(x)},{f(y)}) rotate({f(a)})"><path d="{d}" fill="url(#{g})" stroke="#33461a" stroke-width="{f(max(0.4, s * 0.04))}"/>'
                f'<path d="M0,{f(s * 0.05)} Q{f(s * 0.06)},{f(s * 0.5)} 0,{f(s * 0.92)}" fill="none" stroke="#3d5a18" stroke-width="{f(max(0.4, s * 0.05))}"/></g>')

    def vine(self, x, y, length, s, seed, down=1, side=1):
        """A vine from (x, y), drooping, with leaves along it."""
        rnd = random.Random(seed)
        pts = [(x, y)]
        for k in range(1, 6):
            t = k / 5
            pts.append((x + side * length * 0.25 * math.sin(t * 2.4) + rnd.uniform(-1, 1) * s * 0.15, y + down * length * t))
        d = f"M{f(pts[0][0])},{f(pts[0][1])}" + "".join(
            f" Q{f((pts[k - 1][0] + pts[k][0]) / 2 + side * s * 0.3)},{f((pts[k - 1][1] + pts[k][1]) / 2)} {f(pts[k][0])},{f(pts[k][1])}" for k in range(1, len(pts)))
        out = [f'<path d="{d}" fill="none" stroke="#4e6a22" stroke-width="{f(max(0.8, s * 0.09))}" stroke-linecap="round"/>']
        for k, (px, py) in enumerate(pts[1:]):
            ang = (side * rnd.uniform(25, 70)) * (1 if k % 2 else -1) + (0 if down > 0 else 180)
            out.append(self.leaf(px, py, s * rnd.uniform(0.75, 1.1), ang))
        return "".join(out)

    def leaf_cluster(self, x, y, s, seed, n=4):
        rnd = random.Random(seed)
        return "".join(self.leaf(x + rnd.uniform(-s * 0.5, s * 0.5), y + rnd.uniform(-s * 0.3, s * 0.3), s * rnd.uniform(0.75, 1.15), rnd.uniform(-160, 160)) for _ in range(n))

    def cracks(self, x, y, w, h, seed, n=3):
        rnd = random.Random(seed)
        out = []
        for _ in range(n):
            px, py = x + rnd.uniform(0.05, 0.95) * w, y + rnd.uniform(0.05, 0.95) * h
            a = rnd.uniform(0, math.pi * 2)
            pts = [(px, py)]
            for _ in range(rnd.randint(4, 8)):
                a += rnd.uniform(-0.7, 0.7)
                step = rnd.uniform(0.04, 0.09) * max(w, h)
                px, py = min(x + w, max(x, px + math.cos(a) * step)), min(y + h, max(y, py + math.sin(a) * step))
                pts.append((px, py))
            d = "M" + " L".join(f"{f(a_)},{f(b_)}" for a_, b_ in pts)
            out.append(f'<path d="{d}" fill="none" stroke="#c9b08a" stroke-width="0.8" opacity="0.18" transform="translate(0.8,0.8)"/>'
                       f'<path d="{d}" fill="none" stroke="#0f0b06" stroke-width="0.9" opacity="0.75"/>')
        return "".join(out)

    # ------------------------------------------------------------ a frame
    def frame(self, x, y, w, h, t=None, crest=None, seed=1, panel="#3a3125", vines=True, small=False, sunk=False):
        """A carved frame around (x, y, w, h): the panel inside, blocks round it, volutes
        at the corners, an optional crest (its height) over the top's middle."""
        t = t or max(5, min(w, h) * 0.085)
        r = t * 1.5
        stone = self.filt("stone_small" if small else "stone")
        out = []
        # The panel, its shading under the frame.
        out.append(f'<rect x="{f(x + t * 0.5)}" y="{f(y + t * 0.5)}" width="{f(w - t)}" height="{f(h - t)}" rx="{f(t * 0.4)}" fill="{panel}" filter="url(#{self.filt("panel")})"/>')
        if not small:
            out.append(self.cracks(x + t, y + t, w - 2 * t, h - 2 * t, seed, 2 if w < 160 else 5))
        ig = self.uid("kig")
        self.defs.append(f'<linearGradient id="{ig}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#000" stop-opacity="{0.7 if sunk else 0.5}"/>'
                         f'<stop offset="0.25" stop-color="#000" stop-opacity="0"/></linearGradient>')
        out.append(f'<rect x="{f(x + t)}" y="{f(y + t)}" width="{f(w - 2 * t)}" height="{f(h - 2 * t)}" fill="url(#{ig})"/>')
        # The ring of blocks: cut into stones by joints.
        ring = (f"M{f(x)},{f(y + r)} Q{f(x)},{f(y)} {f(x + r)},{f(y)} H{f(x + w - r)} Q{f(x + w)},{f(y)} {f(x + w)},{f(y + r)} V{f(y + h - r)} "
                f"Q{f(x + w)},{f(y + h)} {f(x + w - r)},{f(y + h)} H{f(x + r)} Q{f(x)},{f(y + h)} {f(x)},{f(y + h - r)}Z "
                f"M{f(x + t)},{f(y + t)} V{f(y + h - t)} H{f(x + w - t)} V{f(y + t)}Z")
        mid = self.uid("kmask")
        joints = []
        blk = max(t * 2.6, 18 if not small else 10)
        n = max(2, round((w - 2 * r) / blk))
        for k in range(1, n):
            jx = x + r + (w - 2 * r) * k / n
            joints.append(f"M{f(jx)},{f(y - 1)} V{f(y + t + 1)} M{f(jx + blk * 0.3)},{f(y + h - t - 1)} V{f(y + h + 1)}")
        m = max(2, round((h - 2 * r) / blk))
        for k in range(1, m):
            jy = y + r + (h - 2 * r) * k / m
            joints.append(f"M{f(x - 1)},{f(jy)} H{f(x + t + 1)} M{f(x + w - t - 1)},{f(jy + blk * 0.3)} H{f(x + w + 1)}")
        self.defs.append(f'<mask id="{mid}" maskUnits="userSpaceOnUse" x="{f(x - 20)}" y="{f(y - 20)}" width="{f(w + 40)}" height="{f(h + 40)}">'
                         f'<rect x="{f(x - 20)}" y="{f(y - 20)}" width="{f(w + 40)}" height="{f(h + 40)}" fill="#fff"/>'
                         f'<path d="{" ".join(joints)}" stroke="#000" stroke-width="{f(max(1.0, t * 0.16))}"/></mask>')
        corner = []
        if not small:
            vr = t * 1.15
            for (cx, cy, sx, sy) in ((x + t * 0.45, y + t * 0.45, 1, 1), (x + w - t * 0.45, y + t * 0.45, -1, 1),
                                     (x + t * 0.45, y + h - t * 0.45, 1, -1), (x + w - t * 0.45, y + h - t * 0.45, -1, -1)):
                corner.append(self.volute(cx + sx * vr * 0.0, cy, vr, -sx, sy))
        out.append(f'<g filter="url(#{stone})"><g mask="url(#{mid})"><path d="{ring}" fill="#7c5f3a" fill-rule="evenodd"/></g>'
                   + "".join(c.replace('stroke="#000"', 'stroke="#8c6e44"') for c in corner) + "</g>")
        # Moss over the frame's top and corners.
        if not small:
            out.append(f'<path d="{ring}" fill-rule="evenodd" fill="#000" filter="url(#{self.filt("moss")})" opacity="0.7"/>')
        # The crest: a face over the top, volutes either side, on a plinth.
        if crest:
            cx = x + w / 2
            ch = crest
            pw = ch * 0.95
            base = f'<rect x="{f(cx - pw / 2)}" y="{f(y - ch * 0.12)}" width="{f(pw)}" height="{f(ch * 0.22)}" rx="{f(ch * 0.04)}"/>'
            solid, cut = self.face(cx, y - ch * 0.08, ch)
            vol = (self.volute(cx - ch * 0.42, y - ch * 0.2, ch * 0.2, -1, 1) + self.volute(cx + ch * 0.42, y - ch * 0.2, ch * 0.2, 1, 1))
            fm = self.uid("kfm")
            self.defs.append(f'<mask id="{fm}" maskUnits="userSpaceOnUse" x="{f(cx - ch)}" y="{f(y - ch * 1.2)}" width="{f(ch * 2)}" height="{f(ch * 1.5)}">'
                             f'<rect x="{f(cx - ch)}" y="{f(y - ch * 1.2)}" width="{f(ch * 2)}" height="{f(ch * 1.5)}" fill="#fff"/>{cut}</mask>')
            out.append(f'<g filter="url(#{stone})"><g mask="url(#{fm})" fill="#8e7148">{base}{solid}</g>'
                       + vol.replace('stroke="#000"', 'stroke="#8c6e44"') + "</g>")
            if vines:
                out.append(f'<g filter="url(#{self.filt("leaf")})">' + self.leaf_cluster(cx - ch * 0.55, y - ch * 0.05, ch * 0.24, seed + 3, 2)
                           + self.leaf_cluster(cx + ch * 0.55, y - ch * 0.05, ch * 0.24, seed + 4, 2)
                           + self.vine(cx - ch * 0.62, y + t * 0.4, min(h * 0.12, ch * 0.45), ch * 0.2, seed + 5, 1, -1) + "</g>")
        if vines and not small:
            s = max(7, min(t * 1.5, 22))
            out.append(f'<g filter="url(#{self.filt("leaf")})">'
                       + self.leaf_cluster(x + t * 1.0, y + t * 0.4, s, seed + 7, 3)
                       + self.vine(x + t * 0.35, y + t, min(h * 0.4, s * 4), s * 0.8, seed + 8, 1, -1)
                       + self.leaf_cluster(x + w - t * 1.0, y + t * 0.4, s, seed + 9, 2)
                       + self.leaf_cluster(x + t * 0.6, y + h - t * 0.4, s * 0.9, seed + 11, 2)
                       + self.leaf_cluster(x + w - t * 0.7, y + h - t * 0.3, s * 0.9, seed + 12, 2) + "</g>")
        return "".join(out)

    # ------------------------------------------------------------ an icon tile
    def tile(self, x, y, w, mask_url, state="idle", small=False, seed=1, crest=-1):
        """A palette tile: its frame body w wide at (x, y), the crest above y when not small."""
        h = w * (1.08 if not small else 1.0)
        if crest == -1:
            crest = None if small else w * 0.36
        sunk = state == "pressed"
        panel = {"idle": "#3a3125", "hover": "#43392a", "pressed": "#2a231a", "disabled": "#36302a"}[state]
        out = []
        body = self.frame(x, y, w, h, t=w * (0.085 if not small else 0.1), crest=crest, seed=seed, panel=panel,
                          vines=not small, small=small, sunk=sunk)
        if state == "hover":
            out.append(f'<g filter="url(#{self.filt("glow")})">{body}</g>')
        elif state == "disabled":
            out.append(f'<g filter="url(#{self.filt("grey")})">{body}</g>')
        else:
            out.append(body)
        if state == "pressed":
            out.append(f'<rect x="{f(x + w * 0.11)}" y="{f(y + w * 0.11)}" width="{f(w * 0.78)}" height="{f(h - w * 0.22)}" fill="none" stroke="#e0a454" stroke-width="{f(max(1, w * 0.02))}" opacity="0.8"/>')
        fill = {"idle": f"url(#{self.filt('glyphgrad')})", "hover": "#f0d9ae", "pressed": "#e0b67a", "disabled": "#7a6e5c"}[state]
        filt = self.filt("relief_sunk" if sunk else ("stone_small" if small else "relief"))
        out.append(f'<g filter="url(#{filt})"><rect x="{f(x)}" y="{f(y)}" width="{f(w)}" height="{f(h)}" fill="{fill}" mask="{mask_url}"/></g>')
        return "".join(out), h
