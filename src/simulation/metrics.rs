use std::collections::BTreeMap;

use serde::Serialize;

use crate::game_state::ScenarioStatus;
use crate::motion::{self, Maneuver};
use crate::movement::Order;
use crate::snapshot::{ShipSnapshot, StateSnapshot};

#[derive(Debug, Clone, Default, Serialize)]
pub struct MatchMetrics {
    pub turns: u32,
    /// Orders successfully applied to the game state.
    pub orders: u64,
    /// Orders proposed by a policy or the runner, including rejected orders.
    pub attempted_orders: u64,
    pub movement_orders: u64,
    pub fire_commits: u64,
    pub hits: u64,
    pub misses: u64,
    pub damage: u64,
    /// Engine power allocated to thrust conversion; not an initiative score.
    pub engine_power_allocated: u64,
    pub weapon_power_allocated: u64,
    pub shield_power_allocated: u64,
    pub rejected_orders: u64,
    pub terminated: bool,
    /// Velocity observations sampled after each resolved movement phase.
    pub velocity_distribution: BTreeMap<u8, u64>,
    /// Thrust consumed by accepted non-coast maneuvers.
    pub thrust_spent: u64,
    /// Hexes translated by accepted Coast maneuvers.
    pub coasting_distance: u64,
    pub course_changes: u64,
    pub facing_rotations: u64,
    /// Scheduled translations whose position remained unchanged.
    pub blocked_translations: u64,
    pub scheduled_translations: u64,
    pub zero_translation_observations: u64,
    pub reversals: u64,
    pub velocity_observations: u64,
    pub hull_efficiency: BTreeMap<String, HullEfficiencyMetrics>,
    #[serde(skip)]
    last_velocity: BTreeMap<u32, (u8, u8)>,
    #[serde(skip)]
    stopped_course: BTreeMap<u32, u8>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct HullEfficiencyMetrics {
    pub allocation_observations: u64,
    pub engine_power_allocated: u64,
    pub weapon_power_allocated: u64,
    pub shield_power_allocated: u64,
    /// Allocations where all three resource buckets are nonzero. This is a
    /// breadth signal, not a claim of excessive combined effectiveness.
    pub broad_resource_allocations: u64,
    pub velocity_observations: u64,
    pub zero_velocity_observations: u64,
    pub scheduled_translations: u64,
    pub zero_translation_observations: u64,
}

impl MatchMetrics {
    pub fn record_attempted_order(&mut self) {
        self.attempted_orders += 1;
    }

    pub fn record_accepted_order(&mut self, order: &Order) {
        self.orders += 1;
        match order {
            Order::Allocate {
                movement,
                weapons,
                shields,
                ..
            } => {
                self.engine_power_allocated += u64::from(*movement);
                self.weapon_power_allocated +=
                    weapons.values().copied().map(u64::from).sum::<u64>();
                self.shield_power_allocated += shields.iter().copied().map(u64::from).sum::<u64>();
            }
            Order::CommitManeuver { .. } | Order::PassMove { .. } => self.movement_orders += 1,
            Order::CommitFire { .. } => self.fire_commits += 1,
            _ => {}
        }
    }

    pub fn record_allocation(&mut self, ship: &ShipSnapshot, order: &Order) {
        let Order::Allocate {
            movement,
            weapons,
            shields,
            ..
        } = order
        else {
            return;
        };
        let entry = self.hull_efficiency.entry(ship.class.clone()).or_default();
        let weapon_power = weapons.values().copied().map(u64::from).sum::<u64>();
        let shield_power = shields.iter().copied().map(u64::from).sum::<u64>();
        entry.allocation_observations += 1;
        entry.engine_power_allocated += u64::from(*movement);
        entry.weapon_power_allocated += weapon_power;
        entry.shield_power_allocated += shield_power;
        if *movement > 0 && weapon_power > 0 && shield_power > 0 {
            entry.broad_resource_allocations += 1;
        }
    }

    pub fn record_maneuver(&mut self, ship: &ShipSnapshot, maneuver: Maneuver) {
        if let Ok(result) = motion::resolve_maneuver(
            motion::Velocity {
                speed: ship.velocity,
                course: ship.course,
            },
            ship.facing,
            ship.max_velocity,
            maneuver,
        ) {
            self.thrust_spent += u64::from(result.thrust_cost);
        }
        match maneuver {
            Maneuver::Turn { .. } | Maneuver::TurnAccel { .. } => {
                self.course_changes += 1; // facing change (turn-in-place)
                self.facing_rotations += 1;
            }
            Maneuver::Coast | Maneuver::Accel => {}
        }
    }

    pub fn record_movement_resolution(
        &mut self,
        before: &StateSnapshot,
        after: &StateSnapshot,
        maneuvers: &BTreeMap<u32, Maneuver>,
        translation_outcomes: &BTreeMap<u32, bool>,
    ) {
        for ship in after.ships.iter().filter(|ship| !ship.destroyed) {
            *self.velocity_distribution.entry(ship.velocity).or_default() += 1;
            self.velocity_observations += 1;
            let hull = self.hull_efficiency.entry(ship.class.clone()).or_default();
            hull.velocity_observations += 1;
            if ship.velocity == 0 {
                hull.zero_velocity_observations += 1;
            }
            let prior = before.ships.iter().find(|old| old.id == ship.id);
            if let Some(old) = prior {
                // Protocol 3: any ship with speed > 0 is scheduled to slide this cycle.
                let scheduled = ship.velocity > 0;
                if scheduled {
                    self.scheduled_translations += 1;
                    hull.scheduled_translations += 1;
                    if !translation_outcomes.get(&ship.id).copied().unwrap_or(false) {
                        self.blocked_translations += 1;
                        self.zero_translation_observations += 1;
                        hull.zero_translation_observations += 1;
                    }
                }
                let previous = self
                    .last_velocity
                    .insert(ship.id, (old.velocity, old.course))
                    .unwrap_or((old.velocity, old.course));
                if previous.0 > 0 && ship.velocity == 0 {
                    self.stopped_course.insert(ship.id, previous.1);
                }
                if ship.velocity > 0 {
                    if let Some(course) = self.stopped_course.remove(&ship.id) {
                        if ship.course == (course + 3) % 6 {
                            self.reversals += 1;
                        }
                    }
                }
                if maneuvers.get(&ship.id) == Some(&Maneuver::Coast) {
                    self.coasting_distance +=
                        u64::from(translation_outcomes.get(&ship.id).copied().unwrap_or(false));
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AggregateMetrics {
    pub matches: u64,
    pub wins: u64,
    pub losses: u64,
    pub stalemates: u64,
    pub termination_rate: f64,
    pub win_rate: f64,
    pub average_turns: f64,
    pub average_damage: f64,
    pub rejected_orders: u64,
    pub velocity_distribution: BTreeMap<u8, u64>,
    pub thrust_spent: u64,
    pub coasting_distance: u64,
    pub course_changes: u64,
    pub facing_rotations: u64,
    pub blocked_translations: u64,
    pub scheduled_translations: u64,
    pub zero_translation_observations: u64,
    pub reversals: u64,
    pub velocity_observations: u64,
    pub hull_efficiency: BTreeMap<String, HullEfficiencyMetrics>,
}

impl AggregateMetrics {
    pub fn from_matches<'a>(
        matches: impl IntoIterator<Item = (&'a ScenarioStatus, &'a MatchMetrics)>,
    ) -> Self {
        let mut aggregate = Self::default();
        let mut turns = 0u64;
        let mut damage = 0u64;
        for (status, metrics) in matches {
            aggregate.matches += 1;
            turns += u64::from(metrics.turns);
            damage += metrics.damage;
            aggregate.rejected_orders += metrics.rejected_orders;
            aggregate.thrust_spent += metrics.thrust_spent;
            aggregate.coasting_distance += metrics.coasting_distance;
            aggregate.course_changes += metrics.course_changes;
            aggregate.facing_rotations += metrics.facing_rotations;
            aggregate.blocked_translations += metrics.blocked_translations;
            aggregate.scheduled_translations += metrics.scheduled_translations;
            aggregate.zero_translation_observations += metrics.zero_translation_observations;
            aggregate.reversals += metrics.reversals;
            aggregate.velocity_observations += metrics.velocity_observations;
            for (velocity, count) in &metrics.velocity_distribution {
                *aggregate
                    .velocity_distribution
                    .entry(*velocity)
                    .or_default() += count;
            }
            for (class, values) in &metrics.hull_efficiency {
                let entry = aggregate.hull_efficiency.entry(class.clone()).or_default();
                entry.allocation_observations += values.allocation_observations;
                entry.engine_power_allocated += values.engine_power_allocated;
                entry.weapon_power_allocated += values.weapon_power_allocated;
                entry.shield_power_allocated += values.shield_power_allocated;
                entry.broad_resource_allocations += values.broad_resource_allocations;
                entry.velocity_observations += values.velocity_observations;
                entry.zero_velocity_observations += values.zero_velocity_observations;
                entry.scheduled_translations += values.scheduled_translations;
                entry.zero_translation_observations += values.zero_translation_observations;
            }
            match status {
                ScenarioStatus::Won => aggregate.wins += 1,
                ScenarioStatus::Lost => aggregate.losses += 1,
                ScenarioStatus::InProgress => aggregate.stalemates += 1,
            }
        }
        if aggregate.matches > 0 {
            let count = aggregate.matches as f64;
            aggregate.termination_rate = (aggregate.wins + aggregate.losses) as f64 / count;
            aggregate.win_rate = aggregate.wins as f64 / count;
            aggregate.average_turns = turns as f64 / count;
            aggregate.average_damage = damage as f64 / count;
        }
        aggregate
    }
}
