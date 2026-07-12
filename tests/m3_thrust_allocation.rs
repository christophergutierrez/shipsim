//! M3: Engine allocation becomes thrust (ADR-0022).
//!
//! Verifies that allocated engine power is converted into `thrust_remaining` via
//! the hull's rational conversion, that the legacy `move_remaining` field mirrors
//! it, that movement initiative and active-mover selection are based on thrust,
//! that over-allocation is rejected without mutating ship state, that a ship
//! cannot be allocated twice, and that a ship allocating zero engine power coasts
//! (preserves its persistent velocity).

use std::collections::BTreeMap;

use shipsim_core::movement::{apply_order, Order, OrderError};
use shipsim_core::scenario::load_scenario;

fn manifest_path(relative: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn empty_charges() -> BTreeMap<String, u32> {
    BTreeMap::new()
}

fn allocate(
    game: &mut shipsim_core::game_state::GameState,
    ship: u32,
    movement: u32,
) -> Result<(), OrderError> {
    apply_order(
        game,
        Order::Allocate {
            ship,
            movement,
            weapons: empty_charges(),
            shields: [0; 6],
        },
    )
}

/// Tiny hull (escort): thrust_per_power = 4, so 1 power → 4 thrust.
#[test]
fn m3_tiny_one_power_yields_four_thrust() {
    let mut game = load_scenario(&manifest_path("scenarios/m3_thrust.toml")).expect("m3 loads");
    let escort = game.ship(1).expect("escort present");
    let power = escort.effective_power();
    assert!(power >= 1, "escort needs at least 1 power, has {power}");

    allocate(&mut game, 1, 1).expect("allocate 1 power");

    let ship = game.ship(1).expect("escort present");
    assert_eq!(ship.thrust_remaining, 4, "1 power → 4 thrust for tiny hull");
    assert_eq!(
        ship.move_remaining, 4,
        "move_remaining mirrors thrust_remaining"
    );
    assert_eq!(
        ship.movement_allocated, 1,
        "movement_allocated records raw power"
    );
}

/// Cruiser hull: 1:1 conversion, so 1 power → 1 thrust.
#[test]
fn m3_cruiser_one_power_yields_one_thrust() {
    let mut game = load_scenario(&manifest_path("scenarios/m3_thrust.toml")).expect("m3 loads");
    allocate(&mut game, 2, 1).expect("allocate 1 power");

    let ship = game.ship(2).expect("cruiser present");
    assert_eq!(
        ship.thrust_remaining, 1,
        "1 power → 1 thrust for cruiser hull"
    );
    assert_eq!(
        ship.move_remaining, 1,
        "move_remaining mirrors thrust_remaining"
    );
}

/// Huge hull: power_per_thrust = 4, so 4 power → 1 thrust.
#[test]
fn m3_huge_four_power_yields_one_thrust() {
    let mut game = load_scenario(&manifest_path("scenarios/m3_thrust.toml")).expect("m3 loads");
    allocate(&mut game, 3, 4).expect("allocate 4 power");

    let ship = game.ship(3).expect("huge present");
    assert_eq!(ship.thrust_remaining, 1, "4 power → 1 thrust for huge hull");
    assert_eq!(
        ship.move_remaining, 1,
        "move_remaining mirrors thrust_remaining"
    );
}

/// Huge hull: 3 power is not enough for 1 thrust (3 < 4), so thrust = 0.
/// The remainder is truncated, not rounded.
#[test]
fn m3_huge_three_power_yields_zero_thrust_fractional_remainder() {
    let mut game = load_scenario(&manifest_path("scenarios/m3_thrust.toml")).expect("m3 loads");
    allocate(&mut game, 3, 3).expect("allocate 3 power");

    let ship = game.ship(3).expect("huge present");
    assert_eq!(
        ship.thrust_remaining, 0,
        "3 power on a 4:1 hull yields 0 thrust (truncated remainder)"
    );
    assert_eq!(ship.move_remaining, 0);
    assert_eq!(ship.movement_allocated, 3, "raw power is still recorded");
}

/// Huge hull: 7 power → 1 thrust (7 / 4 = 1, remainder 3 truncated).
#[test]
fn m3_huge_seven_power_yields_one_thrust_remainder_truncated() {
    let mut game = load_scenario(&manifest_path("scenarios/m3_thrust.toml")).expect("m3 loads");
    allocate(&mut game, 3, 7).expect("allocate 7 power");

    let ship = game.ship(3).expect("huge present");
    assert_eq!(
        ship.thrust_remaining, 1,
        "7 power on a 4:1 hull yields 1 thrust (remainder 3 truncated)"
    );
}

/// Over-allocation is rejected and the ship is not mutated.
#[test]
fn m3_over_allocation_rejected_without_mutation() {
    let mut game = load_scenario(&manifest_path("scenarios/m3_thrust.toml")).expect("m3 loads");
    let escort = game.ship(1).expect("escort present");
    let power = escort.effective_power();
    let over = power + 1;

    let result = allocate(&mut game, 1, over);
    assert!(result.is_err(), "over-allocation must reject");
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("only 14 available") || err.contains("over"),
        "error should mention over-allocation, got: {err}"
    );

    // Ship state must be unchanged.
    let ship = game.ship(1).expect("escort present");
    assert_eq!(
        ship.thrust_remaining, 0,
        "thrust_remaining must not be set on rejection"
    );
    assert_eq!(
        ship.move_remaining, 0,
        "move_remaining must not be set on rejection"
    );
    assert_eq!(
        ship.movement_allocated, 0,
        "movement_allocated must not be set on rejection"
    );
    assert!(
        !game.allocated_this_turn().contains(&1),
        "ship must not be marked allocated on rejection"
    );
}

/// A ship that has already been allocated cannot be allocated again.
#[test]
fn m3_allocation_locks_ship() {
    let mut game = load_scenario(&manifest_path("scenarios/m3_thrust.toml")).expect("m3 loads");
    allocate(&mut game, 1, 1).expect("first allocate succeeds");

    let result = allocate(&mut game, 1, 1);
    assert!(result.is_err(), "second allocation must reject");
    match result.unwrap_err() {
        OrderError::AlreadyAllocated(id) => assert_eq!(id, 1),
        other => panic!("expected AlreadyAllocated, got {other:?}"),
    }

    // Original allocation is preserved.
    let ship = game.ship(1).expect("escort present");
    assert_eq!(
        ship.thrust_remaining, 4,
        "original thrust preserved after lock rejection"
    );
}

/// A ship that allocates zero engine power coasts: thrust_remaining is 0 and
/// its persistent velocity is preserved across the turn boundary.
#[test]
fn m3_zero_power_coasts_preserving_velocity() {
    let mut game = load_scenario(&manifest_path("scenarios/m3_thrust.toml")).expect("m3 loads");

    // Give the escort an initial velocity of 2 (loaded from the scenario).
    let escort = game.ship(1).expect("escort present");
    assert_eq!(escort.velocity.speed, 2, "escort starts with velocity 2");

    // Allocate zero engine power.
    allocate(&mut game, 1, 0).expect("allocate 0 power");

    let ship = game.ship(1).expect("escort present");
    assert_eq!(ship.thrust_remaining, 0, "zero power → zero thrust");
    assert_eq!(ship.move_remaining, 0);
    assert_eq!(ship.movement_allocated, 0);

    // Velocity must persist — it is not cleared by allocation.
    assert_eq!(ship.velocity.speed, 2, "velocity persists when coasting");
}

/// Movement initiative (move_order) is sorted by thrust_remaining, descending.
/// The tiny hull (1 power → 4 thrust) should move before the cruiser (1 → 1)
/// and the huge (4 → 1).
#[test]
fn m3_move_order_sorted_by_thrust_descending() {
    let mut game = load_scenario(&manifest_path("scenarios/m3_thrust.toml")).expect("m3 loads");

    // Allocate so that tiny has the most thrust.
    allocate(&mut game, 1, 1).expect("escort 1 power"); // → 4 thrust
    allocate(&mut game, 2, 1).expect("cruiser 1 power"); // → 1 thrust
    allocate(&mut game, 3, 4).expect("huge 4 power"); // → 1 thrust

    // All ships allocated → game auto-advances to the movement phase.
    let order = game.move_order();
    assert!(!order.is_empty(), "move_order must be populated");
    // The first mover should be the escort (id 1) with 4 thrust.
    assert_eq!(
        order[0], 1,
        "highest-thrust ship (escort, 4 thrust) moves first"
    );
}

/// Active-mover selection is based on thrust_remaining, not raw power allocation.
/// A ship with 0 thrust (e.g. huge hull allocating 3 power) is not an active mover.
#[test]
fn m3_active_mover_requires_nonzero_thrust() {
    let mut game = load_scenario(&manifest_path("scenarios/m3_thrust.toml")).expect("m3 loads");

    // Huge allocates 3 power → 0 thrust (3 < 4).
    allocate(&mut game, 3, 3).expect("huge 3 power → 0 thrust");
    // Cruiser allocates 1 power → 1 thrust.
    allocate(&mut game, 2, 1).expect("cruiser 1 power → 1 thrust");
    // Escort allocates 0 power → 0 thrust.
    allocate(&mut game, 1, 0).expect("escort 0 power → 0 thrust");

    // All ships allocated → game auto-advances to the movement phase.

    // The active mover should be the cruiser (id 2), the only ship with thrust.
    let active = game.active_v2_mover();
    assert_eq!(active, Some(2), "only the cruiser has nonzero thrust");
}
