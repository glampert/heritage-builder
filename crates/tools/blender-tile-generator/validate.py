#!/usr/bin/env python3
"""Check the generator's sprites against the engine's format and write contact
sheets. Plain Python 3 (standard library only); no Blender needed.

    python3 crates/tools/blender-tile-generator/validate.py [TILE ...] [--no-contact]

Checks every sprite the manifest records for the catalog's generated tiles (or
only the named ones), and writes build/contact_<catalog>.png per catalog file.
Exit status: 0 no errors (warnings allowed), 1 errors, 2 a malformed catalog.
"""

import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hbgen import catalog as cat  # noqa: E402
from hbgen import checks  # noqa: E402

BUILD_DIR = os.path.join(cat.TOOL_DIR, "build")


def run(entries, manifest, only=None, contact=True, root=cat.REPO_ROOT, out=print):
    """Validate the sprites; returns the number of errors."""
    sprites = manifest.get("sprites", {})
    nerr = 0
    by_source = {}
    for job in cat.jobs(entries):
        if only and job["tile"] not in only:
            continue
        rec = sprites.get(job["path"]); full = os.path.join(root, job["path"])
        if rec is None or not os.path.exists(full):
            out(f"error: {job['path']}: not rendered; run build.py"); nerr += 1; continue
        errors, warnings, stats = checks.check_sprite(full, rec, job["palette"], job["validate"].get("ground", True))
        label = "/".join(x for x in (job["tile"], job["variation"], job["frame"] if job["frame"] != job["tile"] else "") if x)
        out(f"{'FAIL' if errors else 'ok  '} {label}: {stats['size'][0]}x{stats['size'][1]}, "
            f"luminance {stats['luminance']}, saturation {stats['saturation']}")
        for e in errors: out(f"  error: {e}")
        for w in warnings: out(f"  warning: {w}")
        nerr += len(errors)
        by_source.setdefault(job["source"], []).append((full, rec["density"]))
    if contact:
        for source, items in sorted(by_source.items()):
            path = os.path.join(BUILD_DIR, "contact_" + os.path.splitext(source)[0] + ".png")
            checks.contact_sheet(path, [p for p, _ in items], densities=dict(items))
            out(f"contact sheet: {os.path.relpath(path, root)} ({len(items)} sprite{'s' if len(items) != 1 else ''}, in catalog order)")
    return nerr


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("tiles", nargs="*", help="only these tiles")
    ap.add_argument("--no-contact", action="store_true", help="skip the contact sheets")
    a = ap.parse_args(argv)
    try:
        entries = cat.load()
    except (cat.CatalogError, ValueError) as e:
        print(f"catalog: {e}", file=sys.stderr); return 2
    return 1 if run(entries, cat.load_manifest(), set(a.tiles), not a.no_contact) else 0


if __name__ == "__main__":
    sys.exit(main())
