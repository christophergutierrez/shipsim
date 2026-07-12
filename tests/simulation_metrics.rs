use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use shipsim_core::game_state::GameState;
use shipsim_core::motion::Maneuver;
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario;
use shipsim_core::simulation::metrics::MatchMetrics;
use shipsim_core::simulation::{run_match, MatchConfig};
use shipsim_core::snapshot::StateSnapshot;

fn path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn allocate_all(game: &mut GameState) {
    let ids: Vec<u32> = game
        .ships()
        .iter()
        .filter(|ship| !ship.destroyed)
        .map(|ship| ship.id)
        .collect();
    for ship in ids {
        apply_order(
            game,
            Order::Allocate {
                ship,
                movement: 4,
                weapons: BTreeMap::new(),
                shields: [0; 6],
            },
        )
        .unwrap();
    }
}

fn resolve_movement_phase(
    game: &mut GameState,
    first: (u32, Maneuver),
) -> (StateSnapshot, StateSnapshot, BTreeMap<u32, Maneuver>) {
    let before = StateSnapshot::from_game_state(game);
    let mut maneuvers = BTreeMap::new();
    maneuvers.insert(first.0, first.1);
    apply_order(
        game,
        Order::CommitManeuver {
            ship: first.0,
            maneuver: first.1,
        },
    )
    .unwrap();
    for ship in before
        .ships
        .iter()
        .filter(|ship| ship.id != first.0 && !ship.destroyed)
    {
        maneuvers.insert(ship.id, Maneuver::Coast);
        apply_order(
            game,
            Order::CommitManeuver {
                ship: ship.id,
                maneuver: Maneuver::Coast,
            },
        )
        .unwrap();
    }
    (before, StateSnapshot::from_game_state(game), maneuvers)
}

#[test]
fn metrics_count_legal_reversal_at_resolution_boundary() {
    let mut game = load_scenario(&path("scenarios/m4_movement.toml")).unwrap();
    allocate_all(&mut game);
    let mut metrics = MatchMetrics::default();

    let (before, after, maneuvers) = resolve_movement_phase(&mut game, (2, Maneuver::Decelerate));
    for (id, maneuver) in &maneuvers {
        let ship = before.ships.iter().find(|ship| ship.id == *id).unwrap();
        metrics.record_maneuver(ship, *maneuver);
    }
    metrics.record_movement_resolution(
        &before,
        &after,
        &maneuvers,
        game.last_translation_outcomes(),
    );
    for id in after
        .ships
        .iter()
        .filter(|ship| !ship.destroyed)
        .map(|ship| ship.id)
    {
        apply_order(&mut game, Order::ReadyFire { ship: id }).unwrap();
    }

    let (before, after, maneuvers) =
        resolve_movement_phase(&mut game, (2, Maneuver::Accelerate { course: Some(3) }));
    for (id, maneuver) in &maneuvers {
        let ship = before.ships.iter().find(|ship| ship.id == *id).unwrap();
        metrics.record_maneuver(ship, *maneuver);
    }
    metrics.record_movement_resolution(
        &before,
        &after,
        &maneuvers,
        game.last_translation_outcomes(),
    );

    assert_eq!(metrics.reversals, 1);
    assert!(metrics.thrust_spent > 0);
    assert!(metrics.velocity_distribution.contains_key(&1));
}

#[test]
fn metrics_count_blocked_scheduled_translation_in_conflict_scenario() {
    let mut game = load_scenario(&path("scenarios/m4_translation.toml")).unwrap();
    allocate_all(&mut game);
    let before = StateSnapshot::from_game_state(&game);
    let mut maneuvers = BTreeMap::new();
    for ship in before.ships.iter().filter(|ship| !ship.destroyed) {
        maneuvers.insert(ship.id, Maneuver::Coast);
        apply_order(
            &mut game,
            Order::CommitManeuver {
                ship: ship.id,
                maneuver: Maneuver::Coast,
            },
        )
        .unwrap();
    }
    let after = StateSnapshot::from_game_state(&game);
    let mut metrics = MatchMetrics::default();
    metrics.record_movement_resolution(
        &before,
        &after,
        &maneuvers,
        game.last_translation_outcomes(),
    );
    assert!(metrics.scheduled_translations > 0);
    assert!(metrics.blocked_translations > 0);
}

#[test]
fn match_report_exposes_auditable_motion_metrics_without_initiative() {
    let result = run_match(&MatchConfig {
        scenario: path("scenarios/simulation_duel.toml"),
        seed: 23,
        player_policy: "random".into(),
        opponent_policy: "greedy".into(),
        max_turns: 20,
        max_orders: 20_000,
    })
    .unwrap();
    let json = serde_json::to_value(&result.metrics).unwrap();
    for field in [
        "velocity_distribution",
        "thrust_spent",
        "coasting_distance",
        "course_changes",
        "facing_rotations",
        "blocked_translations",
        "reversals",
        "scheduled_translations",
        "hull_efficiency",
    ] {
        assert!(json.get(field).is_some(), "missing metric {field}");
    }
    assert!(json.get("initiative").is_none());
    assert!(result.terminal_reason.is_some());
}
