"""Flight UX (protocol 4): motion sticky line, path draft, help primer."""

import contextlib
import io
import unittest

from commands import (
    ReplContext,
    build_action,
    movement_summary,
    phase_hint,
    render_help,
)
from hexutil import motion_status_bits
from view import format_ship_line


def _ship(**kw):
    base = {
        "id": 1,
        "class": "Scout",
        "controller": "player",
        "destroyed": False,
        "q": 0,
        "r": 0,
        "facing": 0,
        "motion_available": 6,
        "max_maneuver_actions": 6,
        "structure": 4,
        "power": 10,
        "power_available": 10,
        "movement_allocated": 6,
        "weapons": [],
        "max_shield_per_facing": 2,
    }
    base.update(kw)
    return base


def _snap(ship=None, phase="movement", **kw):
    s = ship or _ship()
    snap = {
        "protocol_version": 4,
        "phase": phase,
        "status": "InProgress",
        "turn": 1,
        "ships": [s],
        "ships_committed_path": [],
        "ships_allocated_this_turn": [],
        "ships_committed_volley": [],
        "combat_log": [],
    }
    snap.update(kw)
    return snap


class Helpers(unittest.TestCase):
    def test_motion_bits(self):
        bits = motion_status_bits(_ship(motion_available=2, facing=1))
        self.assertIn("motion=2", bits)
        self.assertIn("face=", bits)


class Hints(unittest.TestCase):
    def test_movement_hint_sticky(self):
        snap = _snap(_ship(motion_available=1))
        hint = phase_hint(snap, ReplContext(selected=1))
        self.assertIn("motion=1", hint)
        self.assertIn("path", hint.lower())

    def test_summary_and_help(self):
        text = movement_summary(_ship(motion_available=0))
        self.assertIn("path", text.lower())
        # Shorthand tokens are the player-facing action language.
        lower = text.lower()
        self.assertTrue(
            "move_f" in lower or " f " in f" {lower} " or "| f |" in lower or "f | fr" in lower,
            f"expected path action tokens in: {text!r}",
        )
        help_text = render_help("path")
        self.assertIn("protocol 4", help_text.lower())


class Paths(unittest.TestCase):
    def test_path_draft_then_commit(self):
        snap = _snap()
        ctx = ReplContext(selected=1)
        with contextlib.redirect_stdout(io.StringIO()):
            build_action("f", snap, ctx)
            build_action("tr", snap, ctx)
            action = build_action("commit", snap, ctx)
        self.assertEqual("commit_path", action.orders[0]["type"])
        self.assertEqual(["move_f", "turn_right"], action.orders[0]["actions"])
        self.assertEqual(4, action.orders[0]["protocol_version"])

    def test_bare_tokens(self):
        snap = _snap()
        ctx = ReplContext(selected=1)
        with contextlib.redirect_stdout(io.StringIO()):
            build_action("fr fl", snap, ctx)
        self.assertEqual(["move_fr", "move_fl"], ctx.path_draft)

    def test_undo_and_clear(self):
        snap = _snap()
        ctx = ReplContext(selected=1)
        with contextlib.redirect_stdout(io.StringIO()):
            build_action("path f f", snap, ctx)
            build_action("undo", snap, ctx)
        self.assertEqual(["move_f"], ctx.path_draft)
        with contextlib.redirect_stdout(io.StringIO()):
            build_action("clear", snap, ctx)
        self.assertEqual([], ctx.path_draft)

    def test_retired_accel_explains_path(self):
        snap = _snap()
        with contextlib.redirect_stdout(io.StringIO()) as output:
            action = build_action("accel", snap, ReplContext(selected=1))
        self.assertFalse(action.orders)
        self.assertIn("path", output.getvalue().lower())


class ShipLine(unittest.TestCase):
    def test_sticky(self):
        line = format_ship_line(_ship(motion_available=1))
        self.assertIn("motion=1", line)


if __name__ == "__main__":
    unittest.main()
