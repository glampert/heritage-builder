"""Tests for the plain-Python half of the tile generator: the catalog, syncing
tile_set.json, and the sprite checks. Standard library only; no Blender.

    python3 -m unittest discover -s crates/tools/blender-tile-generator/tests
"""

import copy
import json
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from hbgen import catalog as cat  # noqa: E402
from hbgen import checks, pngio, tileset  # noqa: E402

# Hand-formatted like the real objects tile set: inline materials, "x" : 2.
TILE_SET = """{
  "layer": "Objects",
  "categories": [
    {
      "name": "buildings",
      "tile_defs": [
        {
          "name": "temple",
          "cost": 15,
          "materials": { "Wood": 4, "Bricks": 4 },
          "logical_size": {
            "width": 192,
            "height": 96
          },
          "variations": [
            {
              "iso_offset": {
                "x" : 2,
                "y" : 27
              },
              "anim_sets": [ { "frames": [ { "name": "temple" } ] } ]
            }
          ]
        }
      ]
    }
  ]
}
"""


def entry(tile, variations=None, **kw):
    raw = dict(tile=tile, **kw)
    if variations is not None:
        raw["variations"] = variations
    with tempfile.TemporaryDirectory() as d:
        with open(os.path.join(d, "c.json"), "w") as f:
            json.dump({"entries": [raw]}, f)
        return cat.load(catalog_dir=d)[0]


def sprite_rec(w=1024, h=760, density=8):
    return dict(png_size=[w, h], density=density, logical_size={"width": w // density, "height": w // density // 2},
                draw_size={"width": w // density, "height": h // density}, hash="x", fast=False)


class Catalog(unittest.TestCase):
    def test_defaults_and_jobs(self):
        e = entry("kiln")
        self.assertEqual((e["layer"], e["category"], e["recipe"], e["density"]), ("objects", "buildings", "kiln", 8))
        self.assertEqual([j["path"] for j in cat.jobs([e])], ["assets/tiles/objects/buildings/kiln/kiln.png"])
        e = entry("kiln", [{"name": "var0", "seed": 1}, {"name": "var1", "seed": 2}])
        self.assertEqual([j["path"] for j in cat.jobs([e])], ["assets/tiles/objects/buildings/kiln/var0/kiln.png",
                                                              "assets/tiles/objects/buildings/kiln/var1/kiln.png"])
        self.assertEqual(cat.jobs([entry("market", generated=False)]), [])

    def test_frames(self):
        e = entry("scaffold_2x2", frames=[{"name": "stage0", "params": {"stage": 0}}, {"name": "stage1", "params": {"stage": 1}}],
                  category="overlays", params={"size": 2})
        js = cat.jobs([e])
        self.assertEqual([j["path"] for j in js], ["assets/tiles/objects/overlays/scaffold_2x2/stage0.png",
                                                    "assets/tiles/objects/overlays/scaffold_2x2/stage1.png"])
        self.assertEqual([j["params"] for j in js], [{"size": 2, "stage": 0}, {"size": 2, "stage": 1}])
        with self.assertRaises(cat.CatalogError): entry("x", frames=[{"name": "a"}, {"name": "a"}])
        with self.assertRaises(cat.CatalogError): entry("x", frames=[{"params": {}}])

    def test_rejects_bad_entries(self):
        with self.assertRaises(cat.CatalogError): entry("kiln", colour="red")
        with self.assertRaises(cat.CatalogError): entry("a" * 25)
        with self.assertRaises(cat.CatalogError): entry("kiln", [{"name": ""}, {"name": "var1"}])
        with tempfile.TemporaryDirectory() as d:
            for n in ("a.json", "b.json"):
                with open(os.path.join(d, n), "w") as f:
                    json.dump({"entries": [{"tile": "kiln"}]}, f)
            with self.assertRaises(cat.CatalogError): cat.load(catalog_dir=d)

    def test_hash_follows_settings_and_sources(self):
        with tempfile.TemporaryDirectory() as d:
            src = os.path.join(d, "recipe.py")
            with open(src, "w") as f: f.write("a = 1\n")
            j = cat.jobs([entry("kiln")])[0]
            h = cat.job_hash(j, [src])
            self.assertEqual(h, cat.job_hash(j, [src]))
            self.assertNotEqual(h, cat.job_hash(dict(j, seed=5), [src]))
            self.assertNotEqual(h, cat.job_hash(j, [src], {"fast": True}))
            with open(src, "w") as f: f.write("a = 2\n")
            self.assertNotEqual(h, cat.job_hash(j, [src]))


class Spans(unittest.TestCase):
    def test_spans_cover_values(self):
        path = os.path.join(cat.REPO_ROOT, "assets", "tiles", "objects", "tile_set.json")
        with open(path, encoding="utf-8") as f:
            text = f.read()
        doc, spans = tileset.parse_spans(text)
        self.assertEqual(doc, json.loads(text))
        for i, c in enumerate(doc["categories"]):
            for j, t in enumerate(c["tile_defs"]):
                s, e = spans[("categories", i, "tile_defs", j)]
                self.assertEqual(json.loads(text[s:e]), t)


class Sync(unittest.TestCase):
    def sync(self, text, entries, sprites):
        notes = []
        return tileset.sync_text(text, entries, sprites, notes), notes

    def test_appends_new_def_and_leaves_the_rest(self):
        e = entry("kiln", tile_set={"cost": 10, "materials": {"Wood": 3}})
        new, _ = self.sync(TILE_SET, [e], {cat.sprite_path(e, ""): sprite_rec()})
        self.assertTrue(new.startswith(TILE_SET[:TILE_SET.rindex("        }\n      ]")]), "text before the new def changed")
        self.assertIn('"x" : 2', new)
        d = json.loads(new)["categories"][0]["tile_defs"][1]
        self.assertEqual(list(d), ["name", "cost", "materials", "logical_size", "draw_size", "variations"])
        self.assertEqual(d["draw_size"], {"width": 128, "height": 95})
        self.assertEqual(list(d["logical_size"]), ["width", "height"])
        self.assertIn('"materials": { "Wood": 3 }', new)
        again, _ = self.sync(new, [e], {cat.sprite_path(e, ""): sprite_rec()})
        self.assertEqual(again, new, "a second sync must change nothing")

    def test_writes_the_minimap_block_on_one_line(self):
        e = entry("kiln", tile_set={"cost": 10, "minimap": {"class": "Field", "landmark": "Temple"}})
        new, _ = self.sync(TILE_SET, [e], {cat.sprite_path(e, ""): sprite_rec()})
        self.assertIn('"minimap": { "class": "Field", "landmark": "Temple" }', new)
        d = json.loads(new)["categories"][0]["tile_defs"][1]
        self.assertEqual(d["minimap"], {"class": "Field", "landmark": "Temple"})

    def test_updates_sizes_in_place(self):
        e = entry("kiln")
        one, _ = self.sync(TILE_SET, [e], {cat.sprite_path(e, ""): sprite_rec()})
        two, _ = self.sync(one, [e], {cat.sprite_path(e, ""): sprite_rec(h=800)})
        self.assertEqual(json.loads(two)["categories"][0]["tile_defs"][1]["draw_size"]["height"], 100)
        self.assertEqual(len(two.splitlines()), len(one.splitlines()))

    def test_variations_are_append_only(self):
        e = entry("kiln", [{"name": "var0"}, {"name": "var1"}])
        sprites = {cat.sprite_path(e, v): sprite_rec() for v in ("var0", "var1", "var2")}
        one, _ = self.sync(TILE_SET, [e], sprites)
        e2 = entry("kiln", [{"name": "var1"}, {"name": "var2"}])
        two, notes = self.sync(one, [e2], sprites)
        names = [v["name"] for v in json.loads(two)["categories"][0]["tile_defs"][1]["variations"]]
        self.assertEqual(names, ["var0", "var1", "var2"])
        self.assertTrue(any("var0" in n for n in notes))

    def test_frames_in_def(self):
        e = entry("scaffold_1x1", frames=[{"name": "stage0"}, {"name": "stage1"}])
        sprites = {j["path"]: sprite_rec(512, 400) for j in cat.jobs([e])}
        new, _ = self.sync(TILE_SET, [e], sprites)
        d = json.loads(new)["categories"][0]["tile_defs"][1]
        self.assertEqual(d["variations"], [{"anim_sets": [{"frames": [{"name": "stage0"}, {"name": "stage1"}]}]}])
        sprites[cat.jobs([e])[1]["path"]] = sprite_rec(512, 480)
        with self.assertRaises(tileset.SyncError):
            self.sync(TILE_SET, [e], sprites)

    def test_refuses_hand_made_def(self):
        # The catalog renders a variation of "temple", whose existing art the generator never wrote.
        e = entry("temple", [{"name": "var9"}])
        with self.assertRaises(tileset.SyncError):
            self.sync(TILE_SET, [e], {cat.sprite_path(e, "var9"): sprite_rec()})

    def test_new_category(self):
        e = entry("dyke", category="infrastructure")
        new, _ = self.sync(TILE_SET, [e], {cat.sprite_path(e, ""): sprite_rec()})
        self.assertEqual([c["name"] for c in json.loads(new)["categories"]], ["buildings", "infrastructure"])

    def test_check_sprites(self):
        with tempfile.TemporaryDirectory() as root:
            e = entry("kiln")
            path = cat.sprite_path(e, "")
            os.makedirs(os.path.dirname(os.path.join(root, path)))
            pngio.write_rgba(os.path.join(root, path), 16, 16, bytearray(16 * 16 * 4))
            os.makedirs(os.path.join(root, "web"))
            with open(os.path.join(root, "web", "asset_manifest.json"), "w") as f:
                json.dump([], f)
            errors = []
            tileset.check_sprites(root, cat.jobs([e]), {path: dict(sprite_rec(16, 16, 1), fast=True)}, errors)
            text = "\n".join(errors)
            self.assertIn("--fast", text); self.assertIn("asset_manifest", text)
            errors = []
            tileset.check_sprites(root, cat.jobs([e]), {path: sprite_rec(16, 24, 1)}, errors)
            self.assertIn("the manifest says", "\n".join(errors))


def diamond_png(path, cells=2, density=2, lift=0, extra_bottom=False):
    """A ground diamond plus a box, anchored like the engine wants (lift moves it up)."""
    w = cells * 64 * density; h = w // 2 + 40 * density
    px = bytearray(w * h * 4)
    cx, base = w / 2, h - 1 - lift
    for y in range(h):
        for x in range(w):
            dy = base - y
            inside = 0 <= dy <= w / 2 and abs(x + 0.5 - cx) <= (dy if dy <= w / 4 else w / 2 - dy) * 2 + 1
            box = abs(x - cx) < w / 6 and base - w / 2 - 20 * density < y < base - w / 8
            if inside or box:
                o = (y * w + x) * 4; px[o:o + 4] = bytes((150, 110, 70, 255))
    if extra_bottom:
        o = ((h - 1) * w + 3) * 4; px[o:o + 4] = bytes((0, 0, 0, 255))
    pngio.write_rgba(path, w, h, px)
    return dict(density=density, logical_size={"width": cells * 64, "height": cells * 32})


class Checks(unittest.TestCase):
    def test_anchor(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "ok.png")
            errors, _, stats = checks.check_sprite(p, diamond_png(p))
            self.assertEqual(errors, [])
            errors, _, _ = checks.check_sprite(p, diamond_png(p, lift=6))
            self.assertTrue(any("bottom row is empty" in e for e in errors), errors)
            self.assertTrue(any("corner" in e for e in errors), errors)
            errors, _, _ = checks.check_sprite(p, diamond_png(p, extra_bottom=True))
            self.assertTrue(any("below the footprint" in e for e in errors), errors)
            rec = diamond_png(p); rec = copy.deepcopy(rec); rec["logical_size"]["width"] = 192
            errors, _, _ = checks.check_sprite(p, rec)
            self.assertTrue(any("width" in e for e in errors), errors)

    def test_contact_sheet(self):
        with tempfile.TemporaryDirectory() as d:
            a, b = os.path.join(d, "a.png"), os.path.join(d, "b.png")
            diamond_png(a); diamond_png(b, cells=1)
            out = checks.contact_sheet(os.path.join(d, "sheet.png"), [a, b], densities={a: 2, b: 2})
            w, h = pngio.read_size(out)
            self.assertGreater(w, 256); self.assertGreater(h, 100)


class Png(unittest.TestCase):
    def test_round_trip(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "x.png")
            px = bytearray(range(256)) * (5 * 7 * 4 // 256 + 1)
            px = px[:5 * 7 * 4]
            pngio.write_rgba(p, 5, 7, px)
            self.assertEqual(pngio.read_size(p), (5, 7))
            self.assertEqual(pngio.read_rgba(p), (5, 7, px))


if __name__ == "__main__":
    unittest.main()
