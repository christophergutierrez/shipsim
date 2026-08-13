import unittest

from responsive import (
    FrameBlock,
    choose_layout,
    render_compact_contacts,
    render_compact_map,
    render_compact_player,
)


class ResponsiveLayoutTests(unittest.TestCase):
    def test_full_layout_is_preserved_when_it_fits(self):
        full = [FrameBlock("banner", "header"), FrameBlock("player", "player")]
        decision = choose_layout(
            4,
            80,
            "allocate",
            full,
            [FrameBlock("banner", "compact")],
        )
        self.assertFalse(decision.compact)
        self.assertEqual(decision.text, "header\nplayer")

    def test_compact_layout_reserves_prompt_row_and_keeps_required_blocks(self):
        full = [FrameBlock("banner", "one\ntwo\nthree\nfour\nfive\nsix")]
        compact = [
            FrameBlock("terminal_banner", "banner", required=True),
            FrameBlock("banner", "phase", required=True),
            FrameBlock("player", "player", required=True),
            FrameBlock("draft", "draft", required=True),
            FrameBlock("history", "history\nrow"),
        ]
        decision = choose_layout(6, 80, "allocate", full, compact)
        self.assertTrue(decision.compact)
        self.assertEqual(decision.height, 4)
        self.assertIn("history", decision.hidden_roles)

    def test_compact_renderers_are_width_bounded(self):
        ship = {
            "id": 1,
            "controller": "player",
            "class": "Heavy Cruiser With A Long Name",
            "size": 4,
            "power_available": 22,
            "motion_available": 3,
            "max_maneuver_actions": 6,
            "facing": 1,
            "structure": 8,
            "shields_remaining": [1, 2, 3, 4, 5, 6],
            "max_shield_per_facing": 6,
            "weapons": [{"id": "beam_1", "charge": 4, "max_charge": 4}],
        }
        self.assertTrue(all(len(line) <= 32 for line in render_compact_player(ship, width=32).splitlines()))

        snap = {
            "map": {"mode": "unbounded"},
            "ships": [ship, {**ship, "id": 2, "controller": "scripted", "q": 100, "r": -50}],
        }
        contacts = render_compact_contacts(snap, selected=1, width=32)
        compact_map = render_compact_map(snap, selected=1, width=32)
        self.assertTrue(all(len(line) <= 32 for line in contacts.splitlines()))
        self.assertTrue(all(len(line) <= 32 for line in compact_map.splitlines()))

    def test_small_pane_squeezes_required_banner_instead_of_raising(self):
        # 20x84 is a common split-pane size. The compact header is two long
        # lines; it used to raise ValueError and kill the REPL on first paint.
        header = (
            "──────────────────────────────── shipsim ───────────────────────────────\n"
            "turn 1  phase=allocate  status=InProgress  focus=#1  "
            "actions=power:14  (@ = your focused ship)"
        )
        compact = [
            FrameBlock("banner", header, required=True),
            FrameBlock("player", "player line", required=True),
            FrameBlock("draft", "draft line"),
        ]
        decision = choose_layout(
            20,
            84,
            "allocate",
            [FrameBlock("snapshot", "\n".join(["full"] * 40))],
            compact,
        )
        self.assertTrue(decision.compact)
        self.assertTrue(any(block.role == "banner" for block in decision.blocks))
        self.assertLessEqual(decision.height, 19)
        self.assertTrue(
            all(len(line) <= 84 for line in decision.text.splitlines())
        )


if __name__ == "__main__":
    unittest.main()
