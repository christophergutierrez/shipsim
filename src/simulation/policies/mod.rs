use std::collections::BTreeMap;

use crate::movement::{MoveMode, Order};
use crate::snapshot::ShipSnapshot;

use super::policy::{DecisionContext, Policy};

#[derive(Debug, Clone, Copy)]
enum Style {
    Random,
    Greedy,
    Aggressive,
    Defensive,
    Mobility,
}

pub struct BaselinePolicy {
    name: &'static str,
    style: Style,
    random_state: u64,
}

pub fn build_policy(name: &str, seed: u64) -> Option<Box<dyn Policy>> {
    let (canonical, style) = match name {
        "random" => ("random", Style::Random),
        "greedy" => ("greedy", Style::Greedy),
        "aggressive" => ("aggressive", Style::Aggressive),
        "defensive" => ("defensive", Style::Defensive),
        "mobility" | "mobility_first" => ("mobility", Style::Mobility),
        _ => return None,
    };
    Some(Box::new(BaselinePolicy {
        name: canonical,
        style,
        random_state: seed.max(1),
    }))
}

impl BaselinePolicy {
    fn next_random(&mut self) -> u64 {
        let mut value = self.random_state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.random_state = value;
        value
    }

    fn allocation(&mut self, ship: &ShipSnapshot) -> (u32, BTreeMap<String, u32>, [u32; 6]) {
        let power = ship.power_available;
        let movement = match self.style {
            Style::Aggressive => ship.speed.min(power / 3),
            Style::Defensive => ship.speed.min(2).min(power),
            Style::Mobility => ship.speed.min(power),
            Style::Greedy => ship.speed.min((power / 3).max(1)),
            Style::Random => ship
                .speed
                .min((self.next_random() % u64::from(ship.speed.saturating_add(1))) as u32)
                .min(power),
        };
        let mut remaining = power - movement;
        let mut weapons = BTreeMap::new();
        let defensive_reserve = if matches!(self.style, Style::Defensive) {
            remaining / 2
        } else {
            0
        };
        let weapon_budget = remaining - defensive_reserve;
        let mut weapon_remaining = weapon_budget;
        for weapon in ship.weapons.iter().filter(|weapon| weapon.operational) {
            if weapon_remaining == 0 {
                break;
            }
            let desired = match self.style {
                Style::Mobility => 1,
                Style::Random => {
                    1 + (self.next_random() % u64::from(weapon.max_charge.max(1))) as u32
                }
                _ => weapon.max_charge,
            };
            let charge = desired.min(weapon.max_charge).min(weapon_remaining);
            if charge > 0 {
                weapons.insert(weapon.id.clone(), charge);
                weapon_remaining -= charge;
            }
        }
        remaining -= weapon_budget - weapon_remaining;

        let mut shields = [0u32; 6];
        let facing_order = match self.style {
            Style::Defensive => [0, 1, 5, 2, 4, 3],
            _ => [0, 1, 5, 3, 2, 4],
        };
        for facing in facing_order {
            if remaining == 0 {
                break;
            }
            let amount = remaining.min(ship.max_shield_per_facing);
            shields[facing] = amount;
            remaining -= amount;
        }
        (movement, weapons, shields)
    }

    fn choose_preferred(&mut self, context: &DecisionContext<'_>) -> Option<Order> {
        let commits: Vec<_> = context
            .legal_orders
            .iter()
            .filter(|order| matches!(order, Order::CommitFire { .. }))
            .cloned()
            .collect();
        if !commits.is_empty() {
            let index = match self.style {
                Style::Random => (self.next_random() as usize) % commits.len(),
                Style::Defensive => commits.len() - 1,
                _ => 0,
            };
            return commits.get(index).cloned();
        }

        if matches!(self.style, Style::Random) && !context.legal_orders.is_empty() {
            let index = (self.next_random() as usize) % context.legal_orders.len();
            return context.legal_orders.get(index).cloned();
        }
        let preferred_move = match self.style {
            Style::Defensive => Some(MoveMode::TurnPort),
            Style::Mobility | Style::Aggressive | Style::Greedy => Some(MoveMode::Forward),
            Style::Random => None,
        };
        if let Some(mode) = preferred_move {
            if let Some(order) = context.legal_orders.iter().find(
                |order| matches!(order, Order::Move { mode: candidate, .. } if *candidate == mode),
            ) {
                return Some(order.clone());
            }
        }
        context.legal_orders.first().cloned()
    }
}

impl Policy for BaselinePolicy {
    fn name(&self) -> &str {
        self.name
    }

    fn allocate(&mut self, ship: &ShipSnapshot) -> Order {
        let (movement, weapons, shields) = self.allocation(ship);
        Order::Allocate {
            ship: ship.id,
            movement,
            weapons,
            shields,
        }
    }

    fn choose_order(&mut self, context: &DecisionContext<'_>) -> Order {
        self.choose_preferred(context)
            .expect("runner provides at least one legal order")
    }
}
