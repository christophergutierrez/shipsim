use std::path::PathBuf;

use serde_json::Value;
use shipsim_core::game_state::{GameState, ScenarioStatus};
use shipsim_core::hex::Hex;
use shipsim_core::impulse::{move_count, moves_on_impulse};
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario;
use shipsim_core::snapshot::StateSnapshot;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn load_slice1() -> GameState {
    load_scenario(&manifest_path("scenarios/slice1.toml")).expect("slice1 scenario loads")
}

fn run_winning_route() -> StateSnapshot {
    let mut game = load_slice1();

    assert_eq!(Hex::new(0, 0).distance(Hex::new(4, 1)), 5);

    // Turn 1: plot four hexes along +q (speed 4).
    apply_order(&mut game, Order::Plot {
        ship: 1,
        path: vec![
            Hex::new(1, 0),
            Hex::new(2, 0),
            Hex::new(3, 0),
            Hex::new(4, 0),
        ],
    })
    .expect("plot is legal");
    apply_order(&mut game, Order::RunTurn).expect("run turn 1");
    assert_eq!(game.status, ScenarioStatus::InProgress);
    assert_eq!(game.ship(1).unwrap().pos, Hex::new(4, 0));

    // Turn 2: final step onto objective.
    apply_order(&mut game, Order::Plot {
        ship: 1,
        path: vec![Hex::new(4, 1)],
    })
    .expect("final plot is legal");
    apply_order(&mut game, Order::RunTurn).expect("run turn 2");

    StateSnapshot::from_game_state(&game)
}

#[test]
fn test_player_reaches_objective_wins() {
    let snapshot = run_winning_route();
    let json = serde_json::to_value(&snapshot).expect("snapshot serializes");

    println!("{}", serde_json::to_string(&snapshot).unwrap());
    // Started turn 1; two RunTurns -> turn 3.
    assert_eq!(json["turn"], 3);
    assert_eq!(json["impulse"], 0);
    assert_eq!(json["status"], "Won");
    assert_eq!(json["objective"]["q"], 4);
    assert_eq!(json["objective"]["r"], 1);
    assert_eq!(json["ships"][0]["q"], 4);
    assert_eq!(json["ships"][0]["r"], 1);
}

#[test]
fn test_non_winning_run_stays_in_progress() {
    let mut game = load_slice1();

    apply_order(&mut game, Order::Plot {
        ship: 1,
        path: vec![Hex::new(1, 0)],
    })
    .unwrap();
    apply_order(&mut game, Order::RunTurn).unwrap();

    let snapshot = serde_json::to_value(StateSnapshot::from_game_state(&game)).unwrap();
    assert_eq!(snapshot["status"], "InProgress");
    assert_eq!(game.status, ScenarioStatus::InProgress);
}

#[test]
fn test_run_is_reproducible() {
    let first = serde_json::to_value(run_winning_route()).unwrap();
    let second = serde_json::to_value(run_winning_route()).unwrap();

    let _: Value = first.clone();
    assert_eq!(first, second);
}

#[test]
fn test_impulse_turn_end_positions() {
    let mut game =
        load_scenario(&manifest_path("scenarios/impulse.toml")).expect("impulse scenario loads");

    let ship1_speed = game.ship(1).unwrap().speed as u8;
    let ship2_speed = game.ship(2).unwrap().speed as u8;
    assert_eq!(ship1_speed, 4);
    assert_eq!(ship2_speed, 3);
    assert_eq!(move_count(ship1_speed), 4);
    assert_eq!(move_count(ship2_speed), 3);

    apply_order(&mut game, Order::Plot {
        ship: 1,
        path: vec![
            Hex::new(1, 5),
            Hex::new(2, 5),
            Hex::new(3, 5),
            Hex::new(4, 5),
        ],
    })
    .expect("ship 1 plot");
    apply_order(&mut game, Order::Plot {
        ship: 2,
        path: vec![Hex::new(1, 0), Hex::new(2, 0), Hex::new(3, 0)],
    })
    .expect("ship 2 plot");
    apply_order(&mut game, Order::RunTurn).expect("run turn");

    assert_eq!(game.ship(1).unwrap().pos, Hex::new(4, 5));
    assert_eq!(game.ship(2).unwrap().pos, Hex::new(3, 0));
    assert_eq!(game.impulse, 0);
    assert_eq!(game.status, ScenarioStatus::InProgress);

    // IMC anchors used by the turn (not observed mid-turn, but pinned).
    for impulse in 1u8..=32 {
        if moves_on_impulse(ship1_speed, impulse) {
            // speed 4 has exactly 4 move impulses
        }
    }
    assert_eq!(
        (1u8..=32)
            .filter(|&i| moves_on_impulse(ship1_speed, i))
            .count(),
        4
    );
    assert_eq!(
        (1u8..=32)
            .filter(|&i| moves_on_impulse(ship2_speed, i))
            .count(),
        3
    );
}
