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

import base64
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

# palette.py's realm classes: the dialogs' (and so the menus' and the screens') drawing methods,
# with the HUD's icon tiles, panels and ornaments on top.
REALM_CLASSES = {"dailong": ("DaiLongPalette", "DAILONG"), "kambara": ("KambaraPalette", "KAMBARA"),
                 "tampadesa": ("TampadesaPalette", "TAMPADESA")}


# ----------------------------------------------------------------------------
# The generator
# ----------------------------------------------------------------------------
_GEN = None


def load_generator():
    """Imports the generator once, with its text helper stripped. The generator reads
    build/fonts.css at import; sprites have no text (but a seal's letter, which embeds its own
    font: _with_text), so a missing file reads as empty."""
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
        import world_map, screens, menus, pages, dialogs, frames, palette  # noqa: E401
        import dialogs_content
    finally:
        pathlib.Path.read_text = original

    def no_text(*a, **k):
        return ""
    global _TEXT
    _TEXT = world_map.T
    for m in (world_map, screens, menus, pages, dialogs, frames, palette):
        m.T = no_text
    _GEN = (palette, dialogs_content)
    return _GEN


# The generator's text helper, kept for the few sprites that draw a letter (_with_text).
_TEXT = None

# The game's font files for the families a sprite's text may use: the page embeds the one it
# needs, so a sprite's text doesn't depend on the generator's downloaded fonts.
TEXT_FONTS = {"Protest Revolution": "protest_revolution_regular.ttf", "Spectral": "spectral_semi_bold.ttf"}


def _with_text(th, family, draw):
    """Calls draw() with the generator's text helper back in world_map (where seal() lives), and
    embeds `family` from assets/fonts in the sprite's defs."""
    _embed_font(th, family)
    import world_map
    stripped, world_map.T = world_map.T, _TEXT
    try:
        return draw()
    finally:
        world_map.T = stripped


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


def _paper_box(th, x, y, w, h, stroke):
    # Đại Long's box() starts with its frame, a dark rect 6 points larger all round: drop it and
    # outline the paper instead.
    svg = th.box(x, y, w, h)
    frame_end = svg.index("/>") + 2
    frame = svg[:frame_end]
    if 'fill="#2a170c"' not in frame:
        raise ValueError("paper_box: box() no longer starts with Đại Long's frame")
    return (svg[frame_end:]
            + f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="3" fill="none" stroke="#2a170c" stroke-width="{stroke}"/>')


def _glyph(th, spec):
    """A pictogram's elements by "group:name": building (palette_glyphs, in the realm's architecture),
    sector or hud (palette.py's sector column and HUD buttons, the minimap's included); "" is none."""
    if not spec:
        return []
    import palette
    import palette_glyphs
    group, name = spec.split(":", 1)
    if group == "building":
        return palette_glyphs.glyph(name, th.dd["key"])
    if group == "sector":
        return palette.sector_glyph(name, th.dd["key"])
    if group == "hud":
        return palette.hud_glyph(name)
    raise ValueError(f"unknown glyph group {group!r}")


def _hud_tile(th, x, y, w, h, a):
    """An icon tile w wide at (x, y) (its height is the realm's for its kind, the crest above y):
    `part` frame or glyph, `glyph` "group:name", `state` idle, hover, pressed or disabled, `small`
    (the top bar's and the minimap's), `crest_k` (the sector column's crest, a share of the realm's)."""
    k = a.get("crest_k")
    return th.icon_tile(x, y, w, _glyph(th, a.get("glyph", "")), a.get("state", "idle"), a.get("small", False),
                        None if k is None else th.crest_k * k, a.get("part", "all"))


def _hud_panel(th, x, y, w, h, a):
    """A HUD panel at its rect without a large panel's ornaments (`big` keeps its thicker frame):
    Đại Long's carved wood round parchment, Kambara's carved frame round dark stone (`t` thick),
    Tampadesa's brick round dark mudbrick."""
    realm = th.dd["key"]
    big = a.get("big", False)
    if realm == "dailong":
        return th.woodwork().panel(x, y, w, h, big=big, ornaments=False)
    if realm == "kambara":
        return th.carving().frame(x, y, w, h, t=a.get("t", 13), crest=None, seed=a.get("seed", 1), vines=True)
    return th.brickwork().panel(x, y, w, h, big=big, ornaments=False)


def _hud_panel_ornament(th, x, y, w, h, a):
    """A large panel's ornaments (`parts`, by name) round a panel `panel` ([w, h]) whose top left is
    `origin` ([x, y]) from the rect's: Đại Long's and Tampadesa's panel_ornaments, Kambara's crest
    (`ch` high, over a frame `t` thick). The rect is the ornament's own, so the engine anchors it on
    the panel by that origin."""
    realm = th.dd["key"]
    px, py = x + a["origin"][0], y + a["origin"][1]
    pw, ph = a["panel"]
    if realm == "kambara":
        return th.carving().crest(px, py, pw, ph, a.get("t", 13), a["ch"], a.get("seed", 1))
    if realm == "dailong":
        return th.woodwork().ornaments_markup(px, py, pw, ph, a["parts"])
    parts = th.brickwork().panel_ornaments(px, py, pw, ph)
    return "".join(parts[n] for n in a["parts"])


def _hud_ornament(th, x, y, w, h, a):
    """A gap ornament (palette.gap_ornament) filling the rect, h tall."""
    return th.gap_ornament(x, x + w, y + h / 2, h)


def _with_lines(th, n, draw):
    # caption() and speech() size themselves from their wrapped text; text is stripped, so
    # wrap to n empty lines for the height wanted.
    th.wrap = lambda *a, **k: [""] * n
    return draw()


CALLS = {
    # Panels.
    "mpanel": lambda th, x, y, w, h, a: th.mpanel(x, y, w, h, crest=a.get("crest", False)),
    "box": lambda th, x, y, w, h, a: th.box(x, y, w, h, big=a.get("big", False)),
    # box() with a thin outline (`stroke` points, 2 by default, in its frame's colour) in place of
    # the 6-point frame Đại Long's draws round it, for small panels such as tooltips.
    "paper_box": lambda th, x, y, w, h, a: _paper_box(th, x, y, w, h, a.get("stroke", 2)),
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
    # A seal (Đại Long's red square with a letter in the title font), s points large, at the
    # rect's centre.
    "seal": lambda th, x, y, w, h, a: _with_text(
        th, th.title_font, lambda: th.seal(*_at(x, y, w, h, a), a.get("s", min(w, h)), a.get("letter", ""), a.get("color"))),
    # The home menu's backdrop scene (menus.py), drawn for 1920 x 1080 at the rect and cut to it.
    "home_scene": lambda th, x, y, w, h, a: _scene(th, x, y, w, h, th.home_scene()),
    # The world map (world_map.py), for the world map screen: its base without markers, labels,
    # threats or routes (`map`: the content.py map it draws, by realm), at the map viewport (its
    # rect is the viewport, 1290 x 867 points: 1000 x 672 map units at common.MAP_S); a threat's
    # storm clouds, centred on the point `anchor` ([x, y] points from the rect's top left), which
    # the game puts on the threat's position; the frame around that viewport, with the viewport
    # left transparent; and the screen's background, 1920 x 1080.
    "world_map_base": lambda th, x, y, w, h, a: _map_view(th, x, y, w, h, lambda: th.base_map(_world_map(a))),
    "world_map_threat": lambda th, x, y, w, h, a: th.threat_clouds(x + a["anchor"][0], y + a["anchor"][1]),
    "world_map_frame": lambda th, x, y, w, h, a: _at_viewport(x, y, th.map_frame()),
    "world_map_backdrop": lambda th, x, y, w, h, a: _scene(th, x, y, w, h, th.background()),
    # The world map's marks, which the game draws over the base at their cities' positions, the
    # mark's point at the rect's centre: a player city of `kind` (capital, city, monument; `mini`:
    # the legend's glyph) without its name or a seal's letter, the compass without its N, the top
    # bar's coin, the side panel's emblem (the realm's trade partner's, as the mockup draws it) and
    # a power's emblem for the cities that have none of their own: the selected foreign city's
    # rings at `scale`.
    "world_map_mark": lambda th, x, y, w, h, a: th.player_city(
        *_at(x, y, w, h, a), " ", a.get("kind", "city"), False, mini=a.get("mini", False)),
    "world_map_compass": lambda th, x, y, w, h, a: th.compass(*_at(x, y, w, h, a)),
    "world_map_coin": lambda th, x, y, w, h, a: th.coin(*_at(x, y, w, h, a)),
    "world_map_emblem": lambda th, x, y, w, h, a: th.panel_emblem(*_at(x, y, w, h, a)),
    "world_map_power_emblem": lambda th, x, y, w, h, a: _scaled(
        *_at(x, y, w, h, a), a.get("scale", 1), th.foreign_city(*_at(x, y, w, h, a), "", "", True)),
    # The intermission (screens.py): the stele as the debrief draws it at its rect (640 x 950 there),
    # its carved text left out (the entry omits stele_lines: the game carves the title, the lines,
    # the rule under the title, the newest line's mark and the empty rows live); the stele's
    # backdrop (stele_backdrop: Đại Long's lacquer niche, Kambara's vines, Tampadesa's whitewashed
    # niche), drawn where the screen puts it, moved so the screen point `origin` ([x, y]) is the
    # rect's top left; and the newest line's mark (fresh_mark) at `at` in the rect.
    "stele": lambda th, x, y, w, h, a: th.stele(x, y, w, h, "", []),
    "stele_backdrop": lambda th, x, y, w, h, a: _at_origin(x, y, a["origin"], th.stele_backdrop()),
    "stele_mark": lambda th, x, y, w, h, a: th.fresh_mark(*_at(x, y, w, h, a)),
    # The story comics (screens.py's intro): a placeholder panel, the realm's story scene
    # (intro_scene) cut to the rect (the mockup's panel, 1800 x 870 points), with `label` and the
    # panel's visual description from the story script (`text`) as a dim caption along its
    # bottom; and the realm frame round the panel (frame_back_at with the panel cut out of it,
    # then frame_front_at), the panel left transparent.
    "story_panel": lambda th, x, y, w, h, a: _story_panel(th, x, y, w, h, a.get("label", ""), a.get("text", "")),
    "story_frame": lambda th, x, y, w, h, a: _story_frame(th, x, y, w, h),
    # The HUD (palette.py): an icon tile's frame or glyph, a HUD panel and a large one's ornaments,
    # a gap ornament, and a line icon (screens.py's icon(): the resources', the messages') `s`
    # large in the accent colour, at the rect's centre.
    "hud_tile": _hud_tile,
    "hud_panel": _hud_panel,
    "hud_panel_ornament": _hud_panel_ornament,
    "hud_ornament": _hud_ornament,
    "hud_icon": lambda th, x, y, w, h, a: th.icon(a["icon"], *_at(x, y, w, h, a), a.get("s", 1), a.get("color") or th.accent),
}


# The story panels' dim caption: its font (render.TEXT_FONTS), sizes in points, and how wide a
# character is taken to be, in ems, to wrap the description.
STORY_CAPTION_FONT = "Spectral"
STORY_CAPTION_LABEL_SIZE = 20
STORY_CAPTION_SIZE = 26
STORY_CAPTION_CHAR_EM = 0.5


def _xml(text):
    return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def _embed_font(th, family):
    if family not in TEXT_FONTS:
        raise cat.CatalogError(f"no font file for the family {family!r} (render.TEXT_FONTS)")
    data = (pathlib.Path(cat.REPO_ROOT) / "assets" / "fonts" / TEXT_FONTS[family]).read_bytes()
    th.defs.append(f"<style>@font-face{{font-family:'{family}';"
                   f"src:url(data:font/ttf;base64,{base64.b64encode(data).decode()})}}</style>")


def _story_panel(th, x, y, w, h, label, text):
    import textwrap
    cid = th.nid("panel")
    th.defs.append(f'<clipPath id="{cid}"><rect x="{x:g}" y="{y:g}" width="{w:g}" height="{h:g}"/></clipPath>')
    out = [f'<g clip-path="url(#{cid})">{th.intro_scene(x, y, w, h)}']
    margin = 60
    lines = textwrap.wrap(text, max(20, int((w - 2 * margin) / (STORY_CAPTION_SIZE * STORY_CAPTION_CHAR_EM))))
    if label or lines:
        _embed_font(th, STORY_CAPTION_FONT)
        line_h = STORY_CAPTION_SIZE * 1.3
        band = 28 + (STORY_CAPTION_LABEL_SIZE * 1.4 if label else 0) + len(lines) * line_h + 16
        top = y + h - band
        out.append(f'<rect x="{x:g}" y="{top:g}" width="{w:g}" height="{band:g}" fill="#000" opacity="0.42"/>')
        cy = top + 28 + STORY_CAPTION_LABEL_SIZE * 0.8
        font = f"font-family=\"'{STORY_CAPTION_FONT}'\""
        if label:
            out.append(f'<text x="{x + margin:g}" y="{cy:g}" {font} font-size="{STORY_CAPTION_LABEL_SIZE}" '
                       f'letter-spacing="3" fill="#f0e2c0" opacity="0.7">{_xml(label.upper())}</text>')
            cy += STORY_CAPTION_LABEL_SIZE * 1.4
        for line in lines:
            cy_line = cy + STORY_CAPTION_SIZE * 0.4
            out.append(f'<text x="{x + margin:g}" y="{cy_line:g}" {font} font-size="{STORY_CAPTION_SIZE}" '
                       f'fill="#f0e2c0" opacity="0.75">{_xml(line)}</text>')
            cy += line_h
    out.append("</g>")
    return "".join(out)


def _story_frame(th, x, y, w, h):
    # Everything but the panel: a page-sized path with the rect cut out (evenodd).
    cid = th.nid("framehole")
    th.defs.append(f'<clipPath id="{cid}"><path clip-rule="evenodd" '
                   f'd="M{x - 500:g},{y - 500:g} h{w + 1000:g} v{h + 1000:g} h-{w + 1000:g} Z '
                   f'M{x:g},{y:g} h{w:g} v{h:g} h-{w:g} Z"/></clipPath>')
    return f'<g clip-path="url(#{cid})">{th.frame_back_at(x, y, w, h)}</g>{th.frame_front_at(x, y, w, h)}'


def _at_origin(x, y, origin, body):
    # A drawing in screen points, moved so the screen point `origin` lands on (x, y).
    return f'<g transform="translate({x - origin[0]:g},{y - origin[1]:g})">{body}</g>'


def _scaled(cx, cy, scale, body):
    # `body` scaled by `scale` about (cx, cy).
    return f'<g transform="translate({cx:g},{cy:g}) scale({scale:g}) translate({-cx:g},{-cy:g})">{body}</g>'


def _world_map(a):
    # The world map content (generator's content.py) of a realm: DAILONG, KAMBARA, TAMPADESA.
    import content
    key = a.get("map")
    if key not in REALM_CLASSES:
        raise cat.CatalogError(f"world_map_base: map must be one of {sorted(REALM_CLASSES)}, not {key!r}")
    return getattr(content, REALM_CLASSES[key][1])


def _viewport():
    import common
    return common.MAP_X, common.MAP_Y, common.MAP_W, common.MAP_H


def _at_viewport(x, y, body):
    # World map layers are drawn in screen points with the viewport at (MAP_X, MAP_Y): moved so
    # the viewport's top left is the rect's.
    vx, vy, _, _ = _viewport()
    return f'<g transform="translate({x - vx:g},{y - vy:g})">{body}</g>'


def _map_view(th, x, y, w, h, draw):
    # The map clipped to the rect. Most of its layers stop at whole points (int(MAP_H): 866 of
    # the viewport's 866.88), so the map is cut there and a copy 1 point lower fills the last row.
    vx, vy, _, vh = _viewport()
    rect, whole = th.nid("view"), th.nid("view")
    th.defs.append(f'<clipPath id="{rect}"><rect x="{vx:g}" y="{vy:g}" width="{w:g}" height="{h:g}"/></clipPath>'
                   f'<clipPath id="{whole}"><rect x="{vx:g}" y="{vy:g}" width="{w:g}" height="{math.floor(vh):g}"/></clipPath>')
    under, body = draw(), draw()
    return _at_viewport(x, y, f'<g clip-path="url(#{rect})"><g transform="translate(0,1)">{under}</g>'
                              f'<g clip-path="url(#{whole})">{body}</g></g>')


def _scene(th, x, y, w, h, body):
    # A full-screen scene drawn at the origin: moved onto the rect and clipped to it, since its
    # palms and clouds run past the screen's edges.
    cid = th.nid("scene")
    th.defs.append(f'<clipPath id="{cid}"><rect x="0" y="0" width="{w:g}" height="{h:g}"/></clipPath>')
    return f'<g transform="translate({x:g},{y:g})"><g clip-path="url(#{cid})">{body}</g></g>'


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


# A JPEG's quality (Pillow's scale): the backdrops, which the web build preloads.
JPEG_QUALITY = 85


def finish(rec, page_png):
    """Crops a shot page to the sprite, resized to its own scale if that isn't the shot's, and
    encodes it in its format. Returns (image bytes, spill): spill is how far, in points, the
    drawing reaches past the sprite on each side (left, top, right, bottom)."""
    Image = _pil()
    im = Image.open(io.BytesIO(page_png)).convert("RGBA")
    s = rec["shot_scale"]
    _, _, crop = layout(rec)
    box = tuple(int(round(v * s)) for v in (crop[0], crop[1], crop[0] + crop[2], crop[1] + crop[3]))
    sprite = im.crop(box)
    if rec["scale"] != s:
        sprite = sprite.resize(tuple(rec["png_size"]), Image.LANCZOS)
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
    if rec["format"] == "jpeg":
        # Opaque: whatever the scene leaves transparent shows black.
        flat = Image.new("RGB", sprite.size, (0, 0, 0))
        flat.paste(sprite, mask=sprite.getchannel("A"))
        flat.save(buf, "JPEG", quality=JPEG_QUALITY, optimize=True, progressive=True)
    else:
        sprite.save(buf, "PNG", optimize=True)
    return buf.getvalue(), spill


def extent(rec, page_png):
    """How far the drawing reaches past the call's rect, in points (left, top, right, bottom);
    negative when it stops inside it. For --measure."""
    Image = _pil()
    im = Image.open(io.BytesIO(page_png)).convert("RGBA")
    s = rec["shot_scale"]
    bb = im.getchannel("A").point(lambda v: 255 if v > SPILL_ALPHA else 0).getbbox()
    if not bb:
        return None
    _, _, crop = layout(rec)
    x0, y0, x1, y1 = crop[0] + rec["bleed"][0], crop[1] + rec["bleed"][1], crop[0] + crop[2] - rec["bleed"][2], crop[1] + crop[3] - rec["bleed"][3]
    return [x0 - bb[0] / s, y0 - bb[1] / s, bb[2] / s - x1, bb[3] / s - y1]
