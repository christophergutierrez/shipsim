import builtins
import contextlib
import io
import unittest
from unittest.mock import patch

from commands import COMMAND_REGISTRY, ReplContext, build_action, interactive_fire, render_help
from view import format_combat_events, format_weapons
from tests.test_characterization import snapshot


def _fire_snapshot():
    return {
        "protocol_version": 4,
        "phase": "firing",
        "status": "InProgress",
        "turn": 1,
        "ships": [
            {"id": 1, "class": "Heavy Cruiser", "controller": "player", "destroyed": False,
             "q": 1, "r": 0, "facing": 3, "structure": 12, "power": 22,
             "motion_available": 0, "max_maneuver_actions": 4,
             "weapons": [], "max_shield_per_facing": 6},
            {"id": 2, "class": "Escort", "controller": "scripted", "destroyed": False,
             "q": 0, "r": 0, "facing": 0, "structure": 12, "power": 14,
             "motion_available": 0, "max_maneuver_actions": 4,
             "weapons": [], "max_shield_per_facing": 6},
        ],
        "ships_committed_volley": [],
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
        self.assertIn("volley", text.lower())

    def test_help_recognizes_documented_aliases_and_phase_commands(self):
        expected = {
            "p": "hold",
            "r": "ready | nofire | r",
            "commit": "commit | c | ok",
            "engine": "engine N",
            "w": "w [weapon] N",
            "sh": "sh [face] N",
            "path": "path",
        }
        for topic, syntax in expected.items():
            with self.subTest(topic=topic):
                text = render_help(topic)
                self.assertNotIn("unknown help topic", text)
                self.assertIn(syntax, text)
                self.assertIn("example:", text)

    def test_question_mark_and_attack_alias(self):
        snap = snapshot(phase="movement")
        self.assertEqual("help", build_action("?", snap, ReplContext()).side)
        snap["phase"] = "firing"
        self.assertEqual("fire_loop", build_action("ATTACK", snap, ReplContext(selected=1)).side)

    def test_one_line_fire_accepts_weapon_alias_and_contact(self):
        snap = snapshot(phase="firing")
        attacker = snap["ships"][0]
        attacker.update(q=0, r=0, facing=0, controller="player")
        attacker["weapons"] = [{
            "id": "beam_1", "kind": "Beam", "charge": 4,
            "max_charge": 4, "max_range": 10, "mount": "forward",
            "operational": True, "fired": False,
        }]
        snap["ships"].append({
            "id": 2, "class": "Escort", "controller": "ai", "q": 3,
            "r": 0, "facing": 3, "destroyed": False, "weapons": [],
        })
        ctx = ReplContext(selected=1)
        action = build_action("fire b1 #2", snap, ctx)
        self.assertFalse(action.orders)  # drafted locally, not committed yet
        self.assertEqual(1, len(ctx.volley_draft))
        self.assertEqual("beam_1", ctx.volley_draft[0]["weapon"])
        self.assertEqual(2, ctx.volley_draft[0]["target"])

    def _firing_snap_with_enemy(self):
        snap = snapshot(phase="firing")
        attacker = snap["ships"][0]
        attacker.update(q=0, r=0, facing=0, controller="player")
        attacker["weapons"] = [{
            "id": "beam_1", "kind": "Beam", "charge": 4,
            "max_charge": 4, "max_range": 10, "mount": "forward",
            "operational": True, "fired": False,
        }]
        snap["ships"].append({
            "id": 2, "class": "Escort", "controller": "ai", "q": 3,
            "r": 0, "facing": 3, "destroyed": False, "weapons": [],
        })
        return snap

    def test_one_line_fire_never_drops_to_interactive_menu(self):
        # Regression (Phase 1d): a syntactically complete one-liner
        # `fire <weapon> <target>` must fire directly and must NOT fall through
        # to the interactive weapon menu (which prints "Enter weapon number"
        # and consumes the next piped line as the answer, desyncing scripted
        # play). Asserted for both the bare form and the leading-ship-id form.
        for line in ("fire b1 2", "fire b1 #2", "fire 1 b1 2", "f b1 #2"):
            with self.subTest(line=line):
                snap = self._firing_snap_with_enemy()
                ctx = ReplContext(selected=1)
                out = io.StringIO()
                with contextlib.redirect_stdout(out):
                    action = build_action(line, snap, ctx)
                self.assertNotEqual(
                    "fire_loop", action.side,
                    f"{line!r} fell through to the interactive fire menu",
                )
                self.assertTrue(
                    ctx.volley_draft and ctx.volley_draft[0]["weapon"] == "beam_1",
                    f"{line!r} did not draft a beam_1 shot",
                )
                self.assertEqual(2, ctx.volley_draft[0]["target"])
                self.assertNotIn(
                    "Enter weapon number", out.getvalue(),
                    f"{line!r} printed the interactive weapon-menu prompt",
                )

    def test_weapon_picker_accepts_number_and_name(self):
        snap = snapshot(phase="firing")
        attacker = snap["ships"][0]
        attacker.update(q=0, r=0, facing=0, controller="player")
        attacker["weapons"] = [{
            "id": "beam_1", "kind": "Beam", "charge": 4,
            "max_charge": 4, "max_range": 10, "mount": "forward",
            "operational": True, "fired": False,
        }]
        snap["ships"].append({
            "id": 2, "class": "Escort", "controller": "ai", "q": 3,
            "r": 0, "facing": 3, "destroyed": False, "weapons": [],
        })
        with patch("builtins.input", return_value="b1"):
            action = interactive_fire(snap, 1)
        self.assertEqual("beam_1", action["weapon"])

    def test_weapon_picker_accepts_advertised_one_line_form(self):
        snap = snapshot(phase="firing")
        attacker = snap["ships"][0]
        attacker.update(q=0, r=0, facing=0, controller="player")
        attacker["weapons"] = [{
            "id": "beam_1", "kind": "Beam", "charge": 4,
            "max_charge": 4, "max_range": 10, "mount": "forward",
            "operational": True, "fired": False,
        }]
        snap["ships"].append({
            "id": 2, "class": "Escort", "controller": "ai", "q": 3,
            "r": 0, "facing": 3, "destroyed": False, "weapons": [],
        })
        ctx = ReplContext(selected=1)
        with patch("builtins.input", return_value="fire b1 #2"):
            action = interactive_fire(snap, 1, ctx)
        self.assertEqual("beam_1", action["weapon"])
        self.assertEqual(2, action["target"])

    def test_minus_one_at_firing_prompt_means_commit_volley(self):
        snap = snapshot(phase="firing")
        action = build_action("-1", snap, ReplContext(selected=1))
        self.assertEqual("commit_volley", action.orders[0]["type"])
        self.assertEqual([], action.orders[0]["shots"])

    def test_allocate_e_with_value_warns_about_engine_alias_hazard(self):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            action = build_action("e 10", snapshot(phase="allocate"), ReplContext())
        self.assertEqual("empty", action.side)
        self.assertIn("engine 10", out.getvalue())

    def test_coast_outside_movement_is_refused(self):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            action = build_action("coast", snapshot(phase="firing"), ReplContext())
        self.assertEqual("empty", action.side)
        self.assertIn("unavailable", out.getvalue().lower())

    def test_path_commit_emits_protocol4(self):
        snap = snapshot(phase="movement")
        ctx = ReplContext(selected=1)
        build_action("f", snap, ctx)
        action = build_action("commit", snap, ctx)
        self.assertEqual("commit_path", action.orders[0]["type"])
        self.assertEqual(["move_f"], action.orders[0]["actions"])
        self.assertEqual(4, action.orders[0]["protocol_version"])

    def test_direction_legend_uses_all_six_diagonals(self):
        text = render_help()
        self.assertIn("0→ 1↗ 2↖ 3← 4↙ 5↘", text)

    def test_combat_event_explains_shield_and_hull_split(self):
        snap = snapshot(phase="firing")
        snap["ships"].append({
            "id": 2, "class": "Escort", "controller": "ai", "q": 3,
            "r": 0, "facing": 3, "destroyed": False, "structure": 10,
            "weapons": [],
        })
        text = format_combat_events([{
            "attacker": 2, "target": 1, "weapon": "beam_1", "kind": "hit",
            "damage": 6, "shield": 0, "shield_absorbed": 4, "hull_damage": 2,
        }], snap)
        self.assertIn("shield absorbed 4, internal damage 2", text)

    def test_persistent_weapon_summary_labels_shield_as_a_facing(self):
        ship = {
            "id": 1,
            "weapons": [{
                "id": "beam_1", "kind": "Beam", "charge": 0,
                "max_charge": 4, "max_range": 10, "arc": "forward",
                "operational": True, "fired": True,
            }],
        }
        snap = {"combat_log": [{
            "attacker": 1, "target": 2, "weapon": "beam_1", "kind": "hit",
            "damage": 6, "shield": 0, "shield_absorbed": 4, "hull_damage": 2,
        }]}
        text = format_weapons(ship, snap=snap)
        self.assertIn("shield face=0:F", text)
        self.assertNotIn("shield=0 absorbed", text)

    def test_direct_fire_explains_already_drafted_weapon(self):
        snap = snapshot(phase="firing")
        attacker = snap["ships"][0]
        attacker.update(q=0, r=0, facing=0, controller="player")
        attacker["weapons"] = [{
            "id": "beam_1", "kind": "Beam", "charge": 4,
            "max_charge": 4, "max_range": 10, "mount": "forward",
            "operational": True, "fired": False,
        }]
        snap["ships"].append({
            "id": 2, "class": "Escort", "controller": "ai", "q": 3,
            "r": 0, "facing": 3, "destroyed": False, "weapons": [],
        })
        ctx = ReplContext(selected=1)
        ctx.volley_draft = [{"weapon": "beam_1", "target": 2, "shield_facing": 0}]
        ctx.volley_ship = 1
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            action = build_action("fire b1 #2", snap, ctx)
        self.assertFalse(action.orders)
        self.assertIn("already in the volley draft", out.getvalue())
        self.assertIn("ready", out.getvalue())

    def test_numeric_input_during_firing_does_not_change_focus(self):
        snap = snapshot(phase="firing")
        ctx = ReplContext(selected=1)
        with contextlib.redirect_stdout(io.StringIO()) as output:
            action = build_action("2", snap, ctx)
        self.assertEqual("empty", action.side)
        self.assertEqual(1, ctx.selected)
        self.assertIn("fire/attack/f", output.getvalue())

    def test_target_picker_lists_choices_instead_of_bare_dash_hint(self):
        # Two legal enemies in an all-arc weapon's range so the target
        # picker can't auto-skip to a sole target — this is the path that
        # used to render as the unexplained "  [-1] Done [0]: " prompt.
        snap = snapshot(phase="firing")
        attacker = snap["ships"][0]
        attacker.update(q=1, r=0, facing=3, controller="player")
        attacker["weapons"] = [{
            "id": "beam_1", "kind": "Beam", "charge": 4,
            "max_charge": 4, "max_range": 10, "mount": "all",
            "operational": True, "fired": False,
        }]
        snap["ships"].append({
            "id": 2, "class": "Escort", "controller": "ai", "destroyed": False,
            "q": 0, "r": 0, "facing": 0, "weapons": [],
        })
        snap["ships"].append({
            "id": 3, "class": "Escort", "controller": "ai", "destroyed": False,
            "q": 0, "r": 1, "facing": 0, "weapons": [],
        })
        answers = iter(["0", "1"])
        out = io.StringIO()
        with patch("builtins.input", side_effect=lambda *_a, **_k: next(answers)) as mock_input:
            with contextlib.redirect_stdout(out):
                order = interactive_fire(snap, 1)
        prompts = " ".join(str(c.args[0]) for c in mock_input.call_args_list if c.args)
        self.assertIn("Enter target number", prompts)
        self.assertIn("0, 1", prompts)
        self.assertEqual(3, order.get("target"))

    def test_blank_and_unknown_are_actionable(self):
        snap = snapshot()
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual("empty", build_action("", snap, ReplContext()).side)
            self.assertEqual("unknown", build_action("atack", snap, ReplContext()).side)
        text = out.getvalue()
        self.assertIn("help", text)
        self.assertIn("Did you mean 'attack'", text)


class EngineCommandOpensAllocateDraft(unittest.TestCase):
    """`engine N` is the renamed primary spelling of the old `mov N`
    allocate command. It must open the draft the same way `mov`/`w`/`sh`
    already do — both when there's exactly one ship left to allocate
    (auto-opens) and when there are several (prompts to pick one first,
    not "unknown command")."""

    def _two_ship_snap(self):
        snap = snapshot(phase="allocate")
        snap["ships"][0].update(id=1, controller="player")
        snap["ships"].append({
            "id": 2, "class": "Escort", "controller": "player", "destroyed": False,
            "q": 0, "r": 1, "facing": 0, "structure": 12, "power": 14,
            "motion_available": 0, "max_maneuver_actions": 4,
            "weapons": [], "max_shield_per_facing": 6,
        })
        return snap

    def test_engine_opens_draft_when_one_ship_pending(self):
        snap = snapshot(phase="allocate")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            build_action("engine 6", snap, ReplContext())
        text = out.getvalue()
        self.assertNotIn("unknown command", text)
        self.assertIn("draft", text.lower())

    def test_bare_focused_ship_id_opens_its_allocate_draft(self):
        snap = self._two_ship_snap()
        ctx = ReplContext(selected=1)

        build_action("1", snap, ctx)

        self.assertIsNotNone(ctx.draft)
        self.assertEqual(1, ctx.draft.ship_id)

    def test_bare_destroyed_ship_id_does_not_open_allocate_draft(self):
        snap = self._two_ship_snap()
        snap["ships"][0]["destroyed"] = True
        ctx = ReplContext(selected=2)

        build_action("1", snap, ctx)

        self.assertIsNone(ctx.draft)

    def test_engine_with_multiple_pending_prompts_for_ship_not_unknown(self):
        snap = self._two_ship_snap()
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            build_action("engine 6", snap, ReplContext())
        text = out.getvalue()
        self.assertNotIn("unknown command", text)
        self.assertIn("which ship", text.lower())


class DraftGroupNavigation(unittest.TestCase):
    """UI_SUCKS Issues 2 & 3: inside a draft group, the other group's name
    must switch groups (`sh` from weapons, `w` from shields, `engine` from
    either) instead of erroring, and the bare-number rejection in the
    weapons group must say how to get out."""

    def _draft_ctx(self):
        snap = snapshot(phase="allocate")
        ship = snap["ships"][0]
        ship["power"] = 10
        ship["max_shield_per_facing"] = 6
        ship["weapons"] = [{
            "id": "beam_1", "kind": "Beam", "charge": 0,
            "max_charge": 4, "max_range": 10, "mount": "forward",
            "operational": True, "fired": False,
        }]
        ctx = ReplContext()
        with contextlib.redirect_stdout(io.StringIO()):
            build_action("a 1", snap, ctx)
        return snap, ctx

    def test_sh_switches_from_weapons_group_to_shields(self):
        snap, ctx = self._draft_ctx()
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            build_action("w", snap, ctx)
            build_action("b1 2", snap, ctx)
            build_action("sh", snap, ctx)
            build_action("0 3", snap, ctx)
        self.assertEqual("sh", ctx.draft_group)
        self.assertEqual(2, ctx.draft.weapons["beam_1"])
        self.assertEqual(3, ctx.draft.shields[0])
        self.assertNotIn("need a weapon id", out.getvalue())

    def test_w_switches_from_shields_group_to_weapons(self):
        snap, ctx = self._draft_ctx()
        with contextlib.redirect_stdout(io.StringIO()):
            build_action("sh", snap, ctx)
            build_action("w", snap, ctx)
            build_action("b1 1", snap, ctx)
        self.assertEqual("w", ctx.draft_group)
        self.assertEqual(1, ctx.draft.weapons["beam_1"])

    def test_engine_works_from_inside_a_group(self):
        snap, ctx = self._draft_ctx()
        with contextlib.redirect_stdout(io.StringIO()):
            build_action("w", snap, ctx)
            build_action("engine 4", snap, ctx)
        self.assertEqual(4, ctx.draft.movement)

    def test_bare_number_in_weapons_group_says_how_to_leave(self):
        snap, ctx = self._draft_ctx()
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            build_action("w", snap, ctx)
            build_action("3", snap, ctx)
        text = out.getvalue()
        self.assertIn("need a weapon id", text)
        self.assertIn("done", text)
        self.assertIn("sh = shields", text)


class PlayLikeCommandsGetPhaseHint(unittest.TestCase):
    """UI_SUCKS Issue 1: `play`/`next`/`continue` aren't commands, but the
    reply must say what actually advances the current phase — not the
    generic unknown-command line."""

    def test_play_in_movement_points_to_path(self):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            action = build_action("play", snapshot(phase="movement"), ReplContext())
        self.assertEqual("empty", action.side)
        text = out.getvalue()
        self.assertIn("path", text)
        self.assertNotIn("unknown command", text)

    def test_next_in_firing_points_to_ready(self):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            build_action("next", snapshot(phase="firing"), ReplContext())
        self.assertIn("ready", out.getvalue())

    def test_continue_in_allocate_points_to_commit(self):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            build_action("continue", snapshot(phase="allocate"), ReplContext())
        self.assertIn("commit", out.getvalue())


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

    def test_end_turn_is_retired(self):
        snap = _fire_snapshot()
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            action = build_action("end", snap, ReplContext())
        self.assertEqual("empty", action.side)
        self.assertIn("removed", out.getvalue().lower())

    def test_empty_commit_accepts_inline_confirmation(self):
        snap = snapshot(phase="allocate")
        ctx = ReplContext()
        with contextlib.redirect_stdout(io.StringIO()):
            build_action("a 1", snap, ctx)
            action = build_action("commit yes", snap, ctx)
        self.assertEqual("allocate", action.orders[0]["type"])

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


class VolleySubmitTests(unittest.TestCase):
    """Protocol v4: ready submits commit_volley; end_turn is gone."""

    def test_ready_submits_drafted_shots(self):
        snap = _fire_snapshot()
        snap["ships"][0]["weapons"] = [{
            "id": "beam_1", "kind": "Beam", "charge": 4,
            "max_charge": 4, "max_range": 10, "mount": "forward",
            "operational": True, "fired": False,
        }]
        ctx = ReplContext(selected=1)
        ctx.volley_draft = [
            {"weapon": "beam_1", "target": 2, "shield_facing": 0}
        ]
        ctx.volley_ship = 1
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            action = build_action("ready", snap, ctx)
        self.assertEqual("commit_volley", action.orders[0]["type"])
        self.assertEqual(1, len(action.orders[0]["shots"]))
        # Draft cleared on successful send, not at parse time.
        self.assertEqual(1, len(ctx.volley_draft))

    def test_end_turn_is_removed(self):
        snap = _fire_snapshot()
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            action = build_action("end", snap, ReplContext())
        self.assertFalse(action.orders)
        self.assertIn("protocol v4", out.getvalue().lower())


if __name__ == "__main__":
    unittest.main()
