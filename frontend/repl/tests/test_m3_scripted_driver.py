"""M3 scripted-ship auto-driver (combat.toml deadlock, task #8) — protocol v4.

Design decision under test: the engine does NOT auto-advance ships with
controller "scripted". In interactive play the REPL drives them passively with
empty allocate / empty path / empty volley when the stage is blocked ONLY on
scripted ships.
"""

import unittest

import repl
from commands import ReplContext
from tests.test_characterization import FakeSession, FakeUI


def _weapon(mount="forward", max_range=5, charge=1, **kw):
    w = {
        "id": kw.get("id", "beam_1"),
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


def _ship(sid, controller, q=0, r=0, facing=0, destroyed=False, weapons=None):
    return {
        "id": sid,
        "class": "Escort",
        "controller": controller,
        "destroyed": destroyed,
        "q": q,
        "r": r,
        "facing": facing,
        "structure": 4,
        "power": 4,
        "power_available": 4,
        "motion_available": 0,
        "max_maneuver_actions": 4,
        "weapons": weapons if weapons is not None else [_weapon()],
        "max_shield_per_facing": 2,
    }


def _snap(ships, phase, **kw):
    snap = {
        "protocol_version": 4,
        "phase": phase,
        "status": "Playing",
        "turn": 1,
        "ships": ships,
        "combat_log": [],
        "ships_allocated_this_turn": [],
        "ships_committed_path": [],
        "ships_committed_volley": [],
    }
    snap.update(kw)
    return snap


def _drive(session):
    ui = FakeUI()
    ctx = ReplContext()
    repl.auto_drive_scripted(ui, session, ctx)
    return session.sent


class AllocatePhaseTests(unittest.TestCase):
    def test_c8_allocate_drives_scripted_ship_passively(self):
        snap = _snap(
            [_ship(1, "player"), _ship(2, "scripted")],
            phase="allocate",
            ships_allocated_this_turn=[1],
        )
        sent = _drive(FakeSession(snap))
        self.assertEqual(1, len(sent))
        order = sent[0]
        self.assertEqual("allocate", order.get("type"))
        self.assertEqual(2, order.get("ship"))
        self.assertEqual(0, int(order.get("movement") or 0))
        self.assertTrue(all(int(v) == 0 for v in (order.get("weapons") or {}).values()))
        self.assertTrue(all(int(v) == 0 for v in (order.get("shields") or [0] * 6)))

    def test_destroyed_weapons_are_omitted_from_passive_allocate(self):
        destroyed = _weapon(id="beam_1", operational=False)
        snap = _snap(
            [_ship(1, "player"), _ship(2, "scripted", weapons=[destroyed])],
            phase="allocate",
            ships_allocated_this_turn=[1],
        )
        sent = _drive(FakeSession(snap))
        self.assertEqual({}, sent[0]["weapons"])

    def test_pump_stops_when_passive_order_makes_no_progress(self):
        snap = _snap(
            [_ship(1, "player"), _ship(2, "scripted")],
            phase="allocate",
            ships_allocated_this_turn=[1],
        )
        session = FakeSession(snap)
        repl.pump_scripted(FakeUI(), session, ReplContext(), 0, max_steps=10)
        self.assertEqual(1, len(session.sent))


class MovementPhaseTests(unittest.TestCase):
    def test_c8_movement_drives_scripted_ship_with_empty_path(self):
        snap = _snap(
            [_ship(1, "player"), _ship(2, "scripted")],
            phase="movement",
            ships_committed_path=[1],
        )
        sent = _drive(FakeSession(snap))
        self.assertEqual(1, len(sent))
        self.assertEqual(
            {"type": "commit_path", "ship": 2, "actions": []},
            {k: v for k, v in sent[0].items() if k in ("type", "ship", "actions")},
        )


class FiringPhaseTests(unittest.TestCase):
    def test_c8_firing_drives_scripted_ship_with_empty_volley(self):
        snap = _snap(
            [_ship(1, "player"), _ship(2, "scripted")],
            phase="firing",
            ships_committed_volley=[1],
        )
        sent = _drive(FakeSession(snap))
        self.assertEqual(1, len(sent))
        self.assertEqual(
            {"type": "commit_volley", "ship": 2, "shots": []},
            {k: v for k, v in sent[0].items() if k in ("type", "ship", "shots")},
        )


class NegativeControlTests(unittest.TestCase):
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
        snap = _snap(
            [_ship(1, "player"), _ship(2, "player"), _ship(3, "scripted")],
            phase="allocate",
            ships_allocated_this_turn=[],
        )
        sent = _drive(FakeSession(snap))
        self.assertEqual([], sent)


if __name__ == "__main__":
    unittest.main()
