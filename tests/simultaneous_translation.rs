//! M4: Simultaneous translation and destination-conflict resolution (ADR-0022).

use std::collections::BTreeMap;
use std::path::PathBuf;

use shipsim_core::board::Board;
use shipsim_core::game_state::GameState;
use shipsim_core::hex::Hex;
use shipsim_core::motion::Maneuver;
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn allocate_all(game: &mut GameState, ships: &[u32]) {
    for &ship in ships {
        apply_order(
            game,
            Order::Allocate {
                ship,
                movement: 0,
                weapons: BTreeMap::new(),
                shields: [0; 6],
            },
        )
        .expect("allocate");
    }
}

fn coast_all(game: &mut GameState, ships: &[u32]) {
    for &ship in ships {
        apply_order(
            game,
            Order::CommitManeuver {
                ship,
                maneuver: Maneuver::Coast,
            },
        )
        .expect("coast commits");
    }
}

#[test]
fn same_destination_attempts_block_all_contenders() {
    let mut game =
        load_scenario(&manifest_path("scenarios/m4_translation.toml")).expect("scenario loads");
    let ships: Vec<u32> = (1..=7).collect();
    allocate_all(&mut game, &ships);
    coast_all(&mut game, &ships);

    assert_eq!(game.ship(1).unwrap().pos, Hex::new(4, 5), "ship 1 blocked");
    assert_eq!(game.ship(2).unwrap().pos, Hex::new(6, 4), "ship 2 blocked");
    assert_eq!(
        game.ship(1).unwrap().velocity.speed,
        4,
        "blocked movement does not erase velocity"
    );
    assert_eq!(game.ship(2).unwrap().velocity.speed, 4);
}

#[test]
fn hex_swaps_and_path_crossings_succeed() {
    let mut game =
        load_scenario(&manifest_path("scenarios/m4_translation.toml")).expect("scenario loads");
    let ships: Vec<u32> = (1..=7).collect();
    allocate_all(&mut game, &ships);
    coast_all(&mut game, &ships);

    assert_eq!(
        game.ship(3).unwrap().pos,
        Hex::new(2, 1),
        "ship 3 swaps into ship 4's origin"
    );
    assert_eq!(
        game.ship(4).unwrap().pos,
        Hex::new(1, 1),
        "ship 4 swaps into ship 3's origin"
    );
}

#[test]
fn entry_into_non_departing_occupied_hex_blocks() {
    let mut game =
        load_scenario(&manifest_path("scenarios/m4_translation.toml")).expect("scenario loads");
    let ships: Vec<u32> = (1..=7).collect();
    allocate_all(&mut game, &ships);
    coast_all(&mut game, &ships);

    assert_eq!(
        game.ship(5).unwrap().pos,
        Hex::new(1, 3),
        "ship 5 blocked by stationary ship 6"
    );
    assert_eq!(
        game.ship(6).unwrap().pos,
        Hex::new(2, 3),
        "stationary ship 6 never moves"
    );
}

#[test]
fn hard_map_exit_blocks_without_changing_velocity() {
    let mut game =
        load_scenario(&manifest_path("scenarios/m4_translation.toml")).expect("scenario loads");
    let ships: Vec<u32> = (1..=7).collect();
    allocate_all(&mut game, &ships);
    coast_all(&mut game, &ships);

    let ship7 = game.ship(7).unwrap();
    assert_eq!(ship7.pos, Hex::new(9, 7), "hard-map exit blocks the move");
    assert_eq!(
        ship7.velocity.speed, 4,
        "velocity is retained on a blocked exit"
    );
    assert_eq!(
        ship7.velocity.course, 0,
        "course is retained on a blocked exit"
    );
}

#[test]
fn floating_map_recenters_exactly_once_after_the_batch() {
    let mut game =
        load_scenario(&manifest_path("scenarios/m4_floating.toml")).expect("scenario loads");
    let ships = [1, 2];
    allocate_all(&mut game, &ships);
    coast_all(&mut game, &ships);

    // Both ships translate east by one hex (velocity 4 translates every
    // phase) before any recenter is applied.
    let translated = [Hex::new(1, 0), Hex::new(1, 1)];
    let (dq, dr) = Board::float_delta(&translated, 6, 6);
    let expected: Vec<Hex> = translated
        .iter()
        .map(|h| Hex::new(h.q + dq, h.r + dr))
        .collect();

    // If recenter ran twice the shift would be doubled (or, if it ran zero
    // times, absent); either way these would no longer match a single
    // application of `Board::float_delta` over the post-translation batch.
    assert_eq!(game.ship(1).unwrap().pos, expected[0]);
    assert_eq!(game.ship(2).unwrap().pos, expected[1]);
}
