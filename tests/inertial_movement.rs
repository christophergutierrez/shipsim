//! M4: Four-phase maneuver and translation core (ADR-0022).
//!
//! Covers the per-ship maneuver/thrust mechanics: exact-phase translation
//! scheduling from post-maneuver velocity, thrust costs, commit-before-resolve
//! semantics, and the fixed four-phase turn termination (no `can_any_move`
//! re-entry heuristic).

use std::collections::BTreeMap;
use std::path::PathBuf;

use shipsim_core::board::Board;
use shipsim_core::game_state::GameState;
use shipsim_core::hex::Hex;
use shipsim_core::motion::{Maneuver, Velocity};
use shipsim_core::movement::{apply_order, Order, OrderError};
use shipsim_core::scenario::load_scenario;
use shipsim_core::ship::Ship;
use shipsim_core::snapshot::StateSnapshot;
use shipsim_core::ssd::Ssd;
use shipsim_core::thrust::ThrustConversion;

const ALL_SHIPS: [u32; 5] = [1, 2, 3, 4, 5];

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn load() -> GameState {
    load_scenario(&manifest_path("scenarios/m4_movement.toml")).expect("m4_movement loads")
}

/// Allocate `movement` engine power (converted to thrust 1:1 for heavy_cruiser
/// hulls) for every ship, with zero weapons/shields. All five ships allocating
/// completes the turn and enters movement phase 1.
fn allocate_all(game: &mut GameState, movement: u32) {
    for ship in ALL_SHIPS {
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
}

fn commit(game: &mut GameState, ship: u32, maneuver: Maneuver) -> Result<(), OrderError> {
    apply_order(game, Order::CommitManeuver { ship, maneuver })
}

/// Commit `maneuver` for `ship` and `Maneuver::Coast` for every other living ship
/// in `ALL_SHIPS`, resolving the current phase.
fn resolve_phase_with(game: &mut GameState, ship: u32, maneuver: Maneuver) {
    for other in ALL_SHIPS {
        if other == ship {
            commit(game, other, maneuver).expect("maneuver commits");
        } else {
            commit(game, other, Maneuver::Coast).expect("coast commits");
        }
    }
}

fn coast_all(game: &mut GameState) {
    for ship in ALL_SHIPS {
        commit(game, ship, Maneuver::Coast).expect("coast commits");
    }
}

fn ship_q(game: &GameState, id: u32) -> i32 {
    game.ship(id).expect("ship present").pos.q
}

#[test]
fn translation_phases_match_exact_schedule_table() {
    // Ships 1..5 carry velocity 0..4 respectively (scenarios/m4_movement.toml),
    // all coasting east (course 0). Expected q after each phase, from the
    // frozen schedule table: v0 none; v1 phase 4; v2 phases 2,4; v3 phases
    // 1,2,4; v4 phases 1,2,3,4.
    let mut game = load();
    allocate_all(&mut game, 0);
    assert_eq!(game.phase_name(), "movement");
    assert_eq!(game.movement_phase(), 1);

    let starting_q = [1, 1, 1, 1, 1];
    let expected_q_after_phase = [
        [1, 1, 1, 2, 2], // after phase 1: only v3, v4 translate
        [1, 1, 2, 3, 3], // after phase 2: v2, v3, v4 translate
        [1, 1, 2, 3, 4], // after phase 3: only v4 translates
        [1, 2, 3, 4, 5], // after phase 4: v1, v2, v3, v4 translate
    ];
    assert_eq!(
        ALL_SHIPS.map(|id| ship_q(&game, id)),
        starting_q,
        "starting positions"
    );

    for (phase_index, expected) in expected_q_after_phase.iter().enumerate() {
        coast_all(&mut game);
        let positions = ALL_SHIPS.map(|id| ship_q(&game, id));
        assert_eq!(
            positions,
            *expected,
            "positions after phase {}",
            phase_index + 1
        );
    }

    // The turn terminates after exactly four phases: no fire phase is entered
    // in M4 (no integration yet), and there is no fifth movement phase.
    assert_eq!(game.phase_name(), "turn_end");
}

#[test]
fn coasting_ship_with_zero_thrust_terminates_turn_after_exactly_four_phases() {
    let mut game = load();
    allocate_all(&mut game, 0);
    for ship in ALL_SHIPS {
        assert_eq!(game.ship(ship).unwrap().thrust_remaining, 0);
    }

    for phase in 1..=4u8 {
        assert_eq!(game.phase_name(), "movement");
        assert_eq!(game.movement_phase(), phase);
        coast_all(&mut game);
    }
    assert_eq!(
        game.phase_name(),
        "turn_end",
        "turn must terminate after exactly four phases with no re-entry heuristic"
    );
}

#[test]
fn accelerating_past_schedule_threshold_causes_immediate_translation() {
    // Ship 3 starts at velocity 2 (schedule: phases 2, 4 only). Accelerating to
    // 3 in phase 1 makes it eligible for phase 1 (schedule: phases 1, 2, 4),
    // and eligibility is drawn from the *post-maneuver* velocity.
    let mut game = load();
    allocate_all(&mut game, 2); // 1:1 conversion -> 2 thrust, enough for one Accelerate.
    let before = ship_q(&game, 3);

    resolve_phase_with(&mut game, 3, Maneuver::Accelerate { course: None });

    assert_eq!(game.ship(3).unwrap().velocity.speed, 3);
    assert_eq!(
        ship_q(&game, 3),
        before + 1,
        "post-maneuver velocity 3 translates in phase 1"
    );
}

#[test]
fn decelerating_below_schedule_threshold_prevents_translation() {
    // Ship 4 starts at velocity 3 (schedule: phases 1, 2, 4). Decelerating to 2
    // in phase 1 drops it out of phase 1's schedule (v2: phases 2, 4 only), so
    // it does not translate this phase even though its pre-maneuver velocity
    // would have.
    let mut game = load();
    allocate_all(&mut game, 2);
    let before = ship_q(&game, 4);

    resolve_phase_with(&mut game, 4, Maneuver::Decelerate);

    assert_eq!(game.ship(4).unwrap().velocity.speed, 2);
    assert_eq!(
        ship_q(&game, 4),
        before,
        "post-maneuver velocity 2 does not translate in phase 1"
    );
}

#[test]
fn coast_consumes_no_thrust_and_preserves_velocity_and_facing() {
    let mut game = load();
    allocate_all(&mut game, 4);
    let before = game.ship(1).unwrap().clone();
    coast_all(&mut game);
    let after = game.ship(1).unwrap();
    assert_eq!(after.thrust_remaining, before.thrust_remaining);
    assert_eq!(after.velocity, before.velocity);
    assert_eq!(after.facing, before.facing);
}

#[test]
fn maneuvers_consume_exact_thrust_costs() {
    // Accelerate/decelerate cost 1; rotate costs 1; course-turn costs
    // max(current speed, 1).
    let mut game = load();
    allocate_all(&mut game, 4);

    let thrust_before = game.ship(1).unwrap().thrust_remaining;
    resolve_phase_with(&mut game, 1, Maneuver::Accelerate { course: None });
    assert_eq!(game.ship(1).unwrap().thrust_remaining, thrust_before - 1);

    let mut game = load();
    allocate_all(&mut game, 4);
    let thrust_before = game.ship(3).unwrap().thrust_remaining;
    resolve_phase_with(&mut game, 3, Maneuver::Decelerate);
    assert_eq!(game.ship(3).unwrap().thrust_remaining, thrust_before - 1);

    let mut game = load();
    allocate_all(&mut game, 4);
    let thrust_before = game.ship(1).unwrap().thrust_remaining;
    resolve_phase_with(&mut game, 1, Maneuver::RotatePort);
    assert_eq!(game.ship(1).unwrap().thrust_remaining, thrust_before - 1);

    // Ship 4 (velocity 3): course-turn cost equals current speed (3), not the
    // flat rotation cost.
    let mut game = load();
    allocate_all(&mut game, 4);
    let thrust_before = game.ship(4).unwrap().thrust_remaining;
    resolve_phase_with(&mut game, 4, Maneuver::TurnCourseStarboard);
    assert_eq!(game.ship(4).unwrap().thrust_remaining, thrust_before - 3);
}

#[test]
fn insufficient_thrust_maneuver_rejects_without_mutation() {
    let mut game = load();
    allocate_all(&mut game, 0); // zero thrust for everyone.
    let before = game.ship(1).unwrap().clone();

    let err = commit(&mut game, 1, Maneuver::Accelerate { course: None })
        .expect_err("accelerate needs 1 thrust, ship has 0");
    assert!(matches!(
        err,
        OrderError::InsufficientThrust { ship: 1, .. }
    ));

    let after = game.ship(1).unwrap();
    assert_eq!(
        after.velocity, before.velocity,
        "rejection must not mutate velocity"
    );
    assert_eq!(
        after.facing, before.facing,
        "rejection must not mutate facing"
    );
    assert_eq!(
        after.thrust_remaining, before.thrust_remaining,
        "rejection must not mutate thrust"
    );
    assert!(!game.has_committed_this_phase(1));
}

#[test]
fn ship_cannot_commit_twice_in_the_same_phase() {
    let mut game = load();
    allocate_all(&mut game, 4);
    commit(&mut game, 1, Maneuver::Coast).expect("first commit");
    let err = commit(&mut game, 1, Maneuver::Coast).expect_err("second commit rejects");
    assert!(matches!(err, OrderError::AlreadyCommittedThisPhase(1)));
}

#[test]
fn resolution_is_independent_of_commit_order() {
    let mut forward = load();
    allocate_all(&mut forward, 2);
    for ship in ALL_SHIPS {
        let maneuver = if ship == 3 {
            Maneuver::Accelerate { course: None }
        } else {
            Maneuver::Coast
        };
        commit(&mut forward, ship, maneuver).expect("commit");
    }

    let mut reversed = load();
    allocate_all(&mut reversed, 2);
    for ship in ALL_SHIPS.iter().rev() {
        let maneuver = if *ship == 3 {
            Maneuver::Accelerate { course: None }
        } else {
            Maneuver::Coast
        };
        commit(&mut reversed, *ship, maneuver).expect("commit");
    }

    let snap_forward = StateSnapshot::from_game_state(&forward);
    let snap_reversed = StateSnapshot::from_game_state(&reversed);
    for id in ALL_SHIPS {
        let a = snap_forward.ships.iter().find(|s| s.id == id).unwrap();
        let b = snap_reversed.ships.iter().find(|s| s.id == id).unwrap();
        assert_eq!(
            a.q, b.q,
            "ship {id} q must match regardless of commit order"
        );
        assert_eq!(
            a.r, b.r,
            "ship {id} r must match regardless of commit order"
        );
        assert_eq!(
            a.velocity, b.velocity,
            "ship {id} velocity must match regardless of commit order"
        );
        assert_eq!(
            a.thrust_remaining, b.thrust_remaining,
            "ship {id} thrust must match regardless of commit order"
        );
    }
    assert_eq!(snap_forward.movement_phase, snap_reversed.movement_phase);
}

/// A minimal, allocation-free ship for order-independence testing: velocity 4
/// (translates every phase) and a rational thrust conversion that never
/// matters here since every commit is `Coast`.
fn bare_ship(id: u32, q: i32, r: i32, course: u8) -> Ship {
    Ship {
        id,
        class: "t".into(),
        pos: Hex::new(q, r),
        facing: 0,
        speed: 4,
        power: 8,
        weapons: vec![],
        shields_powered: [0; 6],
        shields_remaining: [0; 6],
        max_shield_per_facing: 6,
        movement_allocated: 0,
        weapon_charges: BTreeMap::new(),
        ssd: Ssd::new(10, 4, 2, 0),
        destroyed: false,
        max_velocity: 4,
        thrust_conversion: ThrustConversion::new(1, 1, 4).unwrap(),
        velocity: Velocity::new(4, course).unwrap(),
        thrust_remaining: 0,
    }
}

/// The M4 exit criterion "results are deterministic across ship insertion
/// ... order" (docs/MOVEMENT-INERTIA-PLAN.md) is about the order ships are
/// stored in `GameState`, independent of the order maneuvers are committed in
/// (already covered by `resolution_is_independent_of_commit_order`). Build
/// the same three-ship same-destination conflict from two different
/// `Vec<Ship>` insertion orders and confirm the resolved outcome — including
/// which ships are blocked — is identical either way.
#[test]
fn resolution_is_independent_of_ship_insertion_order() {
    // Ship 1 and ship 2 both target hex (5, 0): a genuine conflict, so the
    // outcome actually depends on the fixed-point blocking algorithm rather
    // than being trivially insertion-order-agnostic. Ship 3 is unrelated
    // (coasts to a free hex) and included only to make the ship list longer
    // than a trivial pair.
    let ships_forward = vec![
        bare_ship(1, 4, 5, 0), // course 0 = (1,0): targets (5,5)
        bare_ship(2, 6, 4, 4), // course 4 = (-1,1): targets (5,5)
        bare_ship(3, 0, 0, 0),
    ];
    let mut ships_reversed = ships_forward.clone();
    ships_reversed.reverse();
    assert_ne!(
        ships_forward.iter().map(|s| s.id).collect::<Vec<_>>(),
        ships_reversed.iter().map(|s| s.id).collect::<Vec<_>>(),
        "sanity: the two insertion orders are actually different"
    );

    let run = |ships: Vec<Ship>| {
        let mut game = GameState::new(Board::new(20, 20), ships, Hex::new(19, 19));
        for id in [1u32, 2, 3] {
            apply_order(
                &mut game,
                Order::Allocate {
                    ship: id,
                    movement: 0,
                    weapons: BTreeMap::new(),
                    shields: [0; 6],
                },
            )
            .expect("allocate");
        }
        for id in [1u32, 2, 3] {
            commit(&mut game, id, Maneuver::Coast).expect("coast commits");
        }
        StateSnapshot::from_game_state(&game)
    };

    let snap_forward = run(ships_forward);
    let snap_reversed = run(ships_reversed);
    for id in [1u32, 2, 3] {
        let a = snap_forward.ships.iter().find(|s| s.id == id).unwrap();
        let b = snap_reversed.ships.iter().find(|s| s.id == id).unwrap();
        assert_eq!(
            (a.q, a.r),
            (b.q, b.r),
            "ship {id} position must match regardless of ship insertion order"
        );
    }
    // Ships 1 and 2 both wanted (5, 5): the conflict must block both,
    // regardless of which insertion order discovers the collision first.
    let ship1 = snap_forward.ships.iter().find(|s| s.id == 1).unwrap();
    let ship2 = snap_forward.ships.iter().find(|s| s.id == 2).unwrap();
    assert_eq!((ship1.q, ship1.r), (4, 5), "ship 1 blocked by the conflict");
    assert_eq!((ship2.q, ship2.r), (6, 4), "ship 2 blocked by the conflict");
}
