"""M3 UI/UX fixes: failing (red) tests for known bugs C7-C14.

Each test documents the *intended* behavior for a bug that is currently
present in commands.py / view.py / style.py / screen.py. All tests in this
file are expected to FAIL until the corresponding fix lands. Synthetic
snapshot/ship dicts only — no engine process (see test_m2_tactical.py for
fixture style this file follows).
"""

import contextlib
import io
import re
import unittest

from commands import AllocDraft, ReplContext, build_action, default_allocate
from screen import TerminalUI
from style import _visible_len, panel
from view import format_board, format_combat_events, format_header, format_terminal_banner

ANSI = re.compile(r"\x1b\[[0-9;]*m")


def _weapon(wid="beam_1", kind="Beam", max_charge=4, charge=0, operational=True,
            mount="forward", max_range=5):
    return {
        "id": wid, "kind": kind, "mount": mount, "max_range": max_range,
        "max_charge": max_charge, "charge": charge, "operational": operational,
        "fired": False,
    }


def _ship(sid, controller="player", weapons=None, destroyed=False, power=22,
          max_shield_per_facing=6, q=0, r=0, facing=0, structure=10):
    return {
        "id": sid, "class": "Cruiser", "controller": controller,
        "destroyed": destroyed, "q": q, "r": r, "facing": facing,
        "structure": structure, "power": power, "weapons": weapons or [],
        "max_shield_per_facing": max_shield_per_facing,
        "shields_remaining": [max_shield_per_facing] * 6,
        "shields_powered": [0] * 6,
        "bridge": 1, "engine": 1, "power_sys": 1, "keel": 4,
    }


def _snap(ships, **kw):
    snap = {
        "protocol_version": 3, "phase": "allocate", "status": "Playing",
        "turn": 1, "active_ship": 1, "ships": ships, "combat_log": [],
    }
    snap.update(kw)
    return snap


class C7DefaultAllocateSurvivable(unittest.TestCase):
    """`ad` quick-default must leave the ship able to survive a hit, not
    dump the whole pool into movement with a token 1-point beam charge."""

    def test_c7_ad_default_is_survivable(self):
        ship = _ship(1, weapons=[_weapon("beam_1", "Beam", max_charge=4)], power=22,
                     max_shield_per_facing=6)
        snap = _snap([ship])
        with contextlib.redirect_stdout(io.StringIO()):
            order = default_allocate(snap, 1)
        self.assertIsNotNone(order)
        self.assertEqual(4, order["weapons"].get("beam_1"),
                          "beam should be charged to its max_charge, not left at 1")
        self.assertGreaterEqual(order["shields"][0], 2,
                                 "forward shield should get some power for survivability")
        self.assertGreaterEqual(order["movement"], 2)
        total = order["movement"] + sum(order["weapons"].values()) + sum(order["shields"])
        self.assertEqual(22, total)


class C8LeftoverWarningPhaseGated(unittest.TestCase):
    """The '⚠ leftover useful actions' header warning must only show during
    firing/turn_end, not during allocate (alarm fatigue on nearly every frame)."""

    def test_c8_leftover_warning_hidden_during_allocate(self):
        snap = {"status": "Playing", "phase": "allocate", "turn": 1,
                "active_ship": 1, "end_turn_warning": True}
        out = ANSI.sub("", format_header(snap))
        self.assertNotIn("⚠ leftover", out)

    def test_c8_leftover_warning_shown_during_firing(self):
        snap = {"status": "Playing", "phase": "firing", "turn": 1,
                "active_ship": 1, "end_turn_warning": True}
        out = ANSI.sub("", format_header(snap))
        self.assertIn("⚠ end skips unresolved actions", out)


class C9PanelGeometry(unittest.TestCase):
    """Panel width discipline: wrapping, no nested borders, consistent widths."""

    def test_c9a_panel_wraps_long_lines_within_width(self):
        long_line = ("word " * 20) + "DISTINCTIVETAIL"
        out = panel("TITLE", long_line, width=40)
        for line in out.splitlines():
            self.assertLessEqual(
                _visible_len(line), 40,
                f"line exceeds panel width: {line!r}",
            )
        self.assertIn("DISTINCTIVETAIL", out)

    def test_c9b_combat_events_have_no_nested_borders(self):
        snap = _snap([
            _ship(1, "player", q=0, r=0, facing=0),
            _ship(2, "ai", q=1, r=0, facing=3),
        ])
        events = [{"attacker": 1, "target": 2, "weapon": "L1", "kind": "hit",
                   "damage": 2, "shield": 0}]
        out = ANSI.sub("", format_combat_events(events, snap))
        self.assertNotIn("┌", out)
        self.assertNotIn("└", out)

    def test_c9b_terminal_banner_has_no_nested_borders(self):
        out = ANSI.sub("", format_terminal_banner("Won"))
        self.assertNotIn("┌", out)
        self.assertNotIn("└", out)

    def test_c9c_all_panel_top_borders_share_same_width(self):
        snap = _snap(
            [
                _ship(1, "player", q=0, r=0, facing=0),
                _ship(2, "ai", q=1, r=0, facing=3),
            ],
            phase="firing",
            map={"width": 3, "height": 1},
        )
        ui = TerminalUI(session_path=None, scroll=False)
        ui.log("some recent event")
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            ui.redraw(snap, selected=1, hull_max={})
        out = ANSI.sub("", buf.getvalue())
        borders = [line for line in out.splitlines() if line.startswith("┌")]
        widths = {_visible_len(b) for b in borders}
        self.assertGreaterEqual(len(borders), 2, "expected multiple panels in the frame")
        self.assertEqual(1, len(widths), f"panel top borders differ in width: {widths}")


class C10RecentCoalescesDuplicates(unittest.TestCase):
    """TerminalUI.log must coalesce consecutive duplicate lines with a repeat
    marker instead of appending N identical history entries."""

    def test_c10_consecutive_duplicates_collapse_with_repeat_marker(self):
        ui = TerminalUI(session_path=None, scroll=False)
        ui.log("order_illegal")
        ui.log("order_illegal")
        ui.log("order_illegal")
        self.assertEqual(1, len(ui.history),
                          f"expected duplicates to coalesce, got {list(ui.history)!r}")
        last = ui.history[-1]
        self.assertTrue("×3" in last or "x3" in last.lower(),
                         f"expected a x3 repeat marker in {last!r}")

    def test_c10_distinct_lines_remain_separate_and_ordered(self):
        ui = TerminalUI(session_path=None, scroll=False)
        ui.log("a")
        ui.log("b")
        ui.log("a")
        self.assertEqual(["a", "b", "a"], list(ui.history))


class C11DraftClampNotes(unittest.TestCase):
    """AllocDraft must print a note whenever it silently clamps/rejects a
    value instead of doing so silently."""

    def _draft(self, operational=True):
        ship = _ship(1, weapons=[_weapon("beam_1", "Beam", max_charge=4,
                                          operational=operational)])
        return AllocDraft.from_ship(ship)

    def test_c11a_set_weapon_over_max_prints_clamp_note(self):
        d = self._draft()
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            d.set_weapon("b1", 99)
        out = buf.getvalue().lower()
        self.assertEqual(4, d.weapons["beam_1"])
        self.assertTrue("clamp" in out or "max" in out,
                         f"expected a clamp/max note, got {out!r}")

    def test_c11b_set_shield_over_max_prints_clamp_note(self):
        d = self._draft()
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            d.set_shield("0", 99)
        out = buf.getvalue().lower()
        self.assertEqual(6, d.shields[0])
        self.assertTrue("clamp" in out or "max" in out,
                         f"expected a clamp/max note, got {out!r}")

    def test_c11c_negative_movement_prints_guidance(self):
        ship = _ship(1, weapons=[_weapon("beam_1", "Beam", max_charge=4)])
        snap = _snap([ship])
        ctx = ReplContext(selected=1, draft=AllocDraft.from_ship(ship))
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            build_action("mov -3", snap, ctx)
        self.assertEqual(0, ctx.draft.movement)
        self.assertIn("negative", buf.getvalue().lower(),
                       "expected explicit guidance about the negative input")

    def test_c11d_destroyed_weapon_reports_destroyed_not_unknown(self):
        d = self._draft(operational=False)
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            result = d.set_weapon("b1", 2)
        out = buf.getvalue().lower()
        self.assertFalse(result)
        self.assertTrue("destroyed" in out or "not operational" in out,
                         f"expected a destroyed/not-operational note, got {out!r}")


class C12MapLegendExplainsDestroyedGlyph(unittest.TestCase):
    """The map legend must explain that 'x' means a destroyed ship."""

    ROW_RE = re.compile(r"^\s*r\d\d\s")

    def test_c12_legend_mentions_destroyed_glyph(self):
        snap = {"map": {"width": 3, "height": 1}, "ships": [
            {"id": 2, "controller": "ai", "q": 1, "r": 0, "facing": 3, "destroyed": True},
        ]}
        out = ANSI.sub("", format_board(snap))
        legend_text = "\n".join(
            line for line in out.splitlines() if not self.ROW_RE.match(line)
        ).lower()
        self.assertIn("x", legend_text)
        self.assertTrue("destroyed" in legend_text or "dead" in legend_text,
                         f"legend does not explain the x glyph: {legend_text!r}")


class C13DraftHelpOffersGlobalHelp(unittest.TestCase):
    """Draft-mode help must tell the player how to reach global help."""

    def test_c13_draft_help_references_global_help(self):
        ship = _ship(1, weapons=[_weapon("beam_1", "Beam", max_charge=4)])
        snap = _snap([ship])
        ctx = ReplContext(selected=1, draft=AllocDraft.from_ship(ship))
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            build_action("help", snap, ctx)
        out = buf.getvalue().lower()
        self.assertTrue(
            "global" in out or ("quit" in out and "status" in out),
            f"draft help does not point toward global help: {out!r}",
        )


class C14FocusingAiShipPrintsObserverCue(unittest.TestCase):
    """Focusing a non-player ship should warn the player they cannot order it."""

    def test_c14_focus_on_ai_ship_prints_observer_cue(self):
        snap = _snap(
            [_ship(1, "player"), _ship(2, "ai", q=3, r=0, facing=3)],
            phase="movement",
        )
        ctx = ReplContext(selected=1)
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            build_action("ship 2", snap, ctx)
        out = buf.getvalue().lower()
        self.assertTrue("observer" in out or "cannot order" in out,
                         f"expected an observer/cannot-order note, got {out!r}")


if __name__ == "__main__":
    unittest.main()
