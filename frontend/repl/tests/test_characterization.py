import unittest

from commands import ReplContext, build_action, plan_absolute_move
from repl import send_orders


def snapshot(phase="movement", status="Playing"):
    return {
        "protocol_version": 4,
        "phase": phase,
        "status": status,
        "turn": 1,
        "ships": [
            {
                "id": 1,
                "class": "Scout",
                "controller": "player",
                "destroyed": False,
                "q": 0,
                "r": 0,
                "facing": 0,
                "structure": 4,
                "power": 4,
                "motion_available": 4,
                "max_maneuver_actions": 4,
                "weapons": [],
                "max_shield_per_facing": 2,
            }
        ],
        "ships_allocated_this_turn": [],
        "ships_committed_path": [],
        "ships_committed_volley": [],
        "combat_log": [],
    }


class FakeUI:
    scroll = True

    def __init__(self):
        self.lines = []

    def log(self, value):
        self.lines.append(str(value))

    def log_order(self, value):
        self.lines.append(str(value))


class FakeSession:
    def __init__(self, snap):
        self.snapshot, self.sent = snap, []

    def send_order(self, order):
        self.sent.append(order)
        return self.snapshot


class PreservedBehavior(unittest.TestCase):
    def test_absolute_course_turns_or_moves(self):
        orders, note = plan_absolute_move(snapshot(), 1, 1)
        self.assertEqual("commit_path", orders[0]["type"])
        self.assertTrue(orders[0]["actions"])
        self.assertTrue(note)

    def test_path_tokens_draft_locally(self):
        ctx = ReplContext(selected=1)
        action = build_action("path f fr tl", snapshot(), ctx)
        self.assertFalse(action.orders)
        self.assertEqual(["move_f", "move_fr", "turn_left"], ctx.path_draft)

    def test_hold_emits_empty_commit_path(self):
        for command in ("pass", "pass_move", "p", "hold"):
            with self.subTest(command=command):
                action = build_action(command, snapshot(), ReplContext(selected=1))
                self.assertEqual(
                    [
                        {
                            "protocol_version": 4,
                            "type": "commit_path",
                            "ship": 1,
                            "actions": [],
                        }
                    ],
                    action.orders,
                )

    def test_commit_path_after_draft(self):
        ctx = ReplContext(selected=1)
        build_action("f fr", snapshot(), ctx)
        action = build_action("commit", snapshot(), ctx)
        self.assertEqual("commit_path", action.orders[0]["type"])
        self.assertEqual(["move_f", "move_fr"], action.orders[0]["actions"])
        # Draft is cleared by send_orders on successful accept, not at parse time.
        self.assertEqual(["move_f", "move_fr"], ctx.path_draft)

    def test_raw_order_preserves_expert_payload(self):
        action = build_action('order {"type":"probe","x":7}', snapshot(), ReplContext())
        self.assertEqual(
            [{"type": "probe", "x": 7, "protocol_version": 4}], action.orders
        )

    def test_raw_retired_movement_is_rejected_before_transmission(self):
        action = build_action(
            'order {"type":"move","ship":1,"mode":"forward"}',
            snapshot(),
            ReplContext(),
        )
        self.assertFalse(action.orders)

    def test_raw_retired_v3_orders_rejected(self):
        for typ in ("commit_maneuver", "commit_fire", "ready_fire", "end_turn"):
            with self.subTest(typ=typ):
                action = build_action(
                    f'order {{"type":"{typ}","ship":1}}',
                    snapshot(),
                    ReplContext(),
                )
                self.assertFalse(action.orders)


class NamedBugReproductions(unittest.TestCase):
    def test_terminal_snapshot_blocks_wire_send(self):
        snap = snapshot(status="Won")
        session = FakeSession(snap)
        send_orders(
            FakeUI(),
            session,
            ReplContext(),
            [{"type": "commit_path", "ship": 1, "actions": []}],
            prev_log_len=0,
        )
        self.assertEqual([], session.sent)


if __name__ == "__main__":
    unittest.main()
