#!/usr/bin/env python3
"""Merge the catalog's generated tiles into assets/tiles/<layer>/tile_set.json.
Plain Python 3 (standard library only); no Blender needed.

    python3 crates/tools/blender-tile-generator/sync_tileset.py          # write
    python3 crates/tools/blender-tile-generator/sync_tileset.py --check  # CI: report drift, write nothing

It also checks that every rendered sprite exists at the size the manifest
records and is listed in web/asset_manifest.json. Exit status: 0 in sync,
1 drift or errors, 2 a malformed catalog.
"""

import argparse
import difflib
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hbgen import catalog as cat  # noqa: E402
from hbgen import tileset  # noqa: E402


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--check", action="store_true", help="report differences and errors without writing")
    ap.add_argument("--root", default=cat.REPO_ROOT, help="repository root (default: this checkout)")
    ap.add_argument("--catalog-dir", default=None, help="catalog directory (default: the tool's catalog/)")
    a = ap.parse_args(argv)
    cdir = a.catalog_dir or os.path.join(a.root, os.path.relpath(cat.CATALOG_DIR, cat.REPO_ROOT))
    try:
        entries = cat.load(catalog_dir=cdir)
    except (cat.CatalogError, ValueError) as e:
        print(f"catalog: {e}", file=sys.stderr); return 2
    manifest = cat.load_manifest(os.path.join(cdir, cat.MANIFEST_NAME))
    changes, notes, errors = tileset.sync(a.root, entries, manifest, write=not a.check)
    for n in notes:
        print(f"note: {n}")
    for path, (old, new) in changes.items():
        rel = os.path.relpath(path, a.root)
        if a.check:
            print(f"{rel} is out of date with the catalog:")
            sys.stdout.writelines(difflib.unified_diff(old.splitlines(True), new.splitlines(True), rel, rel + " (synced)"))
        else:
            print(f"updated {rel}")
    for e in errors:
        print(f"error: {e}", file=sys.stderr)
    generated = sum(1 for e in entries if e["generated"])
    if errors or (a.check and changes):
        return 1
    print(f"tile sets in sync with {generated} generated tile{'s' if generated != 1 else ''}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
