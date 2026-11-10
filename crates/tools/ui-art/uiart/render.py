"""Draws catalog sprites with the mockup generator and shoots them with Playwright's Chromium.

The generator (docs/design/mockups/generator/) draws each realm's materials as SVG. A sprite
is one call of a realm's drawing method (CALLS) at (pad, pad, width, height) on a page with
room around it; the page is shot at the catalog's scale on a transparent background, then
cropped to the sprite (the rect plus its bleed) and saved. All text the generator would draw
is stripped (its T() helper draws nothing), so no words end up in the art.

For a 9-slice whose middle tiles ("center": "tile"), the drawing is made to repeat cleanly:
noise and the paper's rough edge repeat with the middle's size and line up with its borders
(stitched feTurbulence and feTile), the paper's vignette is left out, horizontal gradients
become their middle colour and Tampadesa's gilt gradient repeats along the diagonal.

Importing this module needs only the standard library; drawing needs the generator's imports
(Pillow, numpy) and rendering needs Pillow, Node and Playwright.
"""

import hashlib
import io
import json
import math
import os
import pathlib
import re
import subprocess
import sys
import tempfile

from . import catalog as cat

GENERATOR_DIR = os.path.join(cat.REPO_ROOT, "docs", "design", "mockups", "generator")
RENDER_JS = os.path.join(cat.HERE, "render.js")
# Bump when the drawing or post-processing changes in a way the SVG doesn't show, so every
# sprite renders again.
RENDERER_VERSION = 1
# Alpha at or below this, outside a sprite's crop, doesn't count as a spill (faint shadow edges).
SPILL_ALPHA = 3
# A tiled middle holds at least this many periods of its noise's lowest octave.
MIN_TILE_PERIODS = 5
# In a tiled sprite, noise layers with a base frequency below BLOTCH_FREQ (features larger than
# about 10 points: the paper's stains, the stone's mottling, moss) keep this share of their
# contrast about their middle tone. Full-strength blotches repeat as a visible lattice on a large
# panel, and make a step where the engine cuts the last tile; the fine grain hides both.
BLOTCH_FREQ = 0.1
TILE_BLOTCH_CONTRAST = 0.45

REALM_CLASSES = {"dailong": ("DaiLongDialogs", "DAILONG"), "kambara": ("KambaraDialogs", "KAMBARA"),
                 "tampadesa": ("TampadesaDialogs", "TAMPADESA")}


# ----------------------------------------------------------------------------
# The generator
# ----------------------------------------------------------------------------
_GEN = None


def load_generator():
    """Imports the generator once, with its text helper stripped. The generator reads
    build/fonts.css at import; sprites have no text, so a missing file reads as empty."""
    global _GEN
    if _GEN is not None:
        return _GEN
    sys.dont_write_bytecode = True
    if GENERATOR_DIR not in sys.path:
        sys.path.insert(0, GENERATOR_DIR)
    fonts_css = pathlib.Path(GENERATOR_DIR) / "build" / "fonts.css"
    original = pathlib.Path.read_text
    if not fonts_css.exists():
        def read_text(self, *a, **k):
            return "" if self == fonts_css else original(self, *a, **k)
        pathlib.Path.read_text = read_text
    try:
        import world_map, screens, menus, pages, dialogs  # noqa: E401
        import dialogs_content
    finally:
        pathlib.Path.read_text = original

    def no_text(*a, **k):
        return ""
    for m in (world_map, screens, menus, pages, dialogs):
        m.T = no_text
    _GEN = (dialogs, dialogs_content)
    return _GEN


def theme(realm):
    """A fresh realm theme: every drawing method of the generator, for one sprite (themes keep
    their defs and id counter, so each sprite gets its own)."""
    dialogs, content = load_generator()
    cls_name, data_name = REALM_CLASSES[realm]
    return getattr(dialogs, cls_name)(getattr(content, data_name))


# ----------------------------------------------------------------------------
# Calls: the rect (x, y, w, h) and args to one generator method
# ----------------------------------------------------------------------------
def _at(x, y, w, h, a):
    ax, ay = a.get("at", [w / 2, h / 2])
    return x + ax, y + ay


def _with_lines(th, n, draw):
    # caption() and speech() size themselves from their wrapped text; text is stripped, so
    # wrap to n empty lines for the height wanted.
    th.wrap = lambda *a, **k: [""] * n
    return draw()


CALLS = {
    # Panels.
    "mpanel": lambda th, x, y, w, h, a: th.mpanel(x, y, w, h, crest=a.get("crest", False)),
    "box": lambda th, x, y, w, h, a: th.box(x, y, w, h, big=a.get("big", False)),
    "bar_bg": lambda th, x, y, w, h, a: th.bar_bg(x, y, w, h),
    "panel_bg": lambda th, x, y, w, h, a: th.panel_bg(x, y, w, h),
    "card": lambda th, x, y, w, h, a: th.card(x, y, w, h),
    "row_bg": lambda th, x, y, w, h, a: th.row_bg(x, y, w, h, a.get("selected", False)),
    # 48 high, centred on the rect.
    "text_input": lambda th, x, y, w, h, a: th.text_input(x, y + h / 2, w, "", a.get("focus", False)),
    "caption": lambda th, x, y, w, h, a: _with_lines(th, a.get("lines", 1), lambda: th.caption(x, y, w, "")),
    # The tail's tip, relative to the rect's top left.
    "speech": lambda th, x, y, w, h, a: _with_lines(
        th, a.get("lines", 1), lambda: th.speech(x, y, w, "", "", (x + a["tip"][0], y + a["tip"][1]))),
    # Buttons and markers.
    "button": lambda th, x, y, w, h, a: th.button(x, y, w, h, "", a.get("primary", False)),
    "tab": lambda th, x, y, w, h, a: th.tab(x, y, w, "", a.get("on", False)),       # 50 high
    "item_marker": lambda th, x, y, w, h, a: th.item_marker(*_at(x, y, w, h, a)),
    "icon_button": lambda th, x, y, w, h, a: th.icon_button(*_at(x, y, w, h, a), a.get("icon", "house"), a.get("on", False)),
    "build_button": lambda th, x, y, w, h, a: th.build_button(*_at(x, y, w, h, a), a.get("icon", "house"), a.get("hover", False)),
    # Controls.
    "checkbox": lambda th, x, y, w, h, a: th.checkbox(x, y + h / 2, a.get("on", False)),   # 28 x 28 at the left
    "toggle": lambda th, x, y, w, h, a: th.toggle(x + w, y + h / 2, a.get("on", False)),   # 62 x 30 at the right
    "slider": lambda th, x, y, w, h, a: th.slider(x + w, y + h / 2, w, a.get("v", 0), ""),
    "dropdown": lambda th, x, y, w, h, a: th.dropdown(x + w, y + h / 2, w, "", True),      # 44 high
    # Separators and emblems.
    "divider": lambda th, x, y, w, h, a: th.divider(x, y + h / 2, w),
    "realm_emblem": lambda th, x, y, w, h, a: th.realm_emblem(
        a["key"], *_at(x, y, w, h, a), a.get("r", 40), a.get("locked", False), a.get("current", False)),
}


# ----------------------------------------------------------------------------
# SVG helpers (pure)
# ----------------------------------------------------------------------------
_TAG = re.compile(r"<(/?)([A-Za-z][\w:.-]*)((?:[^>\"']|\"[^\"]*\"|'[^']*')*?)(/?)>")


def split_top_level(svg):
    """The top-level elements of an SVG fragment, in order."""
    out, depth, start = [], 0, None
    for m in _TAG.finditer(svg):
        closing, selfclosing = m.group(1) == "/", m.group(4) == "/"
        if closing:
            depth -= 1
            if depth < 0:
                raise ValueError("unbalanced SVG fragment")
            if depth == 0:
                out.append(svg[start:m.end()])
        elif selfclosing:
            if depth == 0:
                out.append(m.group(0))
        else:
            if depth == 0:
                start = m.start()
            depth += 1
    if depth:
        raise ValueError("unbalanced SVG fragment")
    return out


def _union(a, b):
    x0, y0 = min(a[0], b[0]), min(a[1], b[1])
    x1, y1 = max(a[0] + a[2], b[0] + b[2]), max(a[1] + a[3], b[1] + b[3])
    return x0, y0, x1 - x0, y1 - y0


def make_tileable(th, middle, page):
    """Overrides the theme's material helpers so what they draw repeats with the 9-slice's
    middle (x, y, w, h, in page points) and lines up with its borders.

    Chromium's stitchTiles makes turbulence seamless only for a tile at the filter space's
    origin, so the noise is made there, moved onto the middle with feOffset and repeated
    from it with feTile."""
    mx, my, mw, mh = middle
    tile = (f'stitchTiles="stitch" x="0" y="0" width="{mw:g}" height="{mh:g}"/>'
            f'%s<feOffset dx="{mx:g}" dy="{my:g}" x="{mx:g}" y="{my:g}" width="{mw:g}" height="{mh:g}"/>'
            f'<feTile')

    def noise(x, y, w, h, freq, octaves, rgb, ak, ab, seed=1, extra="", typ="fractalNoise"):
        fid = th.nid("nz")
        r, g, b = rgb
        ak, ab = tile_contrast(freq, ak, ab)
        freq = tile_frequency(freq, mw, mh)
        fx, fy, fw, fh = _union(_union((x, y, w, h), middle), (0, 0, mw, mh))
        matrix = f'<feColorMatrix type="matrix" values="0 0 0 0 {r} 0 0 0 0 {g} 0 0 0 0 {b} {ak} 0 0 0 {ab}"/>'
        th.defs.append(
            f'<filter id="{fid}" filterUnits="userSpaceOnUse" x="{fx:g}" y="{fy:g}" width="{fw:g}" height="{fh:g}">'
            f'<feTurbulence type="{typ}" baseFrequency="{freq}" numOctaves="{octaves}" seed="{seed}" {tile % matrix}/>'
            f'<feComposite in2="SourceGraphic" operator="in"/></filter>')
        return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" filter="url(#{fid})" {extra}/>'

    def rough(scale=2.2, freq=0.05):
        fid = th.nid("rough")
        th.defs.append(
            f'<filter id="{fid}" filterUnits="userSpaceOnUse" x="0" y="0" width="{page[0]:g}" height="{page[1]:g}">'
            f'<feTurbulence type="fractalNoise" baseFrequency="{freq}" numOctaves="2" seed="4" {tile % ""} result="t"/>'
            f'<feDisplacementMap in="SourceGraphic" in2="t" scale="{scale}" xChannelSelector="R" yChannelSelector="G"/></filter>')
        return f"url(#{fid})"

    original_lin = th.lin

    def lin(stops, x1=0, y1=0, x2=0, y2=1):
        if y1 == y2 and x1 != x2:
            # Across the whole panel, it would jump at every repeat of a top or bottom edge.
            return min(stops, key=lambda s: abs(s[0] - 0.5))[1]
        return original_lin(stops, x1, y1, x2, y2)

    th.noise = noise
    th.rough = rough
    th.lin = lin
    th.rad = lambda *a, **k: "none"     # A vignette can't repeat.
    if hasattr(th, "gold"):
        # The gilt runs along the diagonal, repeating with a period that divides the middle's
        # width and height, so the edges repeat cleanly. Its stops are the generator's own.
        original_gold = th.gold
        before = len(th.defs)
        original_gold()
        stops = "".join(re.findall(r"<stop [^>]*/>", "".join(th.defs[before:])))
        period = _gcd_points(mw, mh)
        gid = th.nid("gold")
        th.defs.append(f'<linearGradient id="{gid}" gradientUnits="userSpaceOnUse" spreadMethod="repeat" '
                       f'x1="{mx:g}" y1="{my:g}" x2="{mx + period / 2:g}" y2="{my + period / 2:g}">{stops}</linearGradient>')
        th.gold = lambda: f"url(#{gid})"


def tile_frequency(freq, mw, mh, periods=MIN_TILE_PERIODS):
    """A noise frequency ("f" or "fx fy") raised so a tile holds at least `periods` of its
    lowest octave: one large blotch per tile repeats as a visible grid."""
    parts = [float(v) for v in str(freq).split()]
    fx, fy = (parts[0], parts[0]) if len(parts) == 1 else parts
    fx, fy = max(fx, periods / mw), max(fy, periods / mh)
    return f"{fx:.5g}" if len(parts) == 1 and fx == fy else f"{fx:.5g} {fy:.5g}"


def tile_contrast(freq, ak, ab, k=TILE_BLOTCH_CONTRAST):
    """A noise layer's alpha (ak x noise + ab) with its contrast scaled by k about its value at
    the noise's middle (0.5), when the layer is blotches rather than grain."""
    if min(float(v) for v in str(freq).split()) >= BLOTCH_FREQ:
        return ak, ab
    return round(ak * k, 4), round(ab + 0.5 * ak * (1 - k), 4)


def _gcd_points(a, b):
    """The gilt's period for a tiled middle of a x b points: their greatest common divisor (in
    half points, so 0.5 pt sizes work), so it divides both and repeats cleanly. A period under
    16 pt would show as stripes, so sides that share no such factor are a catalog error, unless
    the middle is square (its side is the period)."""
    g = math.gcd(int(round(a * 2)), int(round(b * 2))) / 2
    if g < 16 and a != b:
        raise cat.CatalogError(f"a tiled middle of {a:g} x {b:g} pt: its sides share no factor of 16 pt or more, "
                               "so the gilt can't repeat across it; change the size or the slice")
    return g


# ----------------------------------------------------------------------------
# One sprite's page
# ----------------------------------------------------------------------------
def layout(rec):
    """(page size, rect origin, crop) in points: the call draws its rect at (pad, pad) on a
    page with pad all round; the sprite is the rect plus its bleed. A rotated sprite's page is
    square, and its crop is the turned rect plus its bleed."""
    pad = rec["pad"]
    w, h = rec["size"]
    bl, bt, br, bb = rec["bleed"]
    if rec.get("rotate"):
        side = max(w, h) + 2 * pad
        x, y = (side - w) / 2, (side - h) / 2
        cx, cy = side / 2, side / 2
        return (side, side), (x, y), (cx - h / 2 - bl, cy - w / 2 - bt, h + bl + br, w + bt + bb)
    page = (w + 2 * pad, h + 2 * pad)
    crop = (pad - bl, pad - bt, w + bl + br, h + bt + bb)
    return page, (pad, pad), crop


def middle(rec):
    """The 9-slice's middle in page points."""
    _, _, crop = layout(rec)
    sl, st, sr, sb = (v / rec["scale"] for v in rec["slice"])
    return crop[0] + sl, crop[1] + st, crop[2] - sl - sr, crop[3] - st - sb


def sprite_svg(rec, state):
    """The SVG page of one sprite in one state, and its source hash."""
    th = theme(rec["realm"])
    page, (x, y), crop = layout(rec)
    for k, v in rec["attrs"].items():
        setattr(th, k, v)
    if rec["center"] == "tile":
        make_tileable(th, middle(rec), page)
    for name in rec["omit"]:
        setattr(th, name, lambda *a, **k: "")
    captured = None
    cap = rec["capture"]
    if cap:
        captured, helper = [], getattr(th, cap["helper"])

        def capture(*a, **k):
            captured.append(helper(*a, **k))
            return ""
        setattr(th, cap["helper"], capture)
    w, h = rec["size"]
    body = CALLS[state["call"]](th, x, y, w, h, state["args"])
    if cap:
        if cap.get("index", 0) >= len(captured):
            raise cat.CatalogError(f"{rec['realm']}/{rec['id']}: {cap['helper']} drew {len(captured)} times")
        body = captured[cap.get("index", 0)]
    if rec["elements"]:
        parts = split_top_level(body)
        if max(rec["elements"]) >= len(parts):
            raise cat.CatalogError(f"{rec['realm']}/{rec['id']}: the call drew {len(parts)} elements, "
                                   f"not {max(rec['elements']) + 1}")
        body = "".join(parts[i] for i in rec["elements"])
    if rec.get("rotate"):
        body = f'<g transform="rotate({rec["rotate"]} {x + w / 2:g} {y + h / 2:g})">{body}</g>'
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" '
           f'width="{page[0]:g}" height="{page[1]:g}" viewBox="0 0 {page[0]:g} {page[1]:g}">'
           f'<defs>{"".join(th.defs)}</defs>{body}</svg>')
    source = json.dumps({"renderer": RENDERER_VERSION, "svg": svg, "css": state["css"], "page": page,
                         "crop": crop, "scale": rec["scale"]}, sort_keys=True)
    return svg, hashlib.sha256(source.encode()).hexdigest()[:16]


# ----------------------------------------------------------------------------
# Shooting
# ----------------------------------------------------------------------------
def shoot(jobs, scale):
    """Shoots each job's page; jobs are dicts with "svg", "css" (a CSS filter) and "page" (its
    size in points). Returns the PNG bytes of each whole page, in order, from one browser."""
    if not jobs:
        return []
    with tempfile.TemporaryDirectory(prefix="ui-art-") as tmp:
        shots = [{"svg": j["svg"], "css": j["css"], "w": j["page"][0], "h": j["page"][1],
                  "out": os.path.join(tmp, f"s{i}.png")} for i, j in enumerate(jobs)]
        vw = max(int(math.ceil(j["page"][0])) for j in jobs)
        vh = max(int(math.ceil(j["page"][1])) for j in jobs)
        job_path = os.path.join(tmp, "job.json")
        with open(job_path, "w", encoding="utf-8") as f:
            json.dump({"viewport": [vw, vh], "scale": scale, "shots": shots}, f)
        try:
            subprocess.run(["node", RENDER_JS, job_path], check=True, cwd=cat.REPO_ROOT)
        except FileNotFoundError:
            raise SystemExit("make_ui_art.py needs Node.js (node) with Playwright; see the README")
        except subprocess.CalledProcessError as e:
            raise SystemExit(f"render.js failed ({e.returncode}); see its output above")
        out = []
        for s in shots:
            with open(s["out"], "rb") as f:
                out.append(f.read())
        return out


def _pil():
    try:
        from PIL import Image
    except ImportError:
        raise SystemExit("make_ui_art.py needs Pillow to render: pip install Pillow")
    return Image


def finish(rec, page_png):
    """Crops a shot page to the sprite. Returns (PNG bytes, spill): spill is how far, in
    points, the drawing reaches past the sprite on each side (left, top, right, bottom)."""
    Image = _pil()
    im = Image.open(io.BytesIO(page_png)).convert("RGBA")
    s = rec["scale"]
    _, _, crop = layout(rec)
    box = tuple(int(round(v * s)) for v in (crop[0], crop[1], crop[0] + crop[2], crop[1] + crop[3]))
    sprite = im.crop(box)
    if list(sprite.size) != rec["png_size"]:
        raise SystemExit(f"{rec['realm']}/{rec['id']}: cropped to {sprite.size}, expected {rec['png_size']}")
    alpha = im.getchannel("A").point(lambda v: 255 if v > SPILL_ALPHA else 0)
    alpha.paste(0, box)
    bb = alpha.getbbox()
    spill = [0.0, 0.0, 0.0, 0.0]
    if bb:
        spill = [max(0, box[0] - bb[0]) / s, max(0, box[1] - bb[1]) / s,
                 max(0, bb[2] - box[2]) / s, max(0, bb[3] - box[3]) / s]
    buf = io.BytesIO()
    sprite.save(buf, "PNG", optimize=True)
    return buf.getvalue(), spill


def extent(rec, page_png):
    """How far the drawing reaches past the call's rect, in points (left, top, right, bottom);
    negative when it stops inside it. For --measure."""
    Image = _pil()
    im = Image.open(io.BytesIO(page_png)).convert("RGBA")
    s = rec["scale"]
    bb = im.getchannel("A").point(lambda v: 255 if v > SPILL_ALPHA else 0).getbbox()
    if not bb:
        return None
    _, _, crop = layout(rec)
    x0, y0, x1, y1 = crop[0] + rec["bleed"][0], crop[1] + rec["bleed"][1], crop[0] + crop[2] - rec["bleed"][2], crop[1] + crop[3] - rec["bleed"][3]
    return [x0 - bb[0] / s, y0 - bb[1] / s, bb[2] / s - x1, bb[3] / s - y1]
