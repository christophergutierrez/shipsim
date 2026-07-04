use std::path::PathBuf;

use serde_json::Value;
use shipsim_core::game_state::{GameState, ScenarioStatus};
use shipsim_core::hex::Hex;
use shipsim_core::movement::Order;
use shipsim_core::scenario::load_scenario;
use shipsim_core::snapshot::StateSnapshot;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn load_slice1() -> GameState {
    load_scenario(&manifest_path("scenarios/slice1.toml")).expect("slice1 scenario loads")
}

fn move_ship(game: &mut GameState, q: i32, r: i32) {
    game.apply_order(Order::Move {
        ship: 1,
        to: Hex::new(q, r),
    })
    .expect("player route uses legal adjacent movement");
}

fn run_winning_route() -> StateSnapshot {
    let mut game = load_slice1();

    assert_eq!(Hex::new(0, 0).distance(Hex::new(4, 1)), 5);
    move_ship(&mut game, 1, 0);
    move_ship(&mut game, 2, 0);
    move_ship(&mut game, 3, 0);
    move_ship(&mut game, 4, 0);
    assert_eq!(game.status, ScenarioStatus::InProgress);

    game.apply_order(Order::EndTurn)
        .expect("turn boundary resets movement budget");
    move_ship(&mut game, 4, 1);

    StateSnapshot::from_game_state(&game)
}

#[test]
fn test_player_reaches_objective_wins() {
    let snapshot = run_winning_route();
    let json = serde_json::to_value(&snapshot).expect("snapshot serializes");

    println!("{}", serde_json::to_string(&snapshot).unwrap());
    assert_eq!(json["turn"], 2);
    assert_eq!(json["status"], "Won");
    assert_eq!(json["objective"]["q"], 4);
    assert_eq!(json["objective"]["r"], 1);
    assert_eq!(json["ships"][0]["q"], 4);
    assert_eq!(json["ships"][0]["r"], 1);
}

#[test]
fn test_non_winning_run_stays_in_progress() {
    let mut game = load_slice1();

    move_ship(&mut game, 1, 0);
    game.apply_order(Order::EndTurn).unwrap();

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
