use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::game_state::{GameState, Phase, ScenarioStatus, Terminal};
use crate::motion::Maneuver;
use crate::movement::{apply_order, Order, OrderError};
use crate::scenario::{load_scenario, load_scenario_def, LoadError};
use crate::schema::ScenarioDef;
use crate::snapshot::StateSnapshot;

use super::fleet::{
    build_engagement_scenario, engagement_costs, engagement_report_path, validate_engagement_costs,
    EngagementSpec, FleetError, FleetMapSpec, PowerSweepSpec,
};
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

/// How to adjudicate matches the engine leaves `InProgress` (turn cap, order
/// cap, or mutual disarm). The engine `status` stays authoritative; the
/// adjudication is reported separately as `adjudicated_status`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StalemateScoring {
    /// Leave status InProgress with no adjudication (historical default).
    #[default]
    None,
    /// Score undecided matches by hull-damage differential (`undecided_margin`):
    /// positive → player adjudicated Won, negative → Lost, zero → undecided.
    DamageDiff,
}

#[derive(Debug, Clone)]
pub struct MatchConfig {
    pub scenario: PathBuf,
    pub seed: u64,
    pub player_policy: String,
    pub opponent_policy: String,
    pub max_turns: u32,
    pub max_orders: usize,
    /// When set, load this definition instead of reading `scenario` from disk.
    pub built_scenario: Option<ScenarioDef>,
    /// Repository root used to resolve `data/ships` for built scenarios.
    pub data_root: Option<PathBuf>,
    /// Optional engagement label for reports.
    pub engagement: Option<String>,
    pub player_cost: Option<u32>,
    pub opponent_cost: Option<u32>,
    pub stalemate_scoring: StalemateScoring,
}

impl MatchConfig {
    pub fn from_scenario(
        scenario: PathBuf,
        seed: u64,
        player_policy: String,
        opponent_policy: String,
        max_turns: u32,
        max_orders: usize,
    ) -> Self {
        Self {
            scenario,
            seed,
            player_policy,
            opponent_policy,
            max_turns,
            max_orders,
            built_scenario: None,
            data_root: None,
            engagement: None,
            player_cost: None,
            opponent_cost: None,
            stalemate_scoring: StalemateScoring::None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MatchResult {
    pub scenario: PathBuf,
    pub seed: u64,
    pub player_policy: String,
    pub opponent_policy: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engagement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_cost: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opponent_cost: Option<u32>,
    pub status: ScenarioStatus,
    pub terminal_reason: Option<String>,
    /// Stalemate adjudication of an `InProgress` match (per `stalemate_scoring`).
    /// The engine `status` above stays authoritative.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adjudicated_status: Option<ScenarioStatus>,
    pub undecided_margin: Option<i64>,
    pub closest_approach: Option<u32>,
    pub turns_in_weapon_range: u32,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engagement: Option<String>,
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
    /// Fixed scenario path. Required when `engagements` is empty.
    #[serde(default)]
    pub scenario: Option<PathBuf>,
    pub seeds: Vec<u64>,
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    #[serde(default = "default_max_orders")]
    pub max_orders: usize,
    #[serde(default)]
    pub rubrics: Vec<PathBuf>,
    pub matchups: Vec<MatchupSpec>,
    /// Cost-matched fleet vs fleet pairings. When non-empty, each engagement is
    /// expanded with every matchup × seed (map + budget apply).
    #[serde(default)]
    pub engagements: Vec<EngagementSpec>,
    /// Generate engagements by varying `power` on one class (see PowerSweepSpec).
    #[serde(default)]
    pub power_sweeps: Vec<PowerSweepSpec>,
    #[serde(default)]
    pub map: Option<FleetMapSpec>,
    /// Target construction budget both fleets should approximate.
    #[serde(default)]
    pub budget: Option<u32>,
    #[serde(default = "default_cost_tolerance_suite")]
    pub cost_tolerance: u32,
    /// When true, skip cost balance checks (typical for pure stat sweeps).
    #[serde(default)]
    pub skip_cost_validation: bool,
    /// Resolve turn-cap stalemates via damage/hull differential (balance suites).
    #[serde(default)]
    pub stalemate_scoring: StalemateScoring,
    /// Repository root for `data/ships` (defaults to cwd).
    #[serde(default)]
    pub data_root: Option<PathBuf>,
}

fn default_cost_tolerance_suite() -> u32 {
    60
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
    #[error("fleet composition: {0}")]
    Fleet(#[from] FleetError),
    #[error("suite requires `scenario` when `engagements` is empty")]
    MissingScenario,
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

fn load_match_game(config: &MatchConfig) -> Result<GameState, SimulationError> {
    if let Some(def) = &config.built_scenario {
        let root = config
            .data_root
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));
        Ok(load_scenario_def(def, &root)?)
    } else {
        Ok(load_scenario(&config.scenario)?)
    }
}

fn axial_distance(a: &crate::snapshot::ShipSnapshot, b: &crate::snapshot::ShipSnapshot) -> u32 {
    let dq = b.q - a.q;
    let dr = b.r - a.r;
    dq.abs().max(dr.abs()).max((dq + dr).abs()) as u32
}

/// Update geometry diagnostics while the runner still has the live snapshot.
/// This deliberately does not inspect the trace or the turn-scoped combat log.
fn observe_diagnostics(
    snapshot: &StateSnapshot,
    closest_approach: &mut Option<u32>,
    turns_in_weapon_range: &mut BTreeSet<u32>,
) {
    let living: Vec<_> = snapshot.ships.iter().filter(|ship| !ship.destroyed).collect();
    let mut closest = None;
    let mut in_weapon_range = false;
    for ship in &living {
        for enemy in &living {
            if ship.controller == enemy.controller {
                continue;
            }
            let distance = axial_distance(ship, enemy);
            closest = Some(closest.map_or(distance, |current: u32| current.min(distance)));
            if ship
                .weapons
                .iter()
                .any(|weapon| weapon.operational && weapon.max_range >= distance)
            {
                in_weapon_range = true;
            }
        }
    }
    if let Some(distance) = closest {
        *closest_approach = Some(closest_approach.map_or(distance, |current| current.min(distance)));
    }
    if in_weapon_range {
        turns_in_weapon_range.insert(snapshot.turn);
    }
}

fn effective_damage_by_side(initial: &StateSnapshot, final_snapshot: &StateSnapshot) -> (i64, i64) {
    let mut player_damage = 0i64;
    let mut opponent_damage = 0i64;
    for initial_ship in &initial.ships {
        let Some(final_ship) = final_snapshot
            .ships
            .iter()
            .find(|ship| ship.id == initial_ship.id)
        else {
            continue;
        };
        let damage = if final_ship.destroyed {
            initial_ship.structure
        } else {
            initial_ship.structure.saturating_sub(final_ship.structure)
        } as i64;
        if initial_ship.controller == "player" {
            player_damage += damage;
        } else {
            opponent_damage += damage;
        }
    }
    (player_damage, opponent_damage)
}

fn undecided_margin(initial: &StateSnapshot, final_snapshot: &StateSnapshot) -> Option<i64> {
    if final_snapshot.status != ScenarioStatus::InProgress {
        return None;
    }
    let (player_damage, opponent_damage) = effective_damage_by_side(initial, final_snapshot);
    Some(opponent_damage - player_damage)
}

/// Both sides still have survivors, but no surviving ship on either side has an
/// operational weapon. Such a match can never resolve by combat (weapon boxes do
/// not repair), so continuing to the turn cap only burns wall clock.
fn mutually_disarmed(snapshot: &StateSnapshot) -> bool {
    let living: Vec<_> = snapshot.ships.iter().filter(|ship| !ship.destroyed).collect();
    let player_alive = living.iter().any(|ship| ship.controller == "player");
    let enemy_alive = living.iter().any(|ship| ship.controller != "player");
    player_alive
        && enemy_alive
        && living
            .iter()
            .all(|ship| ship.weapons.iter().all(|weapon| !weapon.operational))
}

/// Score an undecided match from its damage margin per `stalemate_scoring`.
/// Positive margin (opponent took more hull damage) → player Won; zero stays
/// undecided.
fn adjudicate_stalemate(
    status: ScenarioStatus,
    scoring: StalemateScoring,
    margin: Option<i64>,
) -> Option<ScenarioStatus> {
    if status != ScenarioStatus::InProgress || scoring != StalemateScoring::DamageDiff {
        return None;
    }
    match margin? {
        m if m > 0 => Some(ScenarioStatus::Won),
        m if m < 0 => Some(ScenarioStatus::Lost),
        _ => None,
    }
}

fn run_match_with_policies(
    config: &MatchConfig,
    mut player: Box<dyn Policy>,
    mut opponent: Box<dyn Policy>,
) -> Result<MatchResult, SimulationError> {
    let mut game = load_match_game(config)?;
    game.reseed(config.seed);
    let mut trace = Vec::new();
    let mut metrics = MatchMetrics::default();
    let mut pending_maneuvers = BTreeMap::new();
    let initial_snapshot = StateSnapshot::from_game_state(&game);
    let mut closest_approach = None;
    let mut turns_in_weapon_range = BTreeSet::new();
    let mut mutual_disarm = false;
    observe_diagnostics(
        &initial_snapshot,
        &mut closest_approach,
        &mut turns_in_weapon_range,
    );

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
            policy.allocate_with_context(ship, &snapshot)
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
                    engagement: config.engagement.clone(),
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
        observe_diagnostics(
            &after,
            &mut closest_approach,
            &mut turns_in_weapon_range,
        );
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
        if game.status() == ScenarioStatus::InProgress && mutually_disarmed(&after) {
            mutual_disarm = true;
            break;
        }
    }

    metrics.turns = game.turn_number();
    let terminal_reason = match (game.status(), game.terminal()) {
        (ScenarioStatus::Won, Some(Terminal::DestroyShip(_))) => {
            Some("destruction_target_reached".into())
        }
        (ScenarioStatus::Won, Some(Terminal::AnnihilateEnemies)) => {
            Some("enemy_fleet_annihilated".into())
        }
        (ScenarioStatus::Won, Some(Terminal::ReachHex(_))) => Some("objective_reached".into()),
        (ScenarioStatus::Won, None) => Some("objective_reached".into()),
        (ScenarioStatus::Lost, _) => Some("player_fleet_destroyed".into()),
        (ScenarioStatus::InProgress, _) if mutual_disarm => Some("mutual_disarm".into()),
        (ScenarioStatus::InProgress, _) if trace.len() >= config.max_orders => {
            Some("max_orders_reached".into())
        }
        (ScenarioStatus::InProgress, _) if game.turn_number() > config.max_turns => {
            Some("max_turns_reached".into())
        }
        (ScenarioStatus::InProgress, _) => Some("in_progress".into()),
    };

    let final_snapshot = StateSnapshot::from_game_state(&game);
    let margin = undecided_margin(&initial_snapshot, &final_snapshot);
    metrics.undecided_margin = margin;
    metrics.closest_approach = closest_approach;
    metrics.turns_in_weapon_range = turns_in_weapon_range.len() as u32;
    metrics.terminated = game.status() != ScenarioStatus::InProgress;
    Ok(MatchResult {
        scenario: config.scenario.clone(),
        seed: config.seed,
        player_policy: config.player_policy.clone(),
        opponent_policy: config.opponent_policy.clone(),
        engagement: config.engagement.clone(),
        player_cost: config.player_cost,
        opponent_cost: config.opponent_cost,
        status: game.status(),
        adjudicated_status: adjudicate_stalemate(
            game.status(),
            config.stalemate_scoring,
            margin,
        ),
        terminal_reason,
        undecided_margin: margin,
        closest_approach,
        turns_in_weapon_range: turns_in_weapon_range.len() as u32,
        final_snapshot,
        metrics,
        trace,
    })
}

pub fn run_suite(spec: &SuiteSpec) -> Result<SuiteReport, SimulationError> {
    let data_root = spec
        .data_root
        .clone()
        .unwrap_or_else(|| PathBuf::from("."));
    let mut matches = Vec::new();

    let has_fleet = !spec.engagements.is_empty() || !spec.power_sweeps.is_empty();
    if !has_fleet {
        let scenario = spec
            .scenario
            .clone()
            .ok_or(SimulationError::MissingScenario)?;
        for matchup in &spec.matchups {
            for seed in &spec.seeds {
                let mut cfg = MatchConfig::from_scenario(
                    scenario.clone(),
                    *seed,
                    matchup.player.clone(),
                    matchup.opponent.clone(),
                    spec.max_turns,
                    spec.max_orders,
                );
                cfg.stalemate_scoring = spec.stalemate_scoring;
                matches.push(run_match(&cfg)?);
            }
        }
    } else {
        let map = spec.map.clone().unwrap_or_default();
        let suite_dir = spec
            .scenario
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("simulation/suites"));
        let mut engagements = spec.engagements.clone();
        for sweep in &spec.power_sweeps {
            engagements.extend(sweep.expand()?);
        }
        if engagements.is_empty() {
            return Err(SimulationError::MissingScenario);
        }
        for engagement in &engagements {
            let costs = engagement_costs(&data_root, engagement)?;
            if !spec.skip_cost_validation {
                validate_engagement_costs(
                    &costs,
                    &engagement.name,
                    spec.budget,
                    spec.cost_tolerance,
                )?;
            }
            for matchup in &spec.matchups {
                for seed in &spec.seeds {
                    let def = build_engagement_scenario(engagement, &map, *seed)?;
                    matches.push(run_match(&MatchConfig {
                        scenario: engagement_report_path(&suite_dir, &engagement.name),
                        seed: *seed,
                        player_policy: matchup.player.clone(),
                        opponent_policy: matchup.opponent.clone(),
                        max_turns: spec.max_turns,
                        max_orders: spec.max_orders,
                        built_scenario: Some(def),
                        data_root: Some(data_root.clone()),
                        engagement: Some(engagement.name.clone()),
                        player_cost: Some(costs.player),
                        opponent_cost: Some(costs.opponent),
                        stalemate_scoring: spec.stalemate_scoring,
                    })?);
                }
            }
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
        Phase::Movement => {
            let mut candidates = vec![Maneuver::Coast, Maneuver::Accel];
            for facing in 0..6u8 {
                candidates.push(Maneuver::Turn { facing });
                candidates.push(Maneuver::TurnAccel { facing });
            }
            candidates
                .into_iter()
                .map(|maneuver| Order::CommitManeuver { ship, maneuver })
                .filter(|order| {
                    let mut candidate = game.clone();
                    apply_order(&mut candidate, order.clone()).is_ok()
                })
                .collect()
        }
        Phase::Firing => {
            let mut orders = Vec::new();
            if let Some(attacker) = game.ship(ship) {
                let attacker_side = game.controller_label(ship);
                for weapon in &attacker.weapons {
                    for target in game.ships().iter().filter(|target| {
                        !target.destroyed
                            && target.id != ship
                            && game.controller_label(target.id) != attacker_side
                    }) {
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
                .into_iter()
                .filter(|order| {
                    if let Order::CommitFire { target, .. } = order {
                        let Some(attacker) = game.ship(ship) else {
                            return false;
                        };
                        let Some(target_ship) = game.ship(*target) else {
                            return false;
                        };
                        attacker.pos.distance(target_ship.pos) > 0
                    } else {
                        true
                    }
                })
                .collect()
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
    use crate::snapshot::{ShipSnapshot, WeaponSnapshot};

    fn test_ship(id: u32, controller: &str, destroyed: bool, weapon_operational: bool) -> ShipSnapshot {
        ShipSnapshot {
            id,
            class: "Test".into(),
            size: 2,
            controller: controller.into(),
            q: 0,
            r: 0,
            facing: 0,
            speed: 1,
            power: 10,
            power_available: 10,
            movement_allocated: 0,
            shields_powered: [0; 6],
            shields_remaining: [0; 6],
            max_shield_per_facing: 4,
            structure: if destroyed { 0 } else { 5 },
            engine: 1,
            power_sys: 1,
            bridge: 1,
            weapon_boxes: vec![u32::from(weapon_operational)],
            destroyed,
            weapons: vec![WeaponSnapshot {
                id: "beam_1".into(),
                kind: "Beam".into(),
                arc: "Forward".into(),
                mount: None,
                max_range: 10,
                charge: 0,
                fired: false,
                max_charge: 4,
                operational: weapon_operational,
            }],
            max_velocity: 1,
            thrust_per_power: 1,
            power_per_thrust: 1,
            velocity: 0,
            course: 0,
            thrust_remaining: 0,
        }
    }

    fn test_snapshot(ships: Vec<ShipSnapshot>) -> StateSnapshot {
        StateSnapshot {
            protocol_version: 3,
            turn: 1,
            status: ScenarioStatus::InProgress,
            phase: "allocate".into(),
            movement_phase: 0,
            ships_committed_this_phase: Vec::new(),
            ships_ready_fire: Vec::new(),
            ships_allocated_this_turn: Vec::new(),
            seed: 1,
            prng_state: 1,
            map: crate::snapshot::MapSnapshot {
                width: 24,
                height: 18,
                mode: "hard".into(),
            },
            objective: None,
            ships,
            fire_commits: Vec::new(),
            combat_log: Vec::new(),
            end_turn_warning: false,
        }
    }

    #[test]
    fn mutual_disarm_requires_survivors_on_both_sides_and_no_operational_weapons() {
        // Both sides alive, all weapons out -> disarmed.
        assert!(mutually_disarmed(&test_snapshot(vec![
            test_ship(1, "player", false, false),
            test_ship(2, "player", true, false),
            test_ship(3, "scripted", false, false),
        ])));
        // Enemy still has a working weapon -> not disarmed.
        assert!(!mutually_disarmed(&test_snapshot(vec![
            test_ship(1, "player", false, false),
            test_ship(3, "scripted", false, true),
        ])));
        // Destroyed ships' weapons are ignored.
        assert!(mutually_disarmed(&test_snapshot(vec![
            test_ship(1, "player", false, false),
            test_ship(3, "scripted", false, false),
            test_ship(4, "scripted", true, true),
        ])));
        // One side annihilated -> engine terminal handles it, not disarm.
        assert!(!mutually_disarmed(&test_snapshot(vec![
            test_ship(1, "player", false, false),
            test_ship(3, "scripted", true, false),
        ])));
    }

    #[test]
    fn stalemate_adjudication_follows_margin_sign_under_damage_diff() {
        use ScenarioStatus::*;
        let dd = StalemateScoring::DamageDiff;
        assert_eq!(adjudicate_stalemate(InProgress, dd, Some(12)), Some(Won));
        assert_eq!(adjudicate_stalemate(InProgress, dd, Some(-3)), Some(Lost));
        assert_eq!(adjudicate_stalemate(InProgress, dd, Some(0)), None);
        assert_eq!(adjudicate_stalemate(InProgress, dd, None), None);
        assert_eq!(
            adjudicate_stalemate(InProgress, StalemateScoring::None, Some(12)),
            None
        );
        assert_eq!(adjudicate_stalemate(Won, dd, Some(12)), None);
    }

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
        let config = MatchConfig::from_scenario(
            PathBuf::from("scenarios/simulation_duel.toml"),
            41,
            "test_rejecting_policy".into(),
            "test_rejecting_policy".into(),
            10,
            100,
        );
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
