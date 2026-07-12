import contextlib
import io
import unittest
from pathlib import Path
from unittest.mock import patch

from commands import AllocDraft, ReplContext, build_action, phase_hint
from repl import run_repl, send_orders
from screen import TerminalUI
import repl
from client import TransportError
from tests.test_characterization import FakeSession, FakeUI, snapshot


class CommandPolicyTests(unittest.TestCase):
    def test_terminal_guard_is_authoritative(self):
        workflows = ("allocate", "move", "pass_move", "commit_fire", "ready_fire",
                     "end_turn", "expert_raw")
        for workflow in workflows:
            with self.subTest(workflow=workflow):
                snap = snapshot(status="Lost")
                session = FakeSession(snap)
                ui = FakeUI()
                send_orders(ui, session, ReplContext(), [{"type": workflow}], prev_log_len=0)
                self.assertEqual([], session.sent)
                self.assertTrue(any("Lost" in line for line in ui.lines))

    def test_terminal_transition_stops_multi_order_batch(self):
        class TransitionSession(FakeSession):
            def send_order(self, order):
                self.sent.append(order)
                self.snapshot = {**self.snapshot, "status": "Won"}
                return self.snapshot
        session = TransitionSession(snapshot())
        send_orders(FakeUI(), session, ReplContext(),
                    [{"type": "move"}, {"type": "move"}], prev_log_len=0)
        self.assertEqual(1, len(session.sent))

    def test_terminal_hint_and_banner_are_once(self):
        snap = snapshot(status="Won")
        self.assertIn("quit", phase_hint(snap, ReplContext()))
        self.assertIn("log", phase_hint(snap, ReplContext()))
        session = FakeSession(snap)
        session.bin = "fake-bin"
        session.scenario = "fake-scenario"
        session.orders_log = type("P", (), {"name": "orders.jsonl"})()
        ui = TerminalUI(session_path=None, scroll=True)
        output = io.StringIO()
        with contextlib.redirect_stdout(output), unittest.mock.patch("builtins.input", return_value="quit"):
            self.assertEqual(0, run_repl(session, ui))
        self.assertEqual(1, output.getvalue().count("SCENARIO WON"))

    def test_main_reports_transport_failure_cleanly(self):
        class FailedSession:
            def __init__(self, *args, **kwargs): pass
            def start(self): raise TransportError("read failed; see stderr-test.log")
            def close(self): pass
        stderr = io.StringIO()
        with patch.object(repl, "setup_readline"), \
             patch.object(repl, "find_repo_root", return_value=Path("/tmp")), \
             patch.object(repl, "find_shipsim_bin", return_value=Path("/bin/true")), \
             patch.object(repl, "pick_scenario", return_value="scenario.toml"), \
             patch.object(repl, "ShipsimSession", FailedSession), \
             contextlib.redirect_stderr(stderr):
            self.assertEqual(1, repl.main(["scenario.toml", "--no-session-log", "--scroll"]))
        self.assertIn("engine terminated", stderr.getvalue())
        self.assertIn("stderr-test.log", stderr.getvalue())

    def test_move_and_pass_require_movement_phase(self):
        snap = snapshot(phase="firing")
        ctx = ReplContext(selected=1)
        for command in ("m f", "pass"):
            with self.subTest(command=command):
                with contextlib.redirect_stdout(io.StringIO()):
                    self.assertFalse(build_action(command, snap, ctx).orders)

    def test_ad_does_not_replace_dirty_draft(self):
        snap = snapshot(phase="allocate")
        ctx = ReplContext(selected=1, draft=AllocDraft.from_ship(snap["ships"][0]))
        ctx.draft.movement = 1
        with contextlib.redirect_stdout(io.StringIO()):
            action = build_action("ad", snap, ctx)
        self.assertFalse(action.orders)
        self.assertIsNotNone(ctx.draft)

    def test_focus_switch_removes_empty_orphan_draft(self):
        snap = snapshot(phase="allocate")
        snap["ships"].append({**snap["ships"][0], "id": 2})
        ctx = ReplContext(selected=1, draft=AllocDraft.from_ship(snap["ships"][0]))
        with contextlib.redirect_stdout(io.StringIO()):
            build_action("ship 2", snap, ctx)
        self.assertEqual(2, ctx.selected)
        self.assertEqual(2, ctx.draft.ship_id)

    def test_non_player_explicit_fire_is_refused_before_wizard(self):
        snap = snapshot(phase="firing")
        snap["ships"].append({**snap["ships"][0], "id": 2, "controller": "ai"})
        with contextlib.redirect_stdout(io.StringIO()):
            action = build_action("fire 2", snap, ReplContext(selected=1))
        self.assertFalse(action.orders)


if __name__ == "__main__":
    unittest.main()
