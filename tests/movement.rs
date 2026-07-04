use std::collections::HashSet;
use std::path::PathBuf;

use shipsim_core::board::Board;
use shipsim_core::game_state::{GameState, ScenarioStatus};
use shipsim_core::hex::Hex;
use shipsim_core::movement::{declare, resolve, Order, OrderError};
use shipsim_core::scenario::load_scenario;
use shipsim_core::ship::Ship;
use shipsim_core::snapshot::StateSnapshot;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn two_ship_state() -> GameState {
    GameState::new(
        Board::new(5, 5),
        vec![
            Ship {
                id: 1,
                class: "Heavy Cruiser".to_string(),
                pos: Hex::new(0, 0),
                facing: 0,
                speed_max: 2,
                turn_mode: 2,
            },
            Ship {
                id: 2,
                class: "Escort".to_string(),
                pos: Hex::new(2, 0),
                facing: 3,
                speed_max: 2,
                turn_mode: 1,
            },
        ],
        Hex::new(4, 4),
    )
}

#[test]
fn test_hex_distance_and_neighbors() {
    let origin = Hex::ORIGIN;
    assert_eq!(origin.distance(Hex::new(2, -1)), 2);
    assert_eq!(
        origin.distance(Hex::new(3, -2)),
        Hex::new(3, -2).distance(origin)
    );

    let neighbors = origin.neighbors();
    let unique: HashSet<_> = neighbors.iter().copied().collect();
    assert_eq!(neighbors.len(), 6);
    assert_eq!(unique.len(), 6);
    for neighbor in neighbors {
        assert_eq!(origin.distance(neighbor), 1);
    }
}

#[test]
fn test_declare_then_resolve() {
    let mut game = two_ship_state();
    let before = StateSnapshot::from_game_state(&game);

    let declared = declare(
        &game,
        Order::Move {
            ship: 1,
            to: Hex::new(1, 0),
        },
    )
    .expect("declaration succeeds");

    let after_declare = StateSnapshot::from_game_state(&game);
    assert_eq!(
        serde_json::to_value(before).unwrap(),
        serde_json::to_value(after_declare).unwrap()
    );

    resolve(&mut game, declared);
    assert_eq!(game.ship(1).unwrap().pos, Hex::new(1, 0));
}

#[test]
fn test_over_speed_rejected_without_mutation() {
    let mut game = two_ship_state();
    game.apply_order(Order::Move {
        ship: 1,
        to: Hex::new(1, 0),
    })
    .unwrap();
    game.apply_order(Order::Move {
        ship: 1,
        to: Hex::new(1, 1),
    })
    .unwrap();

    let before = StateSnapshot::from_game_state(&game);
    let err = game
        .apply_order(Order::Move {
            ship: 1,
            to: Hex::new(0, 1),
        })
        .expect_err("third move exceeds speed 2");

    assert_eq!(
        err,
        OrderError::BeyondSpeed {
            ship: 1,
            speed_max: 2,
            attempted: 3,
        }
    );
    assert_eq!(
        serde_json::to_value(before).unwrap(),
        serde_json::to_value(StateSnapshot::from_game_state(&game)).unwrap()
    );
}

#[test]
fn test_off_map_rejected_without_mutation() {
    let mut game = two_ship_state();
    let before = StateSnapshot::from_game_state(&game);

    let err = game
        .apply_order(Order::Move {
            ship: 1,
            to: Hex::new(-1, 0),
        })
        .expect_err("off-map destination rejected");

    assert_eq!(err, OrderError::OffMap { q: -1, r: 0 });
    assert_eq!(
        serde_json::to_value(before).unwrap(),
        serde_json::to_value(StateSnapshot::from_game_state(&game)).unwrap()
    );
}

#[test]
fn test_occupied_hex_rejected_without_mutation() {
    let mut game = two_ship_state();
    let before = StateSnapshot::from_game_state(&game);

    let err = game
        .apply_order(Order::Move {
            ship: 1,
            to: Hex::new(2, 0),
        })
        .expect_err("occupied destination rejected");

    assert_eq!(err, OrderError::HexOccupied { q: 2, r: 0 });
    assert_eq!(
        serde_json::to_value(before).unwrap(),
        serde_json::to_value(StateSnapshot::from_game_state(&game)).unwrap()
    );
}

#[test]
fn test_non_adjacent_rejected_without_mutation() {
    let mut game = two_ship_state();
    let before = StateSnapshot::from_game_state(&game);

    let err = game
        .apply_order(Order::Move {
            ship: 1,
            to: Hex::new(0, 2),
        })
        .expect_err("non-adjacent destination rejected");

    assert_eq!(err, OrderError::NotAdjacent { q: 0, r: 2 });
    assert_eq!(
        serde_json::to_value(before).unwrap(),
        serde_json::to_value(StateSnapshot::from_game_state(&game)).unwrap()
    );
}

#[test]
fn test_turn_mode_carried_and_unenforced() {
    let scenario_path = manifest_path("scenarios/tracer.toml");
    let mut game = load_scenario(&scenario_path).expect("tracer scenario loads");

    game.apply_order(Order::Face { ship: 1, facing: 5 })
        .expect("turn mode is not enforced in slice 1");
    game.apply_order(Order::Face { ship: 1, facing: 0 })
        .expect("free facing changes remain legal");

    let snapshot = serde_json::to_value(StateSnapshot::from_game_state(&game)).unwrap();
    assert_eq!(snapshot["ships"][0]["turn_mode"], 2);
    assert_eq!(snapshot["ships"][0]["facing"], 0);
    assert_eq!(snapshot["status"], "InProgress");
}

#[test]
fn test_end_turn_resets_speed_tracking() {
    let mut game = two_ship_state();
    game.apply_order(Order::Move {
        ship: 1,
        to: Hex::new(1, 0),
    })
    .unwrap();
    game.apply_order(Order::Move {
        ship: 1,
        to: Hex::new(1, 1),
    })
    .unwrap();

    game.apply_order(Order::EndTurn).unwrap();
    game.apply_order(Order::Move {
        ship: 1,
        to: Hex::new(0, 1),
    })
    .expect("new turn has a fresh speed budget");

    assert_eq!(game.turn.number(), 2);
    assert_eq!(game.status, ScenarioStatus::InProgress);
}

#[test]
fn test_scripted_ship_follows_waypoints() {
    let scenario_path = manifest_path("scenarios/movement.toml");
    let mut game = load_scenario(&scenario_path).expect("movement scenario loads");

    println!(
        "turn {} scripted {:?}",
        game.turn.number(),
        game.ship(2).unwrap().pos
    );
    assert_eq!(game.ship(2).unwrap().pos, Hex::new(3, 0));
    game.apply_order(Order::EndTurn).unwrap();
    println!(
        "turn {} scripted {:?}",
        game.turn.number(),
        game.ship(2).unwrap().pos
    );
    assert_eq!(game.ship(2).unwrap().pos, Hex::new(3, 1));
    game.apply_order(Order::EndTurn).unwrap();
    println!(
        "turn {} scripted {:?}",
        game.turn.number(),
        game.ship(2).unwrap().pos
    );
    assert_eq!(game.ship(2).unwrap().pos, Hex::new(3, 2));
    game.apply_order(Order::EndTurn).unwrap();
    println!(
        "turn {} scripted {:?}",
        game.turn.number(),
        game.ship(2).unwrap().pos
    );
    assert_eq!(
        game.ship(2).unwrap().pos,
        Hex::new(3, 2),
        "scripted ship remains at the final waypoint"
    );
}

#[test]
fn test_turn_container_advances_via_policy() {
    let scenario_path = manifest_path("scenarios/movement.toml");
    let mut game = load_scenario(&scenario_path).expect("movement scenario loads");

    game.apply_order(Order::Move {
        ship: 1,
        to: Hex::new(1, 0),
    })
    .expect("player move is resolved before turn advance");
    game.apply_order(Order::EndTurn)
        .expect("turn policy advances scripted ship");

    assert_eq!(game.turn.number(), 2);
    assert_eq!(game.ship(1).unwrap().pos, Hex::new(1, 0));
    assert_eq!(game.ship(2).unwrap().pos, Hex::new(3, 1));
}

#[test]
fn test_run_without_objective_stays_in_progress() {
    let scenario_path = manifest_path("scenarios/movement.toml");
    let mut game = load_scenario(&scenario_path).expect("movement scenario loads");

    game.apply_order(Order::EndTurn).unwrap();
    game.apply_order(Order::EndTurn).unwrap();

    let snapshot = serde_json::to_value(StateSnapshot::from_game_state(&game)).unwrap();
    assert_eq!(snapshot["status"], "InProgress");
    assert_eq!(game.status, ScenarioStatus::InProgress);
}
