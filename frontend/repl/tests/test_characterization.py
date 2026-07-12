import unittest

from commands import ReplContext, build_action, plan_absolute_move
from repl import send_orders


def snapshot(phase="movement", status="Playing"):
    return {
        "protocol_version": 2,
        "phase": phase,
        "status": status,
        "turn": 1,
        "active_ship": 1,
        "ships": [{"id": 1, "class": "Scout", "controller": "player", "destroyed": False,
                   "q": 0, "r": 0, "facing": 0, "structure": 4, "power": 4,
                   "velocity": 0, "course": 0, "thrust_remaining": 4,
                   "max_velocity": 4,
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
    def test_absolute_course_accelerates_from_rest(self):
        orders, note = plan_absolute_move(snapshot(), 1, 1)
        self.assertEqual("commit_maneuver", orders[0]["type"])
        self.assertEqual({"type": "accelerate", "course": 1}, orders[0]["maneuver"])
        self.assertIn("course 1", note)

    def test_inertial_maneuver_commands_emit_v2_orders(self):
        cases = {
            "accel 2": {"type": "accelerate", "course": 2},
            "decel": {"type": "decelerate"},
            "course port": {"type": "turn_course_port"},
            "course starboard": {"type": "turn_course_starboard"},
            "rotate port": {"type": "rotate_port"},
            "rotate starboard": {"type": "rotate_starboard"},
        }
        for command, maneuver in cases.items():
            with self.subTest(command=command):
                action = build_action(command, snapshot(), ReplContext(selected=1))
                self.assertEqual(maneuver, action.orders[0]["maneuver"])

    def test_interactive_pass_emits_v2_coast(self):
        for command in ("pass", "pass_move", "p"):
            with self.subTest(command=command):
                action = build_action(command, snapshot(), ReplContext(selected=1))
                self.assertEqual(
                    [{"protocol_version": 2, "type": "commit_maneuver", "ship": 1,
                      "maneuver": {"type": "coast"}}],
                    action.orders,
                )

    def test_raw_order_preserves_expert_payload(self):
        action = build_action('order {"type":"probe","x":7}', snapshot(), ReplContext())
        self.assertEqual([{"type": "probe", "x": 7, "protocol_version": 2}], action.orders)

    def test_raw_retired_movement_is_rejected_before_transmission(self):
        action = build_action('order {"type":"move","ship":1,"mode":"forward"}',
                              snapshot(), ReplContext())
        self.assertFalse(action.orders)


class NamedBugReproductions(unittest.TestCase):
    def test_terminal_snapshot_blocks_wire_send(self):
        snap = snapshot(status="Won")
        session = FakeSession(snap)
        send_orders(FakeUI(), session, ReplContext(), [{"type": "end_turn"}], prev_log_len=0)
        self.assertEqual([], session.sent)


if __name__ == "__main__":
    unittest.main()
