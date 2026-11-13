"""The intermission's art against each realm's skin: the stele, its niche and the newest line's
mark are drawn by ui-art at the size, origin and bleed its catalog gives them, and the skin's
`intermission` block, which places them on the debrief's 1920 x 1080 screen, must say the same, or
the art lands offset with every other check passing. Standard library only.

    python3 -m unittest discover -s crates/tools/ui-art/tests
"""

import os
import sys
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from test_world_map_art import record, skins  # noqa: E402


class IntermissionArtTest(unittest.TestCase):
    def test_niche_is_where_the_catalog_draws_it(self):
        for realm, skin in skins().items():
            niche, rec = skin["intermission"]["niche"], record(realm, "stele_niche")
            self.assertEqual(niche["sprite"], rec["states"][0]["out"], realm)
            self.assertEqual(niche["rect"][:2], rec["args"]["origin"], realm)
            self.assertEqual(niche["rect"][2:], rec["size"], realm)
            self.assertEqual(niche["bleed"], rec["bleed"], realm)

    def test_stele_has_the_catalog_size_and_bleed(self):
        for realm, skin in skins().items():
            stele, rec = skin["intermission"]["stele"], record(realm, "stele")
            self.assertEqual(stele["sprite"], rec["states"][0]["out"], realm)
            self.assertEqual(stele["rect"][2:], rec["size"], realm)
            self.assertEqual(stele["bleed"], rec["bleed"], realm)

    def test_mark_is_the_catalog_size_grown_by_its_bleed(self):
        for realm, skin in skins().items():
            stele, rec = skin["intermission"]["stele"], record(realm, "stele_mark")
            self.assertEqual(stele["mark"], rec["states"][0]["out"], realm)
            left, top, right, bottom = rec["bleed"]
            width, height = rec["size"]
            self.assertEqual(stele["mark_size"], [width + left + right, height + top + bottom], realm)


if __name__ == "__main__":
    unittest.main()
