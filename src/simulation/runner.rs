use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::game_state::{GameState, Phase, ScenarioStatus};
use crate::movement::{apply_order, MoveMode, Order, OrderError};
use crate::scenario::{load_scenario, LoadError};
use crate::snapshot::StateSnapshot;

use super::metrics::{AggregateMetrics, MatchMetrics};
use super::policies::build_policy;
use super::policy::{DecisionContext, Policy};
use super::rubric::{evaluate_rubric, RubricResult, RubricSpec};
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
    IllegalPolicyOrder { policy: String, source: OrderError },
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

pub fn run_match(config: &MatchConfig) -> Result<MatchResult, SimulationError> {
    let mut game = load_scenario(&config.scenario)?;
    game.reseed(config.seed);
    let mut player = build_policy(&config.player_policy, config.seed ^ 0xA5A5_A5A5)
        .ok_or_else(|| SimulationError::UnknownPolicy(config.player_policy.clone()))?;
    let mut opponent = build_policy(&config.opponent_policy, config.seed ^ 0x5A5A_5A5A)
        .ok_or_else(|| SimulationError::UnknownPolicy(config.opponent_policy.clone()))?;
    let mut trace = Vec::new();
    let mut metrics = MatchMetrics::default();

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
        let turn = game.turn_number();
        let phase = game.phase_name().to_string();
        let policy_name = if game.phase() == Phase::TurnEnd {
            "runner".to_string()
        } else {
            policy.name().to_string()
        };
        let prior_log_len = game.combat_log().len();
        metrics.record_order(&order);
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
            });
        }
        for event in game.combat_log().iter().skip(prior_log_len) {
            if event.kind == "hit" {
                metrics.hits += 1;
            } else if event.kind == "miss" {
                metrics.misses += 1;
            }
            metrics.damage += u64::from(event.damage);
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
        rubrics.push(evaluate_rubric(&rubric, &aggregate));
    }
    Ok(SuiteReport {
        name: spec.name.clone(),
        aggregate,
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
        Phase::Movement => snapshot.active_ship,
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
        Phase::Movement => vec![
            Order::Move {
                ship,
                mode: MoveMode::Forward,
            },
            Order::Move {
                ship,
                mode: MoveMode::Reverse,
            },
            Order::Move {
                ship,
                mode: MoveMode::TurnPort,
            },
            Order::Move {
                ship,
                mode: MoveMode::TurnStarboard,
            },
            Order::PassMove { ship },
        ],
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
