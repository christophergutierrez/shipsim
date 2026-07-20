from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]


class DocumentationTests(unittest.TestCase):
    def test_readme_lists_bin_and_save_flags(self):
        text = (ROOT / "README.md").read_text()
        self.assertIn("--bin", text)
        self.assertIn("--save", text)

    def test_gameplay_uses_literal_allocate_spacing_and_v4_prompts(self):
        text = (ROOT / "GAMEPLAY.md").read_text()
        self.assertIn("  engine accepted allocate #1: engine=", text)
        self.assertIn("t1/movement@1*1 path3 actions=motion:5>", text)
        self.assertIn("protocol v4", text.lower())
        self.assertIn("commit_path", text)
        self.assertIn("commit_volley", text)
        self.assertNotIn("t1/turn_end@1>", text)

    def test_ascii_ui_matches_semantic_helpers_and_map_glyphs(self):
        text = (ROOT / "ASCII-UI.md").read_text()
        for helper in ("fired", "queued", "available", "dead"):
            self.assertIn(f"`{helper}`", text)
        self.assertIn("Map cell `····`", text)
        self.assertIn("Map cell ` x  `", text)


if __name__ == "__main__":
    unittest.main()
