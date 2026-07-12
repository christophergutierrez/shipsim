import builtins
import contextlib
import io
import unittest

from commands import COMMAND_REGISTRY, ReplContext, build_action, render_help
from tests.test_characterization import snapshot


def _fire_snapshot():
    return {
        "protocol_version": 2,
        "phase": "firing",
        "status": "InProgress",
        "turn": 1,
        "ships": [
            {"id": 1, "class": "Heavy Cruiser", "controller": "player", "destroyed": False,
             "q": 1, "r": 0, "facing": 3, "structure": 12, "power": 22,
             "velocity": 0, "course": 3, "thrust_remaining": 4, "max_velocity": 4,
             "weapons": [], "max_shield_per_facing": 6},
            {"id": 2, "class": "Escort", "controller": "scripted", "destroyed": False,
             "q": 0, "r": 0, "facing": 0, "structure": 12, "power": 14,
             "velocity": 0, "course": 0, "thrust_remaining": 0, "max_velocity": 4,
             "weapons": [], "max_shield_per_facing": 6},
        ],
        "fire_commits": [{"ship": 1, "weapon": "beam_1", "target": 2, "shield_facing": 0}],
        "combat_log": [],
    }


class InterfaceGoldenTests(unittest.TestCase):
    def test_help_is_generated_from_registry(self):
        text = render_help()
        for syntax, description in COMMAND_REGISTRY.values():
            self.assertIn(syntax, text)
            self.assertIn(description, text)

    def test_help_topic_has_syntax_example_and_description(self):
        text = render_help("attack")
        self.assertIn("fire", text)
        self.assertIn("example:", text)
        self.assertIn("charged weapon", text)

    def test_question_mark_and_attack_alias(self):
        snap = snapshot(phase="movement")
        self.assertEqual("help", build_action("?", snap, ReplContext()).side)
        snap["phase"] = "firing"
        self.assertEqual("fire_loop", build_action("ATTACK", snap, ReplContext(selected=1)).side)

    def test_blank_and_unknown_are_actionable(self):
        snap = snapshot()
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual("empty", build_action("", snap, ReplContext()).side)
            self.assertEqual("unknown", build_action("atack", snap, ReplContext()).side)
        text = out.getvalue()
        self.assertIn("help", text)
        self.assertIn("Did you mean 'attack'", text)


class ConfirmPromptEOFSafety(unittest.TestCase):
    """A confirmation prompt hitting EOF (e.g. stdin closed / piped input
    exhausted) must not raise an unhandled EOFError — it must fall back to
    the safe (non-destructive, or explicit) default."""

    def _run_with_eof(self, line, snap, ctx):
        orig_input = builtins.input

        def fake_input(*_a, **_k):
            raise EOFError()

        builtins.input = fake_input
        out = io.StringIO()
        try:
            with contextlib.redirect_stdout(out):
                return build_action(line, snap, ctx)
        finally:
            builtins.input = orig_input

    def test_end_turn_confirm_eof_does_not_crash(self):
        snap = _fire_snapshot()
        action = self._run_with_eof("end", snap, ReplContext())
        self.assertEqual("empty", action.side)

    def test_prompt_int_eof_does_not_crash_and_returns_default(self):
        # `w` with no args at the draft root, then a bare weapon alias with
        # no value, exercises _prompt_int via the interactive draft path.
        snap = _fire_snapshot()
        snap["phase"] = "allocate"
        ctx = ReplContext()
        build_action("a 1", snap, ctx)
        build_action("w", snap, ctx)
        action = self._run_with_eof("b1", snap, ctx)
        self.assertEqual("empty", action.side)


class EndTurnDiscardsQueuedShotsIsWarned(unittest.TestCase):
    """Ending the whole turn mid-firing silently discards any shot already
    queued (commit_fire) but not yet resolved via ready_fire — the engine
    drops it with no combat_log entry. The confirmation prompt must warn
    about this explicitly, not just say "ends the WHOLE turn"."""

    def test_pending_shot_warning_names_the_queued_shot(self):
        snap = _fire_snapshot()
        out = io.StringIO()
        orig_input = builtins.input
        builtins.input = lambda *_a, **_k: "no"
        try:
            with contextlib.redirect_stdout(out):
                build_action("end", snap, ReplContext())
        finally:
            builtins.input = orig_input
        text = out.getvalue()
        self.assertIn("DISCARD", text)
        self.assertIn("beam_1", text)


if __name__ == "__main__":
    unittest.main()
