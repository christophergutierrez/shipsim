use std::collections::BTreeMap;
use std::path::PathBuf;

use shipsim_core::hex::Hex;
use shipsim_core::motion::Maneuver;
use shipsim_core::movement::{apply_order, Order, OrderError};
use shipsim_core::scenario::load_scenario;
use shipsim_core::snapshot::StateSnapshot;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn load_combat() -> shipsim_core::game_state::GameState {
    load_scenario(&manifest_path("scenarios/combat.toml")).expect("combat loads")
}

fn load_fleet() -> shipsim_core::game_state::GameState {
    load_scenario(&manifest_path("scenarios/fleet.toml")).expect("fleet loads")
}

fn charges(entries: &[(&str, u32)]) -> BTreeMap<String, u32> {
    entries
        .iter()
        .map(|(id, charge)| ((*id).to_string(), *charge))
        .collect()
}

fn allocate(
    game: &mut shipsim_core::game_state::GameState,
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

fn commit_maneuver(
    game: &mut shipsim_core::game_state::GameState,
    ship: u32,
    maneuver: Maneuver,
) -> Result<(), OrderError> {
    apply_order(game, Order::CommitManeuver { ship, maneuver })
}

fn living_ship_ids(game: &shipsim_core::game_state::GameState) -> Vec<u32> {
    StateSnapshot::from_game_state(game)
        .ships
        .iter()
        .filter(|s| !s.destroyed)
        .map(|s| s.id)
        .collect()
}

/// Commit `Maneuver::Coast` for every living ship, resolving the current
/// movement phase into its fire window. Every living ship must commit exactly
/// once per phase (ADR-0022 M4), so a single round always reaches firing.
fn enter_firing(game: &mut shipsim_core::game_state::GameState) {
    for id in living_ship_ids(game) {
        commit_maneuver(game, id, Maneuver::Coast).expect("coast commits");
    }
    assert_eq!(StateSnapshot::from_game_state(game).phase, "firing");
}

fn ready_all(game: &mut shipsim_core::game_state::GameState, ships: &[u32]) {
    for &ship in ships {
        apply_order(game, Order::ReadyFire { ship }).expect("ready fire");
    }
}

#[test]
fn test_v2_fire_illegal_before_closing_then_commit_legal_after_move() {
    let mut game = load_fleet();
    game.set_ship_pos(1, Hex::new(11, 0)).unwrap();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, Hex::new(0, 0)).unwrap();
    game.set_ship_facing(2, 0).unwrap();
    game.set_ship_pos(3, Hex::new(5, 5)).unwrap();
    game.set_ship_pos(4, Hex::new(6, 5)).unwrap();
    // Ship 1 accelerates west (course 3) toward ship 2 across all four
    // movement phases this turn: velocity schedule means it only actually
    // translates in phases 2 and 4 (v2's and v4's schedules), closing two
    // hexes (11 -> 9) by the end of the turn — accelerate costs 1 thrust each.
    allocate(&mut game, 1, 4, &[("beam_1", 1)], [0; 6]);
    allocate(&mut game, 2, 0, &[("beam_1", 1)], [2, 0, 0, 0, 0, 0]);
    allocate(&mut game, 3, 0, &[("beam_1", 1)], [0; 6]);
    allocate(&mut game, 4, 0, &[("beam_1", 1)], [0; 6]);

    // Phase 1: accelerate from a stop, selecting the course. Velocity 1 does
    // not translate in phase 1, so ship 1 is still at range 11.
    commit_maneuver(&mut game, 1, Maneuver::Accelerate { course: Some(3) }).expect("accelerate");
    for id in [2u32, 3, 4] {
        commit_maneuver(&mut game, id, Maneuver::Coast).expect("coast");
    }
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
    .expect_err("range 11 beam is illegal");
    assert!(matches!(err, OrderError::OutOfRange { .. }));
    ready_all(&mut game, &[1, 2, 3, 4]);

    // Phases 2-3: keep accelerating (velocity 1 -> 2 -> 3). Velocity 2 translates
    // in phase 2, closing one hex (11 -> 10); velocity 3 does not translate in
    // phase 3.
    for _ in 0..2 {
        commit_maneuver(&mut game, 1, Maneuver::Accelerate { course: None }).expect("accelerate");
        for id in [2u32, 3, 4] {
            commit_maneuver(&mut game, id, Maneuver::Coast).expect("coast");
        }
        ready_all(&mut game, &[1, 2, 3, 4]);
    }

    // Phase 4: accelerate to velocity 4, which translates in phase 4, closing
    // the second hex (10 -> 9) — check before readying, so the fire window is
    // still open to commit into.
    commit_maneuver(&mut game, 1, Maneuver::Accelerate { course: None }).expect("accelerate");
    for id in [2u32, 3, 4] {
        commit_maneuver(&mut game, id, Maneuver::Coast).expect("coast");
    }
    // scenarios/fleet.toml is a floating map: the whole formation recenters after
    // each phase that moves a ship, so check the closed *distance* rather than an
    // absolute hex (recentering preserves relative positions).
    let distance = game
        .ship(1)
        .unwrap()
        .pos
        .distance(game.ship(2).unwrap().pos);
    assert_eq!(distance, 9, "closed two hexes (11 -> 9)");
    assert_eq!(StateSnapshot::from_game_state(&game).phase, "firing");
    apply_order(
        &mut game,
        Order::CommitFire {
            ship: 1,
            weapon: "beam_1".into(),
            target: 2,
            shield_facing: 0,
        },
    )
    .expect("commit legal after closing");
}

#[test]
fn test_v2_ready_fire_resolves_and_consumes_weapon() {
    let mut game = load_combat();
    game.set_ship_pos(1, Hex::new(1, 0)).unwrap();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, Hex::new(0, 0)).unwrap();
    game.set_ship_facing(2, 0).unwrap();
    allocate(&mut game, 1, 0, &[("beam_1", 2), ("torp_1", 1)], [0; 6]);
    allocate(&mut game, 2, 0, &[("beam_1", 1)], [3, 0, 0, 0, 0, 0]);
    enter_firing(&mut game);

    apply_order(
        &mut game,
        Order::CommitFire {
            ship: 1,
            weapon: "beam_1".into(),
            target: 2,
            shield_facing: 0,
        },
    )
    .expect("commit");
    apply_order(&mut game, Order::ReadyFire { ship: 1 }).expect("ready ship 1");
    apply_order(&mut game, Order::ReadyFire { ship: 2 }).expect("ready ship 2 resolves");

    let snapshot = StateSnapshot::from_game_state(&game);
    let ship1 = snapshot.ships.iter().find(|ship| ship.id == 1).unwrap();
    let beam = ship1
        .weapons
        .iter()
        .find(|weapon| weapon.id == "beam_1")
        .unwrap();
    assert_eq!(beam.charge, 0);
    assert!(beam.fired);
    assert!(!snapshot.combat_log.is_empty());
    // ADR-0022 M5: the fixed four-phase schedule advances to the next movement
    // phase regardless of what remains charged/legal — no re-entry heuristic.
    assert_eq!(snapshot.phase, "movement");
    assert_eq!(snapshot.movement_phase, 2);
    assert!(snapshot.fire_commits.is_empty());
    assert!(snapshot.ships_ready_fire.is_empty());

    // The uncommitted torp is committable once phase 2 reaches its own firing window.
    enter_firing(&mut game);
    apply_order(
        &mut game,
        Order::CommitFire {
            ship: 1,
            weapon: "torp_1".into(),
            target: 2,
            shield_facing: 0,
        },
    )
    .expect("torp committable in phase 2's firing window");
}

#[test]
fn test_v2_miss_consumes_weapon_without_damage() {
    let mut game = load_combat();
    game.set_ship_pos(1, Hex::new(3, 0)).unwrap();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, Hex::new(0, 0)).unwrap();
    game.set_ship_facing(2, 0).unwrap();
    game.set_ship_structure(2, 12).unwrap();
    // Seed 4242's first d20 roll is 16, a miss vs a range-3 beam threshold of 15.
    allocate(&mut game, 1, 0, &[("beam_1", 2)], [0; 6]);
    allocate(&mut game, 2, 0, &[], [3, 0, 0, 0, 0, 0]);
    enter_firing(&mut game);

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
    apply_order(&mut game, Order::ReadyFire { ship: 1 }).unwrap();
    apply_order(&mut game, Order::ReadyFire { ship: 2 }).unwrap();

    let snapshot = StateSnapshot::from_game_state(&game);
    assert_eq!(snapshot.combat_log[0].kind, "miss");
    assert_eq!(snapshot.combat_log[0].damage, 0);
    let attacker = snapshot.ships.iter().find(|ship| ship.id == 1).unwrap();
    let beam = attacker
        .weapons
        .iter()
        .find(|weapon| weapon.id == "beam_1")
        .unwrap();
    assert_eq!(beam.charge, 0);
    assert!(beam.fired);
    let target = snapshot.ships.iter().find(|ship| ship.id == 2).unwrap();
    assert_eq!(target.shields_remaining[0], 3);
    assert_eq!(target.structure, 12);
}

#[test]
fn test_v2_shield_depletes_then_hull_takes_overflow() {
    let mut game = load_combat();
    game.set_ship_pos(1, Hex::new(1, 0)).unwrap();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, Hex::new(0, 0)).unwrap();
    game.set_ship_facing(2, 0).unwrap();
    game.set_ship_structure(2, 12).unwrap();
    allocate(&mut game, 1, 0, &[("beam_1", 3)], [0; 6]);
    allocate(&mut game, 2, 1, &[], [3, 0, 0, 0, 0, 0]);
    enter_firing(&mut game);

    apply_order(
        &mut game,
        Order::CommitFire {
            ship: 1,
            weapon: "beam_1".into(),
            target: 2,
            shield_facing: 0,
        },
    )
    .expect("commit");
    apply_order(&mut game, Order::ReadyFire { ship: 1 }).unwrap();
    apply_order(&mut game, Order::ReadyFire { ship: 2 }).unwrap();

    let target = StateSnapshot::from_game_state(&game)
        .ships
        .into_iter()
        .find(|ship| ship.id == 2)
        .unwrap();
    assert_eq!(
        StateSnapshot::from_game_state(&game).combat_log[0].kind,
        "hit"
    );
    assert_eq!(target.shields_remaining[0], 0);
    assert!(target.structure < 12);
}

#[test]
fn test_v2_unpowered_facing_absorbs_zero() {
    let mut game = load_combat();
    game.set_ship_pos(1, Hex::new(1, 0)).unwrap();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, Hex::new(0, 0)).unwrap();
    game.set_ship_facing(2, 0).unwrap();
    game.set_ship_structure(2, 12).unwrap();
    allocate(&mut game, 1, 0, &[("beam_1", 2)], [0; 6]);
    allocate(&mut game, 2, 1, &[], [0; 6]);
    enter_firing(&mut game);

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
    apply_order(&mut game, Order::ReadyFire { ship: 1 }).unwrap();
    apply_order(&mut game, Order::ReadyFire { ship: 2 }).unwrap();

    let snapshot = StateSnapshot::from_game_state(&game);
    assert_eq!(snapshot.combat_log[0].kind, "hit");
    let target = snapshot
        .ships
        .into_iter()
        .find(|ship| ship.id == 2)
        .unwrap();
    assert_eq!(target.shields_remaining[0], 0);
    assert!(target.structure < 12);
}

#[test]
fn test_v2_multi_weapon_commit_same_phase() {
    let mut game = load_combat();
    game.set_ship_pos(1, Hex::new(1, 0)).unwrap();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, Hex::new(0, 0)).unwrap();
    game.set_ship_facing(2, 0).unwrap();
    allocate(&mut game, 1, 0, &[("beam_1", 1), ("torp_1", 1)], [0; 6]);
    allocate(&mut game, 2, 0, &[], [6, 0, 0, 0, 0, 0]);
    enter_firing(&mut game);

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
            ship: 1,
            weapon: "torp_1".into(),
            target: 2,
            shield_facing: 0,
        },
    )
    .unwrap();
    assert_eq!(StateSnapshot::from_game_state(&game).fire_commits.len(), 2);
}
