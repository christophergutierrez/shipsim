"""Flight UX: sticky motion status, delayed-translate notes, course≠face cues."""

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
from hexutil import (
    motion_status_bits,
    next_translation_note,
    translation_schedule_label,
)
from view import format_ship_line, snapshot_delta


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
        "protocol_version": 2,
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


class HexutilMotionHelpers(unittest.TestCase):
    def test_schedule_labels(self):
        self.assertEqual("none", translation_schedule_label(0))
        self.assertEqual("4", translation_schedule_label(1))
        self.assertEqual("2,4", translation_schedule_label(2))
        self.assertEqual("1,2,4", translation_schedule_label(3))
        self.assertEqual("1,2,3,4", translation_schedule_label(4))

    def test_speed1_teaches_late_slide(self):
        note = next_translation_note(1, 1)
        self.assertIn("cycle 4/4", note)
        self.assertIn("speed 1", note)
        self.assertIn("POSITION HOLDS", note)

    def test_scheduled_phase_says_move_occurs_after_maneuver(self):
        note = next_translation_note(1, 4)
        self.assertIn("MOVE OCCURS", note)
        self.assertIn("after this maneuver", note)

    def test_diverge_flag_in_status(self):
        bits = motion_status_bits(_ship(velocity=2, course=1, facing=0))
        self.assertIn("sliding", bits)
        self.assertIn("nose", bits)
        self.assertIn("slides=[2,4]", bits)

    def test_aligned_no_warning(self):
        bits = motion_status_bits(_ship(velocity=1, course=0, facing=0))
        self.assertNotIn("sliding", bits)


class PhaseHintSticky(unittest.TestCase):
    def test_movement_hint_includes_sticky_and_when(self):
        snap = _snap(_ship(velocity=1, course=0, facing=0, thrust_remaining=5), movement_phase=1)
        hint = phase_hint(snap, ReplContext(selected=1))
        self.assertIn("v=1", hint)
        self.assertIn("slides=[4]", hint)
        self.assertIn("cycle 4/4", hint)
        self.assertIn("POSITION HOLDS", hint)
        self.assertIn("position @(0,0)", hint)
        self.assertIn("choose one", hint)

    def test_diverge_in_hint(self):
        snap = _snap(_ship(velocity=2, course=1, facing=0), movement_phase=2)
        hint = phase_hint(snap, ReplContext(selected=1))
        self.assertIn("sliding", hint)
        self.assertIn("nose", hint)


class MovementSummaryHelp(unittest.TestCase):
    def test_summary_teaches_course_vs_face(self):
        text = movement_summary(_ship(velocity=0), 1)
        self.assertIn("course = where you slide", text)
        self.assertIn("speed 1: only phase 4/4", text)

    def test_summary_flags_diverge(self):
        text = movement_summary(_ship(velocity=3, course=1, facing=5), 2)
        self.assertIn("sliding", text)
        self.assertIn("nose", text)


class ManeuverNotes(unittest.TestCase):
    def test_accel_from_rest_warns_not_in_new_hex(self):
        snap = _snap(_ship(velocity=0), movement_phase=1)
        with contextlib.redirect_stdout(io.StringIO()):
            action = build_action("accel", snap, ReplContext(selected=1))
        self.assertTrue(action.orders)
        self.assertIn("not in a new hex yet", action.note or "")
        self.assertIn("cycle 4/4", action.note or "")

    def test_accel_while_moving_notes_translate_this_phase(self):
        # speed 1 → 2 during phase 2: schedule for 2 includes phase 2
        snap = _snap(_ship(velocity=1, course=0), movement_phase=2)
        with contextlib.redirect_stdout(io.StringIO()):
            action = build_action("accel", snap, ReplContext(selected=1))
        self.assertIn("MOVE OCCURS", action.note or "")

    def test_course_port_note_shows_new_course_and_nose(self):
        snap = _snap(_ship(velocity=2, course=0, facing=0), movement_phase=1)
        with contextlib.redirect_stdout(io.StringIO()):
            action = build_action("course port", snap, ReplContext(selected=1))
        self.assertIn("1↗", action.note or "")
        self.assertIn("nose still", action.note or "")

    def test_rotate_note_shows_still_sliding(self):
        snap = _snap(_ship(velocity=2, course=0, facing=0), movement_phase=1)
        with contextlib.redirect_stdout(io.StringIO()):
            action = build_action("rotate starboard", snap, ReplContext(selected=1))
        self.assertIn("still sliding", action.note or "")
        self.assertIn("5↘", action.note or "")


class HelpPrimer(unittest.TestCase):
    def test_help_motion_includes_inertial_primer(self):
        text = render_help("motion")
        self.assertIn("Inertial flight", text)
        self.assertIn("phase 4 only", text)
        self.assertIn("course = where you travel", text)

    def test_help_accel_includes_primer(self):
        text = render_help("accel")
        self.assertIn("Inertial flight", text)


class ShipLineSticky(unittest.TestCase):
    def test_player_line_has_slides(self):
        line = format_ship_line(_ship(velocity=1, course=0, facing=0))
        self.assertIn("slides=[4]", line)
        self.assertIn("v=1", line)

    def test_player_line_flags_diverge(self):
        line = format_ship_line(_ship(velocity=2, course=1, facing=0))
        self.assertIn("sliding", line)


class MovementDelta(unittest.TestCase):
    def test_coordinate_change_is_called_out(self):
        before = _snap(_ship(q=0, r=0))
        after = _snap(_ship(q=1, r=0), phase="firing")
        delta = snapshot_delta(before, after)
        self.assertIn("MOVED (0,0)→(1,0)", delta)


if __name__ == "__main__":
    unittest.main()
