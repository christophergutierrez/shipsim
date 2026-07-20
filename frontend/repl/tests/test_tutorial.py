"""Strict rear-attack tutorial sequencing and narration (protocol 4)."""

import unittest

from tutorial import Tutorial, load_tutorial


def _snapshot(*, turn=1, phase="allocate", status="InProgress"):
    return {
        "turn": turn,
        "phase": phase,
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
        self.assertIn("mov 8", tutorial.reject_text("mov 9"))

    def test_required_choice_advances_but_inspection_does_not(self):
        tutorial = Tutorial()
        for command in ("help", "status", "board", "ships", "log", "motion"):
            self.assertTrue(tutorial.accepts(command))
            self.assertFalse(tutorial.advances_for(command))
        self.assertEqual(0, tutorial.index)
        self.assertTrue(tutorial.accepts("  MOV   8 "))
        self.assertTrue(tutorial.advances_for("  MOV   8 "))
        tutorial.advance()
        self.assertEqual("w b1 4", tutorial.step.command)

    def test_panel_narrates_reason_and_required_command(self):
        text = Tutorial().panel_text(_snapshot())
        self.assertIn(f"Step 1/{len(Tutorial().steps)}", text)
        self.assertIn("MISSION", text)
        self.assertIn("protocol 4", text.lower())
        self.assertIn("Type exactly: mov 8", text)

    def test_live_prompt_repeats_exact_choice(self):
        text = Tutorial().prompt_text()
        self.assertIn("tutorial next:", text)
        self.assertIn("mov 8", text)

    def test_state_guard_checks_turn_and_phase(self):
        tutorial = Tutorial()
        self.assertIsNone(tutorial.state_error(_snapshot()))
        self.assertIn(
            "expected turn/phase",
            tutorial.state_error(_snapshot(phase="movement")),
        )
        while tutorial.step and tutorial.step.phase != "movement":
            tutorial.advance()
        self.assertIsNone(tutorial.state_error(_snapshot(phase="movement")))

    def test_recorded_sequence_reaches_victory_narration(self):
        tutorial = Tutorial()
        for step in tutorial.steps:
            snap = _snapshot(turn=step.turn, phase=step.phase)
            self.assertIsNone(tutorial.state_error(snap), step.command)
            self.assertTrue(tutorial.accepts(step.command), step.command)
            tutorial.advance()
        self.assertTrue(tutorial.complete)
        text = tutorial.panel_text(_snapshot(turn=1, phase="firing", status="Won"))
        self.assertIn("complete", text.lower())
        self.assertIn("Won", text)

    def test_motion_steps_teach_protocol4_verbs(self):
        tutorial = Tutorial()
        cmds = [s.command for s in tutorial.steps]
        self.assertTrue(any(c.startswith("path ") for c in cmds))
        self.assertIn("preview", cmds)
        self.assertIn("commit", cmds)
        self.assertNotIn("accel", cmds)
        self.assertNotIn("end_turn", cmds)
        self.assertNotIn("e", cmds)
        self.assertIn("fire b1 B2", cmds)
        self.assertIn("fire t1 B2", cmds)
        self.assertIn("fire p1 B2", cmds)
        self.assertIn("ready", cmds)
        self.assertEqual(1, max(s.turn for s in tutorial.steps))

    def test_arms_all_weapons_on_turn_one(self):
        cmds = [s.command for s in Tutorial().steps if s.turn == 1]
        self.assertIn("w b1 4", cmds)
        self.assertIn("w t1 1", cmds)
        self.assertIn("w p1 1", cmds)


if __name__ == "__main__":
    unittest.main()
