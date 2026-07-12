//! M0: Characterization tests for the current (pre-inertia) movement model.
//!
//! These tests pin down the behavior that the inertial movement plan
//! (docs/MOVEMENT-INERTIA-PLAN.md) will replace. They serve as a regression
//! baseline: when the legacy movement path is deleted in M4, these tests
//! will be removed or rewritten for the new model.

use std::collections::BTreeMap;
use std::path::PathBuf;

use shipsim_core::game_state::GameState;
use shipsim_core::hex::Hex;
use shipsim_core::movement::{apply_order, MoveMode, Order};
use shipsim_core::scenario::load_scenario;

/// Resolve a path relative to the workspace manifest root.
fn manifest_path(relative: &str) -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(manifest_dir).join(relative)
}

/// Load the standard v2 duel scenario used across the test suite.
fn v2_duel() -> GameState {
    load_scenario(&manifest_path("scenarios/v2_duel.toml")).expect("v2_duel loads")
}

#[test]
fn m0_allocate_writes_movement_power() {
    // Current model: allocate writes movement_allocated, not thrust_remaining.
    let mut game = v2_duel();
    let ship_id = game.ships().first().unwrap().id;

    let order = Order::Allocate {
        ship: ship_id,
        movement: 2,
        weapons: BTreeMap::new(),
        shields: [0; 6],
    };
    apply_order(&mut game, order).unwrap();

    let ship = game.ship(ship_id).unwrap();
    assert_eq!(ship.movement_allocated, 2);
    assert_eq!(ship.move_remaining, 2);
    // No velocity or thrust fields exist yet in the current model.
}

/// Allocate movement for a single ship with no weapons or shields.
fn allocate(game: &mut GameState, ship: u32, movement: u32) {
    apply_order(
        game,
        Order::Allocate {
            ship,
            movement,
            weapons: BTreeMap::new(),
            shields: [0; 6],
        },
    )
    .expect("allocate");
}

/// Allocate movement for both ships so the phase advances to movement.
fn allocate_both(game: &mut GameState, movement_a: u32, movement_b: u32) {
    let ship1 = game.ships().first().unwrap().id;
    let ship2 = game.ships().get(1).unwrap().id;
    allocate(game, ship1, movement_a);
    allocate(game, ship2, movement_b);
}

#[test]
fn m0_move_consumes_move_remaining() {
    // Current model: each Forward move spends 1 move_remaining.
    let mut game = v2_duel();
    let ship_id = game.ships().first().unwrap().id;

    allocate_both(&mut game, 2, 0);

    let before = game.ship(ship_id).unwrap().move_remaining;
    apply_order(
        &mut game,
        Order::Move {
            ship: ship_id,
            mode: MoveMode::Forward,
        },
    )
    .unwrap();
    let after = game.ship(ship_id).unwrap().move_remaining;
    assert_eq!(before - after, 1);
}

#[test]
fn m0_initiative_derived_from_movement_allocated() {
    // Current model: build_v2_move_order sorts by movement_allocated descending.
    // The first active mover is the ship with the most movement allocated.
    let mut game = v2_duel();
    let ship1 = game.ships().first().unwrap().id;
    let ship2 = game.ships().get(1).unwrap().id;

    // Ship 1 allocates 3 movement, ship 2 allocates 1.
    apply_order(
        &mut game,
        Order::Allocate {
            ship: ship1,
            movement: 3,
            weapons: BTreeMap::new(),
            shields: [0; 6],
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::Allocate {
            ship: ship2,
            movement: 1,
            weapons: BTreeMap::new(),
            shields: [0; 6],
        },
    )
    .unwrap();

    // M3: initiative is now derived from thrust_remaining, not raw
    // movement_allocated. Ship 1 (heavy_cruiser, 1:1) gets 3 thrust from 3
    // power; ship 2 (escort, 4:1) gets 4 thrust from 1 power. Ship 2 moves
    // first.
    let active = game.active_v2_mover();
    assert_eq!(active, Some(ship2));
}

#[test]
fn m0_turn_loop_reenters_movement_when_move_remaining() {
    // Current model: resolve_fire_phase_v2 re-enters movement when
    // can_any_move() is true. This is the heuristic that the inertial model
    // replaces with a fixed four-phase schedule.
    let mut game = v2_duel();
    let ship_id = game.ships().first().unwrap().id;

    apply_order(
        &mut game,
        Order::Allocate {
            ship: ship_id,
            movement: 2,
            weapons: BTreeMap::new(),
            shields: [0; 6],
        },
    )
    .unwrap();

    // After allocation, the ship can move — can_any_move should be true.
    assert!(game.can_any_move());
}

#[test]
fn m0_keel_tracks_last_move_direction() {
    // Current model: keel records the last movement direction for momentum.
    let mut game = v2_duel();
    let ship_id = game.ships().first().unwrap().id;

    allocate_both(&mut game, 2, 0);

    let keel_before = game.ship(ship_id).unwrap().keel;
    apply_order(
        &mut game,
        Order::Move {
            ship: ship_id,
            mode: MoveMode::Forward,
        },
    )
    .unwrap();
    let keel_after = game.ship(ship_id).unwrap().keel;
    // Keel changes after a forward move.
    assert_ne!(keel_before, keel_after);
}

#[test]
fn m0_protocol_version_is_1() {
    // Current model: protocol version is 1. The inertial plan increments to 2.
    assert_eq!(shipsim_core::protocol::PROTOCOL_VERSION, 1);
}

#[test]
fn m0_save_version_is_1() {
    // Current model: save document version is 1.
    let game = v2_duel();
    let doc = shipsim_core::save::SaveDocument::capture(
        manifest_path("scenarios/v2_duel.toml"),
        Vec::new(),
        &game,
    );
    assert_eq!(doc.protocol_version, 1);
}

/// Load the M0 floating-map scenario (small board, off-center formation).
fn m0_floating() -> GameState {
    load_scenario(&manifest_path("scenarios/m0_floating.toml")).expect("m0_floating loads")
}

#[test]
fn m0_floating_map_recenters_after_each_move() {
    // Current model: maybe_float_recenter runs after every single move and
    // translates ALL ships so the formation bounding box is centered on the
    // board. The inertial plan changes this to recenter once after the batch.
    //
    // Setup: 12x10 floating board, ship 1 at (1,4) facing 0, ship 2 at (2,6).
    let mut game = m0_floating();
    let ship1 = game.ships().first().unwrap().id;
    let ship2 = game.ships().get(1).unwrap().id;

    // Pin exact pre-move positions.
    assert_eq!(game.ship(ship1).unwrap().pos, Hex::new(1, 4));
    assert_eq!(game.ship(ship2).unwrap().pos, Hex::new(2, 6));

    allocate_both(&mut game, 2, 0);

    // Move ship 1 forward (facing 0 = +1,0): (1,4) -> (2,4).
    // After the move, maybe_float_recenter centers the formation.
    // Post-move positions before recenter: ship1=(2,4), ship2=(2,6).
    // Span q:[2,2] r:[4,6]. float_delta:
    //   target_min_q = (12-0)/2 = 6, target_min_r = (10-2)/2 = 4
    //   delta = (6-2, 4-4) = (4, 0)
    // Both ships shift by (+4, 0): ship1 -> (6,4), ship2 -> (6,6).
    apply_order(
        &mut game,
        Order::Move {
            ship: ship1,
            mode: MoveMode::Forward,
        },
    )
    .unwrap();
    assert_eq!(game.ship(ship1).unwrap().pos, Hex::new(6, 4));
    assert_eq!(game.ship(ship2).unwrap().pos, Hex::new(6, 6));
}
