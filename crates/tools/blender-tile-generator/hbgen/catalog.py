"""The catalog: which tiles the generator owns, how to render them, and their
tile_set.json fields. Standard library only (used inside and outside Blender).

A catalog file (catalog/*.json, one per realm):

    {
      "realm": "daiLong",
      "palette": "viet",            # default for its entries
      "density": 8,                 # default PNG px per logical px
      "entries": [
        {
          "tile": "roof_tile_kiln",  # tile def name, also the sprite's file name
          "layer": "objects",        # assets/tiles/<layer>/ (default objects)
          "category": "buildings",   # tile_set.json category (default buildings)
          "recipe": "roof_tile_kiln",# hbgen.recipes.RECIPES key (default: tile)
          "params": {},              # recipe parameters
          "variations": [            # optional; omitted = one unnamed variation
            {"name": "var0", "seed": 1, "params": {}}  # a variation may list its own "frames",
          ],                                           # in place of the entry's (a walker's 8 walk, 2 idle)
          "frames": [                # optional; omitted = one frame named after the tile.
            {"name": "stage0", "params": {"stage": 0}}   # One sprite per frame, in one anim set:
          ],                                             # <tile>/[<variation>/]<frame>.png
          "tile_set": {"cost": 10},  # fields written into the tile def, in this order
          "generated": true,         # false: the art is hand-made; leave it alone
          "validate": {"ground": false}, # an overlay: skip the ground diamond's checks
          "unit": {                  # a unit (category "units"): its one variation, written as it is
            "iso_offset": {"x": 0, "y": 32},   # into the tile def, with its anim sets; the
            "anim_sets": [...]                 # variations above are the rendered anim sets'
          },                                   # folders, their frames the frames
          "frame": {"width": 40, "height": 45} # a hand-made unit's missing directions: rendered into
        }                                      # its def's frame (its draw_size), the model standing on
                                               # the frame's bottom edge as the hand-made walkers do
                                               # (or "origin_x"/"origin_up" px from its middle), and
                                               # only the unit's anim_sets are added to its def (sync)
      ]
    }

The manifest (catalog/manifest.json, written by build.py and committed) records
every sprite the generator wrote: its hash, PNG size and draw_size. A sprite file
that isn't in the manifest is hand-made, and the tools never overwrite it.
"""

import hashlib
import json
import os

TOOL_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REPO_ROOT = os.path.normpath(os.path.join(TOOL_DIR, "..", "..", ".."))
CATALOG_DIR = os.path.join(TOOL_DIR, "catalog")
MANIFEST_NAME = "manifest.json"
DEFAULT_DENSITY = 8
MAX_FRAME_NAME = 24                 # TileSprite::name is an ArrayString<24>

# Bump to re-render everything, e.g. after a Blender upgrade changes the look.
GENERATOR_VERSION = 1

_ENTRY_KEYS = {"tile", "layer", "category", "recipe", "params", "palette", "density", "variations", "frames", "tile_set", "generated", "validate",
               "unit", "frame", "notes"}
_FRAME_KEYS = {"name", "params"}
_VARIATION_KEYS = {"name", "seed", "params", "tile_set", "frames"}


class CatalogError(Exception):
    pass


def catalog_files(catalog_dir=CATALOG_DIR):
    return sorted(os.path.join(catalog_dir, f) for f in os.listdir(catalog_dir)
                  if f.endswith(".json") and f != MANIFEST_NAME)


def load(paths=None, catalog_dir=CATALOG_DIR):
    """Entries from the catalog files, each with its defaults filled in and a
    "source" key naming its file. Raises CatalogError on a malformed entry."""
    entries, seen = [], {}
    for path in paths or catalog_files(catalog_dir):
        with open(path, encoding="utf-8") as f:
            doc = json.load(f)
        name = os.path.basename(path)
        for i, raw in enumerate(doc.get("entries", [])):
            where = f"{name} entry {i}"
            unknown = set(raw) - _ENTRY_KEYS
            if unknown:
                raise CatalogError(f"{where}: unknown keys {sorted(unknown)}")
            if "tile" not in raw:
                raise CatalogError(f"{where}: no 'tile'")
            e = dict(raw)
            e.setdefault("layer", "objects"); e.setdefault("category", "buildings")
            e.setdefault("recipe", e["tile"]); e.setdefault("params", {})
            e.setdefault("palette", doc.get("palette", "viet")); e.setdefault("density", doc.get("density", DEFAULT_DENSITY))
            e.setdefault("tile_set", {}); e.setdefault("generated", True); e.setdefault("validate", {})
            if "unit" in e and not (isinstance(e["unit"], dict) and isinstance(e["unit"].get("anim_sets"), list)):
                raise CatalogError(f"{where}: 'unit' needs its 'anim_sets'")
            if "frame" in e:
                fr = e["frame"]
                if "unit" not in e or not isinstance(fr, dict) or not {"width", "height"} <= set(fr) <= {"width", "height", "origin_x", "origin_up"} \
                        or not all(isinstance(fr[k], int) for k in fr) or fr["width"] <= 0 or fr["height"] <= 0:
                    raise CatalogError(f"{where}: 'frame' is a unit's {{width, height[, origin_x, origin_up]}} in logical px, with its 'unit' block")
            e["variations"] = [dict(v) for v in raw.get("variations", [{"name": ""}])]
            for v in e["variations"]:
                if set(v) - _VARIATION_KEYS:
                    raise CatalogError(f"{where}: unknown variation keys {sorted(set(v) - _VARIATION_KEYS)}")
                v.setdefault("name", ""); v.setdefault("seed", 0); v.setdefault("params", {}); v.setdefault("tile_set", {})
            e["frames"] = _frames(raw.get("frames", [{"name": e["tile"]}]), where)
            for v in e["variations"]:
                if "frames" in v:
                    v["frames"] = _frames(v["frames"], f"{where} variation '{v['name']}'")
            names = [v["name"] for v in e["variations"]]
            if len(set(names)) != len(names) or ("" in names and len(names) > 1):
                raise CatalogError(f"{where}: variation names must be unique, and only a lone variation may be unnamed")
            if len(e["tile"]) > MAX_FRAME_NAME:
                raise CatalogError(f"{where}: tile name '{e['tile']}' is longer than {MAX_FRAME_NAME} characters")
            key = (e["layer"], e["tile"])
            if key in seen:
                raise CatalogError(f"{where}: '{e['tile']}' is already in {seen[key]}")
            seen[key] = name
            e["source"] = name
            entries.append(e)
    return entries


def _frames(raw, where):
    frames = [dict(f) for f in raw]
    if not frames:
        raise CatalogError(f"{where}: no frames")
    for f in frames:
        if set(f) - _FRAME_KEYS or not f.get("name"):
            raise CatalogError(f"{where}: a frame needs a name, and may only have {sorted(_FRAME_KEYS)}")
        if len(f["name"]) > MAX_FRAME_NAME:
            raise CatalogError(f"{where}: frame name '{f['name']}' is longer than {MAX_FRAME_NAME} characters")
        f.setdefault("params", {})
    if len({f["name"] for f in frames}) != len(frames):
        raise CatalogError(f"{where}: frame names must be unique")
    return frames


def variation_frames(entry, variation):
    """A variation's frames: its own, else the entry's."""
    return variation.get("frames") or entry["frames"]


def sprite_path(entry, variation_name, frame_name=None):
    """Repository-relative path of a sprite, as the engine looks it up:
    assets/tiles/<layer>/<category>/<tile>/[<variation>/]<frame>.png. The frame
    defaults to the variation's first (its own frames', else the entry's, named after
    the tile unless it lists frames)."""
    parts = ["assets", "tiles", entry["layer"], entry["category"], entry["tile"]]
    if variation_name:
        parts.append(variation_name)
    if frame_name is None:
        v = next((v for v in entry["variations"] if v["name"] == variation_name), {})
        frame_name = variation_frames(entry, v)[0]["name"]
    return "/".join(parts + [frame_name + ".png"])


def render_frame(entry):
    """The frame a hand-made unit's added sprites are rendered into (an entry with 'frame'): its
    size in logical px and where the model's origin (its feet) falls, measured from the frame's
    bottom edge's middle: on it by default, as the hand-made walkers stand (the engine stands the
    frame's bottom on the cell's centre, moved by iso_offset, so both land in one place). None for
    a tile the generator sizes itself."""
    if "frame" not in entry:
        return None
    fr = entry["frame"]
    return dict(width=fr["width"], height=fr["height"], origin_x=fr.get("origin_x", 0), origin_up=fr.get("origin_up", 0))


def jobs(entries):
    """One render job per generated variation and frame."""
    out = []
    for e in entries:
        if not e["generated"]:
            continue
        for v in e["variations"]:
            for f in variation_frames(e, v):
                out.append(dict(tile=e["tile"], layer=e["layer"], category=e["category"], recipe=e["recipe"],
                                params={**e["params"], **v["params"], **f["params"]}, seed=v["seed"], palette=e["palette"],
                                density=e["density"], variation=v["name"], frame=f["name"],
                                path=sprite_path(e, v["name"], f["name"]), validate=e["validate"], source=e["source"],
                                frame_box=render_frame(e)))
    return out


def job_hash(job, source_files, extra=None):
    """Hash of everything that decides a render: the job's settings, the
    generator's source files and version, and extra (e.g. --fast, forced height)."""
    h = hashlib.sha256()
    keys = ("recipe", "params", "seed", "palette", "density", "variation")
    h.update(json.dumps({k: job[k] for k in keys}, sort_keys=True).encode())
    h.update(json.dumps(extra or {}, sort_keys=True).encode())
    h.update(str(GENERATOR_VERSION).encode())
    for path in sorted(source_files):
        with open(path, "rb") as f:
            h.update(os.path.relpath(path, TOOL_DIR).encode()); h.update(f.read())
    return h.hexdigest()[:16]


def load_manifest(path=None):
    path = path or os.path.join(CATALOG_DIR, MANIFEST_NAME)
    if not os.path.exists(path):
        return {"version": 1, "sprites": {}}
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def save_manifest(manifest, path=None):
    path = path or os.path.join(CATALOG_DIR, MANIFEST_NAME)
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2, sort_keys=True)
        f.write("\n")
    os.replace(tmp, path)
