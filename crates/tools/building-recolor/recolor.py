#!/usr/bin/env python3
"""Makes building tile variations by recolouring hand-made sprites, from recipes.json.

    python3 crates/tools/building-recolor/recolor.py [TILE[/VARIATION] ...] [--list] [--force]
    python3 crates/tools/building-recolor/recolor.py --check

A recipe names a tile, one of its variations, a source sprite under sources/ (the hand-made art,
kept here so it can be recoloured again) and the hue shifts to apply. For each recipe (all of
them, or the ones named) it writes assets/tiles/objects/buildings/<tile>/<variation>/<tile>.png.
The tile def's variations are edited in assets/tiles/objects/tile_set.json by hand.

The art is painted, with gradients, so a shift recolours by hue: a pixel whose hue is inside a
shift's `hue` window ([from, to] in degrees; from may be negative, wrapping past 0) and whose
saturation is at least `min_saturation` is moved to around `to_hue`, keeping its spread of hues
(its offset from the window's centre, times `spread`, 1 by default), its saturation scaled by
`saturation` and its value by `value`. Membership fades over `feather` degrees and saturation at the window's edges, so there
are no hard seams. A recipe without shifts copies its source unchanged.

It never overwrites hand-made art: it records what it wrote in manifest.json (each output's
source, recipe and output hashes) and skips an existing file that isn't listed there unless
--force. --check needs no Pillow: it fails when an output is missing, or its source, recipe or
file no longer match the manifest (an edit not rendered, or a sprite edited by hand), as CI runs it.
"""
import argparse
import colorsys
import hashlib
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[2]
BUILDINGS = ROOT / "assets" / "tiles" / "objects" / "buildings"
SOURCES = HERE / "sources"
RECIPES = HERE / "recipes.json"
MANIFEST = HERE / "manifest.json"

# How far past a shift's saturation floor membership reaches full strength.
SATURATION_FEATHER = 0.08


def sha(data):
    return hashlib.sha256(data).hexdigest()[:16]


def recipe_key(recipe):
    return f"{recipe['tile']}/{recipe['variation']}"


def recipe_hash(recipe):
    return sha(json.dumps({k: recipe.get(k) for k in ("source", "shifts")}, sort_keys=True).encode())


def output_path(recipe):
    return BUILDINGS / recipe["tile"] / recipe["variation"] / f"{recipe['tile']}.png"


def hue_offset(hue, window):
    """The hue's offset from the window's centre, in degrees, wrapped to [-180, 180)."""
    centre = (window[0] + window[1]) / 2
    return (hue - centre + 180) % 360 - 180


def membership(hue, saturation, shift):
    """How much a pixel belongs to the shift's window, 0 to 1."""
    window = shift["hue"]
    half = (window[1] - window[0]) / 2
    feather = max(shift.get("feather", 6), 1e-6)
    distance = abs(hue_offset(hue, window))
    by_hue = min(1.0, max(0.0, (half + feather - distance) / feather))
    floor = shift.get("min_saturation", 0.0)
    # Full from the floor up, fading to nothing SATURATION_FEATHER below it.
    by_saturation = min(1.0, max(0.0, (saturation - floor + SATURATION_FEATHER) / SATURATION_FEATHER))
    return by_hue * by_saturation


def recolour_pixel(pixel, shifts):
    r, g, b, a = pixel
    if a == 0 or not shifts:
        return pixel
    h, s, v = colorsys.rgb_to_hsv(r / 255, g / 255, b / 255)
    hue = h * 360
    for shift in shifts:
        weight = membership(hue, s, shift)
        if weight <= 0:
            continue
        new_hue = (shift["to_hue"] + hue_offset(hue, shift["hue"]) * shift.get("spread", 1.0)) % 360
        new_s = min(1.0, s * shift.get("saturation", 1.0))
        new_v = min(1.0, v * shift.get("value", 1.0))
        nr, ng, nb = colorsys.hsv_to_rgb(new_hue / 360, new_s, new_v)
        mix = lambda old, new: round(old * (1 - weight) + new * 255 * weight)
        return (mix(r, nr), mix(g, ng), mix(b, nb), a)
    return pixel


def render(recipe):
    from PIL import Image  # Only rendering needs Pillow.

    image = Image.open(SOURCES / recipe["source"]).convert("RGBA")
    shifts = recipe.get("shifts", [])
    if shifts:
        pixels = image.load()
        width, height = image.size
        for y in range(height):
            for x in range(width):
                pixels[x, y] = recolour_pixel(pixels[x, y], shifts)
    return image


def load_json(path, default):
    return json.loads(path.read_text()) if path.exists() else default


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("names", nargs="*", help="TILE or TILE/VARIATION; default: every recipe")
    parser.add_argument("--list", action="store_true", help="list the recipes and exit")
    parser.add_argument("--force", action="store_true", help="overwrite files the manifest doesn't list")
    parser.add_argument("--check", action="store_true", help="check the outputs against the manifest; write nothing")
    args = parser.parse_args(argv)

    recipes = load_json(RECIPES, {"recipes": []})["recipes"]
    manifest = load_json(MANIFEST, {"sprites": {}})
    sprites = manifest.setdefault("sprites", {})

    keys = [recipe_key(recipe) for recipe in recipes]
    if len(set(keys)) != len(keys):
        sys.exit("recipes.json lists a tile/variation twice")

    if args.list:
        for recipe in recipes:
            print(f"{recipe_key(recipe)}  <- sources/{recipe['source']}  ({len(recipe.get('shifts', []))} shifts)")
        return 0

    chosen = [r for r in recipes if not args.names or r["tile"] in args.names or recipe_key(r) in args.names]
    if args.names and not chosen:
        sys.exit(f"no recipe for {', '.join(args.names)}")

    if args.check:
        problems = []
        for recipe in recipes:
            key = recipe_key(recipe)
            entry = sprites.get(key)
            out = output_path(recipe)
            source = SOURCES / recipe["source"]
            if entry is None:
                problems.append(f"{key}: not rendered (run recolor.py)")
                continue
            if not source.exists():
                problems.append(f"{key}: its source sources/{recipe['source']} is missing")
            elif entry.get("source") != sha(source.read_bytes()):
                problems.append(f"{key}: its source changed since it was rendered")
            if entry.get("recipe") != recipe_hash(recipe):
                problems.append(f"{key}: its recipe changed since it was rendered")
            if not out.exists():
                problems.append(f"{key}: {out.relative_to(ROOT)} is missing")
            elif entry.get("output") != sha(out.read_bytes()):
                problems.append(f"{key}: {out.relative_to(ROOT)} was changed by hand")
        for key in sprites:
            if key not in keys:
                problems.append(f"{key}: in the manifest but no recipe makes it")
        for problem in problems:
            print(problem)
        print("ok" if not problems else f"{len(problems)} problem(s)")
        return 1 if problems else 0

    status = 0
    for recipe in chosen:
        key = recipe_key(recipe)
        out = output_path(recipe)
        if out.exists() and key not in sprites and not args.force:
            print(f"skip (hand-made, not in the manifest; --force to overwrite): {out.relative_to(ROOT)}")
            status = 1
            continue
        image = render(recipe)
        out.parent.mkdir(parents=True, exist_ok=True)
        image.save(out, optimize=True)
        sprites[key] = {
            "source": sha((SOURCES / recipe["source"]).read_bytes()),
            "recipe": recipe_hash(recipe),
            "output": sha(out.read_bytes()),
        }
        print(f"wrote {out.relative_to(ROOT)}")

    manifest["sprites"] = dict(sorted(sprites.items()))
    MANIFEST.write_text(json.dumps(manifest, indent=2) + "\n")
    return status


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
