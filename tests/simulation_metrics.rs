//! Simulation metrics still record under protocol 3 maneuvers.
use std::collections::BTreeMap;
use std::path::PathBuf;

use shipsim_core::game_state::{GameState, Phase, ScenarioStatus};
use shipsim_core::motion::Maneuver;
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario;
use shipsim_core::simulation::metrics::{AggregateMetrics, MatchMetrics};
use shipsim_core::snapshot::StateSnapshot;

fn path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn allocate_all(game: &mut GameState) {
    let ids: Vec<u32> = game.ships().iter().map(|s| s.id).collect();
    for id in ids {
        apply_order(
            game,
            Order::Allocate {
                ship: id,
                movement: 8,
                weapons: BTreeMap::new(),
                shields: [0; 6],
            },
        )
        .unwrap();
    }
}

fn resolve_movement_phase(
    game: &mut GameState,
    special: (u32, Maneuver),
) -> (StateSnapshot, StateSnapshot, BTreeMap<u32, Maneuver>) {
    let before = StateSnapshot::from_game_state(game);
    let mut maneuvers = BTreeMap::new();
    let ids: Vec<u32> = game
        .ships()
        .iter()
        .filter(|s| !s.destroyed)
        .map(|s| s.id)
        .collect();
    for id in ids {
        let maneuver = if id == special.0 {
            special.1
        } else {
            Maneuver::Coast
        };
        maneuvers.insert(id, maneuver);
        apply_order(
            game,
            Order::CommitManeuver {
                ship: id,
                maneuver,
            },
        )
        .unwrap();
    }
    (
        before,
        StateSnapshot::from_game_state(game),
        maneuvers,
    )
}

#[test]
fn metrics_record_accel_and_turn() {
    let mut game = load_scenario(&path("scenarios/v2_duel.toml")).unwrap();
    allocate_all(&mut game);
    assert_eq!(game.phase(), Phase::Movement);
    let mut metrics = MatchMetrics::default();

    let face = game.ship(1).unwrap().facing;
    let (before, after, maneuvers) =
        resolve_movement_phase(&mut game, (1, Maneuver::Turn { facing: (face + 1) % 6 }));
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
    assert!(metrics.facing_rotations >= 1 || metrics.course_changes >= 1);
    assert_eq!(after.phase, "firing");
}

#[test]
fn aggregate_keeps_engine_termination_separate_from_decided_equivalent_rate() {
    let mut capped_with_margin = MatchMetrics::default();
    capped_with_margin.undecided_margin = Some(12);
    capped_with_margin.closest_approach = Some(4);
    capped_with_margin.turns_in_weapon_range = 3;
    let mut capped_tie = MatchMetrics::default();
    capped_tie.undecided_margin = Some(0);
    capped_tie.closest_approach = Some(8);
    capped_tie.turns_in_weapon_range = 0;
    let terminal = MatchMetrics::default();

    let statuses = [
        (ScenarioStatus::Won, terminal),
        (ScenarioStatus::InProgress, capped_with_margin),
        (ScenarioStatus::InProgress, capped_tie),
    ];
    let aggregate = AggregateMetrics::from_matches(
        statuses
            .iter()
            .map(|(status, metrics)| (status, metrics)),
    );

    assert_eq!(aggregate.matches, 3);
    assert_eq!(aggregate.capped_matches, 2);
    assert_eq!(aggregate.decided_equivalent_matches, 2);
    assert!((aggregate.termination_rate - (1.0 / 3.0)).abs() < f64::EPSILON);
    assert!((aggregate.decided_equivalent_rate - (2.0 / 3.0)).abs() < f64::EPSILON);
    assert_eq!(aggregate.closest_approach_distribution.get(&4), Some(&1));
    assert_eq!(aggregate.turns_in_weapon_range_distribution.get(&0), Some(&2));
}
