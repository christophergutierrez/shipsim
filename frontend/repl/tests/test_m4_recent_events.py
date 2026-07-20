"""M4 recent-events panel: enemy fire, weapon destruction, bridge-kill cause.

Phase 2 of the bugfix plan. Four playtest reports stem from one root cause:
important combat events are recorded correctly in the engine's event/Δ data,
but the REPL's screen-repaint model only shows the current turn's compact Δ
summary line (truncated) and does not persist the fuller FIRE RESOLUTION block
across repaints. These tests assert the fix: the enemy's fire, weapon
destruction, and bridge-kill cause must all be visible in the live UI across
the repaint that follows the event — not just at game-over or via error
messages.

Pattern mirrors test_m3_fixes_flow / test_characterization: synthetic snapshot
dicts, a FakeSession that replays a before→after sequence, and a real
TerminalUI (scroll=False) to capture the painted frame.
"""

import contextlib
import io
import re
import unittest

from commands import ReplContext
from repl import send_orders
from screen import TerminalUI
from view import format_combat_events, format_terminal_banner

ANSI = re.compile(r"\x1b\[[0-9;]*m")


def _strip(text: str) -> str:
    return ANSI.sub("", text)


def _weapon(wid="beam_1", kind="Beam", charge=4, operational=True, **kw):
    w = {
        "id": wid, "mount": "forward", "max_range": 10, "max_charge": 4,
        "charge": charge, "operational": operational, "fired": False,
        "kind": kind,
    }
    w.update(kw)
    return w


def _ship(sid, q, r, facing=0, controller="player", weapons=None,
          destroyed=False, structure=12, bridge=1, engine=1, power_sys=1,
          power=22, size=3):
    return {
        "id": sid, "class": "Heavy Cruiser", "controller": controller,
        "destroyed": destroyed, "q": q, "r": r, "facing": facing,
        "structure": structure, "keel": 12, "bridge": bridge, "engine": engine,
        "power_sys": power_sys, "power": power, "size": size,
        "motion_available": 4, "max_maneuver_actions": 4,
        "weapons": weapons or [], "max_shield_per_facing": 6,
        "shields_remaining": [6, 6, 6, 6, 6, 6],
    }


def _snap(ships, *, phase="firing", status="Playing", turn=1, combat_log=None,
          **kw):
    snap = {
        "protocol_version": 4, "phase": phase, "status": status, "turn": turn,
        "ships": ships, "combat_log": combat_log or [],
        "map": {"width": 4, "height": 4},
        "ships_committed_path": [], "ships_committed_volley": [],
    }
    snap.update(kw)
    return snap


class _ScriptedSession:
    """Replays a fixed list of snapshots in order, one per send_order call.

    The first snapshot is the 'before' state (session.snapshot); each
    send_order returns the next snapshot in the list and advances
    session.snapshot to it, mirroring how the real engine updates state.
    """

    def __init__(self, snaps):
        self._snaps = list(snaps)
        self.snapshot = self._snaps[0]
        self._i = 0
        self.sent = []
        # paint_frame reads session.orders_log.name for the footer.
        self.orders_log = type("P", (), {"name": "orders.jsonl"})()

    def send_order(self, order):
        self.sent.append(order)
        self._i += 1
        if self._i < len(self._snaps):
            self.snapshot = self._snaps[self._i]
        return self.snapshot


class _CapturingUI(TerminalUI):
    """TerminalUI that captures the last painted frame for assertions."""

    def __init__(self):
        super().__init__(scroll=False, session_path=None)
        self.last_frame = ""
        # __init__ binds self._real_print to the builtin print; rebind it to
        # our capture so redraw()'s frame is recorded instead of printed.
        self._real_print = self._capture

    def _capture(self, *args, **kwargs):
        sep = kwargs.get("sep", " ")
        end = kwargs.get("end", "\n")
        text = sep.join(str(a) for a in args)
        self.last_frame += text + end


class EnemyFirePersistsAcrossRepaint(unittest.TestCase):
    """The enemy's weapon name and HIT/MISS must survive the next paint_frame.

    Before the fix, format_combat_events produced the full FIRE RESOLUTION
    block (including the enemy's shots) but it was only passed to ui.log(),
    which the RECENT strip truncates and the next repaint overwrites. The
    recent-events panel must retain it.
    """

    def test_enemy_fire_visible_in_recent_events_panel(self):
        # Player ship A1 at (1,0); enemy scripted ship C2 at (0,0) fires a
        # 'pulse' weapon and hits A1.
        before = _snap([
            _ship(1, 1, 0, facing=3, controller="player",
                  weapons=[_weapon("beam_1", "Beam", charge=4)]),
            _ship(2, 0, 0, facing=0, controller="scripted",
                  weapons=[_weapon("pulse", "Pulse", charge=3)]),
        ])
        after = _snap([
            _ship(1, 1, 0, facing=3, controller="player", structure=5,
                  weapons=[_weapon("beam_1", "Beam", charge=4)]),
            _ship(2, 0, 0, facing=0, controller="scripted",
                  weapons=[_weapon("pulse", "Pulse", charge=0, fired=True)]),
        ], combat_log=[{
            "kind": "hit", "attacker": 2, "target": 1, "weapon": "pulse",
            "shield": 0, "damage": 7, "shield_absorbed": 0, "hull_damage": 7,
            "roll": 12,
        }])
        session = _ScriptedSession([before, after])
        ui = _CapturingUI()
        ctx = ReplContext(selected=1)
        ctx.hull_max = {1: 12, 2: 12}
        with contextlib.redirect_stdout(io.StringIO()):
            send_orders(ui, session, ctx, [{"type": "end_turn"}], prev_log_len=0)

        # The recent-events text must contain the enemy's weapon and the HIT.
        recent = _strip(ui.recent_events_text)
        self.assertIn("pulse", recent)
        self.assertIn("HIT", recent)
        # And it must be the FIRE RESOLUTION block (not a reinvented format).
        self.assertIn("FIRE RESOLUTION", recent)

        # A subsequent repaint must still show the enemy's fire — i.e. the
        # panel persists across paint_frame, unlike the old ui.log-only path.
        ui.redraw(after, selected=1, hull_max={1: 12, 2: 12})
        frame = _strip(ui.last_frame)
        self.assertIn("RECENT FIRE", frame)
        self.assertIn("pulse", frame)
        self.assertIn("HIT", frame)


class WeaponDestructionSurfaces(unittest.TestCase):
    """A weapon reduced to operational=False must be called out explicitly.

    Before the fix the player only saw a DESTROYED label next render or hit an
    error trying to use the weapon. The recent-events panel must say so.
    """

    def test_weapon_destroyed_in_recent_events(self):
        before = _snap([
            _ship(1, 1, 0, facing=3, controller="player",
                  weapons=[_weapon("beam_1", "Beam", charge=4, operational=True)]),
            _ship(2, 0, 0, facing=0, controller="scripted",
                  weapons=[_weapon("pulse", "Pulse", charge=3)]),
        ])
        after = _snap([
            _ship(1, 1, 0, facing=3, controller="player",
                  weapons=[_weapon("beam_1", "Beam", charge=4, operational=False)]),
            _ship(2, 0, 0, facing=0, controller="scripted",
                  weapons=[_weapon("pulse", "Pulse", charge=0, fired=True)]),
        ], combat_log=[{
            "kind": "hit", "attacker": 2, "target": 1, "weapon": "pulse",
            "shield": 0, "damage": 7, "shield_absorbed": 0, "hull_damage": 7,
            "roll": 12,
        }])
        session = _ScriptedSession([before, after])
        ui = _CapturingUI()
        ctx = ReplContext(selected=1)
        ctx.hull_max = {1: 12, 2: 12}
        with contextlib.redirect_stdout(io.StringIO()):
            send_orders(ui, session, ctx, [{"type": "end_turn"}], prev_log_len=0)

        recent = _strip(ui.recent_events_text)
        self.assertIn("beam_1", recent)
        self.assertIn("DESTROYED", recent)

        # And it persists across a repaint.
        ui.redraw(after, selected=1, hull_max={1: 12, 2: 12})
        frame = _strip(ui.last_frame)
        self.assertIn("DESTROYED", frame)


class PowerPoolHalvingSurfaces(unittest.TestCase):
    """power_sys damage must call out the usable-power consequence explicitly.

    Before the fix this was only visible in the truncated Δ line.
    """

    def test_power_pool_change_in_recent_events(self):
        before = _snap([
            _ship(1, 1, 0, facing=3, controller="player", power=22, power_sys=2,
                  weapons=[_weapon("beam_1", "Beam", charge=4)]),
            _ship(2, 0, 0, facing=0, controller="scripted",
                  weapons=[_weapon("pulse", "Pulse", charge=3)]),
        ])
        after = _snap([
            _ship(1, 1, 0, facing=3, controller="player", power=11, power_sys=1,
                  weapons=[_weapon("beam_1", "Beam", charge=4)]),
            _ship(2, 0, 0, facing=0, controller="scripted",
                  weapons=[_weapon("pulse", "Pulse", charge=0, fired=True)]),
        ], combat_log=[{
            "kind": "hit", "attacker": 2, "target": 1, "weapon": "pulse",
            "shield": 0, "damage": 7, "shield_absorbed": 0, "hull_damage": 7,
            "roll": 12,
        }])
        session = _ScriptedSession([before, after])
        ui = _CapturingUI()
        ctx = ReplContext(selected=1)
        ctx.hull_max = {1: 12, 2: 12}
        with contextlib.redirect_stdout(io.StringIO()):
            send_orders(ui, session, ctx, [{"type": "end_turn"}], prev_log_len=0)

        recent = _strip(ui.recent_events_text)
        self.assertIn("power_sys", recent)
        self.assertIn("usable power", recent)
        self.assertIn("11", recent)


class BridgeKillCauseExplicit(unittest.TestCase):
    """A bridge=0 loss must say 'bridge destroyed', not just 'hull took N'.

    Before the fix the death screen said 'hull took N' without saying the
    bridge hit was the kill condition.
    """

    def test_bridge_kill_wording(self):
        # Player ship destroyed with bridge=0 (but structure > 0): bridge kill.
        snap = _snap([
            _ship(1, 1, 0, facing=3, controller="player", destroyed=True,
                  bridge=0, structure=5,
                  weapons=[_weapon("beam_1", "Beam", charge=4)]),
            _ship(2, 0, 0, facing=0, controller="scripted",
                  weapons=[_weapon("pulse", "Pulse", charge=0)]),
        ], status="Lost")
        banner = _strip(format_terminal_banner("Lost", snap))
        self.assertIn("bridge destroyed", banner)
        self.assertNotIn("hull breached", banner)

    def test_hull_kill_wording_distinct(self):
        # Player ship destroyed with structure=0 (bridge > 0): hull kill.
        snap = _snap([
            _ship(1, 1, 0, facing=3, controller="player", destroyed=True,
                  bridge=1, structure=0,
                  weapons=[_weapon("beam_1", "Beam", charge=4)]),
            _ship(2, 0, 0, facing=0, controller="scripted",
                  weapons=[_weapon("pulse", "Pulse", charge=0)]),
        ], status="Lost")
        banner = _strip(format_terminal_banner("Lost", snap))
        self.assertIn("hull breached", banner)
        self.assertNotIn("bridge destroyed", banner)

    def test_won_banner_unchanged(self):
        snap = _snap([
            _ship(1, 1, 0, facing=3, controller="player",
                  weapons=[_weapon("beam_1", "Beam", charge=4)]),
            _ship(2, 0, 0, facing=0, controller="scripted", destroyed=True,
                  weapons=[_weapon("pulse", "Pulse", charge=0)]),
        ], status="Won")
        banner = _strip(format_terminal_banner("Won", snap))
        self.assertIn("SCENARIO WON", banner)
        # No cause line on a win.
        self.assertNotIn("bridge destroyed", banner)
        self.assertNotIn("hull breached", banner)


if __name__ == "__main__":
    unittest.main()
