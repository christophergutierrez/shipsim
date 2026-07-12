use std::collections::BTreeMap;
use std::path::PathBuf;

use shipsim_core::movement::{apply_order, Order, OrderError};
use shipsim_core::scenario::load_scenario;
use shipsim_core::snapshot::StateSnapshot;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn load_combat() -> shipsim_core::game_state::GameState {
    load_scenario(&manifest_path("scenarios/combat.toml")).expect("combat loads")
}

fn weapon_charge(id: &str, charge: u32) -> BTreeMap<String, u32> {
    BTreeMap::from([(id.to_string(), charge)])
}

#[test]
fn test_allocate_rejects_over_power_without_mutation() {
    let mut game = load_combat();
    let before = StateSnapshot::from_game_state(&game);

    let err = apply_order(
        &mut game,
        Order::Allocate {
            ship: 1,
            movement: 22,
            weapons: weapon_charge("beam_1", 1),
            shields: [0; 6],
        },
    )
    .expect_err("over-allocation rejects");

    assert!(matches!(err, OrderError::OverAllocated { ship: 1, .. }));
    let after = StateSnapshot::from_game_state(&game);
    assert_eq!(after.phase, before.phase);
    assert_eq!(after.ships[0].movement_allocated, 0);
}

#[test]
fn test_allocate_all_living_ships_enters_movement_phase() {
    let mut game = load_combat();

    apply_order(
        &mut game,
        Order::Allocate {
            ship: 1,
            movement: 6,
            weapons: weapon_charge("beam_1", 2),
            shields: [1, 1, 1, 1, 1, 1],
        },
    )
    .expect("ship 1 allocation");
    let mid = StateSnapshot::from_game_state(&game);
    assert_eq!(mid.phase, "allocate");

    apply_order(
        &mut game,
        Order::Allocate {
            ship: 2,
            movement: 3,
            weapons: weapon_charge("beam_1", 1),
            shields: [1, 1, 1, 1, 1, 0],
        },
    )
    .expect("ship 2 allocation");

    let snapshot = StateSnapshot::from_game_state(&game);
    assert_eq!(snapshot.phase, "movement");
    assert_eq!(snapshot.movement_phase, 1);
    assert_eq!(snapshot.ships[0].movement_allocated, 6);
    assert_eq!(snapshot.ships[0].shields_powered, [1, 1, 1, 1, 1, 1]);
    assert_eq!(snapshot.ships[0].shields_remaining, [1, 1, 1, 1, 1, 1]);
    let beam = snapshot.ships[0]
        .weapons
        .iter()
        .find(|weapon| weapon.id == "beam_1")
        .expect("beam snapshot");
    assert_eq!(beam.kind, "Beam");
    assert_eq!(beam.mount.as_deref(), Some("forward"));
    assert_eq!(beam.max_charge, 4);
    assert_eq!(beam.charge, 2);
}
