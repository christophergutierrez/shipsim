"""M3 scripted-ship auto-driver (combat.toml deadlock, task #8).

Design decision under test: the engine does NOT auto-advance ships with
controller "scripted" (they are externally driven, e.g. by a recorded
orders file). In interactive play the REPL is the only order source, so a
phase blocked ONLY on scripted-controller ships is driven passively by
`repl.auto_drive_scripted` / `plan_scripted_orders` (and the live loop's
`pump_scripted`).

Never drives AI (harness) or pending player ships.
"""

import unittest

import repl
from commands import ReplContext
from tests.test_characterization import FakeSession, FakeUI


def _weapon(mount="forward", max_range=5, charge=1, **kw):
    w = {
        "id": kw.get("id", "beam_1"), "mount": mount, "max_range": max_range,
        "max_charge": 1, "charge": charge, "operational": True,
        "fired": False, "kind": "Laser",
    }
    w.update(kw)
    return w


def _ship(sid, controller, q=0, r=0, facing=0, destroyed=False, weapons=None):
    return {
        "id": sid, "class": "Escort", "controller": controller,
        "destroyed": destroyed, "q": q, "r": r, "facing": facing,
        "structure": 4, "power": 4, "power_available": 4,
        "weapons": weapons if weapons is not None else [_weapon()],
        "max_shield_per_facing": 2,
    }


def _snap(ships, phase, **kw):
    snap = {
        "protocol_version": 1, "phase": phase, "status": "Playing",
        "turn": 1, "active_ship": None, "ships": ships, "combat_log": [],
        "ships_allocated_this_turn": [], "ships_moved_this_phase": [],
        "ships_ready_fire": [], "move_order": [], "fire_commits": [],
    }
    snap.update(kw)
    return snap


def _drive(session):
    """Call the (not-yet-existing) client-side scripted driver."""
    ui = FakeUI()
    ctx = ReplContext()
    repl.auto_drive_scripted(ui, session, ctx)
    return session.sent


class AllocatePhaseTests(unittest.TestCase):
    """Only the scripted ship is unallocated -> client sends its allocate."""

    def test_c8_allocate_drives_scripted_ship_passively(self):
        snap = _snap(
            [_ship(1, "player"), _ship(2, "scripted")],
            phase="allocate",
            ships_allocated_this_turn=[1],  # player already allocated
        )
        sent = _drive(FakeSession(snap))
        self.assertEqual(1, len(sent))
        order = sent[0]
        self.assertEqual("allocate", order.get("type"))
        self.assertEqual(2, order.get("ship"))
        self.assertEqual(0, int(order.get("movement") or 0))
        self.assertTrue(all(int(v) == 0 for v in (order.get("weapons") or {}).values()))
        self.assertTrue(all(int(v) == 0 for v in (order.get("shields") or [0] * 6)))


class MovementPhaseTests(unittest.TestCase):
    """Active ship is the scripted one -> client sends pass_move for it."""

    def test_c8_movement_drives_scripted_ship_with_pass_move(self):
        snap = _snap(
            [_ship(1, "player"), _ship(2, "scripted")],
            phase="movement",
            active_ship=2,
            move_order=[1, 2],
            ships_moved_this_phase=[1],
        )
        sent = _drive(FakeSession(snap))
        self.assertEqual(1, len(sent))
        self.assertEqual({"type": "pass_move", "ship": 2}, {
            k: v for k, v in sent[0].items() if k in ("type", "ship")
        })


class FiringPhaseTests(unittest.TestCase):
    """Only the scripted ship isn't ready_fire -> client sends its ready_fire."""

    def test_c8_firing_drives_scripted_ship_with_ready_fire(self):
        snap = _snap(
            [_ship(1, "player"), _ship(2, "scripted")],
            phase="firing",
            ships_ready_fire=[1],
        )
        sent = _drive(FakeSession(snap))
        self.assertEqual(1, len(sent))
        self.assertEqual({"type": "ready_fire", "ship": 2}, {
            k: v for k, v in sent[0].items() if k in ("type", "ship")
        })


class NegativeControlTests(unittest.TestCase):
    """The client must never drive ai ships (harness advances those) or
    pending player ships (only the human drives those)."""

    def test_c8_ai_controller_not_auto_driven(self):
        snap = _snap(
            [_ship(1, "player"), _ship(2, "ai")],
            phase="allocate",
            ships_allocated_this_turn=[1],
        )
        sent = _drive(FakeSession(snap))
        self.assertEqual([], sent)

    def test_c8_pending_player_ship_not_auto_driven(self):
        snap = _snap(
            [_ship(1, "player"), _ship(2, "player")],
            phase="allocate",
            ships_allocated_this_turn=[1],
        )
        sent = _drive(FakeSession(snap))
        self.assertEqual([], sent)

    def test_c8_mixed_scripted_and_player_pending_not_driven(self):
        """Blocked on BOTH a scripted and a still-pending player ship: the
        client must not drive the scripted one out of turn, since the phase
        isn't blocked ONLY on scripted ships."""
        snap = _snap(
            [_ship(1, "player"), _ship(2, "player"), _ship(3, "scripted")],
            phase="allocate",
            ships_allocated_this_turn=[],  # nobody has allocated yet
        )
        sent = _drive(FakeSession(snap))
        self.assertEqual([], sent)


if __name__ == "__main__":
    unittest.main()
