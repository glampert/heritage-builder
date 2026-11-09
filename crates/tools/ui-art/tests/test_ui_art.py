"""Tests for the plain-Python half of ui-art: the catalog, the manifest, --check and the SVG
helpers. Standard library only; no Pillow, generator or browser.

    python3 -m unittest discover -s crates/tools/ui-art/tests
"""

import copy
import json
import os
import struct
import sys
import tempfile
import unittest
import zlib

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from uiart import catalog as cat  # noqa: E402
from uiart import render  # noqa: E402

BASE = {
    "scale": 2,
    "pad": 40,
    "state_filters": {"hovered": "brightness(1.1)", "disabled": "grayscale(1)"},
    "sprites": [
        {"id": "panel", "call": "box", "size": [60, 40], "bleed": [2, 2, 4, 6], "slice": [20, 20, 24, 28],
         "center": "tile"},
        {"id": "button", "call": "button", "size": [50, 20], "states": ["idle", "hovered", "disabled"],
         "realms": {"dailong": {}, "kambara": {"args": {"primary": True}, "bleed": [1, 1, 1, 1]}}},
    ],
}


def expand(change=None):
    raw = copy.deepcopy(BASE)
    if change:
        change(raw)
    return cat.expand(raw)


def write_png(path, w, h):
    """A blank RGBA PNG, with the standard library."""
    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)
    raw = b"".join(b"\x00" + bytes(4 * w) for _ in range(h))
    data = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(data)


class CatalogTest(unittest.TestCase):
    def test_expand(self):
        recs = expand()
        self.assertEqual(len(recs), 3 + 2)   # panel in every realm, button in two
        panel = next(r for r in recs if r["realm"] == "tampadesa" and r["id"] == "panel")
        self.assertEqual(panel["png_size"], [132, 96])           # (60 + 6, 40 + 8) x 2
        self.assertEqual(panel["states"][0]["out"], "skins/tampadesa/panel.png")
        self.assertIsNone(panel["states"][0]["state"])
        btn = next(r for r in recs if r["realm"] == "kambara" and r["id"] == "button")
        self.assertEqual(btn["args"], {"primary": True})
        self.assertEqual(btn["png_size"], [104, 44])
        self.assertEqual([s["out"] for s in btn["states"]],
                         [f"skins/kambara/button_{s}.png" for s in ("idle", "hovered", "disabled")])
        self.assertEqual([s["css"] for s in btn["states"]], ["", "brightness(1.1)", "grayscale(1)"])
        self.assertEqual(btn["center"], None)

    def test_state_overrides_and_rotate(self):
        def change(raw):
            raw["sprites"].append({"id": "knob", "call": "slider", "size": [64, 28], "rotate": 90, "bleed": [-8, 1, -8, 1],
                                   "states": {"idle": {}, "pressed": {"args": {"v": 1}, "css": "brightness(0.9)"},
                                              "selected": {"call": "toggle"}}})
        knob = next(r for r in expand(change) if r["id"] == "knob")
        self.assertEqual(knob["png_size"], [24, 132])            # turned: (28 - 16) x (64 + 2)
        pressed = knob["states"][1]
        self.assertEqual((pressed["args"], pressed["css"]), ({"v": 1}, "brightness(0.9)"))
        self.assertEqual(knob["states"][2]["call"], "toggle")

    def test_select(self):
        recs = expand()
        self.assertEqual(len(cat.select(recs, ["kambara"])), 2)
        self.assertEqual(len(cat.select(recs, None, ["button"])), 2)
        with self.assertRaises(cat.CatalogError):
            cat.select(recs, None, ["nope"])
        # An id that other realms have, but not the chosen one, is an error too.
        self.assertEqual(len(cat.select(recs, ["kambara"], ["button"])), 1)
        with self.assertRaises(cat.CatalogError) as ctx:
            cat.select(recs, ["tampadesa"], ["button"])
        self.assertIn("in tampadesa", str(ctx.exception))

    def assertRejects(self, change, words):
        with self.assertRaises(cat.CatalogError) as ctx:
            expand(change)
        self.assertIn(words, str(ctx.exception))

    def test_rejects(self):
        def entry(**kw):
            def change(raw):
                raw["sprites"][0].update(kw)
            return change
        self.assertRejects(entry(colour="red"), "unknown keys")
        self.assertRejects(entry(call="teapot"), "unknown call")
        self.assertRejects(entry(id="Panel"), "id must match")
        self.assertRejects(entry(slice=[80, 20, 80, 20]), "doesn't fit")
        self.assertRejects(entry(slice=[66, 20, 66, 20]), "needs a middle")
        self.assertRejects(entry(slice=[20, 20, 24, 28.5]), "whole pixels")
        self.assertRejects(entry(slice=None), "center without slice")
        self.assertRejects(entry(center="spread"), "center must be")
        self.assertRejects(entry(states=["idle", "sleepy"]), "unknown state")
        self.assertRejects(entry(out="ui/{realm}/{id}.png"), "under skins/")
        self.assertRejects(entry(out="skins/../{id}.png"), "under skins/")
        self.assertRejects(entry(bleed=[40, 0, 0, 0]), "pad")
        self.assertRejects(entry(size=[60.25, 40]), "whole pixels")
        self.assertRejects(entry(bleed=[-60, 0, 0, 0]), "positive")
        self.assertRejects(entry(rotate=45), "rotate")
        self.assertRejects(entry(states=["idle", "hovered"], out="skins/{realm}/{id}.png"), "share an output")
        self.assertRejects(entry(realms={"atlantis": {}}), "unknown realm")
        self.assertRejects(entry(realms={"dailong": {"id": "x"}}), "unknown keys")

        def dup(raw):
            raw["sprites"].append(copy.deepcopy(raw["sprites"][0]))
        self.assertRejects(dup, "id used twice")

        def same_out(raw):
            raw["sprites"].append({"id": "other", "call": "box", "size": [10, 10], "out": "skins/{realm}/panel.png"})
        self.assertRejects(same_out, "also")

    def test_real_catalog(self):
        recs = cat.load_catalog()
        self.assertTrue(recs)
        self.assertEqual({r["realm"] for r in recs}, set(cat.REALMS))
        for rec in recs:
            if rec["center"] == "tile":
                mx, my, mw, mh = render.middle(rec)
                self.assertGreater(mw, 0, rec["id"])
                self.assertGreater(mh, 0, rec["id"])

    def test_calls_match(self):
        self.assertEqual(set(render.CALLS), set(cat.KNOWN_CALLS))


class CheckTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = self.tmp.name
        self.recs = expand()
        self.manifest = {"version": 1, "sprites": {}}
        for rec, st, path in cat.outputs(self.recs):
            full = os.path.join(self.root, path)
            write_png(full, *rec["png_size"])
            self.manifest["sprites"][path] = {"png_hash": cat.file_hash(full), "png_size": rec["png_size"]}

    def tearDown(self):
        self.tmp.cleanup()

    def errors(self):
        return cat.check(self.root, self.recs, self.manifest)

    def test_clean(self):
        self.assertEqual(self.errors(), [])

    def test_missing_and_wrong_size(self):
        os.remove(os.path.join(self.root, "assets/ui/skins/dailong/panel.png"))
        write_png(os.path.join(self.root, "assets/ui/skins/kambara/panel.png"), 10, 10)
        errs = self.errors()
        self.assertTrue(any("dailong/panel.png" in e and "missing" in e for e in errs), errs)
        self.assertTrue(any("kambara/panel.png" in e and "is 10x10 px" in e for e in errs), errs)
        self.assertTrue(any("kambara/panel.png" in e and "changed since" in e for e in errs), errs)

    def test_manifest(self):
        del self.manifest["sprites"]["assets/ui/skins/dailong/button_idle.png"]
        self.manifest["sprites"]["assets/ui/skins/dailong/old.png"] = {"png_hash": "x", "png_size": [1, 1]}
        errs = self.errors()
        self.assertTrue(any("button_idle.png" in e and "not in manifest.json" in e for e in errs), errs)
        self.assertTrue(any("old.png" in e and "--prune" in e for e in errs), errs)

    def test_web_manifest(self):
        listed = [p for _, _, p in cat.outputs(self.recs)]
        os.makedirs(os.path.join(self.root, "web"))
        with open(os.path.join(self.root, cat.WEB_MANIFEST), "w") as f:
            json.dump(listed[1:], f)
        errs = self.errors()
        self.assertEqual(len(errs), 1)
        self.assertIn(listed[0], errs[0])

    def write_skin(self, realm, skin):
        path = os.path.join(self.root, cat.SKIN_CONFIG_DIR, f"{realm}.json")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            json.dump(skin, f)

    def test_skins(self):
        self.write_skin("dailong", {
            "panels": {"dialog": {"sprite": "skins/dailong/panel.png", "slice": [20, 20, 24, 28], "center": "tile"}},
            "buttons": {"primary": {"sprite": "skins/dailong/button", "content": [1, 2, 3, 4]}},
            "backdrops": {"home": "misc/home_menu_static_bg.png"},
        })
        self.assertEqual(self.errors(), [])
        self.write_skin("kambara", {
            "panels": {"dialog": {"sprite": "skins/kambara/panel.png", "slice": [20, 20, 20, 20], "center": "stretch"}},
            "buttons": {"primary": {"sprite": "skins/kambara/buton"}},
            "emblems": ["skins/kambara/emblem.png"],
            "controls": {"knob": {"sprite": "skins/kambara/button", "slice": [1, 1, 1, 1]}},
        })
        errs = self.errors()
        self.assertTrue(any("kambara.json:panels:dialog" in e and "slice [20, 20, 20, 20]" in e for e in errs), errs)
        self.assertTrue(any("center 'stretch'" in e for e in errs), errs)
        self.assertTrue(any("skins/kambara/buton doesn't exist" in e for e in errs), errs)
        self.assertTrue(any("skins/kambara/emblem.png doesn't exist" in e for e in errs), errs)
        self.assertTrue(any("isn't a 9-slice" in e for e in errs), errs)


    def test_skin_folders(self):
        # The plan's icon sets name a folder, not a sprite.
        os.makedirs(os.path.join(self.root, cat.UI_DIR, "skins/dailong/icons"))
        self.write_skin("dailong", {"buttons": {"icon": {"folder": "skins/dailong/icons", "frame": "skins/dailong/button"}}})
        self.assertEqual(self.errors(), [])
        self.write_skin("dailong", {"buttons": {"icon": {"folder": "skins/dailong/glyphs"}}})
        errs = self.errors()
        self.assertTrue(any("skins/dailong/glyphs doesn't exist" in e for e in errs), errs)


class DecisionTest(unittest.TestCase):
    """The rule that the script never overwrites or deletes hand-made art without --force."""
    ENTRY = {"png_hash": "aaa", "source_hash": "src"}

    def test_render(self):
        d = cat.render_decision
        self.assertEqual(d(False, None, None, "src", False), cat.RENDER)               # a new sprite
        self.assertEqual(d(False, self.ENTRY, None, "src", False), cat.RENDER)         # deleted by hand
        self.assertEqual(d(True, None, "zzz", "src", False), cat.SKIP_UNLISTED)        # hand-made art
        self.assertEqual(d(True, self.ENTRY, "zzz", "new", False), cat.SKIP_CHANGED)   # edited by hand
        self.assertEqual(d(True, self.ENTRY, "aaa", "src", False), cat.UNCHANGED)
        self.assertEqual(d(True, self.ENTRY, "aaa", "new", False), cat.RENDER)         # the drawing changed
        for known, h in ((None, "zzz"), (self.ENTRY, "zzz"), (self.ENTRY, "aaa")):
            self.assertEqual(d(True, known, h, "src", True), cat.RENDER)

    def test_prune(self):
        d = cat.prune_decision
        self.assertEqual(d(True, self.ENTRY, "aaa", False), cat.DELETE)
        self.assertEqual(d(True, self.ENTRY, "zzz", False), cat.KEEP_CHANGED)
        self.assertEqual(d(True, self.ENTRY, "zzz", True), cat.DELETE)
        self.assertEqual(d(False, self.ENTRY, None, False), cat.GONE)


class ManifestTest(unittest.TestCase):
    def test_round_trip(self):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "manifest.json")
            self.assertEqual(cat.load_manifest(path), {"version": 1, "sprites": {}})
            m = {"version": 1, "sprites": {"b": {"png_size": [1, 2]}, "a": {"png_size": [3, 4]}}}
            cat.save_manifest(m, path)
            with open(path) as f:
                text = f.read()
            self.assertLess(text.index('"a"'), text.index('"b"'))
            self.assertEqual(cat.load_manifest(path), m)
            with open(path, "w") as f:
                json.dump({"files": []}, f)
            with self.assertRaises(cat.CatalogError):
                cat.load_manifest(path)

    def test_png_size(self):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "x.png")
            write_png(path, 7, 3)
            self.assertEqual(cat.png_size(path), [7, 3])
            with open(path, "wb") as f:
                f.write(b"GIF89a" + bytes(20))
            with self.assertRaises(ValueError):
                cat.png_size(path)


class SvgTest(unittest.TestCase):
    def test_split_top_level(self):
        svg = ('<rect x="1"/><g transform="rotate(4 1 1)"><rect/><g><path d="M0,0"/></g></g>'
               '<path d="M1,1 L2,2" stroke="#000"/><text x="2">a &gt; b</text>')
        parts = render.split_top_level(svg)
        self.assertEqual(len(parts), 4)
        self.assertTrue(parts[1].startswith("<g ") and parts[1].endswith("</g></g>"))
        self.assertEqual("".join(parts), svg)
        with self.assertRaises(ValueError):
            render.split_top_level("<g><rect/>")

    def test_tile_frequency(self):
        self.assertEqual(render.tile_frequency("0.008", 128, 128, 3), "0.023438")
        self.assertEqual(render.tile_frequency("0.9", 128, 128, 3), "0.9")
        self.assertEqual(render.tile_frequency("0.004 0.1", 100, 200, 2), "0.02 0.1")

    def test_tile_contrast(self):
        self.assertEqual(render.tile_contrast("0.008", 1.0, -0.42, 0.5), (0.5, -0.17))
        self.assertEqual(render.tile_contrast("0.004 0.3", 1.0, -0.4, 0.5), (0.5, -0.15))
        self.assertEqual(render.tile_contrast("0.6", 0.5, -0.2, 0.5), (0.5, -0.2))

    def test_layout(self):
        rec = expand()[0]
        page, origin, crop = render.layout(rec)
        self.assertEqual((page, origin, crop), ((140, 120), (40, 40), (38, 38, 66, 48)))
        self.assertEqual(render.middle(rec), (48, 48, 44, 24))
        turned = dict(rec, size=[64, 28], bleed=[-8, 1, -8, 1], rotate=90)
        page, origin, crop = render.layout(turned)
        self.assertEqual(page, (144, 144))
        self.assertEqual(crop, (72 - 14 + 8, 72 - 32 - 1, 12, 66))

    def test_make_tileable(self):
        class Theme:
            def __init__(self):
                self.defs, self.n = [], 0

            def nid(self, p):
                self.n += 1
                return f"{p}{self.n}"

            def lin(self, stops, x1=0, y1=0, x2=0, y2=1):
                self.defs.append("<linearGradient/>")
                return "url(#lg)"

            def rad(self, *a):
                return "url(#rg)"

            def gold(self):
                self.defs.append('<linearGradient id="g"><stop offset="0" stop-color="#111"/>'
                                 '<stop offset="1" stop-color="#222"/></linearGradient>')
                return "url(#g)"

        th = Theme()
        render.make_tileable(th, (100, 50, 120, 80), (400, 300))
        rect = th.noise(90, 40, 200, 100, "0.008", 4, (0.5, 0.4, 0.3), 1.0, -0.4, 7)
        self.assertRegex(rect, r'filter="url\(#nz\d+\)"')
        f = th.defs[-1]
        self.assertIn('stitchTiles="stitch" x="0" y="0" width="120" height="80"', f)
        self.assertIn('<feOffset dx="100" dy="50" x="100" y="50" width="120" height="80"/><feTile/>', f)
        self.assertIn('filterUnits="userSpaceOnUse" x="0" y="0" width="290" height="140"', f)
        self.assertIn('baseFrequency="0.041667 0.0625"', f)          # at least 5 periods per tile
        # Blotches keep 45% of their contrast about their middle tone (alpha 0.5 x 1.0 - 0.4).
        self.assertIn('0 0 0 0 0.3 0.45 0 0 0 -0.125"/>', f)
        th.noise(0, 0, 10, 10, "0.9", 2, (0.5, 0.4, 0.3), 0.3, -0.06, 8)
        self.assertIn('0 0 0 0 0.3 0.3 0 0 0 -0.06"/>', th.defs[-1])  # grain keeps its own
        self.assertEqual(th.lin([(0, "#100"), (0.5, "#200"), (1, "#300")], 0, 0, 1, 0), "#200")
        self.assertEqual(th.lin([(0, "#100"), (1, "#300")]), "url(#lg)")
        self.assertEqual(th.rad([(0, "#000", 0)]), "none")
        self.assertIn("url(#gold", th.gold())
        g = next(d for d in th.defs if 'id="gold' in d)
        self.assertIn('spreadMethod="repeat"', g)
        self.assertIn('x1="100" y1="50" x2="120" y2="70"', g)     # gcd(120, 80) = 40: half of it
        self.assertIn('stop-color="#222"', g)
        self.assertRegex(th.rough(), r"url\(#rough\d+\)")
        self.assertIn('<feTile result="t"/><feDisplacementMap', th.defs[-1])


if __name__ == "__main__":
    unittest.main()
