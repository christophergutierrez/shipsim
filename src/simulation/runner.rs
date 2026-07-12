use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::game_state::{GameState, Phase, ScenarioStatus, Terminal};
use crate::motion::Maneuver;
use crate::movement::{apply_order, Order, OrderError};
use crate::scenario::{load_scenario, LoadError};
use crate::snapshot::StateSnapshot;

use super::metrics::{AggregateMetrics, MatchMetrics};
use super::policies::build_policy;
use super::policy::{DecisionContext, Policy};
use super::rubric::{evaluate_rubric, MatchupBreakdown, RubricResult, RubricSpec};
use super::trace::{TraceEvent, TraceOutcome};

fn default_max_turns() -> u32 {
    50
}

fn default_max_orders() -> usize {
    20_000
}

#[derive(Debug, Clone)]
pub struct MatchConfig {
    pub scenario: PathBuf,
    pub seed: u64,
    pub player_policy: String,
    pub opponent_policy: String,
    pub max_turns: u32,
    pub max_orders: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MatchResult {
    pub scenario: PathBuf,
    pub seed: u64,
    pub player_policy: String,
    pub opponent_policy: String,
    pub status: ScenarioStatus,
    pub terminal_reason: Option<String>,
    pub final_snapshot: StateSnapshot,
    pub metrics: MatchMetrics,
    pub trace: Vec<TraceEvent>,
}

/// Structured failure payload retained when a policy submits an illegal order.
/// The runner still returns an error, but callers do not lose the rejected trace
/// event or the metrics collected before failure.
#[derive(Debug, Clone, Serialize)]
pub struct FailedMatch {
    pub scenario: PathBuf,
    pub seed: u64,
    pub player_policy: String,
    pub opponent_policy: String,
    pub status: ScenarioStatus,
    pub terminal_reason: String,
    pub final_snapshot: StateSnapshot,
    pub metrics: MatchMetrics,
    pub trace: Vec<TraceEvent>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchupSpec {
    pub player: String,
    pub opponent: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuiteSpec {
    pub name: String,
    pub scenario: PathBuf,
    pub seeds: Vec<u64>,
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    #[serde(default = "default_max_orders")]
    pub max_orders: usize,
    #[serde(default)]
    pub rubrics: Vec<PathBuf>,
    pub matchups: Vec<MatchupSpec>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SuiteReport {
    pub name: String,
    pub aggregate: AggregateMetrics,
    pub matchup_breakdown: MatchupBreakdown,
    pub rubrics: Vec<RubricResult>,
    pub matches: Vec<MatchResult>,
}

#[derive(Debug, Error)]
pub enum SimulationError {
    #[error("cannot load scenario: {0}")]
    Scenario(#[from] LoadError),
    #[error("unknown policy {0:?}")]
    UnknownPolicy(String),
    #[error("no actor is available in phase {0:?}")]
    NoActor(Phase),
    #[error("policy {policy:?} produced illegal order: {source}")]
    IllegalPolicyOrder {
        policy: String,
        source: OrderError,
        failure: Box<FailedMatch>,
    },
    #[error("no legal orders for ship {ship} in phase {phase:?}")]
    NoLegalOrders { ship: u32, phase: Phase },
    #[error("cannot read rubric {path:?}: {source}")]
    RubricRead {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot parse rubric {path:?}: {source}")]
    RubricParse {
        path: PathBuf,
        source: toml::de::Error,
    },
}

impl SimulationError {
    pub fn failed_match(&self) -> Option<&FailedMatch> {
        match self {
            Self::IllegalPolicyOrder { failure, .. } => Some(failure),
            _ => None,
        }
    }
}

pub fn run_match(config: &MatchConfig) -> Result<MatchResult, SimulationError> {
    let player = build_policy(&config.player_policy, config.seed ^ 0xA5A5_A5A5)
        .ok_or_else(|| SimulationError::UnknownPolicy(config.player_policy.clone()))?;
    let opponent = build_policy(&config.opponent_policy, config.seed ^ 0x5A5A_5A5A)
        .ok_or_else(|| SimulationError::UnknownPolicy(config.opponent_policy.clone()))?;
    run_match_with_policies(config, player, opponent)
}

fn run_match_with_policies(
    config: &MatchConfig,
    mut player: Box<dyn Policy>,
    mut opponent: Box<dyn Policy>,
) -> Result<MatchResult, SimulationError> {
    let mut game = load_scenario(&config.scenario)?;
    game.reseed(config.seed);
    let mut trace = Vec::new();
    let mut metrics = MatchMetrics::default();
    let mut pending_maneuvers = BTreeMap::new();

    while game.status() == ScenarioStatus::InProgress
        && game.turn_number() <= config.max_turns
        && trace.len() < config.max_orders
    {
        let snapshot = StateSnapshot::from_game_state(&game);
        let (actor, is_player) = actor_for(&snapshot, game.phase())?;
        let policy: &mut dyn Policy = if is_player {
            player.as_mut()
        } else {
            opponent.as_mut()
        };
        let order = if game.phase() == Phase::Allocate {
            let ship = snapshot
                .ships
                .iter()
                .find(|ship| ship.id == actor.expect("allocation actor"))
                .expect("snapshot actor");
            policy.allocate(ship)
        } else if game.phase() == Phase::TurnEnd {
            Order::EndTurn
        } else {
            let ship_id = actor.expect("ship actor");
            let ship = snapshot
                .ships
                .iter()
                .find(|ship| ship.id == ship_id)
                .expect("snapshot actor");
            let legal_orders = legal_orders(&game, ship_id);
            if legal_orders.is_empty() {
                return Err(SimulationError::NoLegalOrders {
                    ship: ship_id,
                    phase: game.phase(),
                });
            }
            policy.choose_order(&DecisionContext {
                snapshot: &snapshot,
                ship,
                legal_orders: &legal_orders,
            })
        };
        let before = snapshot.clone();
        let turn = game.turn_number();
        let phase = game.phase_name().to_string();
        let policy_name = if game.phase() == Phase::TurnEnd {
            "runner".to_string()
        } else {
            policy.name().to_string()
        };
        let prior_log_len = game.combat_log().len();
        metrics.record_attempted_order();
        if let Err(source) = apply_order(&mut game, order.clone()) {
            metrics.rejected_orders += 1;
            trace.push(TraceEvent {
                sequence: trace.len(),
                turn,
                phase,
                actor,
                policy: policy_name.clone(),
                order,
                outcome: TraceOutcome::Rejected {
                    error: source.to_string(),
                },
                status_after: game.status(),
                prng_state_after: game.prng_state(),
            });
            return Err(SimulationError::IllegalPolicyOrder {
                policy: policy_name,
                source,
                failure: Box::new(FailedMatch {
                    scenario: config.scenario.clone(),
                    seed: config.seed,
                    player_policy: config.player_policy.clone(),
                    opponent_policy: config.opponent_policy.clone(),
                    status: game.status(),
                    terminal_reason: "policy_order_rejected".into(),
                    final_snapshot: StateSnapshot::from_game_state(&game),
                    metrics,
                    trace,
                }),
            });
        }
        metrics.record_accepted_order(&order);
        if let Order::CommitManeuver { ship, maneuver } = &order {
            let ship_snapshot = before
                .ships
                .iter()
                .find(|candidate| candidate.id == *ship)
                .expect("maneuver actor snapshot");
            metrics.record_maneuver(ship_snapshot, *maneuver);
            pending_maneuvers.insert(*ship, *maneuver);
        }
        if let Order::Allocate { ship, .. } = &order {
            if let Some(ship_snapshot) = before.ships.iter().find(|candidate| candidate.id == *ship)
            {
                metrics.record_allocation(ship_snapshot, &order);
            }
        }
        for event in game.combat_log().iter().skip(prior_log_len) {
            if event.kind == "hit" {
                metrics.hits += 1;
            } else if event.kind == "miss" {
                metrics.misses += 1;
            }
            metrics.damage += u64::from(event.damage);
        }
        let after = StateSnapshot::from_game_state(&game);
        if before.phase == "movement"
            && (after.phase != "movement" || after.movement_phase != before.movement_phase)
        {
            metrics.record_movement_resolution(
                &before,
                &after,
                &pending_maneuvers,
                game.last_translation_outcomes(),
            );
            pending_maneuvers.clear();
        }
        trace.push(TraceEvent {
            sequence: trace.len(),
            turn,
            phase,
            actor,
            policy: policy_name,
            order,
            outcome: TraceOutcome::Accepted,
            status_after: game.status(),
            prng_state_after: game.prng_state(),
        });
    }

    metrics.turns = game.turn_number();
    metrics.terminated = game.status() != ScenarioStatus::InProgress;
    Ok(MatchResult {
        scenario: config.scenario.clone(),
        seed: config.seed,
        player_policy: config.player_policy.clone(),
        opponent_policy: config.opponent_policy.clone(),
        status: game.status(),
        terminal_reason: match (game.status(), game.terminal()) {
            (ScenarioStatus::Won, Some(Terminal::DestroyShip(_))) => {
                Some("destruction_target_reached".into())
            }
            (ScenarioStatus::Won, Some(Terminal::ReachHex(_))) => Some("objective_reached".into()),
            (ScenarioStatus::Won, None) => Some("objective_reached".into()),
            (ScenarioStatus::Lost, _) => Some("player_fleet_destroyed".into()),
            (ScenarioStatus::InProgress, _) if trace.len() >= config.max_orders => {
                Some("max_orders_reached".into())
            }
            (ScenarioStatus::InProgress, _) if game.turn_number() > config.max_turns => {
                Some("max_turns_reached".into())
            }
            (ScenarioStatus::InProgress, _) => Some("in_progress".into()),
        },
        final_snapshot: StateSnapshot::from_game_state(&game),
        metrics,
        trace,
    })
}

pub fn run_suite(spec: &SuiteSpec) -> Result<SuiteReport, SimulationError> {
    let mut matches = Vec::new();
    for matchup in &spec.matchups {
        for seed in &spec.seeds {
            matches.push(run_match(&MatchConfig {
                scenario: spec.scenario.clone(),
                seed: *seed,
                player_policy: matchup.player.clone(),
                opponent_policy: matchup.opponent.clone(),
                max_turns: spec.max_turns,
                max_orders: spec.max_orders,
            })?);
        }
    }
    let aggregate = AggregateMetrics::from_matches(
        matches
            .iter()
            .map(|result| (&result.status, &result.metrics)),
    );
    let matchup_breakdown = MatchupBreakdown::from_results(matches.iter().map(|result| {
        (
            result.player_policy.clone(),
            result.opponent_policy.clone(),
            result.status,
        )
    }));
    let mut rubrics = Vec::new();
    for path in &spec.rubrics {
        let text = std::fs::read_to_string(path).map_err(|source| SimulationError::RubricRead {
            path: path.clone(),
            source,
        })?;
        let rubric: RubricSpec =
            toml::from_str(&text).map_err(|source| SimulationError::RubricParse {
                path: path.clone(),
                source,
            })?;
        rubrics.push(evaluate_rubric(&rubric, &aggregate, &matchup_breakdown));
    }
    Ok(SuiteReport {
        name: spec.name.clone(),
        aggregate,
        matchup_breakdown,
        rubrics,
        matches,
    })
}

fn actor_for(
    snapshot: &StateSnapshot,
    phase: Phase,
) -> Result<(Option<u32>, bool), SimulationError> {
    let actor = match phase {
        Phase::Allocate => snapshot
            .ships
            .iter()
            .find(|ship| !ship.destroyed && !snapshot.ships_allocated_this_turn.contains(&ship.id))
            .map(|ship| ship.id),
        Phase::Movement => snapshot
            .ships
            .iter()
            .find(|ship| !ship.destroyed && !snapshot.ships_committed_this_phase.contains(&ship.id))
            .map(|ship| ship.id),
        Phase::Firing => snapshot
            .ships
            .iter()
            .find(|ship| !ship.destroyed && !snapshot.ships_ready_fire.contains(&ship.id))
            .map(|ship| ship.id),
        Phase::TurnEnd => return Ok((None, true)),
    };
    let actor = actor.ok_or(SimulationError::NoActor(phase))?;
    let is_player = snapshot
        .ships
        .iter()
        .find(|ship| ship.id == actor)
        .is_some_and(|ship| ship.controller == "player");
    Ok((Some(actor), is_player))
}

fn legal_orders(game: &GameState, ship: u32) -> Vec<Order> {
    let candidates = match game.phase() {
        Phase::Movement => [
            Maneuver::Coast,
            Maneuver::Accelerate { course: None },
            Maneuver::Accelerate { course: Some(0) },
            Maneuver::Accelerate { course: Some(1) },
            Maneuver::Accelerate { course: Some(2) },
            Maneuver::Accelerate { course: Some(3) },
            Maneuver::Accelerate { course: Some(4) },
            Maneuver::Accelerate { course: Some(5) },
            Maneuver::Decelerate,
            Maneuver::TurnCoursePort,
            Maneuver::TurnCourseStarboard,
            Maneuver::RotatePort,
            Maneuver::RotateStarboard,
        ]
        .into_iter()
        .map(|maneuver| Order::CommitManeuver { ship, maneuver })
        .filter(|order| {
            let mut candidate = game.clone();
            apply_order(&mut candidate, order.clone()).is_ok()
        })
        .collect(),
        Phase::Firing => {
            let mut orders = Vec::new();
            if let Some(attacker) = game.ship(ship) {
                for weapon in &attacker.weapons {
                    for target in game
                        .ships()
                        .iter()
                        .filter(|target| !target.destroyed && target.id != ship)
                    {
                        for shield_facing in 0..6 {
                            orders.push(Order::CommitFire {
                                ship,
                                weapon: weapon.id.clone(),
                                target: target.id,
                                shield_facing,
                            });
                        }
                    }
                }
            }
            orders.push(Order::ReadyFire { ship });
            orders
        }
        _ => Vec::new(),
    };
    candidates
        .into_iter()
        .filter(|order| {
            let mut candidate = game.clone();
            apply_order(&mut candidate, order.clone()).is_ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    struct RejectingPolicy;

    impl Policy for RejectingPolicy {
        fn name(&self) -> &str {
            "test_rejecting_policy"
        }

        fn allocate(&mut self, ship: &crate::snapshot::ShipSnapshot) -> Order {
            Order::Allocate {
                ship: ship.id,
                movement: 0,
                weapons: BTreeMap::new(),
                shields: [0; 6],
            }
        }

        fn choose_order(&mut self, context: &DecisionContext<'_>) -> Order {
            Order::Move {
                ship: context.ship.id,
                mode: "test-invalid".into(),
            }
        }
    }

    #[test]
    fn failed_match_retains_rejected_trace_metrics_snapshot_and_serialized_report() {
        let config = MatchConfig {
            scenario: PathBuf::from("scenarios/simulation_duel.toml"),
            seed: 41,
            player_policy: "test_rejecting_policy".into(),
            opponent_policy: "test_rejecting_policy".into(),
            max_turns: 10,
            max_orders: 100,
        };
        let error = run_match_with_policies(
            &config,
            Box::new(RejectingPolicy),
            Box::new(RejectingPolicy),
        )
        .expect_err("the deliberate legacy order must fail");
        let failure = error.failed_match().expect("structured failed match");

        assert_eq!(failure.metrics.rejected_orders, 1);
        assert_eq!(failure.metrics.attempted_orders, 3);
        assert_eq!(failure.metrics.orders, 2);
        assert_eq!(failure.metrics.movement_orders, 0);
        assert_eq!(failure.metrics.thrust_spent, 0);
        assert_eq!(failure.metrics.engine_power_allocated, 0);
        assert_eq!(failure.trace.len(), 3);
        assert!(matches!(
            failure.trace.last().map(|event| &event.outcome),
            Some(TraceOutcome::Rejected { .. })
        ));
        assert_eq!(
            failure.trace.last().map(|event| event.prng_state_after),
            Some(failure.final_snapshot.prng_state)
        );
        assert_eq!(failure.final_snapshot.phase, "movement");

        let encoded = serde_json::to_vec_pretty(failure).expect("failed match JSON");
        let report_path = tempfile::NamedTempFile::new().expect("temporary report");
        std::fs::write(report_path.path(), &encoded).expect("write failed report");
        let decoded: serde_json::Value =
            serde_json::from_slice(&std::fs::read(report_path.path()).expect("read failed report"))
                .expect("decode failed report");
        assert_eq!(
            decoded["trace"][2]["outcome"]["rejected"]["error"],
            "the Move order was removed in M4 (ADR-0022); submit CommitManeuver instead"
        );
        assert_eq!(decoded["metrics"]["rejected_orders"], 1);
        assert!(decoded["final_snapshot"]["prng_state"].is_number());
    }
}
