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
            {"name": "var0", "seed": 1, "params": {}}
          ],
          "frames": [                # optional; omitted = one frame named after the tile.
            {"name": "stage0", "params": {"stage": 0}}   # One sprite per frame, in one anim set:
          ],                                             # <tile>/[<variation>/]<frame>.png
          "tile_set": {"cost": 10},  # fields written into the tile def, in this order
          "generated": true,         # false: the art is hand-made; leave it alone
          "validate": {"ground": false}  # an overlay: skip the ground diamond's checks
        }
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

_ENTRY_KEYS = {"tile", "layer", "category", "recipe", "params", "palette", "density", "variations", "frames", "tile_set", "generated", "validate", "notes"}
_FRAME_KEYS = {"name", "params"}
_VARIATION_KEYS = {"name", "seed", "params", "tile_set"}


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
            e["variations"] = [dict(v) for v in raw.get("variations", [{"name": ""}])]
            for v in e["variations"]:
                if set(v) - _VARIATION_KEYS:
                    raise CatalogError(f"{where}: unknown variation keys {sorted(set(v) - _VARIATION_KEYS)}")
                v.setdefault("name", ""); v.setdefault("seed", 0); v.setdefault("params", {}); v.setdefault("tile_set", {})
            e["frames"] = [dict(f) for f in raw.get("frames", [{"name": e["tile"]}])]
            for f in e["frames"]:
                if set(f) - _FRAME_KEYS or not f.get("name"):
                    raise CatalogError(f"{where}: a frame needs a name, and may only have {sorted(_FRAME_KEYS)}")
                if len(f["name"]) > MAX_FRAME_NAME:
                    raise CatalogError(f"{where}: frame name '{f['name']}' is longer than {MAX_FRAME_NAME} characters")
                f.setdefault("params", {})
            if len({f["name"] for f in e["frames"]}) != len(e["frames"]):
                raise CatalogError(f"{where}: frame names must be unique")
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


def sprite_path(entry, variation_name, frame_name=None):
    """Repository-relative path of a sprite, as the engine looks it up:
    assets/tiles/<layer>/<category>/<tile>/[<variation>/]<frame>.png. The frame
    defaults to the entry's first, which is named after the tile unless it lists frames."""
    parts = ["assets", "tiles", entry["layer"], entry["category"], entry["tile"]]
    if variation_name:
        parts.append(variation_name)
    return "/".join(parts + [(frame_name or entry["frames"][0]["name"]) + ".png"])


def jobs(entries):
    """One render job per generated variation and frame."""
    out = []
    for e in entries:
        if not e["generated"]:
            continue
        for v in e["variations"]:
            for f in e["frames"]:
                out.append(dict(tile=e["tile"], layer=e["layer"], category=e["category"], recipe=e["recipe"],
                                params={**e["params"], **v["params"], **f["params"]}, seed=v["seed"], palette=e["palette"],
                                density=e["density"], variation=v["name"], frame=f["name"],
                                path=sprite_path(e, v["name"], f["name"]), validate=e["validate"], source=e["source"]))
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
