//! Simulation metrics still record under protocol 3 maneuvers.
use std::collections::BTreeMap;
use std::path::PathBuf;

use shipsim_core::game_state::{GameState, Phase};
use shipsim_core::motion::Maneuver;
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario;
use shipsim_core::simulation::metrics::MatchMetrics;
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
