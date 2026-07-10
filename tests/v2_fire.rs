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

fn enter_firing(game: &mut shipsim_core::game_state::GameState) {
    while StateSnapshot::from_game_state(game).phase == "movement" {
        let next = StateSnapshot::from_game_state(game)
            .move_order
            .into_iter()
            .find(|id| {
                let snapshot = StateSnapshot::from_game_state(game);
                let ship = snapshot.ships.iter().find(|ship| ship.id == *id).unwrap();
                ship.move_remaining > 0 && !snapshot.ships_moved_this_phase.contains(id)
            });
        if let Some(ship) = next {
            apply_order(game, Order::PassMove { ship }).expect("pass move");
        } else {
            break;
        }
    }
    assert_eq!(StateSnapshot::from_game_state(game).phase, "firing");
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
    allocate(&mut game, 1, 2, &[("beam_1", 1)], [0; 6]);
    allocate(&mut game, 2, 0, &[("beam_1", 1)], [2, 0, 0, 0, 0, 0]);
    allocate(&mut game, 3, 0, &[("beam_1", 1)], [0; 6]);
    allocate(&mut game, 4, 0, &[("beam_1", 1)], [0; 6]);

    enter_firing(&mut game);
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

    game.start_next_movement_phase_for_tests();
    apply_order(
        &mut game,
        Order::Move {
            ship: 1,
            mode: MoveMode::Forward,
        },
    )
    .expect("close one");
    game.start_next_movement_phase_for_tests();
    apply_order(
        &mut game,
        Order::Move {
            ship: 1,
            mode: MoveMode::Forward,
        },
    )
    .expect("close two");

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
    // M5 loop decision: ship1's torp and ship2's beam remain charged and legal, so the
    // batch closes and the turn loops back to a fresh firing phase (nobody can move here).
    assert_eq!(snapshot.phase, "firing");
    assert!(snapshot.fire_commits.is_empty());
    assert!(snapshot.ships_ready_fire.is_empty());

    // The uncommitted torp is committable in the new firing phase.
    apply_order(
        &mut game,
        Order::CommitFire {
            ship: 1,
            weapon: "torp_1".into(),
            target: 2,
            shield_facing: 0,
        },
    )
    .expect("torp committable in the looped-back firing phase");
}

#[test]
fn test_v2_miss_consumes_weapon_without_damage() {
    let mut game = load_combat();
    game.set_ship_pos(1, Hex::new(1, 0)).unwrap();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, Hex::new(0, 0)).unwrap();
    game.set_ship_facing(2, 0).unwrap();
    game.set_ship_structure(2, 12).unwrap();
    // Equal zero movement consumes one PRNG tie roll; seed 4242 then rolls 19
    // on d20, which misses a range-1 beam threshold of 18.
    allocate(&mut game, 1, 0, &[("beam_1", 2)], [0; 6]);
    allocate(&mut game, 2, 0, &[], [3, 0, 0, 0, 0, 0]);

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
