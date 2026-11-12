"""The world map's base art against the mockup generator and the game's data: the catalog draws
the generator's map viewport, each realm's skin maps the world files' units onto the base and its
frame as ui-art draws them, and every city of a world file lands on the base where the generator
draws it. Standard library only: the generator's common.py and content.py import nothing else.

    python3 -m unittest discover -s crates/tools/ui-art/tests
"""

import json
import math
import os
import sys
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from uiart import catalog as cat  # noqa: E402

GENERATOR_DIR = os.path.join(cat.REPO_ROOT, "docs", "design", "mockups", "generator")
sys.path.insert(0, GENERATOR_DIR)
import common as C  # noqa: E402
import content  # noqa: E402

CONTENT = {"dailong": content.DAILONG, "kambara": content.KAMBARA, "tampadesa": content.TAMPADESA}


def load(rel):
    with open(os.path.join(cat.REPO_ROOT, rel), encoding="utf-8") as f:
        return json.load(f)


def skins():
    """Each realm's skin, merged over Đại Long's as the game merges it."""
    base = load(f"{cat.SKIN_CONFIG_DIR}/{cat.SKIN_BASE_FILE}")
    out = {}
    for realm in cat.REALMS:
        skin = json.loads(json.dumps(base))
        if realm != "dailong":
            skin = cat.merge_json(skin, load(f"{cat.SKIN_CONFIG_DIR}/{realm}.json"))
        out[realm] = skin
    return out


def record(realm, sid):
    return next(r for r in cat.load_catalog() if r["realm"] == realm and r["id"] == sid)


class WorldMapArtTest(unittest.TestCase):
    def test_base_is_the_viewport(self):
        for realm, skin in skins().items():
            art, rec = skin["world_map"], record(realm, "world_map_base")
            # The generator's viewport, in whole points: 1290 x 866.88, so 867.
            self.assertEqual(rec["size"], [C.MAP_W, math.ceil(C.MAP_H)], realm)
            self.assertEqual(rec["bleed"], [0, 0, 0, 0], realm)
            self.assertEqual(rec["format"], "jpeg", realm)
            self.assertEqual(art["base"], rec["states"][0]["out"], realm)
            self.assertAlmostEqual(art["pixels_per_unit"], C.MAP_S * rec["scale"], places=6, msg=realm)
            self.assertEqual(art["extent"], [C.MAP_W / C.MAP_S, round(C.MAP_H / C.MAP_S, 6)], realm)

    def test_frame_holds_the_viewport(self):
        for realm, skin in skins().items():
            art, rec = skin["world_map"], record(realm, "world_map_frame")
            self.assertEqual(art["frame"], rec["states"][0]["out"], realm)
            self.assertEqual(rec["size"], [C.MAP_W, math.ceil(C.MAP_H)], realm)
            s = rec["scale"]
            x, y, w, h = art["frame_map_rect"]
            self.assertEqual([x, y], [rec["bleed"][0] * s, rec["bleed"][1] * s], realm)
            self.assertAlmostEqual(w, C.MAP_W * s, places=4, msg=realm)
            self.assertAlmostEqual(h, C.MAP_H * s, places=4, msg=realm)
            self.assertEqual(art["backdrop"], record(realm, "world_map_backdrop")["states"][0]["out"], realm)

    def test_threat_clouds_follow_the_threat(self):
        """A realm whose map has a threat (content.py's `threat`) names its storm clouds, drawn
        round the catalog's `anchor`, and the skin's `threat_anchor` is that point in the sprite's
        pixels; the world file's threat is where the generator draws the storm, P(150, 30)
        (Tampadesa.threat). The others name none."""
        all_skins = skins()
        worlds = {c.get("realm", "dailong"): load(f"assets/configs/game/world/{c.get('world_map', c['id'])}.json")
                  for c in load("assets/configs/game/campaigns.json")["campaigns"] if not c.get("hidden")}
        for realm, skin in all_skins.items():
            art = skin["world_map"]
            if not CONTENT[realm].get("threat"):
                self.assertEqual(art.get("threat", ""), "", realm)
                self.assertEqual(worlds[realm].get("threats", []), [], realm)
                continue
            rec = record(realm, "world_map_threat")
            self.assertEqual(art["threat"], rec["states"][0]["out"], realm)
            self.assertEqual(rec["bleed"], [0, 0, 0, 0], realm)
            self.assertEqual(art["threat_anchor"], [v * rec["scale"] for v in rec["args"]["anchor"]], realm)
            self.assertEqual([t["position"] for t in worlds[realm]["threats"]], [[150, 30]], realm)

    def test_cities_land_where_the_generator_draws_them(self):
        """Every city of a world file that the realm's mockup shows (by its English name) is on
        the base where the generator draws it: P(x, y), less the viewport's origin, at the art's
        scale."""
        names = load("assets/configs/ui/text/en.json")["Realm"]
        all_skins = skins()
        for campaign in load("assets/configs/game/campaigns.json")["campaigns"]:
            if campaign.get("hidden"):
                continue  # It shares another campaign's map.
            realm = campaign.get("realm", "dailong")
            world = load(f"assets/configs/game/world/{campaign.get('world_map', campaign['id'])}.json")
            art = all_skins[realm]["world_map"]
            scale = record(realm, "world_map_base")["scale"]
            self.assertEqual(world["extent"], art["extent"], campaign["id"])
            drawn = {name: (x, y) for name, x, y, *_ in CONTENT[realm]["player"] + CONTENT[realm]["foreign"]}
            matched = 0
            for city in world["cities"]:
                name = names.get(city["name"])
                if name not in drawn:
                    continue
                sx, sy = C.P(*drawn[name])
                expected = ((sx - C.MAP_X) * scale, (sy - C.MAP_Y) * scale)
                actual = [v * art["pixels_per_unit"] for v in city["position"]]
                for e, a in zip(expected, actual):
                    self.assertAlmostEqual(a, e, delta=0.5, msg=f"{campaign['id']}: {city['id']}")
                matched += 1
            self.assertGreaterEqual(matched, 4, f"{campaign['id']}: too few cities to compare with the mockup")


if __name__ == "__main__":
    unittest.main()
