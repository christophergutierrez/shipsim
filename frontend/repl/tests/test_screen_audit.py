import unittest

from screen_audit import audit


class ScreenAuditTests(unittest.TestCase):
    def test_i3_detects_missing_banner_without_player_panel(self):
        frame = ["plain output"] * 24
        frame[10] = "┌─ MAP ─┐"
        violations = audit(frame, "synthetic", rows=24)
        self.assertTrue(any(v.startswith("I3") for v in violations))

    def test_i4_accepts_complete_allocate_frame(self):
        frame = [""] * 24
        frame[0] = "── shipsim ──"
        frame[2] = "┌─ YOUR SHIP ─┐"
        frame[5] = "┌─ ALLOCATE DRAFT ─┐"
        frame[23] = "t1/allocate@1 draft0/22>"
        self.assertEqual(audit(frame, "synthetic", rows=24), [])

    def test_i4_requires_player_and_draft(self):
        frame = [""] * 24
        frame[0] = "── shipsim ──"
        frame[23] = "t1/allocate@1 draft0/22>"
        violations = audit(frame, "synthetic", rows=24)
        self.assertTrue(any("player representation" in v for v in violations))
        self.assertTrue(any("draft representation" in v for v in violations))


if __name__ == "__main__":
    unittest.main()
