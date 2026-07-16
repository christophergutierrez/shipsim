"""M2 tactical: threat panel, unspent-power advisory, callsign initiative.

These tests exercise the view-layer threat panel (format_tactical), the
AllocDraft unspent-power summary line, and callsign-based initiative/readiness
display. All data is synthetic snapshot dicts — no engine process.
"""

import re
import unittest

from hexutil import threats_to_ship
from view import format_engagement, format_tactical, format_ship_card, ship_callsign

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
          structure=4):
    return {
        "id": sid, "class": "Scout", "controller": controller,
        "destroyed": destroyed, "q": q, "r": r, "facing": facing,
        "structure": structure, "power": 4, "size": 2, "weapons": weapons or [],
        "max_shield_per_facing": 2, "shields_remaining": [2, 2, 2, 2, 2, 2],
        "bridge": 1, "engine": 1, "power_sys": 1, "keel": 4,
    }


def _snap(ships, **kw):
    snap = {
        "protocol_version": 3, "phase": "firing", "status": "Playing",
        "turn": 1, "active_ship": 1, "ships": ships, "combat_log": [],
    }
    snap.update(kw)
    return snap


class ThreatPanelTests(unittest.TestCase):
    """format_tactical shows threat info inside CONTACTS when enemies can bear.

    Threats are merged into the CONTACTS panel (UX_ANALYSIS.md §1g) rather than
    shown as a separate THREATS panel, so the player sees each contact and its
    threat status in one place.
    """

    def test_threat_info_appears_when_enemy_can_bear(self):
        snap = _snap([
            _ship(1, 0, 0, 0, "player"),
            _ship(2, 3, 0, 3, "ai", [_weapon("forward", id="L1")]),
        ])
        out = ANSI.sub("", format_tactical(snap, selected=1))
        self.assertIn("CONTACTS", out)
        self.assertIn("can bear", out)
        self.assertIn("L1", out)

    def test_no_threat_info_when_no_enemy_can_bear(self):
        snap = _snap([
            _ship(1, 0, 0, 0, "player"),
            _ship(2, 3, 0, 0, "ai", [_weapon("forward", id="L1")]),  # facing away
        ])
        out = ANSI.sub("", format_tactical(snap, selected=1))
        self.assertNotIn("can bear", out)

    def test_threat_info_shows_range(self):
        snap = _snap([
            _ship(1, 0, 0, 0, "player"),
            _ship(2, 3, 0, 3, "ai", [_weapon("forward", id="L1")]),
        ])
        out = ANSI.sub("", format_tactical(snap, selected=1))
        self.assertIn("range=3", out)


class EngagementPanelTests(unittest.TestCase):
    def test_shows_range_bearing_exposed_shield_and_weapon_solution(self):
        me = _ship(1, 0, 0, 0, "player", [_weapon("forward", max_range=5, id="L1")])
        enemy = _ship(2, -3, 0, 0, "ai")
        out = ANSI.sub("", format_engagement(me, [enemy]))
        self.assertIn("range=3", out)
        self.assertIn("bearing=3:R", out)
        self.assertIn("shield exposed=0:F", out)
        self.assertIn("L1 forward: range 3/5", out)
        self.assertIn("OUT OF ARC", out)

    def test_weapon_solution_marks_current_in_arc_target(self):
        me = _ship(1, 0, 0, 0, "player", [_weapon("forward", max_range=5, id="L1")])
        enemy = _ship(2, 3, 0, 3, "ai")
        out = ANSI.sub("", format_engagement(me, [enemy]))
        self.assertIn("FIRE READY", out)

    def test_overlapping_contact_is_too_close_not_out_of_arc(self):
        me = _ship(1, 0, 0, 0, "player", [_weapon("forward", id="L1")])
        enemy = _ship(2, 0, 0, 3, "ai")

        out = ANSI.sub("", format_engagement(me, [enemy]))

        self.assertIn("OVERLAPPING", out)
        self.assertIn("TOO CLOSE", out)
        self.assertNotIn("OUT OF ARC", out)

    def test_tactical_frame_includes_engagement_panel(self):
        snap = _snap([
            _ship(1, 0, 0, 0, "player", [_weapon("forward", id="L1")]),
            _ship(2, 3, 0, 3, "ai"),
        ])
        out = ANSI.sub("", format_tactical(snap, selected=1))
        self.assertIn("ENGAGEMENT", out)

    def test_tactical_engagement_excludes_friendly_ships(self):
        snap = _snap([
            _ship(1, 0, 0, 0, "player", [_weapon("forward", id="L1")]),
            _ship(2, 1, 0, 0, "player"),
            _ship(3, 3, 0, 3, "ai"),
        ])

        out = ANSI.sub("", format_tactical(snap, selected=1))
        engagement = out.split("┌─ ENGAGEMENT", 1)[1].split("└", 1)[0]

        self.assertNotIn("A2", engagement)
        self.assertIn("B3", engagement)

    def test_engagement_shows_target_size_adjusted_hit_chance(self):
        me = _ship(
            1, 0, 0, 0, "player",
            [_weapon("forward", max_range=5, id="L1", kind="beam")],
        )
        enemy = _ship(2, 3, 0, 3, "ai")
        enemy["size"] = 1

        out = ANSI.sub("", format_engagement(me, [enemy]))

        self.assertIn("size=1", out)
        self.assertIn("to-hit≤8 (40%)", out)


class UnspentPowerTests(unittest.TestCase):
    """AllocDraft.summary warns when power is left unspent after allocation."""

    def test_summary_warns_on_unspent_power(self):
        from commands import AllocDraft
        d = AllocDraft.from_ship(_ship(1, 0, 0, 0, "player",
                                        [_weapon("forward", id="L1", max_charge=2)]))
        d.movement = 0
        d.weapons = {"L1": 0}
        d.shields = [0, 0, 0, 0, 0, 0]
        text = d.summary()
        self.assertTrue("unspent power" in text or "free in pool" in text)

    def test_summary_silent_when_power_fully_spent(self):
        from commands import AllocDraft
        # No carried charge: spending full pool silences the unspent warning.
        ship = _ship(1, 0, 0, 0, "player", [_weapon("forward", id="L1", max_charge=2, charge=0)])
        d = AllocDraft.from_ship(ship)
        d.movement = 2
        d.weapons = {"L1": 2}
        d.shields = [0, 0, 0, 0, 0, 0]
        self.assertEqual(0, d.free())
        text = d.summary()
        self.assertNotIn("unspent power", text)
        self.assertNotIn("free in pool", text)


class CallsignInitiativeTests(unittest.TestCase):
    """Movement/fire-ready queues display callsigns, not raw ship IDs."""

    def test_move_order_uses_callsigns(self):
        snap = _snap([
            _ship(1, 0, 0, 0, "player"),
            _ship(2, 3, 0, 3, "ai"),
        ], move_order=[1, 2], ships_moved_this_phase=[])
        out = ANSI.sub("", format_tactical(snap, selected=1))
        # callsign for player ship id=1 is "A1", ai id=2 is "B2"
        self.assertIn("A1", out)
        self.assertIn("B2", out)

    def test_fire_ready_uses_callsigns(self):
        snap = _snap([
            _ship(1, 0, 0, 0, "player"),
            _ship(2, 3, 0, 3, "ai"),
        ], ships_ready_fire=[1])
        out = ANSI.sub("", format_tactical(snap, selected=1))
        self.assertIn("fire ready:", out)
        self.assertIn("A1", out)

    def test_callsign_is_controller_letter_plus_index(self):
        s1 = _ship(1, 0, 0, 0, "player")
        s2 = _ship(2, 3, 0, 3, "ai")
        self.assertEqual("A1", ship_callsign(s1))
        self.assertEqual("B2", ship_callsign(s2))


if __name__ == "__main__":
    unittest.main()
