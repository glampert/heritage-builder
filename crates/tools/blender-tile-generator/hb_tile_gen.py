# hb_tile_gen.py — Heritage Builder isometric tile generator for Blender 5.x
#
# Renders one tile from a recipe. The code lives in the hbgen/ package next to
# this file; build.py renders a whole catalog. See README.md.
#
# Game tiles (engine camera, 8 px per logical px):
#   Blender -b -P hb_tile_gen.py -- --building roof_tile_kiln --out x.png
# Fortress studies (legacy camera):
#   Blender -b -P hb_tile_gen.py -- --building fortress --cells 15 --scale 1 \
#       --palette viet --out ~/Desktop/fortress_tile.png [--blend out.blend]
#
# Inside a running Blender (Scripting tab or the MCP execute tool), pass the path
# as __file__ so the package is found:
#   p = '/path/hb_tile_gen.py'; exec(open(p).read(), {'__file__': p, '__name__': 'hb'}, g := {})
# or set HB_TILE_GEN_DIR first and exec into the current namespace:
#   HB_TILE_GEN_DIR = '/path'; exec(open(HB_TILE_GEN_DIR + '/hb_tile_gen.py').read())
#   generate_engine('roof_tile_kiln', out='/tmp/x.png')
# Each exec reloads hbgen, so edits to the package take effect without restarting.

import os
import sys

if "__file__" in globals():
    _HERE = os.path.dirname(os.path.abspath(__file__))
else:
    _HERE = globals().get("HB_TILE_GEN_DIR") or os.environ.get("HB_TILE_GEN_DIR")
    if not _HERE:
        raise RuntimeError("hb_tile_gen.py: set HB_TILE_GEN_DIR (or exec with __file__) so hbgen/ can be found")
if _HERE not in sys.path:
    sys.path.insert(0, _HERE)
for _m in [m for m in sys.modules if m == "hbgen" or m.startswith("hbgen.")]:
    del sys.modules[_m]                                     # pick up edits in a live Blender session

import bpy  # noqa: E402

from hbgen.generate import generate, generate_engine  # noqa: E402,F401
from hbgen.palettes import PALETTES  # noqa: E402
from hbgen.recipes import LEGACY_CAMERA, RECIPES  # noqa: E402
from hbgen.scene import DEFAULT_DENSITY  # noqa: E402

BUILDINGS = RECIPES                                         # the name the README and older calls use


def _cli():
    import argparse, json
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    ap = argparse.ArgumentParser(prog="hb_tile_gen.py")
    ap.add_argument("--building", default="fortress", choices=list(RECIPES))
    ap.add_argument("--palette", default="viet", choices=list(PALETTES))
    ap.add_argument("--cells", type=int, default=15, help="legacy camera: footprint in grid cells (render width = cells*64*scale px)")
    ap.add_argument("--scale", type=float, default=1.0, help="legacy camera: pixels per logical pixel")
    ap.add_argument("--height", type=float, default=1.0, help="legacy camera: render height as a factor of width")
    ap.add_argument("--density", type=int, default=DEFAULT_DENSITY, help="engine camera: PNG px per logical px")
    ap.add_argument("--seed", type=int, default=0, help="engine camera: seed for the recipe's random choices")
    ap.add_argument("--params", default="{}", help="engine camera: recipe parameters as a JSON object")
    ap.add_argument("--out", required=True); ap.add_argument("--blend", default=None)
    ap.add_argument("--no-trim", action="store_true", help="legacy camera: keep the full square render instead of cropping empty rows off the top")
    a = ap.parse_args(argv)
    if a.building in LEGACY_CAMERA:
        print(generate(a.building, a.palette, a.cells, a.scale, a.height, a.out, a.blend, trim=not a.no_trim))
    else:
        res = generate_engine(a.building, a.palette, a.density, a.out, a.blend, params=json.loads(a.params), seed=a.seed)
        print("HB_RESULT " + json.dumps(res))


# Only when Blender runs this file (-P hb_tile_gen.py), not when it's exec'd in a session.
if __name__ == "__main__" and bpy.app.background and any(os.path.basename(a) == "hb_tile_gen.py" for a in sys.argv):
    _cli()
