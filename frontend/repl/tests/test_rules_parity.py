"""Parity check: the REPL's presentation-only combat table mirrors in
hexutil.py must match the canonical `data/rules/default.toml` (ADR-0024,
schema version 1, d20 only) exactly.

This test is the ONLY place in the REPL that reads rules TOML — the REPL
runtime itself never loads it; hexutil.py carries a hardcoded copy for display
previews, and this test is what keeps that copy honest. If the copy ever
drifts, this fails loudly instead of silently rendering wrong odds.
"""

import tomllib
import unittest
from pathlib import Path

from hexutil import (
    _TO_HIT,
    BASELINE_TARGET_SIZE,
    CEILING_FLOOR,
    CEILING_MAX,
    DIE_SIDES,
    FIRE_CONTROL_TARGET_SIZE,
    damage_preview,
)

REPO_ROOT = Path(__file__).resolve().parents[3]
RULES_PATH = REPO_ROOT / "data" / "rules" / "default.toml"


def _load_rules() -> dict:
    with RULES_PATH.open("rb") as handle:
        return tomllib.load(handle)


class RulesParityTests(unittest.TestCase):
    def setUp(self):
        self.rules = _load_rules()

    def test_schema_version_is_one(self):
        self.assertEqual(self.rules["schema_version"], 1)

    def test_die_sides_matches(self):
        self.assertEqual(self.rules["combat"]["die_sides"], DIE_SIDES)

    def test_accuracy_constants_match(self):
        accuracy = self.rules["combat"]["accuracy"]
        self.assertEqual(accuracy["baseline_target_size"], BASELINE_TARGET_SIZE)
        self.assertEqual(accuracy["ceiling_floor"], CEILING_FLOOR)
        self.assertEqual(accuracy["ceiling_max"], CEILING_MAX)
        self.assertEqual(accuracy["fire_control_target_size"], FIRE_CONTROL_TARGET_SIZE)

    def test_beam_to_hit_table_matches(self):
        self.assertEqual(
            list(self.rules["combat"]["weapons"]["beam"]["to_hit"]),
            list(_TO_HIT["beam"]),
        )

    def test_plasma_to_hit_table_matches(self):
        self.assertEqual(
            list(self.rules["combat"]["weapons"]["plasma"]["to_hit"]),
            list(_TO_HIT["plasma"]),
        )

    def test_torp_to_hit_table_matches(self):
        self.assertEqual(
            list(self.rules["combat"]["weapons"]["torp"]["to_hit"]),
            list(_TO_HIT["torp"]),
        )

    def test_beam_range_factors_match_damage_preview(self):
        factors = self.rules["combat"]["weapons"]["beam"]["range_factors"]
        for index, factor in enumerate(factors, start=1):
            expected = int(2 * factor + 0.5)  # charge=2, half-up, matches half_up()
            self.assertEqual(damage_preview("beam", 2, index), expected)

    def test_plasma_damage_table_matches_damage_preview(self):
        damage = self.rules["combat"]["weapons"]["plasma"]["damage"]
        for index, expected in enumerate(damage, start=1):
            self.assertEqual(damage_preview("plasma", 1, index), expected)

    def test_torp_flat_damage_matches_damage_preview(self):
        flat_damage = self.rules["combat"]["weapons"]["torp"]["flat_damage"]
        self.assertEqual(damage_preview("torp", 1, 1), flat_damage)
        self.assertEqual(damage_preview("torp", 1, 12), flat_damage)


if __name__ == "__main__":
    unittest.main()
