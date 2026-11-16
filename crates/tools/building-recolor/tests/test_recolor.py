"""Unit tests for recolor.py: the hue window, its fades, and the recipes' consistency. Plain Python."""
import colorsys
import json
import pathlib
import sys
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

import recolor  # noqa: E402

SHIFT = {"hue": [-10, 26], "min_saturation": 0.28, "feather": 6, "to_hue": 195, "saturation": 1.0, "value": 1.0}


def hsv(hue, saturation, value):
    r, g, b = colorsys.hsv_to_rgb(hue / 360, saturation, value)
    return (round(r * 255), round(g * 255), round(b * 255), 255)


class MembershipTests(unittest.TestCase):
    def test_inside_the_window_is_full(self):
        self.assertEqual(recolor.membership(8, 0.6, SHIFT), 1.0)
        self.assertEqual(recolor.membership(355, 0.6, SHIFT), 1.0, "wraps past 0")

    def test_fades_past_the_window(self):
        self.assertAlmostEqual(recolor.membership(29, 0.6, SHIFT), 0.5)
        self.assertEqual(recolor.membership(40, 0.6, SHIFT), 0.0, "ochre walls stay")

    def test_fades_below_the_saturation_floor(self):
        self.assertEqual(recolor.membership(8, 0.1, SHIFT), 0.0, "stone stays")
        self.assertAlmostEqual(recolor.membership(8, 0.24, SHIFT), 0.5)


class RecolourTests(unittest.TestCase):
    def test_a_roof_red_goes_to_the_target_hue(self):
        out = recolor.recolour_pixel(hsv(8, 0.6, 0.5), [SHIFT])
        hue = colorsys.rgb_to_hsv(*(c / 255 for c in out[:3]))[0] * 360
        self.assertAlmostEqual(hue, 195, delta=2)
        self.assertEqual(out[3], 255)

    def test_spread_narrows_the_hues(self):
        narrow = dict(SHIFT, spread=0.5)
        out = recolor.recolour_pixel(hsv(26, 0.6, 0.5), [narrow])
        hue = colorsys.rgb_to_hsv(*(c / 255 for c in out[:3]))[0] * 360
        self.assertAlmostEqual(hue, 195 + 9, delta=2)

    def test_untouched_pixels_and_transparency(self):
        wall = hsv(38, 0.6, 0.7)
        self.assertEqual(recolor.recolour_pixel(wall, [SHIFT]), wall)
        self.assertEqual(recolor.recolour_pixel((10, 20, 30, 0), [SHIFT]), (10, 20, 30, 0))


class RecipeTests(unittest.TestCase):
    def test_every_recipe_has_its_source_and_a_unique_output(self):
        recipes = json.loads(recolor.RECIPES.read_text())["recipes"]
        keys = [recolor.recipe_key(recipe) for recipe in recipes]
        self.assertEqual(len(keys), len(set(keys)))
        for recipe in recipes:
            self.assertTrue((recolor.SOURCES / recipe["source"]).exists(), recipe["source"])


if __name__ == "__main__":
    unittest.main()
