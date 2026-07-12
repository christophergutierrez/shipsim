import unittest

from commands import ReplContext, build_action, plan_absolute_move
from repl import send_orders


def snapshot(phase="movement", status="Playing"):
    return {
        "protocol_version": 1,
        "phase": phase,
        "status": status,
        "turn": 1,
        "active_ship": 1,
        "ships": [{"id": 1, "class": "Scout", "controller": "player", "destroyed": False,
                   "q": 0, "r": 0, "facing": 0, "structure": 4, "power": 4,
                   "weapons": [], "max_shield_per_facing": 2}],
        "combat_log": [],
    }


class FakeUI:
    scroll = True
    def __init__(self): self.lines = []
    def log(self, value): self.lines.append(str(value))
    def log_order(self, value): self.lines.append(str(value))


class FakeSession:
    def __init__(self, snap): self.snapshot, self.sent = snap, []
    def send_order(self, order):
        self.sent.append(order)
        return self.snapshot


class PreservedBehavior(unittest.TestCase):
    def test_absolute_move_remains_one_wire_order(self):
        orders, _ = plan_absolute_move(snapshot(), 1, 1)
        self.assertEqual(1, len(orders))
        self.assertEqual({"protocol_version": 1, "type": "move", "ship": 1,
                          "mode": "turn_starboard"}, orders[0])

    def test_raw_order_preserves_expert_payload(self):
        action = build_action('order {"type":"probe","x":7}', snapshot(), ReplContext())
        self.assertEqual([{"type": "probe", "x": 7, "protocol_version": 1}], action.orders)


class NamedBugReproductions(unittest.TestCase):
    def test_terminal_snapshot_blocks_wire_send(self):
        snap = snapshot(status="Won")
        session = FakeSession(snap)
        send_orders(FakeUI(), session, ReplContext(), [{"type": "end_turn"}], prev_log_len=0)
        self.assertEqual([], session.sent)


if __name__ == "__main__":
    unittest.main()
