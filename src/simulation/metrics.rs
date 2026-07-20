use std::collections::BTreeMap;

use serde::Serialize;

use crate::game_state::ScenarioStatus;
use crate::movement::Order;
use crate::path_resolve::PathResult;
use crate::snapshot::{ShipSnapshot, StateSnapshot};

#[derive(Debug, Clone, Default, Serialize)]
pub struct MatchMetrics {
    pub turns: u32,
    /// Orders successfully applied to the game state.
    pub orders: u64,
    /// Orders proposed by a policy or the runner, including rejected orders.
    pub attempted_orders: u64,
    /// Accepted `CommitPath` orders.
    pub path_orders: u64,
    /// Accepted `CommitVolley` orders.
    pub volley_orders: u64,
    /// Sum of action counts across accepted paths.
    pub path_cost_total: u64,
    /// Sum of shot counts across accepted volleys.
    pub volley_shots: u64,
    pub hits: u64,
    pub misses: u64,
    pub damage: u64,
    /// Engine power allocated to thrust conversion; not an initiative score.
    pub engine_power_allocated: u64,
    pub weapon_power_allocated: u64,
    pub shield_power_allocated: u64,
    pub rejected_orders: u64,
    pub terminated: bool,
    /// Path cost histogram sampled from resolved path results.
    pub path_cost_distribution: BTreeMap<u32, u64>,
    /// Volley size histogram sampled from accepted `CommitVolley` orders.
    pub volley_size_distribution: BTreeMap<u32, u64>,
    /// Sum of `translated_steps` across resolved path results.
    pub path_translated_steps: u64,
    /// Path results that reported a blocked kind or non-zero fallback.
    pub blocked_paths: u64,
    /// Path results with `submitted_cost > 0` (attempted translation/turn spend).
    pub scheduled_paths: u64,
    /// Path results with `translated_steps == 0` among scheduled paths.
    pub zero_translation_observations: u64,
    /// Path observations (one per living ship per resolved movement stage).
    pub path_observations: u64,
    pub hull_efficiency: BTreeMap<String, HullEfficiencyMetrics>,
    /// Match diagnostics are copied to `MatchResult` and remain out of the
    /// serialized metrics object. Keeping them here lets aggregate metrics use
    /// the existing `from_matches` input shape.
    #[serde(skip)]
    pub undecided_margin: Option<i64>,
    #[serde(skip)]
    pub closest_approach: Option<u32>,
    #[serde(skip)]
    pub turns_in_weapon_range: u32,
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
    /// Sum of (engine+weapon+shield) power actually assigned across allocates.
    pub power_spent_total: u64,
    /// Sum of `power_available` at each allocate (0 if reactor dead).
    pub power_available_total: u64,
    /// Allocates where power_available was 0 (power-dead hull).
    pub zero_power_allocations: u64,
    /// Path resolution observations for this class.
    pub path_observations: u64,
    /// Paths submitted with cost 0 while the ship still had motion available.
    pub zero_motion_path_observations: u64,
    pub scheduled_paths: u64,
    pub zero_translation_observations: u64,
    pub path_cost_total: u64,
}

impl HullEfficiencyMetrics {
    /// power_spent / power_available across allocates. 0 if never had power.
    pub fn power_utilization(&self) -> f64 {
        if self.power_available_total == 0 {
            return if self.allocation_observations > 0 {
                0.0
            } else {
                1.0
            };
        }
        self.power_spent_total as f64 / self.power_available_total as f64
    }
}

impl MatchMetrics {
    pub fn record_attempted_order(&mut self) {
        self.attempted_orders += 1;
    }

    pub fn record_accepted_order(&mut self, order: &Order) {
        self.orders += 1;
        match order {
            Order::Allocate {
                movement, shields, ..
            } => {
                // Weapon charge increases are recorded in `record_allocation`
                // (needs pre-allocate snapshot). Here only motion + shields are
                // known without prior charge.
                self.engine_power_allocated += u64::from(*movement);
                self.shield_power_allocated += shields.iter().copied().map(u64::from).sum::<u64>();
            }
            Order::CommitPath { actions, .. } => {
                self.path_orders += 1;
                let cost = actions.len() as u32;
                self.path_cost_total += u64::from(cost);
                *self.path_cost_distribution.entry(cost).or_default() += 1;
            }
            Order::CommitVolley { shots, .. } => {
                self.volley_orders += 1;
                let size = shots.len() as u32;
                self.volley_shots += u64::from(size);
                *self.volley_size_distribution.entry(size).or_default() += 1;
            }
            Order::RetiredUnknown => {}
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
        // Power is spent only on *increases* over carried charge, matching allocate.
        let mut weapon_power = 0u64;
        for (weapon_id, want) in weapons {
            let have = ship
                .weapons
                .iter()
                .find(|w| w.id == *weapon_id)
                .map(|w| u64::from(w.charge))
                .unwrap_or(0);
            weapon_power += u64::from(*want).saturating_sub(have);
        }
        let shield_power = shields.iter().copied().map(u64::from).sum::<u64>();
        let spent = u64::from(*movement) + weapon_power + shield_power;
        let available = u64::from(ship.power_available);
        entry.allocation_observations += 1;
        entry.engine_power_allocated += u64::from(*movement);
        entry.weapon_power_allocated += weapon_power;
        entry.shield_power_allocated += shield_power;
        // Aggregate weapon accounting (charge increases only).
        self.weapon_power_allocated += weapon_power;
        entry.power_spent_total += spent;
        entry.power_available_total += available;
        if available == 0 {
            entry.zero_power_allocations += 1;
        }
        if *movement > 0 && weapon_power > 0 && shield_power > 0 {
            entry.broad_resource_allocations += 1;
        }
    }

    /// Record path resolution telemetry after the movement stage completes.
    pub fn record_path_resolution(&mut self, before: &StateSnapshot, path_results: &[PathResult]) {
        for result in path_results {
            self.path_observations += 1;
            self.path_translated_steps += u64::from(result.translated_steps);

            let prior = before.ships.iter().find(|ship| ship.id == result.ship);
            let class = prior
                .map(|ship| ship.class.clone())
                .unwrap_or_else(|| "unknown".into());
            let hull = self.hull_efficiency.entry(class).or_default();
            hull.path_observations += 1;
            hull.path_cost_total += u64::from(result.submitted_cost);

            let had_motion = prior.is_some_and(|ship| ship.motion_available > 0);
            if had_motion && result.submitted_cost == 0 {
                hull.zero_motion_path_observations += 1;
            }

            let scheduled = result.submitted_cost > 0;
            if scheduled {
                self.scheduled_paths += 1;
                hull.scheduled_paths += 1;
                if result.translated_steps == 0 {
                    self.zero_translation_observations += 1;
                    hull.zero_translation_observations += 1;
                }
            }

            if result.blocked_kind.is_some() || result.fallback_steps > 0 {
                self.blocked_paths += 1;
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
    pub capped_matches: u64,
    pub decided_equivalent_matches: u64,
    pub decided_equivalent_rate: f64,
    pub termination_rate: f64,
    pub win_rate: f64,
    pub average_turns: f64,
    pub average_damage: f64,
    pub rejected_orders: u64,
    pub path_orders: u64,
    pub volley_orders: u64,
    pub path_cost_total: u64,
    pub volley_shots: u64,
    pub path_cost_distribution: BTreeMap<u32, u64>,
    pub volley_size_distribution: BTreeMap<u32, u64>,
    pub path_translated_steps: u64,
    pub blocked_paths: u64,
    pub scheduled_paths: u64,
    pub zero_translation_observations: u64,
    pub path_observations: u64,
    pub closest_approach_distribution: BTreeMap<u32, u64>,
    pub turns_in_weapon_range_distribution: BTreeMap<u32, u64>,
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
            aggregate.path_orders += metrics.path_orders;
            aggregate.volley_orders += metrics.volley_orders;
            aggregate.path_cost_total += metrics.path_cost_total;
            aggregate.volley_shots += metrics.volley_shots;
            aggregate.path_translated_steps += metrics.path_translated_steps;
            aggregate.blocked_paths += metrics.blocked_paths;
            aggregate.scheduled_paths += metrics.scheduled_paths;
            aggregate.zero_translation_observations += metrics.zero_translation_observations;
            aggregate.path_observations += metrics.path_observations;
            if let Some(closest) = metrics.closest_approach {
                *aggregate
                    .closest_approach_distribution
                    .entry(closest)
                    .or_default() += 1;
            }
            *aggregate
                .turns_in_weapon_range_distribution
                .entry(metrics.turns_in_weapon_range)
                .or_default() += 1;
            for (cost, count) in &metrics.path_cost_distribution {
                *aggregate.path_cost_distribution.entry(*cost).or_default() += count;
            }
            for (size, count) in &metrics.volley_size_distribution {
                *aggregate.volley_size_distribution.entry(*size).or_default() += count;
            }
            for (class, values) in &metrics.hull_efficiency {
                let entry = aggregate.hull_efficiency.entry(class.clone()).or_default();
                entry.allocation_observations += values.allocation_observations;
                entry.engine_power_allocated += values.engine_power_allocated;
                entry.weapon_power_allocated += values.weapon_power_allocated;
                entry.shield_power_allocated += values.shield_power_allocated;
                entry.broad_resource_allocations += values.broad_resource_allocations;
                entry.power_spent_total += values.power_spent_total;
                entry.power_available_total += values.power_available_total;
                entry.zero_power_allocations += values.zero_power_allocations;
                entry.path_observations += values.path_observations;
                entry.zero_motion_path_observations += values.zero_motion_path_observations;
                entry.scheduled_paths += values.scheduled_paths;
                entry.zero_translation_observations += values.zero_translation_observations;
                entry.path_cost_total += values.path_cost_total;
            }
            match status {
                ScenarioStatus::Won => aggregate.wins += 1,
                ScenarioStatus::Lost => aggregate.losses += 1,
                ScenarioStatus::InProgress => {
                    aggregate.stalemates += 1;
                    aggregate.capped_matches += 1;
                }
            }
            if !matches!(status, ScenarioStatus::InProgress)
                || metrics.undecided_margin.is_some_and(|margin| margin != 0)
            {
                aggregate.decided_equivalent_matches += 1;
            }
        }
        if aggregate.matches > 0 {
            let count = aggregate.matches as f64;
            aggregate.termination_rate = (aggregate.wins + aggregate.losses) as f64 / count;
            aggregate.decided_equivalent_rate = aggregate.decided_equivalent_matches as f64 / count;
            aggregate.win_rate = aggregate.wins as f64 / count;
            aggregate.average_turns = turns as f64 / count;
            aggregate.average_damage = damage as f64 / count;
        }
        aggregate
    }
}
