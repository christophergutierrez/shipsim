use std::collections::BTreeMap;

use crate::motion::Maneuver;
use crate::movement::Order;
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
        let max_velocity = u32::from(ship.max_velocity);
        let desired_thrust = match self.style {
            Style::Aggressive => max_velocity.min(2),
            Style::Defensive | Style::Greedy => u32::from(max_velocity > 0),
            Style::Mobility => max_velocity,
            Style::Random if max_velocity > 0 => {
                1 + (self.next_random() % u64::from(max_velocity)) as u32
            }
            Style::Random => 0,
        };
        let movement = desired_thrust
            .saturating_mul(ship.power_per_thrust)
            .div_ceil(ship.thrust_per_power)
            .min(power);
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

    fn desired_course(context: &DecisionContext<'_>) -> Option<u8> {
        let target = context
            .snapshot
            .ships
            .iter()
            .filter(|ship| {
                !ship.destroyed
                    && ship.id != context.ship.id
                    && ship.controller != context.ship.controller
            })
            .min_by_key(|ship| {
                let dq = ship.q - context.ship.q;
                let dr = ship.r - context.ship.r;
                (dq.abs().max(dr.abs()).max((dq + dr).abs()), ship.id)
            })?;
        (0..6).min_by_key(|course| {
            let (dq, dr) = match course {
                0 => (1, 0),
                1 => (1, -1),
                2 => (0, -1),
                3 => (-1, 0),
                4 => (-1, 1),
                _ => (0, 1),
            };
            let nq = context.ship.q + dq;
            let nr = context.ship.r + dr;
            let tq = target.q - nq;
            let tr = target.r - nr;
            (tq.abs().max(tr.abs()).max((tq + tr).abs()), *course)
        })
    }

    fn choose_maneuver(&mut self, context: &DecisionContext<'_>) -> Option<Order> {
        let maneuvers: Vec<Order> = context
            .legal_orders
            .iter()
            .filter(|order| matches!(order, Order::CommitManeuver { .. }))
            .cloned()
            .collect();
        if maneuvers.is_empty() {
            return None;
        }
        if matches!(self.style, Style::Random) {
            return maneuvers
                .get((self.next_random() as usize) % maneuvers.len())
                .cloned();
        }
        let desired = Self::desired_course(context);
        let current = context.ship.course;
        maneuvers.into_iter().max_by_key(|order| {
            let Order::CommitManeuver { maneuver, .. } = order else {
                return i32::MIN;
            };
            let mut score = match (self.style, maneuver) {
                (Style::Mobility, Maneuver::Accelerate { .. }) => 100,
                (Style::Aggressive, Maneuver::Accelerate { .. }) => 90,
                (Style::Greedy, Maneuver::Accelerate { .. }) => 80,
                (_, Maneuver::Accelerate { .. }) => 70,
                (Style::Defensive, Maneuver::RotatePort | Maneuver::RotateStarboard) => 75,
                (_, Maneuver::Coast) => 20,
                (_, Maneuver::Decelerate) => 10,
                (_, Maneuver::TurnCoursePort | Maneuver::TurnCourseStarboard) => 60,
                (_, Maneuver::RotatePort | Maneuver::RotateStarboard) => 30,
            };
            if let Some(wanted) = desired {
                match maneuver {
                    Maneuver::Accelerate {
                        course: Some(course),
                    } if *course == wanted => score += 50,
                    Maneuver::TurnCourseStarboard if (current + 1) % 6 == wanted => score += 45,
                    Maneuver::TurnCoursePort if (current + 5) % 6 == wanted => score += 45,
                    _ => {}
                }
            }
            score
        })
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
        self.choose_maneuver(context)
            .or_else(|| context.legal_orders.first().cloned())
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
