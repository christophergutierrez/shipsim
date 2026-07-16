"""Regression tests for tools/generate_size_variants.py.

Run: python3 -m unittest tools.test_generate_size_variants
(or: cd tools && python3 -m unittest test_generate_size_variants)
"""
from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(Path(__file__).resolve().parent))

import generate_size_variants as gen  # noqa: E402


class AttackAccuracyBonusTests(unittest.TestCase):
    def test_titan_bonuses_match_retained_balance_candidate(self):
        # Phase 3 QC fix plan: titan_light=12, titan_heavy=10 (not the
        # generator's earlier stale 10/8).
        self.assertEqual(gen.attack_accuracy_bonus(7, 0), 12)
        self.assertEqual(gen.attack_accuracy_bonus(7, 2), 10)

    def test_non_titan_hulls_have_no_bonus(self):
        for size in range(1, 7):
            for vi in range(3):
                self.assertEqual(gen.attack_accuracy_bonus(size, vi), 0)


class CheckModeTests(unittest.TestCase):
    def write_temp_catalog(self, root: Path) -> dict[Path, str]:
        _, outputs = gen.build_catalog(root)
        for path, text in outputs.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        return outputs

    def test_check_passes_on_tracked_catalog(self):
        result = subprocess.run(
            [sys.executable, str(ROOT / "tools" / "generate_size_variants.py"), "--check"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_check_is_non_destructive(self):
        titan_light = ROOT / "data" / "ships" / "titan_light.toml"
        before = titan_light.read_text()
        subprocess.run(
            [sys.executable, str(ROOT / "tools" / "generate_size_variants.py"), "--check"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        self.assertEqual(titan_light.read_text(), before)

    def test_check_detects_a_mismatched_generated_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            outputs = self.write_temp_catalog(root)
            titan_light = root / "data" / "ships" / "titan_light.toml"
            titan_light.write_text(
                titan_light.read_text().replace(
                    "attack_accuracy_bonus = 12",
                    "attack_accuracy_bonus = 99",
                )
            )
            self.assertIn(titan_light, gen.catalog_mismatches(outputs, root))

    def test_check_detects_an_obsolete_generated_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            outputs = self.write_temp_catalog(root)
            obsolete = root / "data" / "ships" / "obsolete_line.toml"
            obsolete.write_text(
                "# Frame/module cost model.\n"
                f"{gen.GENERATED_MARKER}\n"
                'id = "obsolete_line"\n'
            )
            self.assertIn(obsolete, gen.catalog_mismatches(outputs, root))


if __name__ == "__main__":
    unittest.main()
