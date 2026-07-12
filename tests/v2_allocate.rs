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
    assert_eq!(after.move_order, before.move_order);
    assert_eq!(after.ships[0].movement_allocated, 0);
    assert_eq!(after.ships[0].move_remaining, 0);
}

#[test]
fn test_allocate_all_living_ships_enters_movement_with_move_order() {
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
    assert!(mid.move_order.is_empty());

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
    // M3: move_order is sorted by thrust_remaining descending. Ship 2 (escort,
    // 3 power × 4 = 12 thrust) moves before ship 1 (cruiser, 6 power × 1 = 6).
    assert_eq!(snapshot.move_order, vec![2, 1]);
    assert_eq!(snapshot.ships[0].movement_allocated, 6);
    assert_eq!(snapshot.ships[0].move_remaining, 6);
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

#[test]
fn test_equal_movement_tie_is_frozen_after_allocation() {
    let mut game = load_combat();

    for ship in [1, 2] {
        apply_order(
            &mut game,
            Order::Allocate {
                ship,
                movement: 4,
                weapons: weapon_charge("beam_1", 1),
                shields: [0; 6],
            },
        )
        .expect("allocation");
    }

    let first = StateSnapshot::from_game_state(&game).move_order;
    let second = StateSnapshot::from_game_state(&game).move_order;
    assert!(first == vec![1, 2] || first == vec![2, 1]);
    assert_eq!(second, first);
}

#[test]
fn test_three_or_more_equal_movement_ties_use_prng_order() {
    let mut game =
        load_scenario(&manifest_path("scenarios/fleet.toml")).expect("fleet scenario loads");

    for ship in [1, 2, 3, 4] {
        apply_order(
            &mut game,
            Order::Allocate {
                ship,
                movement: 4,
                weapons: weapon_charge("beam_1", 1),
                shields: [0; 6],
            },
        )
        .expect("allocation");
    }

    let order = StateSnapshot::from_game_state(&game).move_order;
    assert_ne!(order, vec![1, 2, 3, 4]);
    let mut sorted = order.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, vec![1, 2, 3, 4]);
}
