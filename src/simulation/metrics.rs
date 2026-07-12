use serde::Serialize;

use crate::game_state::ScenarioStatus;
use crate::movement::Order;

#[derive(Debug, Clone, Default, Serialize)]
pub struct MatchMetrics {
    pub turns: u32,
    pub orders: u64,
    pub movement_orders: u64,
    pub fire_commits: u64,
    pub hits: u64,
    pub misses: u64,
    pub damage: u64,
    pub movement_power_allocated: u64,
    pub weapon_power_allocated: u64,
    pub shield_power_allocated: u64,
    pub rejected_orders: u64,
    pub terminated: bool,
}

impl MatchMetrics {
    pub fn record_order(&mut self, order: &Order) {
        self.orders += 1;
        match order {
            Order::Allocate {
                movement,
                weapons,
                shields,
                ..
            } => {
                self.movement_power_allocated += u64::from(*movement);
                self.weapon_power_allocated +=
                    weapons.values().copied().map(u64::from).sum::<u64>();
                self.shield_power_allocated += shields.iter().copied().map(u64::from).sum::<u64>();
            }
            Order::CommitManeuver { .. } | Order::PassMove { .. } => self.movement_orders += 1,
            Order::CommitFire { .. } => self.fire_commits += 1,
            _ => {}
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
