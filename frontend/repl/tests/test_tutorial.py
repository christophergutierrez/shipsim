"""Strict rear-attack tutorial sequencing and narration."""

import unittest

from tutorial import Tutorial, load_tutorial


def _snapshot(*, turn=1, phase="allocate", movement_phase=0, status="InProgress"):
    return {
        "turn": turn,
        "phase": phase,
        "movement_phase": movement_phase,
        "status": status,
    }


class RearAttackTutorialTests(unittest.TestCase):
    def test_loader_accepts_documented_names(self):
        self.assertEqual("rear-attack", load_tutorial("rear-attack").name)
        self.assertEqual("rear-attack", load_tutorial("rear").name)
        with self.assertRaises(ValueError):
            load_tutorial("missing")

    def test_wrong_gameplay_choice_is_blocked_without_advancing(self):
        tutorial = Tutorial()

        self.assertFalse(tutorial.accepts("mov 9"))
        self.assertFalse(tutorial.advances_for("mov 9"))
        self.assertEqual(0, tutorial.index)
        self.assertIn("no choice was applied", tutorial.reject_text("mov 9"))
        self.assertIn("mov 10", tutorial.reject_text("mov 9"))

    def test_required_choice_advances_but_inspection_does_not(self):
        tutorial = Tutorial()

        for command in ("help", "status", "board", "ships", "log"):
            self.assertTrue(tutorial.accepts(command))
            self.assertFalse(tutorial.advances_for(command))
        self.assertEqual(0, tutorial.index)

        self.assertTrue(tutorial.accepts("  MOV   10 "))
        self.assertTrue(tutorial.advances_for("  MOV   10 "))
        tutorial.advance()
        self.assertEqual("w b1 4", tutorial.step.command)

    def test_panel_narrates_reason_and_required_command(self):
        text = Tutorial().panel_text(_snapshot())

        self.assertIn("Step 1/29", text)
        self.assertIn("circle below the escort", text)
        self.assertIn("Required command: mov 10", text)

    def test_state_guard_checks_turn_phase_and_movement_cycle(self):
        tutorial = Tutorial()
        self.assertIsNone(tutorial.state_error(_snapshot()))
        self.assertIn(
            "expected turn/phase",
            tutorial.state_error(_snapshot(phase="movement")),
        )

        for _ in range(5):
            tutorial.advance()
        self.assertEqual("accel 5", tutorial.step.command)
        self.assertIsNone(
            tutorial.state_error(
                _snapshot(phase="movement", movement_phase=1)
            )
        )
        self.assertIn(
            "expected movement cycle 1",
            tutorial.state_error(
                _snapshot(phase="movement", movement_phase=2)
            ),
        )

    def test_recorded_sequence_reaches_victory_narration(self):
        tutorial = Tutorial()

        for step in tutorial.steps:
            snap = _snapshot(
                turn=step.turn,
                phase=step.phase,
                movement_phase=step.movement_phase or 0,
            )
            self.assertIsNone(tutorial.state_error(snap))
            self.assertTrue(tutorial.accepts(step.command))
            tutorial.advance()

        self.assertTrue(tutorial.complete)
        text = tutorial.panel_text(_snapshot(turn=2, phase="firing", status="Won"))
        self.assertIn("Rear attack complete", text)
        self.assertIn("shield 3:R", text)
        self.assertIn("Won", text)


if __name__ == "__main__":
    unittest.main()
