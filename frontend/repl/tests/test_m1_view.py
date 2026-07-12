import re
import unittest

from hexutil import bar
from view import format_board


ANSI = re.compile(r"\x1b\[[0-9;]*m")


def board_snapshot(ships):
    return {"map": {"width": 3, "height": 1}, "ships": ships}


class M1ViewTests(unittest.TestCase):
    def test_bar_clamps_filled_to_total_in_both_branches(self):
        self.assertEqual("[###]", bar(9, 3))
        self.assertEqual("[####]", bar(99, 20, width=4))

    def test_board_cells_have_four_printable_columns(self):
        text = ANSI.sub("", format_board(board_snapshot([
            {"id": 1, "controller": "player", "q": 1, "r": 0, "facing": 0}
        ])))
        row = next(line for line in text.splitlines() if line.startswith(" r00 "))
        self.assertEqual("····A1→ ····", row.removeprefix(" r00 "))

    def test_wreck_is_visible_on_an_otherwise_empty_hex(self):
        text = ANSI.sub("", format_board(board_snapshot([
            {"id": 2, "controller": "ai", "q": 1, "r": 0, "facing": 3, "destroyed": True}
        ])))
        self.assertIn(" x  ", text)

    def test_living_ship_wins_over_wreck_at_same_coordinate(self):
        text = ANSI.sub("", format_board(board_snapshot([
            {"id": 2, "controller": "ai", "q": 1, "r": 0, "facing": 3, "destroyed": True},
            {"id": 1, "controller": "player", "q": 1, "r": 0, "facing": 0},
            {"id": 3, "controller": "ai", "q": 1, "r": 0, "facing": 4, "destroyed": True},
        ])))
        self.assertIn("A1→ ", text)
        self.assertNotIn(" x  ", text)


if __name__ == "__main__":
    unittest.main()
