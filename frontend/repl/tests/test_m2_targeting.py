"""M2 targeting: advisory range/arc legality from snapshot + pure geometry.

These tests exercise the pure-geometry helpers in hexutil (weapon_in_arc,
threats_to_ship) and the fire-picker output in commands. No engine process
is involved — all data is synthetic snapshot dicts.
"""

import io
import re
import unittest
from contextlib import redirect_stdout

from hexutil import MOUNT_FACINGS, distance, hit_preview, weapon_in_arc, threats_to_ship

ANSI = re.compile(r"\x1b\[[0-9;]*m")


def _weapon(mount="forward", max_range=5, charge=1, **kw):
    w = {
        "id": kw.get("id", "W1"),
        "mount": mount,
        "max_range": max_range,
        "max_charge": 1,
        "charge": charge,
        "operational": True,
        "fired": False,
        "kind": "Laser",
    }
    w.update(kw)
    return w


def _ship(sid, q, r, facing=0, controller="player", weapons=None, destroyed=False):
    return {
        "id": sid, "class": "Scout", "controller": controller,
        "destroyed": destroyed, "q": q, "r": r, "facing": facing,
        "structure": 4, "power": 4, "size": 2, "weapons": weapons or [],
        "max_shield_per_facing": 2,
    }


class TargetSizeHitPreviewTests(unittest.TestCase):
    def test_size_one_halves_baseline_and_size_four_hits_the_accuracy_ceiling(self):
        # Range 3 beam base threshold is 15 (size-2 baseline, unchanged).
        self.assertEqual((8, 40), hit_preview("beam", 3, 1))
        self.assertEqual((15, 75), hit_preview("beam", 3, 2))
        # Size 4 scales to 30, but the range-aware accuracy ceiling caps it at
        # 15 here (never below the size-2 table value, never a guaranteed hit)
        # instead of the old flat 1..=20 clamp that made this an auto-hit.
        self.assertEqual((15, 75), hit_preview("beam", 3, 4))

    def test_invalid_size_has_no_preview(self):
        self.assertIsNone(hit_preview("beam", 3, 0))

    def test_fire_control_bonus_applies_only_at_the_size_two_baseline(self):
        # Range 8 beam base threshold is 7.
        self.assertEqual((19, 95), hit_preview("beam", 8, 2, 12))
        self.assertEqual((4, 20), hit_preview("beam", 8, 1, 12))

    def test_fire_control_bonus_cannot_exceed_the_accuracy_ceiling(self):
        # Range 1 beam base threshold is 18; a huge bonus still caps at 19.
        self.assertEqual((19, 95), hit_preview("beam", 1, 2, 200))


class WeaponInArcTests(unittest.TestCase):
    """Arc legality is pure geometry mirroring src/arc.rs Mount::relative_facings."""

    def test_forward_mount_bears_on_relative_facing_zero(self):
        w = _weapon("forward")
        self.assertTrue(weapon_in_arc(w, 0, 0, 0, 3, 0))

    def test_forward_mount_cannot_bear_on_relative_facing_one(self):
        w = _weapon("forward")
        self.assertFalse(weapon_in_arc(w, 0, 0, 0, 3, -3))

    def test_forward_starboard_covers_zero_and_five(self):
        w = _weapon("forward_starboard")
        self.assertTrue(weapon_in_arc(w, 0, 0, 0, 3, 0))    # rel 0
        self.assertTrue(weapon_in_arc(w, 0, 0, 0, 0, 3))    # rel 5
        self.assertFalse(weapon_in_arc(w, 0, 0, 0, -3, 0))  # rel 3

    def test_aft_mount_bears_on_relative_facing_three(self):
        w = _weapon("aft")
        self.assertTrue(weapon_in_arc(w, 0, 0, 0, -3, 0))   # rel 3
        self.assertFalse(weapon_in_arc(w, 0, 0, 0, 3, 0))   # rel 0

    def test_self_is_never_in_arc(self):
        w = _weapon("forward")
        self.assertFalse(weapon_in_arc(w, 0, 0, 0, 0, 0))

    def test_broad_arc_fallback_when_mount_absent(self):
        w = {"id": "W1", "arc": "All", "max_range": 5, "charge": 1,
             "operational": True, "fired": False}
        # All-arc bears on every relative facing
        for rq, rr in [(3, 0), (3, -3), (-3, 0), (0, 3)]:
            self.assertTrue(weapon_in_arc(w, 0, 0, 0, rq, rr))


class ThreatsToShipTests(unittest.TestCase):
    """threats_to_ship lists enemy ships with a charged, in-arc, in-range weapon."""

    def test_in_arc_in_range_charged_weapon_is_a_threat(self):
        snap = {"ships": [
            _ship(1, 0, 0, 0, "player"),
            _ship(2, 3, 0, 3, "ai", [_weapon("forward", max_range=5)]),
        ]}
        t = threats_to_ship(snap, 1)
        self.assertEqual(1, len(t))
        self.assertEqual("W1", t[0]["weapon"]["id"])
        self.assertEqual(3, t[0]["range"])

    def test_out_of_range_weapon_is_not_a_threat(self):
        snap = {"ships": [
            _ship(1, 0, 0, 0, "player"),
            _ship(2, 3, 0, 3, "ai", [_weapon("forward", max_range=2)]),
        ]}
        self.assertEqual([], threats_to_ship(snap, 1))

    def test_out_of_arc_weapon_is_not_a_threat(self):
        # AI facing away (facing 0), target behind it (rel 3) — forward mount can't bear
        snap = {"ships": [
            _ship(1, 0, 0, 0, "player"),
            _ship(2, 3, 0, 0, "ai", [_weapon("forward", max_range=5)]),
        ]}
        self.assertEqual([], threats_to_ship(snap, 1))

    def test_uncharged_weapon_is_not_a_threat(self):
        snap = {"ships": [
            _ship(1, 0, 0, 0, "player"),
            _ship(2, 3, 0, 3, "ai", [_weapon("forward", max_range=5, charge=0)]),
        ]}
        self.assertEqual([], threats_to_ship(snap, 1))

    def test_destroyed_attacker_is_not_a_threat(self):
        snap = {"ships": [
            _ship(1, 0, 0, 0, "player"),
            _ship(2, 3, 0, 3, "ai", [_weapon("forward")], destroyed=True),
        ]}
        self.assertEqual([], threats_to_ship(snap, 1))

    def test_same_controller_is_not_a_threat(self):
        snap = {"ships": [
            _ship(1, 0, 0, 0, "player"),
            _ship(2, 3, 0, 3, "player", [_weapon("forward")]),
        ]}
        self.assertEqual([], threats_to_ship(snap, 1))


class FirePickerOutputTests(unittest.TestCase):
    """The fire picker shows range and an advisory [in arc]/[OUT OF ARC]/[OUT OF RANGE] flag."""

    def test_target_list_shows_range_and_arc_flag(self):
        from commands import interactive_fire

        ship = _ship(1, 0, 0, 0, "player",
                     [_weapon("forward", max_range=5, id="L1")])
        enemy = _ship(2, 3, 0, 3, "ai")
        snap = {"ships": [ship, enemy], "phase": "firing", "status": "Playing",
                "turn": 1, "active_ship": 1, "combat_log": []}

        buf = io.StringIO()
        # interactive_fire prompts for input; feed weapon 0 then target 0
        with redirect_stdout(buf):
            import builtins
            orig_input = builtins.input
            answers = iter(["0", "0", "0"])
            builtins.input = lambda *_a, **_k: next(answers)
            try:
                interactive_fire(snap, 1)
            finally:
                builtins.input = orig_input
        text = ANSI.sub("", buf.getvalue())
        self.assertIn("rng=3", text)
        self.assertIn("[in arc]", text)
        self.assertIn("arc=forward", text)

    def test_out_of_range_target_flagged(self):
        from commands import interactive_fire

        ship = _ship(1, 0, 0, 0, "player",
                     [_weapon("forward", max_range=2, id="L1")])
        enemy = _ship(2, 3, 0, 3, "ai")
        snap = {"ships": [ship, enemy], "phase": "firing", "status": "Playing",
                "turn": 1, "active_ship": 1, "combat_log": []}

        buf = io.StringIO()
        with redirect_stdout(buf):
            import builtins
            orig_input = builtins.input
            answers = iter(["0", "0", "0"])
            builtins.input = lambda *_a, **_k: next(answers)
            try:
                interactive_fire(snap, 1)
            finally:
                builtins.input = orig_input
        text = ANSI.sub("", buf.getvalue())
        self.assertIn("[OUT OF RANGE]", text)

    def test_overlapping_target_is_reported_too_close(self):
        from commands import interactive_fire

        ship = _ship(1, 0, 0, 0, "player", [_weapon("forward", id="L1")])
        enemy = _ship(2, 0, 0, 3, "ai")
        snap = {"ships": [ship, enemy], "phase": "firing", "status": "Playing",
                "turn": 1, "active_ship": 1, "combat_log": []}

        buf = io.StringIO()
        with redirect_stdout(buf):
            order = interactive_fire(snap, 1)

        self.assertIsNone(order)
        self.assertIn("[TOO CLOSE]", ANSI.sub("", buf.getvalue()))

    def test_target_picker_displays_size_adjusted_odds(self):
        from commands import interactive_fire

        ship = _ship(1, 0, 0, 0, "player",
                     [_weapon("forward", max_range=5, id="L1", kind="beam")])
        enemy = _ship(2, 3, 0, 3, "ai")
        enemy["size"] = 1
        snap = {"ships": [ship, enemy], "phase": "firing", "status": "Playing",
                "turn": 1, "active_ship": 1, "combat_log": []}

        buf = io.StringIO()
        with redirect_stdout(buf):
            import builtins
            orig_input = builtins.input
            answers = iter(["0", "0", "0"])
            builtins.input = lambda *_a, **_k: next(answers)
            try:
                interactive_fire(snap, 1)
            finally:
                builtins.input = orig_input
        text = ANSI.sub("", buf.getvalue())
        self.assertIn("size=1", text)
        self.assertIn("to-hit d20≤8 (40%)", text)


if __name__ == "__main__":
    unittest.main()
