"""Render the catalog into assets/tiles/ and keep tile_set.json in step.
Runs inside Blender 5.x; tilegen.sh wraps it:

    Blender -b --factory-startup -P build.py -- [TILE ...] [options]

Options:
    TILE ...         only these tiles (default: every generated tile in the catalog)
    --catalog FILE   only this catalog file (repeatable; default: catalog/*.json)
    --fast           32 Eevee samples instead of 96, for iterating; recorded in the
                     manifest, and sync refuses fast renders until they're redone
    --force          render even when the hash says the sprite is up to date
    --adopt TILE     allow overwriting this tile's existing sprites that the manifest
                     doesn't know (e.g. ones rendered by hand); otherwise they count
                     as hand-made and are never overwritten
    --dry-run        list what would render, and stop
    --no-sync        don't update tile_set.json
    --no-validate    skip the checks and contact sheets

A sprite is rendered when its hash changes: the catalog entry's settings, the
generator's core modules, the recipe's module, the forced height, --fast and
GENERATOR_VERSION. Exit status: 0 success, 1 a render, check or sync failed,
2 a malformed catalog or unknown recipe.
"""

import argparse
import inspect
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)
for _m in [m for m in sys.modules if m == "hbgen" or m.startswith("hbgen.")]:
    del sys.modules[_m]

from hbgen import catalog as cat  # noqa: E402
from hbgen import tileset  # noqa: E402
from hbgen.generate import generate_engine  # noqa: E402
from hbgen.palettes import PALETTES  # noqa: E402
from hbgen.recipes import LEGACY_CAMERA, RECIPES  # noqa: E402

# The recipe registry (recipes/__init__.py) only maps names, so it isn't hashed:
# registering a recipe doesn't re-render the others.
CORE_SOURCES = [os.path.join(HERE, "hbgen", f) for f in ("palettes.py", "materials.py", "builder.py", "scene.py", "generate.py")]
FAST_SAMPLES = 32


def log(msg):
    print(msg, flush=True)


def parse_args():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    ap = argparse.ArgumentParser(prog="build.py", description="Render the tile catalog.")
    ap.add_argument("tiles", nargs="*")
    ap.add_argument("--catalog", action="append", default=[])
    ap.add_argument("--fast", action="store_true"); ap.add_argument("--force", action="store_true")
    ap.add_argument("--adopt", action="append", default=[]); ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--no-sync", action="store_true"); ap.add_argument("--no-validate", action="store_true")
    return ap.parse_args(argv)


def main():
    a = parse_args()
    try:
        entries = cat.load([os.path.abspath(p) for p in a.catalog] or None)
    except (cat.CatalogError, ValueError) as e:
        log(f"catalog: {e}"); return 2
    known = {e["tile"] for e in entries}
    unknown = set(a.tiles) - known
    if unknown:
        log(f"not in the catalog: {', '.join(sorted(unknown))}"); return 2
    selected = [e for e in entries if e["generated"] and (not a.tiles or e["tile"] in a.tiles)]
    for e in selected:
        if e["recipe"] not in RECIPES or e["recipe"] in LEGACY_CAMERA:
            log(f"{e['tile']}: no engine recipe '{e['recipe']}' (have: {', '.join(sorted(set(RECIPES) - LEGACY_CAMERA))})"); return 2
        if e["palette"] not in PALETTES:
            log(f"{e['tile']}: no palette '{e['palette']}'"); return 2

    manifest = cat.load_manifest()
    sprites = manifest.setdefault("sprites", {})
    rendered, skipped, failed = 0, 0, 0
    t_all = time.time()
    for e in selected:
        jobs = cat.jobs([e])
        sources = CORE_SOURCES + [inspect.getsourcefile(RECIPES[e["recipe"]])]
        # A tile def has one draw_size, so a tile's sprites (variations and frames) share the tallest one's height.
        height = None
        if len(jobs) > 1:
            height = max(generate_engine(j["recipe"], j["palette"], j["density"], render=False, params=j["params"],
                                         seed=j["seed"])["png_size"][1] for j in jobs)
        for j in jobs:
            label = "/".join(x for x in (j["tile"], j["variation"], j["frame"] if j["frame"] != j["tile"] else "") if x)
            h = cat.job_hash(j, sources, {"fast": a.fast, "height_px": height})
            rec = sprites.get(j["path"]); full = os.path.join(cat.REPO_ROOT, j["path"])
            if not a.force and rec and rec.get("hash") == h and os.path.exists(full):
                skipped += 1; log(f"up to date  {label}"); continue
            if os.path.exists(full) and rec is None and j["tile"] not in a.adopt:
                failed += 1
                log(f"REFUSED     {label}: {j['path']} exists but the generator didn't write it, so it's hand-made. "
                    f"Pass --adopt {j['tile']} to replace it.")
                continue
            if a.dry_run:
                log(f"would render {label} -> {j['path']}"); continue
            t = time.time()
            try:
                res = generate_engine(j["recipe"], j["palette"], j["density"], out=full, params=j["params"], seed=j["seed"],
                                      samples=FAST_SAMPLES if a.fast else None, height_px=height)
            except Exception as ex:  # keep going: one bad recipe shouldn't stop the batch
                failed += 1; log(f"FAILED      {label}: {type(ex).__name__}: {ex}"); continue
            sprites[j["path"]] = dict(tile=j["tile"], variation=j["variation"], frame=j["frame"], layer=j["layer"], category=j["category"],
                                      hash=h, png_size=res["png_size"], density=j["density"],
                                      logical_size=res["logical_size"], draw_size=res["draw_size"], fast=a.fast)
            cat.save_manifest(manifest)                   # after each render, so a crash keeps the finished ones
            rendered += 1
            log(f"rendered    {label}: {res['png_size'][0]}x{res['png_size'][1]} px, draw_size "
                f"{res['draw_size']['width']}x{res['draw_size']['height']} ({time.time() - t:.1f} s)")
    log(f"{rendered} rendered, {skipped} up to date, {failed} failed ({time.time() - t_all:.1f} s)")
    if a.dry_run:
        return 1 if failed else 0

    if not a.no_validate:
        import validate
        nerr = validate.run(selected, manifest, out=log)
        if nerr:
            failed += 1; log(f"validate: {nerr} error{'s' if nerr != 1 else ''}")
    if not a.no_sync:
        changes, notes, errors = tileset.sync(cat.REPO_ROOT, entries, manifest)
        for n in notes: log(f"note: {n}")
        for p in changes: log(f"updated {os.path.relpath(p, cat.REPO_ROOT)}")
        for err in errors:
            log(f"sync: {err}")
        # A sprite missing from web/asset_manifest.json is expected right after a first render;
        # tilegen.sh regenerates the list next, so only the other errors fail the build.
        if any("asset_manifest" not in err for err in errors):
            failed += 1
        elif not changes:
            log("tile sets already in sync")
    return 1 if failed else 0


sys.exit(main())
