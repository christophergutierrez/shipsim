"""Allocate draft reflects protocol-3 carried weapon charge in bars and pool math."""

import unittest

from commands import AllocDraft


def _ship(*, charge_beam=0, charge_torp=0, power=22):
    return {
        "id": 1,
        "class": "Heavy Cruiser",
        "controller": "player",
        "power": power,
        "power_available": power,
        "max_shield_per_facing": 6,
        "weapons": [
            {
                "id": "beam_1",
                "kind": "Beam",
                "max_charge": 4,
                "charge": charge_beam,
                "operational": True,
            },
            {
                "id": "torp_1",
                "kind": "Torp",
                "max_charge": 1,
                "charge": charge_torp,
                "operational": True,
            },
            {
                "id": "plasma_1",
                "kind": "Plasma",
                "max_charge": 1,
                "charge": 0,
                "operational": True,
            },
        ],
    }


class CarriedChargeDraftTests(unittest.TestCase):
    def test_from_ship_seeds_weapon_bars_from_snapshot_charge(self):
        d = AllocDraft.from_ship(_ship(charge_beam=4, charge_torp=1))
        self.assertEqual(4, d.weapons["beam_1"])
        self.assertEqual(1, d.weapons["torp_1"])
        self.assertEqual(0, d.weapons["plasma_1"])
        text = d.summary()
        self.assertIn("4/4", text)
        self.assertIn("1/1", text)
        self.assertIn("0/1", text)
        # Hash count for unscaled bars (max 4 and 1 fit in width)
        self.assertIn("[####]", text)
        self.assertIn("[#]", text)
        self.assertIn("(carried", text)

    def test_used_ignores_carried_charge(self):
        d = AllocDraft.from_ship(_ship(charge_beam=4, charge_torp=1))
        self.assertEqual(0, d.used())
        self.assertEqual(22, d.free())
        d.set_movement(6)
        self.assertEqual(6, d.used())
        d.set_weapon("p1", 1)
        self.assertEqual(7, d.used())  # only +1 plasma
        self.assertEqual(4, d.weapons["beam_1"])

    def test_cannot_strip_carried_charge(self):
        d = AllocDraft.from_ship(_ship(charge_beam=4))
        self.assertFalse(d.set_weapon("b1", 2))
        self.assertEqual(4, d.weapons["beam_1"])

    def test_top_up_costs_only_increase(self):
        d = AllocDraft.from_ship(_ship(charge_beam=2, power=10))
        self.assertTrue(d.set_weapon("b1", 4))
        self.assertEqual(2, d.used())  # +2 only
        self.assertIn("+2 new", d.summary())

    def test_reset_restores_carried_not_zero(self):
        d = AllocDraft.from_ship(_ship(charge_beam=4, charge_torp=1))
        d.set_movement(6)
        d.set_weapon("p1", 1)
        d.reset()
        self.assertEqual(0, d.movement)
        self.assertEqual(4, d.weapons["beam_1"])
        self.assertEqual(1, d.weapons["torp_1"])
        self.assertEqual(0, d.weapons["plasma_1"])
        self.assertEqual(0, d.used())


if __name__ == "__main__":
    unittest.main()
