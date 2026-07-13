//! M5: Fire interleaving and complete turn loop (ADR-0022).
//!
//! Covers the red tests not already exercised by tests/v2_turn_loop.rs,
//! tests/v2_fire.rs, or tests/inertial_movement.rs: exactly four movement/fire
//! windows per nonterminal turn, post-translation range/facing for fire
//! legality, rotation changing weapon arc and shield geometry, mutual
//! destruction resolving from pre-fire state, a destroyed ship's exclusion
//! from later movement phases, end-turn persistence, and schedule repetition
//! across turns without re-allocation.

use std::collections::BTreeMap;
use std::path::PathBuf;

use shipsim_core::arc;
use shipsim_core::board::Board;
use shipsim_core::game_state::{GameState, ScenarioStatus};
use shipsim_core::hex::Hex;
use shipsim_core::motion::{Maneuver, Velocity};
use shipsim_core::movement::{apply_order, Order, OrderError};
use shipsim_core::scenario::load_scenario;
use shipsim_core::ship::Ship;
use shipsim_core::snapshot::StateSnapshot;
use shipsim_core::ssd::Ssd;
use shipsim_core::thrust::ThrustConversion;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn load_combat() -> GameState {
    load_scenario(&manifest_path("scenarios/combat.toml")).expect("combat loads")
}

/// A large, terminal-free two-ship scenario for tests that need room to
/// translate or need to destroy a ship without ending the scenario (see
/// scenarios/m5_turn_loop.toml doc comment).
fn load_turn_loop() -> GameState {
    load_scenario(&manifest_path("scenarios/m5_turn_loop.toml")).expect("m5_turn_loop loads")
}

fn charges(entries: &[(&str, u32)]) -> BTreeMap<String, u32> {
    entries
        .iter()
        .map(|(id, charge)| ((*id).to_string(), *charge))
        .collect()
}

fn allocate(
    game: &mut GameState,
    ship: u32,
    movement: u32,
    weapons: &[(&str, u32)],
    shields: [u32; 6],
) {
    apply_order(
        game,
        Order::Allocate {
            ship,
            movement,
            weapons: charges(weapons),
            shields,
        },
    )
    .expect("allocate");
}

fn commit_maneuver(game: &mut GameState, ship: u32, maneuver: Maneuver) -> Result<(), OrderError> {
    apply_order(game, Order::CommitManeuver { ship, maneuver })
}

/// A minimal, allocation-free ship for reach-hex-objective testing.
fn bare_ship(id: u32, q: i32, r: i32, velocity_speed: u8, course: u8) -> Ship {
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
        velocity: Velocity::new(velocity_speed, course).unwrap(),
        thrust_remaining: 0,
    }
}

fn coast_all(game: &mut GameState, ships: &[u32]) {
    for &ship in ships {
        commit_maneuver(game, ship, Maneuver::Coast).expect("coast commits");
    }
}

fn ready_all(game: &mut GameState, ships: &[u32]) {
    for &ship in ships {
        apply_order(game, Order::ReadyFire { ship }).expect("ready fire");
    }
}

/// Exactly four movement/fire windows occur in a full nonterminal turn: the
/// fixed schedule, not "actions remaining", is what terminates the turn.
#[test]
fn four_movement_fire_windows_occur_in_a_nonterminal_turn() {
    let mut game = load_combat();
    allocate(&mut game, 1, 0, &[], [0; 6]);
    allocate(&mut game, 2, 0, &[], [0; 6]);

    let mut fire_windows = 0;
    loop {
        coast_all(&mut game, &[1, 2]);
        assert_eq!(StateSnapshot::from_game_state(&game).phase, "firing");
        fire_windows += 1;
        ready_all(&mut game, &[1, 2]);
        if StateSnapshot::from_game_state(&game).phase == "turn_end" {
            break;
        }
        assert!(fire_windows <= 4, "more than four fire windows in one turn");
    }
    assert_eq!(
        fire_windows, 4,
        "exactly four movement/fire windows per turn"
    );
}

/// A ship that translates into weapon range during a movement phase can be
/// fired on in that same phase's fire window — legality uses the
/// post-translation position, not the position at the start of the phase.
#[test]
fn fire_legality_uses_post_translation_position() {
    let mut game = load_turn_loop();
    // Beam max range is 10; start ship 1 one hex farther than that (range 11),
    // pointed straight at ship 2, at velocity 4 so it translates every phase.
    game.set_ship_pos(1, Hex::new(11, 0)).unwrap();
    game.set_ship_facing(1, 3).unwrap();
    allocate(&mut game, 1, 4, &[("beam_1", 2)], [0; 6]);
    allocate(&mut game, 2, 0, &[], [0; 6]);

    assert_eq!(
        game.ship(1)
            .unwrap()
            .pos
            .distance(game.ship(2).unwrap().pos),
        11
    );
    // Accelerate from a stop, selecting the course toward ship 2 (west, index
    // 3); velocity 1 does not translate this same phase, so range is still 11.
    commit_maneuver(&mut game, 1, Maneuver::Accelerate { course: Some(3) }).unwrap();
    commit_maneuver(&mut game, 2, Maneuver::Coast).unwrap();
    let err = apply_order(
        &mut game,
        Order::CommitFire {
            ship: 1,
            weapon: "beam_1".into(),
            target: 2,
            shield_facing: 0,
        },
    )
    .expect_err("still out of range before any translation");
    assert!(matches!(err, OrderError::OutOfRange { .. }));
    ready_all(&mut game, &[1, 2]);

    // Phase 2: accelerate to velocity 2, which translates in phase 2 (closing
    // to range 10) — and fire within that same window.
    commit_maneuver(&mut game, 1, Maneuver::Accelerate { course: None }).unwrap();
    commit_maneuver(&mut game, 2, Maneuver::Coast).unwrap();
    assert_eq!(
        game.ship(1)
            .unwrap()
            .pos
            .distance(game.ship(2).unwrap().pos),
        10,
        "velocity 2 translates in phase 2"
    );
    apply_order(
        &mut game,
        Order::CommitFire {
            ship: 1,
            weapon: "beam_1".into(),
            target: 2,
            shield_facing: 0,
        },
    )
    .expect("range 10 is legal for a max-range-10 beam, using the post-translation position");
}

/// Rotating a ship's own facing (without changing course) brings a target
/// into a Forward-mounted weapon's arc within the same fire window.
#[test]
fn rotating_own_facing_changes_weapon_arc() {
    let mut game = load_combat();
    game.set_ship_pos(1, Hex::new(0, 0)).unwrap();
    game.set_ship_facing(1, 0).unwrap();
    game.set_ship_pos(2, Hex::new(3, -3)).unwrap();
    game.set_ship_facing(2, 0).unwrap();
    // At facing 0, (3,-3) has relative bearing 1 — outside the Forward mount's
    // [0] arc (see src/arc.rs's `in_arc_uses_attacker_facing` unit test).
    assert!(!arc::in_arc(
        arc::Mount::Forward,
        0,
        Hex::new(0, 0),
        Hex::new(3, -3)
    ));
    // A port rotation to facing 1 brings it to relative bearing 0 (in arc).
    assert!(arc::in_arc(
        arc::Mount::Forward,
        1,
        Hex::new(0, 0),
        Hex::new(3, -3)
    ));
    let legal_shield_facing = arc::legal_shield_facings(Hex::new(0, 0), Hex::new(3, -3), 0)[0];

    allocate(&mut game, 1, 1, &[("beam_1", 2)], [0; 6]);
    allocate(&mut game, 2, 0, &[], [0; 6]);
    coast_all(&mut game, &[2]);
    commit_maneuver(&mut game, 1, Maneuver::RotatePort).unwrap();
    assert_eq!(StateSnapshot::from_game_state(&game).phase, "firing");

    let err = apply_order(
        &mut game,
        Order::CommitFire {
            ship: 1,
            weapon: "beam_1".into(),
            target: 2,
            shield_facing: legal_shield_facing,
        },
    );
    // The rotation already resolved before this fire window opened, so ship 1
    // is already facing 1 and the target is already in arc.
    assert_eq!(game.ship(1).unwrap().facing, 1);
    err.expect("target is in arc after the rotation resolved");
}

/// Rotating the *target's* facing changes which shield facings are legal for
/// an incoming shot (defender-relative geometry), within the fire pipeline.
#[test]
fn rotating_target_facing_changes_legal_shield_facings() {
    let mut game = load_combat();
    game.set_ship_pos(1, Hex::new(3, 0)).unwrap();
    game.set_ship_facing(1, 3).unwrap(); // faces target dead ahead (Forward arc).
    game.set_ship_pos(2, Hex::new(0, 0)).unwrap();
    game.set_ship_facing(2, 0).unwrap();
    // From src/arc.rs's `legal_shields_are_relative_to_target_facing` fact:
    // facing 0 -> legal shield [0]; facing 5 -> legal shield [1].
    assert_eq!(
        arc::legal_shield_facings(Hex::new(3, 0), Hex::new(0, 0), 0),
        vec![0]
    );
    assert_eq!(
        arc::legal_shield_facings(Hex::new(3, 0), Hex::new(0, 0), 5),
        vec![1]
    );

    allocate(&mut game, 1, 0, &[("beam_1", 2)], [0; 6]);
    allocate(&mut game, 2, 1, &[], [0; 6]);
    coast_all(&mut game, &[1]);
    commit_maneuver(&mut game, 2, Maneuver::RotateStarboard).unwrap(); // facing 0 -> 5
    assert_eq!(game.ship(2).unwrap().facing, 5);
    assert_eq!(StateSnapshot::from_game_state(&game).phase, "firing");

    let err = apply_order(
        &mut game,
        Order::CommitFire {
            ship: 1,
            weapon: "beam_1".into(),
            target: 2,
            shield_facing: 0,
        },
    )
    .expect_err("shield facing 0 was only legal before the target rotated");
    assert!(matches!(err, OrderError::IllegalShieldFacing { .. }));
    apply_order(
        &mut game,
        Order::CommitFire {
            ship: 1,
            weapon: "beam_1".into(),
            target: 2,
            shield_facing: 1,
        },
    )
    .expect("shield facing 1 is legal now that the target has rotated to facing 5");
}

/// Both ships' committed shots resolve from the pre-fire snapshot: a ship
/// destroyed by an earlier-processed commit in the same batch still has its
/// own already-committed shot resolve (consuming its weapon), rather than
/// being silently dropped because it died mid-batch.
#[test]
fn mutual_fire_resolves_from_pre_fire_state() {
    let mut game = load_combat();
    game.set_ship_structure(2, 1).unwrap(); // one hit destroys ship 2.
    allocate(&mut game, 1, 0, &[("beam_1", 4)], [0; 6]);
    allocate(&mut game, 2, 0, &[("beam_1", 4)], [0; 6]);
    coast_all(&mut game, &[1, 2]);

    apply_order(
        &mut game,
        Order::CommitFire {
            ship: 1,
            weapon: "beam_1".into(),
            target: 2,
            shield_facing: 0,
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::CommitFire {
            ship: 2,
            weapon: "beam_1".into(),
            target: 1,
            shield_facing: 0,
        },
    )
    .unwrap();
    ready_all(&mut game, &[1, 2]);

    let snapshot = StateSnapshot::from_game_state(&game);
    assert_eq!(
        snapshot.combat_log.len(),
        2,
        "both commits resolve even though ship 2 dies to the first"
    );
    let ship2 = snapshot.ships.iter().find(|s| s.id == 2).unwrap();
    assert!(ship2.destroyed, "ship 2 dies to ship 1's shot");
    // Ship 2's own outgoing shot (committed before it died) must still resolve:
    // its weapon is consumed even though ship 2 is now destroyed, proving its
    // commit was validated against the pre-fire snapshot rather than dropped
    // because its attacker died mid-batch.
    let beam2 = ship2.weapons.iter().find(|w| w.id == "beam_1").unwrap();
    assert!(
        beam2.fired && beam2.charge == 0,
        "ship 2's own weapon is still consumed by its own shot, proving its commit \
         was resolved against pre-fire state rather than dropped"
    );
}

/// Losing (every player-controlled ship destroyed) parks the turn at
/// `turn_end` immediately, exactly like a Won destruction — not just Won.
/// A prior version of this check only tested `status == Won`, which left a
/// Lost mid-turn in `Phase::Movement` expecting maneuver commits nobody
/// (the now-defeated side) would ever supply.
#[test]
fn losing_also_parks_the_turn_at_turn_end_immediately() {
    let mut game = load_combat(); // ship 1 = player, ship 2 = scripted (NPC).
    game.set_ship_structure(1, 1).unwrap(); // one hit destroys the only player ship.
    allocate(&mut game, 1, 0, &[], [0; 6]);
    allocate(&mut game, 2, 0, &[("beam_1", 4)], [0; 6]);
    coast_all(&mut game, &[1, 2]);
    apply_order(
        &mut game,
        Order::CommitFire {
            ship: 2,
            weapon: "beam_1".into(),
            target: 1,
            shield_facing: 0,
        },
    )
    .unwrap();
    ready_all(&mut game, &[1, 2]);

    let snapshot = StateSnapshot::from_game_state(&game);
    assert_eq!(snapshot.status, ScenarioStatus::Lost);
    assert_eq!(
        snapshot.phase, "turn_end",
        "Lost must park at turn_end immediately, the same as Won — not leave the \
         turn parked in Phase::Movement awaiting commits from a defeated side"
    );
}

/// Reaching a `ReachHex` objective by translation alone wins the scenario
/// and parks at `turn_end` immediately, without waiting for a fire window —
/// status must be refreshed right after the movement batch, not only after
/// fire resolves or on an explicit EndTurn.
#[test]
fn reaching_the_objective_by_translation_wins_immediately() {
    let objective = Hex::new(1, 0);
    let ships = vec![bare_ship(1, 0, 0, 4, 0)]; // velocity 4, course 0 (east): translates every phase.
    let mut game = GameState::new(Board::new(10, 10), ships, objective);

    apply_order(
        &mut game,
        Order::Allocate {
            ship: 1,
            movement: 0,
            weapons: BTreeMap::new(),
            shields: [0; 6],
        },
    )
    .expect("allocate");
    assert_eq!(StateSnapshot::from_game_state(&game).phase, "movement");

    commit_maneuver(&mut game, 1, Maneuver::Coast).expect("coast onto the objective hex");

    let snapshot = StateSnapshot::from_game_state(&game);
    assert_eq!(game.ship(1).unwrap().pos, objective);
    assert_eq!(snapshot.status, ScenarioStatus::Won);
    assert_eq!(
        snapshot.phase, "turn_end",
        "reaching the objective must be recognized right after translation, not \
         deferred until a fire window (which nobody needs to open) resolves"
    );
}

/// A ship destroyed during a fire window is excluded from the next movement
/// phase's maneuver commitments.
#[test]
fn destroyed_ship_is_excluded_from_later_movement_phases() {
    let mut game = load_turn_loop(); // no destruction terminal: the scenario continues.
    game.set_ship_structure(2, 1).unwrap();
    allocate(&mut game, 1, 0, &[("beam_1", 4)], [0; 6]);
    allocate(&mut game, 2, 0, &[], [0; 6]);
    coast_all(&mut game, &[1, 2]);
    apply_order(
        &mut game,
        Order::CommitFire {
            ship: 1,
            weapon: "beam_1".into(),
            target: 2,
            shield_facing: 0,
        },
    )
    .unwrap();
    ready_all(&mut game, &[1, 2]);

    assert!(game.ship(2).unwrap().destroyed);
    assert_eq!(
        StateSnapshot::from_game_state(&game).phase,
        "movement",
        "the scenario continues into the next movement phase"
    );
    let err = commit_maneuver(&mut game, 2, Maneuver::Coast)
        .expect_err("a destroyed ship cannot commit a maneuver");
    assert!(matches!(err, OrderError::ShipNotFound(2)));
    // The surviving ship alone completes the phase — no commitment is owed on
    // the destroyed ship's behalf.
    commit_maneuver(&mut game, 1, Maneuver::Coast).expect("survivor completes the phase alone");
    assert_eq!(StateSnapshot::from_game_state(&game).phase, "firing");
}

/// EndTurn preserves velocity/course (persistent inertia) and clears the
/// per-turn thrust reserve.
#[test]
fn end_turn_preserves_velocity_and_course_and_clears_thrust() {
    let mut game = load_combat();
    allocate(&mut game, 1, 2, &[], [0; 6]);
    allocate(&mut game, 2, 0, &[], [0; 6]);
    commit_maneuver(&mut game, 1, Maneuver::Accelerate { course: Some(2) }).unwrap();
    commit_maneuver(&mut game, 2, Maneuver::Coast).unwrap();
    assert_eq!(game.ship(1).unwrap().velocity.speed, 1);
    assert_eq!(game.ship(1).unwrap().velocity.course, 2);
    assert!(game.ship(1).unwrap().thrust_remaining > 0);

    apply_order(&mut game, Order::EndTurn).expect("end turn");

    let ship1 = game.ship(1).unwrap();
    assert_eq!(
        ship1.velocity.speed, 1,
        "velocity persists across the turn boundary"
    );
    assert_eq!(
        ship1.velocity.course, 2,
        "course persists across the turn boundary"
    );
    assert_eq!(
        ship1.thrust_remaining, 0,
        "thrust reserve is cleared for the new turn"
    );
}

/// A ship that allocates zero engine power next turn still coasts through
/// its exact translation schedule (persistent velocity, no re-allocation
/// needed).
#[test]
fn coasting_ship_repeats_its_schedule_next_turn_without_allocation() {
    let mut game = load_turn_loop();
    // Turn 1: accelerate ship 1 to velocity 2 (schedule: phases 2, 4) across
    // phases 1-2, then coast the rest of the turn.
    allocate(&mut game, 1, 4, &[], [0; 6]);
    allocate(&mut game, 2, 0, &[], [0; 6]);
    commit_maneuver(&mut game, 1, Maneuver::Accelerate { course: Some(0) }).unwrap(); // -> v1
    coast_all(&mut game, &[2]);
    ready_all(&mut game, &[1, 2]);
    commit_maneuver(&mut game, 1, Maneuver::Accelerate { course: None }).unwrap(); // -> v2
    coast_all(&mut game, &[2]);
    ready_all(&mut game, &[1, 2]);
    for _ in 0..2 {
        coast_all(&mut game, &[1, 2]);
        ready_all(&mut game, &[1, 2]);
    }
    assert_eq!(game.ship(1).unwrap().velocity.speed, 2);
    apply_order(&mut game, Order::EndTurn).expect("end turn 1");

    // Turn 2: allocate zero engine power for ship 1 — it should still
    // translate on phases 2 and 4, exactly as velocity 2's schedule dictates,
    // purely from persisted inertia.
    allocate(&mut game, 1, 0, &[], [0; 6]);
    allocate(&mut game, 2, 0, &[], [0; 6]);
    assert_eq!(game.ship(1).unwrap().thrust_remaining, 0);
    assert_eq!(game.ship(1).unwrap().velocity.speed, 2);

    let start = game.ship(1).unwrap().pos;
    coast_all(&mut game, &[1, 2]); // phase 1: v2 schedule excludes phase 1.
    ready_all(&mut game, &[1, 2]);
    assert_eq!(
        game.ship(1).unwrap().pos,
        start,
        "no translation in phase 1"
    );

    coast_all(&mut game, &[1, 2]); // phase 2: v2 schedule includes phase 2.
    ready_all(&mut game, &[1, 2]);
    assert_ne!(
        game.ship(1).unwrap().pos,
        start,
        "translates in phase 2 purely from persisted velocity, no allocation needed"
    );
}
