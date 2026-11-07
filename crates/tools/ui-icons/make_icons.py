#!/usr/bin/env python3
"""Makes the minimap's UI sprites from icons.json (docs/plans/minimap.md, "Art pipeline").

    python3 crates/tools/ui-icons/make_icons.py [--list] [--force]

Writes, under assets/ui/:
- buttons/<folder>/<name>_{idle,hovered,pressed,disabled}.png: the lens and tool buttons, one
  set per realm, icon-only, as in the HUD mockups.
- minimap/icons/<landmark>.png: white glyphs on transparent, tinted by the game.
- minimap/field_stripes.png: the tileable stripes fields are drawn with, tinted by the game.
- minimap/frame_<realm>.png: the widget's frame for realms that don't use the parchment page.

The glyphs are SVG path data (M, L, H, V, Q, C, Z) drawn as round-capped strokes, at 8x and
downsampled, so it needs only Pillow. The mockup generator reads the same paths.

It never overwrites hand-made art: it records what it wrote in manifest.json, and skips an
existing file that isn't listed there unless --force.
"""
import argparse
import json
import pathlib
import re
import sys

try:
    from PIL import Image, ImageDraw, ImageFilter
except ImportError:
    sys.exit("make_icons.py needs Pillow: pip install Pillow")

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[2]
UI = ROOT / "assets" / "ui"
CATALOG = HERE / "icons.json"
MANIFEST = HERE / "manifest.json"
SUPER = 8  # Supersampling.


# ====================================================================== paths
def parse_path(d):
    """SVG path data -> list of (points, closed) polylines in path units."""
    tokens = re.findall(r"[MLHVQCZmlhvqcz]|-?\d*\.?\d+(?:e-?\d+)?", d)
    subpaths, points = [], []
    x = y = 0.0
    start = (0.0, 0.0)
    i, cmd = 0, None

    def num():
        nonlocal i
        value = float(tokens[i])
        i += 1
        return value

    def flush(closed=False):
        nonlocal points
        if len(points) > 1:
            subpaths.append((points, closed))
        points = []

    while i < len(tokens):
        if re.match(r"[A-Za-z]", tokens[i]):
            cmd = tokens[i]
            i += 1
        rel = cmd.islower()
        c = cmd.upper()
        if c == "Z":
            flush(True)
            x, y = start
            continue
        if c == "M":
            flush()
            nx, ny = num(), num()
            x, y = (x + nx, y + ny) if rel else (nx, ny)
            start = (x, y)
            points = [(x, y)]
            cmd = "l" if rel else "L"  # Further pairs are line-tos.
        elif c == "L":
            nx, ny = num(), num()
            x, y = (x + nx, y + ny) if rel else (nx, ny)
            points.append((x, y))
        elif c == "H":
            nx = num()
            x = x + nx if rel else nx
            points.append((x, y))
        elif c == "V":
            ny = num()
            y = y + ny if rel else ny
            points.append((x, y))
        elif c == "Q":
            cx, cy, nx, ny = num(), num(), num(), num()
            if rel:
                cx, cy, nx, ny = x + cx, y + cy, x + nx, y + ny
            for k in range(1, 17):
                t = k / 16
                points.append(((1 - t) ** 2 * x + 2 * (1 - t) * t * cx + t * t * nx, (1 - t) ** 2 * y + 2 * (1 - t) * t * cy + t * t * ny))
            x, y = nx, ny
        elif c == "C":
            c1x, c1y, c2x, c2y, nx, ny = (num() for _ in range(6))
            if rel:
                c1x, c1y, c2x, c2y, nx, ny = x + c1x, y + c1y, x + c2x, y + c2y, x + nx, y + ny
            for k in range(1, 21):
                t = k / 20
                a, b, cc, dd = (1 - t) ** 3, 3 * (1 - t) ** 2 * t, 3 * (1 - t) * t * t, t ** 3
                points.append((a * x + b * c1x + cc * c2x + dd * nx, a * y + b * c1y + cc * c2y + dd * ny))
            x, y = nx, ny
    flush()
    return subpaths


def stroke_path(draw, d, cx, cy, scale, width, color):
    """Draws path `d` centred at (cx, cy), `scale` pixels per unit, `width` pixels wide."""
    for points, closed in parse_path(d):
        pts = [(cx + px * scale, cy + py * scale) for px, py in points]
        if closed:
            pts.append(pts[0])
        draw.line(pts, fill=color, width=max(1, round(width)), joint="curve")
        r = width / 2
        for px, py in (pts[0], pts[-1]):
            draw.ellipse([px - r, py - r, px + r, py + r], fill=color)


def hex_color(h, alpha=255):
    h = h.lstrip("#")
    return tuple(int(h[k:k + 2], 16) for k in (0, 2, 4)) + (alpha,)


def downsample(image, size):
    return image.resize(size, Image.LANCZOS)


# ====================================================================== sprites
def button(spec, realm, glyph, state):
    n = spec["size"]
    s = n * SUPER
    image = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    stroke = 1.2 * n / 28 * SUPER
    radius = spec["corner_radius"] * s

    fill = {"idle": realm["fill"], "hovered": realm["fill_hovered"], "pressed": realm["fill_pressed"], "disabled": realm["fill"]}[state]
    outline = realm["stroke_hovered"] if state == "hovered" else realm["stroke"]
    pad = stroke
    draw.rounded_rectangle([pad, pad, s - pad, s - pad], radius, fill=hex_color(fill), outline=hex_color(outline), width=round(stroke * (1.6 if state == "hovered" else 1.0)))

    ink = realm["ink_pressed"] if state == "pressed" else realm.get(glyph.get("ink", "ink"), realm["ink"])
    alpha = 110 if state == "disabled" else 255
    scale = n / 44 * spec["glyph_scale"] * SUPER
    stroke_path(draw, glyph["path"], s / 2, s / 2, scale, spec["stroke"] * SUPER * n / 56, hex_color(ink, alpha))

    if state == "disabled":
        veil = Image.new("RGBA", image.size, (0, 0, 0, 0))
        ImageDraw.Draw(veil).rounded_rectangle([pad, pad, s - pad, s - pad], radius, fill=(120, 110, 100, 70))
        image = Image.alpha_composite(image, veil)
    return downsample(image, (n, n))


def landmark(spec, path):
    n = spec["size"]
    s = n * SUPER
    image = Image.new("RGBA", (s, s), (255, 255, 255, 0))
    draw = ImageDraw.Draw(image)
    scale = n / 24 * spec["glyph_scale"] / 1.55 * SUPER
    stroke_path(draw, path, s / 2, s / 2, scale, spec["stroke"] * SUPER, (255, 255, 255, 255))
    return downsample(image, (n, n))


def field_stripes(spec):
    n = spec["size"]
    s = n * SUPER
    image = Image.new("RGBA", (s, s), (255, 255, 255, 255))
    value = round(255 * spec["row_value"])
    ImageDraw.Draw(image).rectangle([0, 0, s, round(s * spec["row"])], fill=(value, value, value, 255))
    return downsample(image, (n, n))


def noise(size, scale, seed):
    """Smooth value noise in 0..1 as an L image."""
    import random
    rng = random.Random(seed)
    w, h = size
    small = Image.new("L", (max(2, w // scale), max(2, h // scale)))
    small.putdata([rng.randrange(256) for _ in range(small.width * small.height)])
    return small.resize(size, Image.BICUBIC).filter(ImageFilter.GaussianBlur(scale / 3))


def frame(spec):
    w, h = spec["size"]
    image = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    if spec["style"] == "stone":
        base = Image.new("RGBA", (w, h), hex_color(spec["base"]))
        grain = noise((w, h), 6, 41)
        dark = Image.new("RGBA", (w, h), (60, 52, 36, 255))
        base = Image.composite(dark, base, grain.point(lambda v: max(0, v - 150)))
        moss = noise((w, h), 40, 42).point(lambda v: 255 if v > 168 else int(max(0, v - 140) * 9))
        moss = moss.filter(ImageFilter.GaussianBlur(3))
        mask = Image.new("L", (w, h), 0)
        md = ImageDraw.Draw(mask)
        md.rectangle([0, 0, w, 26], fill=255)
        md.rectangle([0, h - 26, w, h], fill=255)
        md.rectangle([0, 0, 26, h], fill=255)
        md.rectangle([w - 26, 0, w, h], fill=255)
        mask = mask.filter(ImageFilter.GaussianBlur(10))
        moss = Image.composite(moss, Image.new("L", (w, h), 0), mask)
        base = Image.composite(Image.new("RGBA", (w, h), hex_color(spec["moss"])), base, moss)
        image.paste(base)
        draw = ImageDraw.Draw(image)
        draw.rectangle([2, 2, w - 3, h - 3], outline=hex_color(spec["border"]), width=5)
        draw.rectangle([12, 12, w - 13, h - 13], outline=hex_color(spec["inner"]), width=2)
    else:
        top, bottom = hex_color(spec["top"]), hex_color(spec["bottom"])
        for y in range(h):
            t = y / (h - 1)
            draw.line([(0, y), (w, y)], fill=tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)))
        grain = noise((w, h), 3, 131).point(lambda v: max(0, v - 128) // 3)
        image = Image.composite(Image.new("RGBA", (w, h), (30, 18, 8, 255)), image, grain)
        draw = ImageDraw.Draw(image)
        draw.rounded_rectangle([1, 1, w - 2, h - 2], 10, outline=hex_color(spec["rim"]), width=8)
        draw.rounded_rectangle([7, 7, w - 8, h - 8], 7, outline=hex_color(spec["gold"]), width=5)
        draw.rounded_rectangle([16, 16, w - 17, h - 17], 5, outline=hex_color(spec["inner"], 128), width=2)
    return image


# ====================================================================== driver
def outputs(catalog):
    glyphs = catalog["glyphs"]
    spec = catalog["buttons"]
    for realm_name, realm in spec["realms"].items():
        for item in spec["items"]:
            glyph = {"path": glyphs[item["glyph"]], "ink": item.get("ink", "ink")}
            for state in ("idle", "hovered", "pressed", "disabled"):
                path = f"buttons/{realm['folder']}/{item['name']}_{state}.png"
                yield path, (lambda realm=realm, glyph=glyph, state=state: button(spec, realm, glyph, state))
    for kind, glyph in catalog["landmarks"]["items"].items():
        yield f"minimap/icons/{kind}.png", (lambda glyph=glyph: landmark(catalog["landmarks"], glyphs[glyph]))
    yield "minimap/field_stripes.png", (lambda: field_stripes(catalog["field_stripes"]))
    for realm_name, frame_spec in catalog["frames"].items():
        yield f"minimap/frame_{realm_name}.png", (lambda frame_spec=frame_spec: frame(frame_spec))


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--list", action="store_true", help="list the files it makes and stop")
    parser.add_argument("--force", action="store_true", help="overwrite files the manifest doesn't list")
    args = parser.parse_args()

    catalog = json.loads(CATALOG.read_text())
    manifest = set(json.loads(MANIFEST.read_text())["files"]) if MANIFEST.exists() else set()
    written, skipped = [], []

    for rel, make in outputs(catalog):
        if args.list:
            print(rel)
            continue
        target = UI / rel
        if target.exists() and rel not in manifest and not args.force:
            skipped.append(rel)
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        make().save(target, optimize=True)
        written.append(rel)

    if args.list:
        return
    manifest.update(written)
    MANIFEST.write_text(json.dumps({"files": sorted(manifest)}, indent=2) + "\n")
    print(f"wrote {len(written)} files under assets/ui")
    for rel in skipped:
        print(f"skipped {rel}: not made by this script (see manifest.json); --force to overwrite", file=sys.stderr)


if __name__ == "__main__":
    main()
