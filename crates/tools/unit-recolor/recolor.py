#!/usr/bin/env python3
"""Makes units by recolouring another unit's sprites, from recipes.json.

    python3 crates/tools/unit-recolor/recolor.py [UNIT ...] [--list] [--force]

For each recipe (all of them, or the UNITs named), it:
- recolours every frame under assets/tiles/objects/units/<source>/ into
  assets/tiles/objects/units/<name>/, at the same paths;
- adds a tile def named <name> to assets/tiles/objects/tile_set.json, a copy of the source unit's
  def placed right after it, unless the tile set has one already.

The game still needs a UnitConfigKey and a unit config for the new unit, and the web build's asset
list must be refreshed (cargo run -p web-builder -- manifest).

The source art is drawn in flat colours with a dark outline, and antialiased: an edge pixel is a
blend of the outline and a fill. A pixel that is such a blend of the outline and one of the
recipe's `colors` gets the same blend of that colour's new one. A pixel closer to one of the
`keep` colours (or its blend with the outline) is left as it is, so dark details that sit between
the outline and a recoloured fill, like a spear shaft, keep their colour.

It never overwrites hand-made art: it records what it wrote in manifest.json, and skips an
existing file that isn't listed there unless --force. Needs Pillow.
"""
import argparse
import json
import pathlib
import sys

try:
    from PIL import Image
except ImportError:
    sys.exit("recolor.py needs Pillow: pip install Pillow")

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[2]
UNITS = ROOT / "assets" / "tiles" / "objects" / "units"
TILE_SET = ROOT / "assets" / "tiles" / "objects" / "tile_set.json"
RECIPES = HERE / "recipes.json"
MANIFEST = HERE / "manifest.json"

# How far (squared RGB distance) a pixel may sit from the outline-to-fill blend line to count as
# a blend of that fill, and the part of the line that counts: a pixel nearer the outline than
# MIN_BLEND is outline, not fill.
MAX_BLEND_ERROR = 60
MIN_BLEND = 0.3
MAX_BLEND = 1.05
# A kept colour wins only for pixels at least this far from the outline towards it.
MIN_KEEP_BLEND = 0.6


def rgb(hex_color):
    value = hex_color.lstrip("#")
    if len(value) != 6:
        raise ValueError(f"not a #rrggbb colour: {hex_color}")
    return tuple(int(value[i:i + 2], 16) for i in (0, 2, 4))


def blend(outline, color, pixel):
    """(t, error): the pixel as outline + t * (color - outline), and its squared distance from it."""
    d = [color[i] - outline[i] for i in range(3)]
    q = [pixel[i] - outline[i] for i in range(3)]
    length = sum(x * x for x in d)
    if length == 0:
        return 0.0, float("inf")
    t = sum(d[i] * q[i] for i in range(3)) / length
    error = sum((outline[i] + d[i] * t - pixel[i]) ** 2 for i in range(3))
    return t, error


class Recolor:
    def __init__(self, recipe):
        self.outline = rgb(recipe["outline"])
        self.colors = [(rgb(src), rgb(dst)) for src, dst in recipe["colors"].items()]
        self.keep = [rgb(c) for c in recipe.get("keep", [])]

    def pixel(self, p):
        r, g, b, a = p
        if a == 0:
            return p
        px = (r, g, b)

        best = None  # (error, t, new colour)
        for src, dst in self.colors:
            t, error = blend(self.outline, src, px)
            if MIN_BLEND <= t <= MAX_BLEND and error < MAX_BLEND_ERROR and (best is None or error < best[0]):
                best = (error, t, dst)
        if best is None:
            return p
        error, t, dst = best

        for keep in self.keep:
            tk, keep_error = blend(self.outline, keep, px)
            if tk > MIN_KEEP_BLEND and keep_error <= error:
                return p

        t = min(t, 1.0)
        return tuple(int(round(self.outline[i] + (dst[i] - self.outline[i]) * t)) for i in range(3)) + (a,)

    def image(self, source):
        image = Image.open(source).convert("RGBA")
        image.putdata([self.pixel(p) for p in image.get_flattened_data()])
        return image


# ---------------------------------------------------------------- tile set
def object_spans(text):
    """Every JSON object in `text` as (start, end, value): a minimal parser that keeps positions,
    so a def can be copied without reformatting the file around it."""
    spans = []
    decoder = json.JSONDecoder()

    def skip_ws(i):
        while i < len(text) and text[i] in " \t\r\n":
            i += 1
        return i

    def value(i):
        i = skip_ws(i)
        c = text[i]
        if c == "{":
            start, obj = i, {}
            i = skip_ws(i + 1)
            if text[i] == "}":
                spans.append((start, i + 1, obj))
                return obj, i + 1
            while True:
                key, i = decoder.raw_decode(text, skip_ws(i))
                i = skip_ws(i)
                assert text[i] == ":", f"expected ':' at {i}"
                obj[key], i = value(i + 1)
                i = skip_ws(i)
                if text[i] == ",":
                    i += 1
                    continue
                assert text[i] == "}", f"expected '}}' at {i}"
                spans.append((start, i + 1, obj))
                return obj, i + 1
        if c == "[":
            items = []
            i = skip_ws(i + 1)
            if text[i] == "]":
                return items, i + 1
            while True:
                item, i = value(i)
                items.append(item)
                i = skip_ws(i)
                if text[i] == ",":
                    i += 1
                    continue
                assert text[i] == "]", f"expected ']' at {i}"
                return items, i + 1
        return decoder.raw_decode(text, i)

    value(0)
    return spans


def unit_def_span(text, name):
    """(start, end) of the units category's tile def called `name`, or None."""
    spans = object_spans(text)
    units = next((o for _, _, o in spans if o.get("name") == "units" and isinstance(o.get("tile_defs"), list)), None)
    if units is None:
        return None
    for start, end, obj in spans:
        # Only the category's own entries: an anim set or a variation can have the same name.
        if obj.get("name") == name and any(obj is d for d in units["tile_defs"]):
            return start, end
    return None


def add_tile_def(name, source, notes):
    text = TILE_SET.read_text()
    if unit_def_span(text, name):
        notes.append(f"tile def '{name}' exists; left as it is")
        return False
    span = unit_def_span(text, source)
    if not span:
        raise SystemExit(f"no '{source}' tile def in the units category of {TILE_SET}")
    start, end = span
    copy = text[start:end].replace(f'"name": "{source}"', f'"name": "{name}"', 1)
    line_start = text.rfind("\n", 0, start) + 1
    indent = text[line_start:start]
    new_text = text[:end] + ",\n" + indent + copy + text[end:]

    # The copy must be the source's def with its name changed, and nothing else moved.
    old, new = json.loads(text), json.loads(new_text)
    units_old = next(c for c in old["categories"] if c["name"] == "units")["tile_defs"]
    units_new = next(c for c in new["categories"] if c["name"] == "units")["tile_defs"]
    src_def = next(d for d in units_old if d["name"] == source)
    added = next(d for d in units_new if d["name"] == name)
    assert dict(added, name=source) == src_def, "the copied def differs from its source"
    assert [d for d in units_new if d["name"] != name] == units_old, "other defs changed"

    TILE_SET.write_text(new_text)
    notes.append(f"added tile def '{name}' after '{source}'")
    return True


# ---------------------------------------------------------------- main
def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("units", nargs="*", help="recipes to run (default: all)")
    ap.add_argument("--list", action="store_true", help="list the recipes and exit")
    ap.add_argument("--force", action="store_true", help="overwrite files the manifest doesn't list")
    a = ap.parse_args(argv)

    recipes = json.loads(RECIPES.read_text())["units"]
    if a.list:
        for r in recipes:
            print(f"{r['name']}: from {r['source']}, {len(r['colors'])} colours")
        return 0

    wanted = set(a.units)
    unknown = wanted - {r["name"] for r in recipes}
    if unknown:
        sys.exit(f"no recipe for: {', '.join(sorted(unknown))}")

    manifest = json.loads(MANIFEST.read_text()) if MANIFEST.exists() else {"files": []}
    owned = set(manifest["files"])
    status = 0

    for recipe in recipes:
        if wanted and recipe["name"] not in wanted:
            continue
        name, source = recipe["name"], recipe["source"]
        source_dir = UNITS / source
        frames = sorted(source_dir.rglob("*.png"))
        if not frames:
            print(f"{name}: no frames under {source_dir}", file=sys.stderr)
            status = 1
            continue

        recolor = Recolor(recipe)
        written = skipped = 0
        for frame in frames:
            out = UNITS / name / frame.relative_to(source_dir)
            rel = out.relative_to(UNITS).as_posix()
            if out.exists() and rel not in owned and not a.force:
                print(f"{name}: skipped {rel}: not made by this tool (--force to overwrite)", file=sys.stderr)
                skipped += 1
                continue
            out.parent.mkdir(parents=True, exist_ok=True)
            recolor.image(frame).save(out, optimize=True)
            owned.add(rel)
            written += 1

        notes = []
        add_tile_def(name, source, notes)
        print(f"{name}: {written} frames from {source}" + (f", {skipped} skipped" if skipped else "")
              + "".join(f"; {n}" for n in notes))

    manifest["files"] = sorted(owned)
    MANIFEST.write_text(json.dumps(manifest, indent=1) + "\n")
    return status


if __name__ == "__main__":
    sys.exit(main())
