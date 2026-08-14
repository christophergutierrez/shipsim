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
    EVASION_PER_POINT,
    FIRE_CONTROL_TARGET_SIZE,
    damage_preview,
    hit_preview,
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
        self.assertEqual(accuracy["evasion_per_point"], EVASION_PER_POINT)

    def test_natural_defense_is_relative_to_size_two(self):
        size_two = hit_preview("beam", 3, 2)
        fighter = hit_preview("beam", 3, 1)
        heavy = hit_preview("beam", 3, 4)
        self.assertEqual(size_two, (15, 75))
        self.assertEqual(fighter, (7, 35))
        self.assertEqual(heavy, (16, 80))

    def test_evasion_lowers_hit_preview(self):
        base = hit_preview("beam", 3, 2, 0, 0)
        reduced = hit_preview("beam", 3, 2, 0, 3)
        self.assertIsNotNone(base)
        self.assertIsNotNone(reduced)
        self.assertEqual(reduced[0], max(1, base[0] - 3 * EVASION_PER_POINT))

    def test_weapon_accuracy_applies_at_non_baseline_size_and_caps(self):
        base = hit_preview("beam", 1, 3)
        precise = hit_preview("beam", 1, 3, 0, 0, 2)
        self.assertIsNotNone(base)
        self.assertIsNotNone(precise)
        self.assertGreater(precise[0], base[0])
        self.assertLessEqual(precise[0], CEILING_MAX)

    def test_damage_bonus_is_additive(self):
        self.assertEqual(damage_preview("beam", 2, 3, 2), damage_preview("beam", 2, 3) + 2)

    def test_weapon_bonus_is_applied_before_evasion_and_clamp(self):
        # Rust computes threshold + bonuses - evasion, then clamps. A bonus
        # that reaches the ceiling must still be reduced by later evasion.
        self.assertEqual(hit_preview("beam", 1, 2, 0, 2, 10), (19, 95))

    def test_computer_and_cloak_modifiers_follow_engine_order(self):
        plain = hit_preview("beam", 3, 3)
        computer = hit_preview("beam", 3, 3, 0, 0, 0, 3)
        cloaked = hit_preview("beam", 3, 3, 0, 0, 0, 0, True)
        self.assertEqual(computer[0], min(19, plain[0] + 3))
        self.assertEqual(cloaked[0], max(1, plain[0] - 4))

    def test_ecm_only_reduces_missiles(self):
        missile = hit_preview("missile", 3, 2)
        blocked = hit_preview("missile", 3, 2, 0, 0, 0, 0, False, True)
        beam = hit_preview("beam", 3, 2, 0, 0, 0, 0, False, True)
        self.assertEqual(blocked[0], missile[0] - 2)
        self.assertEqual(beam, hit_preview("beam", 3, 2))

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
