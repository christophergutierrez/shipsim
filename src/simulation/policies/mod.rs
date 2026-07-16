use std::collections::BTreeMap;

use crate::motion::{self, Maneuver};
use crate::movement::Order;
use crate::snapshot::{ShipSnapshot, StateSnapshot};

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

    /// Axial hex distance between two ships (same metric as core hex distance).
    fn range_to(a: &ShipSnapshot, b: &ShipSnapshot) -> i32 {
        let dq = b.q - a.q;
        let dr = b.r - a.r;
        dq.abs().max(dr.abs()).max((dq + dr).abs())
    }

    fn nearest_enemy<'a>(context: &'a DecisionContext<'_>) -> Option<&'a ShipSnapshot> {
        context
            .snapshot
            .ships
            .iter()
            .filter(|ship| {
                !ship.destroyed
                    && ship.id != context.ship.id
                    && ship.controller != context.ship.controller
            })
            .min_by_key(|ship| (Self::range_to(context.ship, ship), ship.id))
    }

    fn holding_velocity(&self, ship: &ShipSnapshot) -> u8 {
        let holding = match self.style {
            Style::Mobility => 2,
            Style::Random => 0,
            Style::Aggressive | Style::Greedy | Style::Defensive => 1,
        };
        holding.min(ship.max_velocity)
    }

    fn target_velocity(&self, ship: &ShipSnapshot, snapshot: Option<&StateSnapshot>) -> u8 {
        let holding = self.holding_velocity(ship);
        let Some(snapshot) = snapshot else {
            return holding;
        };
        let Some(enemy) = snapshot.ships.iter().filter(|candidate| {
            !candidate.destroyed
                && candidate.id != ship.id
                && candidate.controller != ship.controller
        }).min_by_key(|candidate| (Self::range_to(ship, candidate), candidate.id)) else {
            return holding;
        };
        let range = Self::range_to(ship, enemy).max(0) as u32;
        let weapon_range = ship
            .weapons
            .iter()
            .filter(|weapon| weapon.operational)
            .map(|weapon| weapon.max_range)
            .max()
            .unwrap_or(0);
        let closing_distance = range.saturating_sub(weapon_range);
        let closing_velocity = closing_distance
            .div_ceil(3)
            .min(u32::from(ship.max_velocity)) as u8;
        if Self::desired_course_for(ship, snapshot).is_some_and(|course| course != ship.course) {
            return holding;
        }
        // Keep deterministic policies below the high-speed regime where a
        // re-vector costs more thrust than a normal turn can buy. Two hexes
        // per turn still closes a standard engagement in one movement turn.
        closing_velocity
            .max(holding)
            .min(2)
            .min(ship.max_velocity)
    }

    fn power_for_thrust(ship: &ShipSnapshot, thrust: u32) -> u32 {
        if thrust == 0 || ship.thrust_per_power == 0 {
            return 0;
        }
        thrust
            .saturating_mul(ship.power_per_thrust)
            .saturating_add(ship.thrust_per_power - 1)
            / ship.thrust_per_power
    }

    fn allocation(
        &mut self,
        ship: &ShipSnapshot,
        snapshot: Option<&StateSnapshot>,
    ) -> (u32, BTreeMap<String, u32>, [u32; 6]) {
        let power = ship.power_available;
        let max_velocity = u32::from(ship.max_velocity);
        // Random retains its historical allocation stream. The governor is
        // intentionally limited to deterministic non-random policies.
        let desired_thrust = match self.style {
            Style::Random if max_velocity > 0 => {
                1 + (self.next_random() % u64::from(max_velocity)) as u32
            }
            Style::Random => 0,
            _ => {
                let target = u32::from(self.target_velocity(ship, snapshot));
                let mut thrust = target.abs_diff(u32::from(ship.velocity));
                if let Some(snapshot) = snapshot {
                    if let Some(wanted) = Self::desired_course_for(ship, snapshot) {
                        let maneuver_thrust = if ship.facing != wanted {
                            motion::facing_turn_cost(ship.facing, wanted)
                        } else if ship.course != wanted && ship.velocity > 0 {
                            u32::from(ship.velocity) + 1
                        } else {
                            0
                        };
                        thrust = thrust.max(maneuver_thrust);
                    }
                }
                thrust.min(4)
            }
        };
        let movement = Self::power_for_thrust(ship, desired_thrust).min(power);
        let mut remaining = power - movement;
        let mut weapons = BTreeMap::new();
        let defensive_reserve = if matches!(self.style, Style::Defensive) {
            remaining / 2
        } else {
            0
        };
        let weapon_budget = remaining - defensive_reserve;
        // Protocol 3: charge carries. Desired totals must be >= current charge;
        // only *increases* spend from the power pool.
        let mut weapon_remaining = weapon_budget;
        for weapon in ship.weapons.iter().filter(|weapon| weapon.operational) {
            let have = weapon.charge;
            let desired = match self.style {
                Style::Mobility => 1u32.max(have),
                Style::Random => {
                    let roll =
                        1 + (self.next_random() % u64::from(weapon.max_charge.max(1))) as u32;
                    roll.max(have)
                }
                _ => weapon.max_charge.max(have),
            };
            let target = desired.min(weapon.max_charge).max(have);
            let increase = target.saturating_sub(have);
            if increase > weapon_remaining {
                // Keep carried charge; cannot afford top-up.
                weapons.insert(weapon.id.clone(), have);
                continue;
            }
            weapons.insert(weapon.id.clone(), target);
            weapon_remaining -= increase;
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

    fn desired_course_for(ship: &ShipSnapshot, snapshot: &StateSnapshot) -> Option<u8> {
        let target = snapshot
            .ships
            .iter()
            .filter(|candidate| {
                !candidate.destroyed
                    && candidate.id != ship.id
                    && candidate.controller != ship.controller
            })
            .min_by_key(|candidate| {
                let dq = candidate.q - ship.q;
                let dr = candidate.r - ship.r;
                (dq.abs().max(dr.abs()).max((dq + dr).abs()), candidate.id)
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
            let nq = ship.q + dq;
            let nr = ship.r + dr;
            let tq = target.q - nq;
            let tr = target.r - nr;
            (tq.abs().max(tr.abs()).max((tq + tr).abs()), *course)
        })
    }

    fn desired_course(context: &DecisionContext<'_>) -> Option<u8> {
        Self::desired_course_for(context.ship, context.snapshot)
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
        let target = Self::nearest_enemy(context);
        let range = target.map(|enemy| Self::range_to(context.ship, enemy));
        let target_velocity = self.target_velocity(context.ship, Some(context.snapshot));
        maneuvers.into_iter().max_by_key(|order| {
            let Order::CommitManeuver { maneuver, .. } = order else {
                return i32::MIN;
            };
            let Ok(result) = motion::resolve_maneuver(
                motion::Velocity {
                    speed: context.ship.velocity,
                    course: context.ship.course,
                },
                context.ship.facing,
                context.ship.max_velocity,
                *maneuver,
            ) else {
                return i32::MIN;
            };
            let speed_error = i32::from(result.velocity.speed.abs_diff(target_velocity));
            let mut score = -1_000 * speed_error;
            if context.ship.velocity < target_velocity
                && result.velocity.speed > context.ship.velocity
            {
                score += 250;
            }
            if context.ship.velocity > target_velocity
                && result.velocity.speed < context.ship.velocity
            {
                score += 250;
            }
            if range.is_some_and(|current| {
                target.is_some_and(|enemy| {
                    let delta = crate::hex::Hex::direction(result.velocity.course)
                        .unwrap_or(crate::hex::Hex::ORIGIN);
                    let projected = crate::hex::Hex::new(
                        context.ship.q + delta.q * i32::from(result.velocity.speed),
                        context.ship.r + delta.r * i32::from(result.velocity.speed),
                    );
                    let post = (enemy.q - projected.q)
                        .abs()
                        .max((enemy.r - projected.r).abs())
                        .max((enemy.q - projected.q + enemy.r - projected.r).abs());
                    current > post
                })
            }) {
                score += 10;
            }
            // In weapon range, overspeed is a diagnostic liability rather than
            // a reason to keep accelerating away from the target.
            if range.is_some_and(|current| {
                target.is_some_and(|enemy| {
                    let weapon_range = context
                        .ship
                        .weapons
                        .iter()
                        .filter(|weapon| weapon.operational)
                        .map(|weapon| weapon.max_range)
                        .max()
                        .unwrap_or(0) as i32;
                    current <= weapon_range && result.velocity.speed > target_velocity
                        && enemy.id != context.ship.id
                })
            }) {
                score -= 300;
            }
            if let Some(wanted) = desired {
                if result.velocity.course == wanted
                    && result.velocity.speed <= target_velocity
                {
                    // Re-vectoring is intentionally allowed to cost one
                    // speed unit when it produces a course toward the target.
                    // Otherwise the speed-error term keeps selecting Coast
                    // forever at the holding velocity.
                    score += 1_500;
                }
                match maneuver {
                    Maneuver::Turn { facing } if *facing == wanted => score += 50,
                    Maneuver::TurnAccel { facing } if *facing == wanted => score += 55,
                    Maneuver::Accel if context.ship.facing == wanted => score += 40,
                    Maneuver::Accel if context.ship.course == wanted => score += 30,
                    _ => {}
                }
            }
            score += match self.style {
                Style::Mobility if matches!(maneuver, Maneuver::Accel | Maneuver::TurnAccel { .. }) => 20,
                Style::Defensive if matches!(maneuver, Maneuver::Coast | Maneuver::Turn { .. }) => 10,
                _ => 0,
            };
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
            if matches!(self.style, Style::Random) {
                let index = (self.next_random() as usize) % commits.len();
                return commits.get(index).cloned();
            }
            return commits.into_iter().min_by_key(|order| {
                let Order::CommitFire { target, weapon, .. } = order else {
                    return (u32::MAX, i32::MAX, String::new());
                };
                let (structure, range) = context
                    .snapshot
                    .ships
                    .iter()
                    .find(|candidate| candidate.id == *target)
                    .map(|candidate| {
                        (
                            candidate.structure,
                            Self::range_to(context.ship, candidate),
                        )
                    })
                    .unwrap_or((u32::MAX, i32::MAX));
                (structure, range, weapon.clone())
            });
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
        let (movement, weapons, shields) = self.allocation(ship, None);
        Order::Allocate {
            ship: ship.id,
            movement,
            weapons,
            shields,
        }
    }

    fn allocate_with_context(&mut self, ship: &ShipSnapshot, snapshot: &StateSnapshot) -> Order {
        let (movement, weapons, shields) = self.allocation(ship, Some(snapshot));
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
