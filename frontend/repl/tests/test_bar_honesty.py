"""Unit invariants for bar/format_bar (I1 without a PTY)."""

import unittest

from hexutil import bar, format_bar


class BarHonesty(unittest.TestCase):
    def test_unscaled_hashes_match_fill(self):
        self.assertEqual(bar(4, 10), "[####......]")
        self.assertEqual(format_bar(4, 10), "[####......] 4/10")

    def test_scaled_bar_includes_denominator(self):
        # pool 22 capped to width 16: 4/22 → ~3 hashes, but label is 4/22
        s = format_bar(4, 22)
        self.assertIn("4/22", s)
        self.assertTrue(s.startswith("["))
        body = s.split("]")[0] + "]"
        hashes = body.count("#")
        # scaled: round(4*16/22) == 3
        self.assertEqual(hashes, 3)
        # bare bar alone is scaled — callers must use format_bar for honesty
        bare = bar(4, 22)
        self.assertEqual(bare.count("#"), 3)
        self.assertNotIn("/", bare)

    def test_engine_four_not_ambiguous_with_format_bar(self):
        # The bug: "[###.............] 4" looks like 3 units. format_bar fixes it.
        label = f"engine {format_bar(4, 22)}"
        self.assertRegex(label, r"\[\S+\].*4/22")
        m = __import__("re").search(r"\[([#.]+)\]\s*(\d+)\s*/\s*(\d+)", label)
        self.assertIsNotNone(m)
        hashes, n, d = m.group(1).count("#"), int(m.group(2)), int(m.group(3))
        self.assertEqual((n, d), (4, 22))
        self.assertEqual(hashes, round(4 * len(m.group(1)) / 22))


if __name__ == "__main__":
    unittest.main()
