use std::collections::BTreeMap;
use std::path::PathBuf;

use shipsim_core::hex::Hex;
use shipsim_core::movement::{apply_order, MoveMode, Order, OrderError};
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

fn allocate(game: &mut shipsim_core::game_state::GameState, ship: u32, movement: u32) {
    apply_order(
        game,
        Order::Allocate {
            ship,
            movement,
            weapons: weapon_charge("beam_1", 1),
            shields: [0; 6],
        },
    )
    .expect("allocate");
}

#[test]
fn test_v2_move_one_decision_per_ship_then_firing() {
    let mut game = load_combat();
    game.set_ship_pos(1, Hex::new(1, 1)).unwrap();
    game.set_ship_facing(1, 0).unwrap();
    game.set_ship_pos(2, Hex::new(3, 3)).unwrap();
    // M3: ship 1 (cruiser, 5 power × 1 = 5 thrust) is the active mover
    // before ship 2 (escort, 1 power × 4 = 4 thrust). Ship 1 moves first;
    // ship 2 is still pending so the phase stays in movement.
    allocate(&mut game, 1, 5);
    allocate(&mut game, 2, 1);

    apply_order(
        &mut game,
        Order::Move {
            ship: 1,
            mode: MoveMode::Forward,
        },
    )
    .expect("ship 1 moves");
    let after_first = StateSnapshot::from_game_state(&game);
    assert_eq!(after_first.phase, "movement");
    assert_eq!(after_first.ships_moved_this_phase, vec![1]);
    assert_eq!(after_first.ships[0].move_remaining, 4);
    assert_eq!(after_first.ships[0].keel, "forward");

    let err = apply_order(
        &mut game,
        Order::Move {
            ship: 1,
            mode: MoveMode::Forward,
        },
    )
    .expect_err("cannot move twice in one movement phase");
    assert!(matches!(err, OrderError::AlreadyMovedThisPhase(1)));

    apply_order(&mut game, Order::PassMove { ship: 2 }).expect("ship 2 passes");
    let snapshot = StateSnapshot::from_game_state(&game);
    assert_eq!(snapshot.phase, "firing");
    assert_eq!(snapshot.ships_moved_this_phase, Vec::<u32>::new());
}

#[test]
fn test_v2_zero_move_ship_auto_passes() {
    let mut game = load_combat();
    game.set_ship_pos(1, Hex::new(1, 1)).unwrap();
    game.set_ship_facing(1, 0).unwrap();
    game.set_ship_pos(2, Hex::new(3, 3)).unwrap();
    allocate(&mut game, 1, 1);
    allocate(&mut game, 2, 0);

    apply_order(
        &mut game,
        Order::Move {
            ship: 1,
            mode: MoveMode::Forward,
        },
    )
    .expect("ship 1 moves");

    assert_eq!(StateSnapshot::from_game_state(&game).phase, "firing");
}

#[test]
fn test_v2_all_zero_move_ships_auto_pass_to_firing() {
    let mut game = load_combat();

    allocate(&mut game, 1, 0);
    allocate(&mut game, 2, 0);

    let snapshot = StateSnapshot::from_game_state(&game);
    assert_eq!(snapshot.phase, "firing");
    assert_eq!(snapshot.ships_moved_this_phase, Vec::<u32>::new());
}

#[test]
fn test_v2_reverse_after_forward_requires_two_remaining() {
    let mut game = load_combat();
    game.set_ship_pos(1, Hex::new(1, 1)).unwrap();
    game.set_ship_facing(1, 0).unwrap();
    game.set_ship_pos(2, Hex::new(3, 3)).unwrap();

    // M3: ship 2 (escort, 1 power × 4 = 4 thrust) is the active mover before
    // ship 1 (cruiser, 2 power × 1 = 2 thrust). Pass ship 2 first so ship 1
    // can move while the phase stays in movement.
    allocate(&mut game, 1, 2);
    allocate(&mut game, 2, 1);

    apply_order(&mut game, Order::PassMove { ship: 2 }).expect("ship 2 passes");
    apply_order(
        &mut game,
        Order::Move {
            ship: 1,
            mode: MoveMode::Forward,
        },
    )
    .expect("forward costs one");

    // After ship 1 moves and ship 2 passes, the phase auto-advances to firing.
    // Start a new movement phase to test the reverse insufficiency.
    game.start_next_movement_phase_for_tests();
    // M3: ship 2 (4 thrust) is the active mover in the new phase; pass it first.
    apply_order(&mut game, Order::PassMove { ship: 2 }).expect("ship 2 passes");
    let err = apply_order(
        &mut game,
        Order::Move {
            ship: 1,
            mode: MoveMode::Reverse,
        },
    )
    .expect_err("reverse after forward needs two");
    assert!(
        matches!(
            err,
            OrderError::InsufficientMovePower {
                ship: 1,
                need: 2,
                have: 1
            }
        ),
        "expected InsufficientMovePower {{ ship: 1, need: 2, have: 1 }}, got {err:?}"
    );
}

#[test]
fn test_v2_reverse_after_forward_with_two_remaining_is_legal() {
    let mut game = load_combat();
    game.set_ship_pos(1, Hex::new(1, 1)).unwrap();
    game.set_ship_facing(1, 0).unwrap();
    game.set_ship_pos(2, Hex::new(3, 3)).unwrap();

    allocate(&mut game, 1, 3);
    allocate(&mut game, 2, 0);

    apply_order(
        &mut game,
        Order::Move {
            ship: 1,
            mode: MoveMode::Forward,
        },
    )
    .expect("forward costs one");

    game.start_next_movement_phase_for_tests();
    apply_order(
        &mut game,
        Order::Move {
            ship: 1,
            mode: MoveMode::Reverse,
        },
    )
    .expect("reverse costs two");

    let ship = StateSnapshot::from_game_state(&game)
        .ships
        .into_iter()
        .find(|ship| ship.id == 1)
        .unwrap();
    assert_eq!(ship.move_remaining, 0);
    assert_eq!(ship.keel, "reverse");
}

#[test]
fn test_v2_off_map_and_occupied_reject_without_spending_move() {
    let mut game = load_combat();
    game.set_ship_pos(1, Hex::new(3, 0)).unwrap();
    game.set_ship_facing(1, 0).unwrap();
    game.set_ship_pos(2, Hex::new(0, 0)).unwrap();
    allocate(&mut game, 1, 1);
    allocate(&mut game, 2, 0);

    let err = apply_order(
        &mut game,
        Order::Move {
            ship: 1,
            mode: MoveMode::Forward,
        },
    )
    .expect_err("off map");
    assert!(matches!(err, OrderError::OffMap { .. }));
    assert_eq!(
        StateSnapshot::from_game_state(&game).ships[0].move_remaining,
        1
    );

    let mut game = load_combat();
    game.set_ship_pos(1, Hex::new(1, 0)).unwrap();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, Hex::new(0, 0)).unwrap();
    allocate(&mut game, 1, 1);
    allocate(&mut game, 2, 0);

    let err = apply_order(
        &mut game,
        Order::Move {
            ship: 1,
            mode: MoveMode::Forward,
        },
    )
    .expect_err("occupied");
    assert!(matches!(err, OrderError::HexOccupied { .. }));
    assert_eq!(
        StateSnapshot::from_game_state(&game).ships[0].move_remaining,
        1
    );
}
