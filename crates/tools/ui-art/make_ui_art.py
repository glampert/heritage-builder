#!/usr/bin/env python3
"""Draws the realm UI skins' sprites from the mockup generator (docs/plans/realm-ui.md, Art pipeline).

    python3 crates/tools/ui-art/make_ui_art.py [--realm R] [ID ...]   # render what changed
    python3 crates/tools/ui-art/make_ui_art.py --check                 # CI: no rendering
    python3 crates/tools/ui-art/make_ui_art.py --list | --measure ID | --prune | --force

art.json lists the sprites: per realm, a generator call, its size and bleed, 9-slice borders and
states. Each renders to assets/ui/skins/<realm>/<id>[_<state>].png at 2x, on a transparent
background, with Playwright's Chromium. manifest.json records what this script wrote, with the
hash of each sprite's source (its SVG) and of the PNG: unchanged sprites are skipped, and a file
the manifest doesn't list (hand-made art), or one changed since it was written, is never
overwritten without --force.

--check needs only the standard library. Rendering needs Pillow and numpy (the generator's
imports), Node.js and Playwright. Exit status: 0 ok, 1 errors, 2 a malformed catalog.
"""

import argparse
import os
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from uiart import catalog as cat  # noqa: E402


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("ids", nargs="*", metavar="ID", help="sprite ids to render (default: all)")
    ap.add_argument("--realm", action="append", choices=cat.REALMS, help="only this realm (repeatable)")
    ap.add_argument("--check", action="store_true", help="check the catalog, sprites, manifest and skins; render nothing")
    ap.add_argument("--list", action="store_true", help="list the sprites and stop")
    ap.add_argument("--measure", action="store_true",
                    help="print how far each selected sprite's drawing reaches past its rect (for the catalog's bleed)")
    ap.add_argument("--force", action="store_true",
                    help="render everything selected, and overwrite files the manifest doesn't list or that changed "
                         "(with --prune: delete changed files too)")
    ap.add_argument("--prune", action="store_true",
                    help="forget sprites the manifest lists but the catalog no longer has, and delete those "
                         "still as this script wrote them (--force: delete changed ones too)")
    ap.add_argument("--root", default=cat.REPO_ROOT, help="repository root (default: this checkout)")
    a = ap.parse_args(argv)

    try:
        records = cat.load_catalog()
        manifest = cat.load_manifest()
        chosen = cat.select(records, a.realm, a.ids)
    except (cat.CatalogError, ValueError) as e:
        print(f"art.json: {e}", file=sys.stderr)
        return 2

    if a.check:
        errors = cat.check(a.root, records, manifest)
        for e in errors:
            print(f"error: {e}", file=sys.stderr)
        if errors:
            return 1
        n = sum(1 for _ in cat.outputs(records))
        print(f"ui-art: {n} sprites match the catalog and manifest")
        return 0

    if a.list:
        for rec, st, path in cat.outputs(chosen):
            slc = f" slice {rec['slice']} {rec['center']}" if rec["slice"] else ""
            print(f"{path}  {rec['png_size'][0]}x{rec['png_size'][1]}  {rec['call']}{slc}")
        return 0

    if a.prune:
        return prune(a.root, records, manifest, a.force)

    from uiart import render
    if a.measure:
        return measure(render, chosen)
    return run(render, a.root, chosen, records, manifest, a.force)


def run(render, root, chosen, records, manifest, force):
    sprites = manifest["sprites"]
    jobs, unchanged, skipped = [], 0, []
    for rec, st, path in cat.outputs(chosen):
        svg, source = render.sprite_svg(rec, st)
        full = os.path.join(root, path)
        exists = os.path.exists(full)
        decision = cat.render_decision(exists, sprites.get(path), cat.file_hash(full) if exists else None,
                                       source, force)
        if decision == cat.SKIP_UNLISTED:
            skipped.append(f"{path}: not made by this script (see manifest.json); --force to overwrite")
            continue
        if decision == cat.SKIP_CHANGED:
            skipped.append(f"{path}: changed since this script wrote it; --force to overwrite")
            continue
        if decision == cat.UNCHANGED:
            unchanged += 1
            continue
        jobs.append({"rec": rec, "state": st, "path": path, "svg": svg, "source": source, "css": st["css"],
                     "page": render.layout(rec)[0]})

    scale = {j["rec"]["scale"] for j in jobs}
    if len(scale) > 1:
        raise SystemExit("one scale per catalog")
    shots = render.shoot(jobs, scale.pop() if scale else 2)
    spills = []
    try:
        for job, page_png in zip(jobs, shots):
            rec, path = job["rec"], job["path"]
            png, spill = render.finish(rec, page_png)
            if any(v > 0 for v in spill):
                sides = ", ".join(f"{n} {v:g}" for n, v in zip(("left", "top", "right", "bottom"), spill) if v > 0)
                spills.append(f"{path}: the drawing reaches past the sprite ({sides} pt); raise its bleed")
            full = os.path.join(root, path)
            os.makedirs(os.path.dirname(full), exist_ok=True)
            with open(full, "wb") as f:
                f.write(png)
            sprites[path] = {"realm": rec["realm"], "id": rec["id"], "state": job["state"]["state"],
                             "source_hash": job["source"], "png_hash": cat.file_hash(full),
                             "png_size": cat.png_size(full)}
    finally:
        # Also when a run stops partway: the files written so far stay this script's.
        cat.save_manifest(manifest)

    print(f"ui-art: rendered {len(jobs)}, unchanged {unchanged}, skipped {len(skipped)}")
    for s in skipped:
        print(f"skipped {s}", file=sys.stderr)
    for s in spills:
        print(f"warning: {s}", file=sys.stderr)
    orphans = [p for p in sprites if p not in {path for _, _, path in cat.outputs(records)}]
    for p in orphans:
        print(f"warning: {p} is in manifest.json but not the catalog; --prune deletes it if unchanged since written", file=sys.stderr)
    missing_web = cat.check_web_manifest(root, [j["path"] for j in jobs])
    if missing_web:
        print("note: new sprites; run `cargo run -p web-builder -- manifest` to list them in web/asset_manifest.json")
    return 1 if spills else 0


def measure(render, chosen):
    jobs = []
    for rec in chosen:
        st = rec["states"][0]
        svg, _ = render.sprite_svg(rec, st)
        jobs.append({"rec": rec, "svg": svg, "css": "", "page": render.layout(rec)[0]})
    shots = render.shoot(jobs, chosen[0]["scale"] if chosen else 2)
    for job, page_png in zip(jobs, shots):
        rec = job["rec"]
        ext = render.extent(rec, page_png)
        label = f"{rec['realm']}/{rec['id']}"
        if ext is None:
            print(f"{label}: draws nothing")
            continue
        drawn = ", ".join(f"{v:+.1f}" for v in ext)
        print(f"{label}: size {rec['size']}, drawn past the rect (l, t, r, b) {drawn} pt; bleed {rec['bleed']}")
    return 0


def prune(root, records, manifest, force):
    keep = {path for _, _, path in cat.outputs(records)}
    gone = [p for p in manifest["sprites"] if p not in keep]
    deleted = kept = 0
    for p in gone:
        full = os.path.join(root, p)
        exists = os.path.exists(full)
        decision = cat.prune_decision(exists, manifest["sprites"][p], cat.file_hash(full) if exists else None, force)
        if decision == cat.DELETE:
            os.remove(full)
            deleted += 1
            print(f"deleted {p}")
        elif decision == cat.KEEP_CHANGED:
            kept += 1
            print(f"kept {p}: changed since this script wrote it, so it's hand-made art now; --force to delete it",
                  file=sys.stderr)
        del manifest["sprites"][p]
    cat.save_manifest(manifest)
    print(f"ui-art: pruned {len(gone)} from manifest.json, deleted {deleted}, kept {kept}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
