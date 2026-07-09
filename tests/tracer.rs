use std::path::PathBuf;

use serde_json::Value;
use shipsim_core::hex::Hex;
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario;
use shipsim_core::snapshot::StateSnapshot;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

#[test]
fn test_snapshot_shape() {
    let scenario_path = manifest_path("scenarios/tracer.toml");
    let mut game = load_scenario(&scenario_path).expect("tracer scenario loads");

    apply_order(&mut game, Order::Plot {
        ship: 1,
        path: vec![Hex::new(1, 0)],
    })
    .expect("plot is valid");
    apply_order(&mut game, Order::RunTurn)
        .expect("run turn applies plot");

    let snapshot = StateSnapshot::from_game_state(&game);
    let json_line = serde_json::to_string(&snapshot).expect("snapshot serializes to JSON");
    println!("{json_line}");

    let json = serde_json::to_value(&snapshot).expect("snapshot serializes");

    assert_eq!(json["turn"], 2);
    assert_eq!(json["impulse"], 0);
    assert_eq!(json["status"], "InProgress");
    assert_eq!(json["map"]["width"], 4);
    assert_eq!(json["map"]["height"], 4);
    assert_eq!(json["objective"]["q"], 3);
    assert_eq!(json["objective"]["r"], 3);

    let ships = json["ships"].as_array().expect("ships is an array");
    assert_eq!(ships.len(), 1);

    let ship = &ships[0];
    assert_eq!(ship["id"], 1);
    assert_eq!(ship["class"], "Heavy Cruiser");
    assert_eq!(ship["q"], 1);
    assert_eq!(ship["r"], 0);
    assert_eq!(ship["facing"], 0);
    assert_eq!(ship["speed"], 4);
    assert_eq!(ship["power"], 8);
    assert_eq!(ship["turn_speed"], 4);
    assert_eq!(ship["turn_mode"], 2);

    let _: Value = json;
}
