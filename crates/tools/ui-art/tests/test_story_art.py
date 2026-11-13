"""The story panels' placeholder art against the story files (assets/configs/story/*.json): every
picture a story file names under story/ is one the catalog draws, and its dim caption is the
panel's `visual` (the story script's description), so the art can't drift from the data; and
every story picture the catalog draws is named by a story file. Standard library only.

    python3 -m unittest discover -s crates/tools/ui-art/tests
"""

import json
import os
import sys
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from uiart import catalog as cat  # noqa: E402

STORY_DIR = os.path.join(cat.REPO_ROOT, "assets", "configs", "story")


def story_panels():
    """(file name, sequence id, panel number, panel) for every panel of every story file."""
    for name in sorted(os.listdir(STORY_DIR)):
        if not name.endswith(".json"):
            continue
        with open(os.path.join(STORY_DIR, name), encoding="utf-8") as f:
            story = json.load(f)
        for sequence in story["sequences"]:
            for number, panel in enumerate(sequence["panels"], 1):
                yield name, sequence["id"], number, panel


def story_outputs():
    """Output path (under assets/ui) -> record, for the catalog's story pictures."""
    out = {}
    for rec in cat.load_catalog():
        for st in rec["states"]:
            if st["out"].startswith(cat.STORY_ART_DIR + "/"):
                out[st["out"]] = (rec, st)
    return out


class StoryArtTest(unittest.TestCase):
    def test_every_story_picture_is_drawn_with_its_description(self):
        outputs = story_outputs()
        for name, sequence, number, panel in story_panels():
            where = f"{name} {sequence} panel {number}"
            image = panel["image"]
            if not image.startswith(cat.STORY_ART_DIR + "/"):
                continue
            self.assertIn(image, outputs, f"{where}: ui-art draws no {image}")
            rec, st = outputs[image]
            self.assertEqual(st["call"], "story_panel", where)
            if panel.get("visual"):
                self.assertEqual(st["args"].get("text"), panel["visual"], f"{where}: the caption isn't its visual")

    def test_every_story_picture_drawn_is_named(self):
        named = {panel["image"] for _, _, _, panel in story_panels()}
        for out in story_outputs():
            self.assertIn(out, named, f"no story file names {out}")


if __name__ == "__main__":
    unittest.main()
