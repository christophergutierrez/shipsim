"""Flight UX (protocol 3): sticky status, accel/turn notes, help primer."""

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
from hexutil import motion_status_bits, next_translation_note, translation_schedule_label
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
        "course": 0,
        "velocity": 0,
        "thrust_remaining": 6,
        "structure": 4,
        "power": 10,
        "power_available": 10,
        "movement_allocated": 6,
        "weapons": [],
        "max_shield_per_facing": 2,
    }
    base.update(kw)
    return base


def _snap(ship=None, phase="movement", movement_phase=1, **kw):
    s = ship or _ship()
    snap = {
        "protocol_version": 3,
        "phase": phase,
        "status": "InProgress",
        "turn": 1,
        "movement_phase": movement_phase,
        "ships": [s],
        "ships_committed_this_phase": [],
        "ships_allocated_this_turn": [],
        "ships_ready_fire": [],
        "combat_log": [],
        "fire_commits": [],
    }
    snap.update(kw)
    return snap


class Helpers(unittest.TestCase):
    def test_schedule_labels(self):
        self.assertEqual("none", translation_schedule_label(0))
        self.assertIn("1", translation_schedule_label(1))
        self.assertIn("hex", next_translation_note(2, 1).lower())

    def test_diverge_flag(self):
        bits = motion_status_bits(_ship(velocity=2, course=1, facing=0))
        self.assertIn("sliding", bits)


class Hints(unittest.TestCase):
    def test_movement_hint_sticky(self):
        snap = _snap(_ship(velocity=1), movement_phase=1)
        hint = phase_hint(snap, ReplContext(selected=1))
        self.assertIn("v=1", hint)
        self.assertIn("slides=", hint)
        self.assertIn("coast", hint)

    def test_summary_and_help(self):
        text = movement_summary(_ship(velocity=0), 1)
        self.assertIn("accel", text.lower())
        self.assertIn("turn", text.lower())
        help_text = render_help("motion")
        self.assertIn("protocol 3", help_text.lower())


class Maneuvers(unittest.TestCase):
    def test_accel_emits_protocol3(self):
        snap = _snap(_ship(velocity=0), movement_phase=1)
        with contextlib.redirect_stdout(io.StringIO()):
            action = build_action("accel", snap, ReplContext(selected=1))
        self.assertEqual("accel", action.orders[0]["maneuver"]["type"])
        self.assertEqual(3, action.orders[0]["protocol_version"])

    def test_turn_absolute(self):
        snap = _snap(_ship(velocity=2, course=0, facing=0), movement_phase=1)
        with contextlib.redirect_stdout(io.StringIO()):
            action = build_action("turn 3", snap, ReplContext(selected=1))
        self.assertEqual("turn", action.orders[0]["maneuver"]["type"])
        self.assertEqual(3, action.orders[0]["maneuver"]["facing"])

    def test_turn_port(self):
        snap = _snap(_ship(velocity=2, course=0, facing=0), movement_phase=1)
        with contextlib.redirect_stdout(io.StringIO()):
            action = build_action("turn port", snap, ReplContext(selected=1))
        self.assertEqual(1, action.orders[0]["maneuver"]["facing"])


class ShipLine(unittest.TestCase):
    def test_sticky(self):
        line = format_ship_line(_ship(velocity=1))
        self.assertIn("slides=", line)


if __name__ == "__main__":
    unittest.main()
