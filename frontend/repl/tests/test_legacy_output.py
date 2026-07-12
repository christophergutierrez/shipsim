import re
import unittest

from commands import HELP
from view import format_board, format_tactical


ANSI = re.compile(r"\x1b\[[0-9;]*m")


class LegacyOutputTests(unittest.TestCase):
    def test_tactical_queue_uses_callsigns_not_raw_id_lists(self):
        snap = {"turn": 1, "phase": "movement", "status": "Playing",
                "ships_committed_this_phase": [],
                "ships_ready_fire": [2], "ships": [
                    {"id": 1, "controller": "player", "destroyed": False, "class": "S",
                     "q": 0, "r": 0, "facing": 0, "structure": 1, "weapons": [],
                     "shields_remaining": [], "shields_powered": []},
                    {"id": 2, "controller": "ai", "destroyed": False, "class": "S",
                     "q": 1, "r": 0, "facing": 0, "structure": 1, "weapons": [],
                     "shields_remaining": [], "shields_powered": []}]}
        text = format_tactical(snap, selected=1)
        self.assertIn("movement: committed=- pending=A1, B2", text)
        self.assertNotIn("move_order=", text)
    def test_help_uses_canonical_screen_facing_legend(self):
        self.assertIn("0→ 1↗ 2↑ 3← 4↙ 5↓", HELP)
        self.assertNotIn("0↑ 1↗ 2↘ 3↓ 4↙ 5↖", HELP)

    def test_board_no_longer_emits_three_column_empty_cells(self):
        out = ANSI.sub("", format_board({"map": {"width": 2, "height": 1}, "ships": []}))
        row = next(line for line in out.splitlines() if line.startswith(" r00 "))
        self.assertEqual(8, len(row.removeprefix(" r00 ")))


if __name__ == "__main__":
    unittest.main()
