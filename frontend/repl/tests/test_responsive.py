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
            "velocity": 3,
            "course": 2,
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


if __name__ == "__main__":
    unittest.main()
