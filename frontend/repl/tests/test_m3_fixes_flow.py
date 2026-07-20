"""M3 fixes: failing (red) reproductions of six known live/UX bugs.

Each test below documents an INTENDED behavior that the current code does
NOT implement yet. They must all fail today (C1 legitimately fails with a
RecursionError — that's the bug). Mirrors the style of test_characterization
/ test_m1_commands / test_m2_targeting: synthetic snapshot dicts, FakeSession
/ FakeUI where suitable, real TerminalUI where the bug is about stdout
behavior specifically.
"""

import contextlib
import io
import re
import unittest

import view
from commands import AllocDraft, ReplContext, build_action, interactive_fire
from repl import send_orders
from screen import TerminalUI
from tests.test_characterization import FakeSession, FakeUI, snapshot

ANSI = re.compile(r"\x1b\[[0-9;]*m")


def _weapon(mount="forward", max_range=5, charge=1, **kw):
    w = {
        "id": kw.get("id", "W1"), "mount": mount, "max_range": max_range,
        "max_charge": 1, "charge": charge, "operational": True,
        "fired": False, "kind": "Laser",
    }
    w.update(kw)
    return w


def _ship(sid, q, r, facing=0, controller="player", weapons=None, destroyed=False,
          structure=4, power=4):
    return {
        "id": sid, "class": "Scout", "controller": controller,
        "destroyed": destroyed, "q": q, "r": r, "facing": facing,
        "structure": structure, "power": power, "weapons": weapons or [],
        "max_shield_per_facing": 2, "shields_remaining": [2, 2, 2, 2, 2, 2],
        "bridge": 1, "engine": 1, "power_sys": 1, "keel": structure,
    }


def _snap(ships, **kw):
    snap = {
        "protocol_version": 4, "phase": "firing", "status": "Playing",
        "turn": 1,
        "ships": ships, "combat_log": [],
        "ships_committed_path": [], "ships_committed_volley": [],
    }
    snap.update(kw)
    return snap


class C1NoRecursionWithoutPlayerShips(unittest.TestCase):
    """format_snapshot must not mutually-recurse forever when no player ship
    exists (only AI ships in the snapshot, selected=None)."""

    def test_c1_no_recursion_without_player_ships(self):
        snap = _snap([
            _ship(1, 0, 0, 0, "ai"),
            _ship(2, 3, 0, 3, "ai"),
        ], phase="movement")
        result = view.format_snapshot(snap, selected=None, hull_max={}, verbose=True)
        self.assertIsInstance(result, str)


class C2EngineRejectionReachesStdout(unittest.TestCase):
    """In play-frame mode (scroll=False), an engine 'error' response from
    send_orders must still surface to the player immediately — ui.log()
    alone is silent in that mode, so the rejection text must reach stdout
    some other way (live print or a repaint before the break)."""

    def test_c2_engine_rejection_reaches_stdout(self):
        class RejectingSession(FakeSession):
            def send_order(self, order):
                self.sent.append(order)
                return {
                    "type": "error",
                    "code": "order_illegal",
                    "message": "DISTINCTIVE_C2_MARKER engine rejected the move",
                }

        ui = TerminalUI(scroll=False, session_path=None)
        session = RejectingSession(snapshot())
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            send_orders(
                ui, session, ReplContext(),
                [{"type": "move", "ship": 1, "mode": "forward"}],
                prev_log_len=0,
            )
        self.assertIn("DISTINCTIVE_C2_MARKER", buf.getvalue())


class C3FailedCommitPreservesDraft(unittest.TestCase):
    """If the engine rejects a committed 'allocate' order, the player's
    draft must not be thrown away — they should still be able to see/adjust
    it, not start over from scratch."""

    def test_c3_failed_commit_preserves_draft(self):
        snap = snapshot(phase="allocate")
        ship = snap["ships"][0]
        ctx = ReplContext(selected=1, draft=AllocDraft.from_ship(ship))
        ctx.draft.movement = 2
        used_before = ctx.draft.used()
        movement_before = ctx.draft.movement

        with contextlib.redirect_stdout(io.StringIO()):
            action = build_action("commit", snap, ctx)
        self.assertTrue(action.orders, "commit with a non-empty draft should produce an order")

        class RejectingSession(FakeSession):
            def send_order(self, order):
                self.sent.append(order)
                return {
                    "type": "error",
                    "code": "power_exceeded",
                    "message": "allocated 14 power, only 11 available",
                }

        session = RejectingSession(snap)
        with contextlib.redirect_stdout(io.StringIO()):
            send_orders(FakeUI(), session, ctx, action.orders, prev_log_len=0)

        self.assertIsNotNone(ctx.draft, "draft was destroyed even though the engine rejected the commit")
        self.assertEqual(used_before, ctx.draft.used())
        self.assertEqual(movement_before, ctx.draft.movement)


class C4DamageReducedPool(unittest.TestCase):
    """AllocDraft and the YOUR SHIP card must use the damage-reduced
    power_available field (when present) instead of the stale power field."""

    def test_c4_alloc_draft_uses_power_available(self):
        ship = {**snapshot()["ships"][0], "power": 22, "power_available": 11}
        draft = AllocDraft.from_ship(ship)
        self.assertEqual(11, draft.power)

    def test_c4_ship_card_shows_effective_pool(self):
        ship = {**snapshot()["ships"][0], "power": 22, "power_available": 11}
        out = ANSI.sub("", view.format_ship_card(ship))
        self.assertNotIn("pwr=22", out)
        self.assertIn("11", out.split("pwr=", 1)[-1][:6] if "pwr=" in out else "")


class C5FireTargetingSanity(unittest.TestCase):
    """(a) interactive_fire must never list a same-controller ship as a
    target. (b) if the chosen weapon has no in-range-and-in-arc target it
    must refuse (return None) rather than silently queuing a wasted shot."""

    def test_c5_target_list_excludes_friendly_ships(self):
        shooter = _ship(1, 0, 0, 0, "player", [_weapon("forward", max_range=5, id="L1")])
        friendly = _ship(2, 5, -5, 0, "player")
        enemy_a = _ship(3, 3, 0, 3, "ai")
        enemy_b = _ship(4, 3, 0, 3, "ai")
        snap = _snap([shooter, friendly, enemy_a, enemy_b], phase="firing")

        buf = io.StringIO()
        import builtins
        orig_input = builtins.input
        answers = iter(["0", "0", "0", "0"])
        builtins.input = lambda *_a, **_k: next(answers)
        try:
            with contextlib.redirect_stdout(buf):
                order = interactive_fire(snap, 1)
        finally:
            builtins.input = orig_input

        text = ANSI.sub("", buf.getvalue())
        # Friendly ship's callsign (controller=player, id=2 -> "A2") must never
        # appear in the printed target list.
        self.assertNotIn("A2", text)
        self.assertIsNotNone(order)
        self.assertNotEqual(2, order.get("target"), "friendly ship was offered/selected as a fire target")

    def test_c5_refuses_when_no_legal_shot(self):
        shooter = _ship(1, 0, 0, 0, "player", [_weapon("forward", max_range=2, id="L1")])
        far_enemy = _ship(2, 3, 0, 3, "ai")  # distance 3 > max_range 2
        snap = _snap([shooter, far_enemy], phase="firing")

        buf = io.StringIO()
        import builtins
        orig_input = builtins.input
        answers = iter(["0", "0", "0"])
        builtins.input = lambda *_a, **_k: next(answers)
        try:
            with contextlib.redirect_stdout(buf):
                order = interactive_fire(snap, 1)
        finally:
            builtins.input = orig_input

        self.assertIsNone(order, "weapon with no in-range/in-arc target was silently queued")

    def test_c5_done_emits_commit_volley_not_none(self):
        """Selecting "Done" (-1) at the weapon menu must submit the volley
        directly by returning a commit_volley order, not drop the player back
        to the main prompt with None."""
        shooter = _ship(1, 0, 0, 0, "player", [_weapon("forward", max_range=5, id="L1")])
        enemy = _ship(2, 3, 0, 3, "ai")
        snap = _snap([shooter, enemy], phase="firing")
        ctx = ReplContext(selected=1)

        buf = io.StringIO()
        import builtins
        orig_input = builtins.input
        answers = iter(["-1"])
        builtins.input = lambda *_a, **_k: next(answers)
        try:
            with contextlib.redirect_stdout(buf):
                order = interactive_fire(snap, 1, ctx)
        finally:
            builtins.input = orig_input

        self.assertIsNotNone(order, "Done returned None instead of a commit_volley order")
        self.assertEqual("commit_volley", order.get("type"))
        self.assertEqual(1, order.get("ship"))
        self.assertEqual([], order.get("shots"))


class C6KillShotAnnouncesDestruction(unittest.TestCase):
    """format_combat_events must call out a kill instead of quietly showing
    a bare ship-line for a destroyed target."""

    def test_c6_kill_shot_announces_destroyed(self):
        attacker = _ship(1, 0, 0, 0, "player", [_weapon("forward", id="L1")])
        target = _ship(2, 3, 0, 3, "ai", destroyed=True, structure=0)
        snap = _snap([attacker, target])
        events = [{
            "attacker": 1, "target": 2, "kind": "hit",
            "damage": 9, "shield": 0, "weapon": "L1",
        }]
        out = ANSI.sub("", view.format_combat_events(events, snap))
        self.assertIn("DESTROYED", out)


if __name__ == "__main__":
    unittest.main()
