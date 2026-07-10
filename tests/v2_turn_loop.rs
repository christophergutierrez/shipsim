//! M5 gates: turn loop, EndTurn + end_turn_warning, multi-cycle, destruction win,
//! and per-turn reset. See `implementation-plan-combat-v2.md` milestone M5.

use std::collections::BTreeMap;
use std::path::PathBuf;

use shipsim_core::game_state::ScenarioStatus;
use shipsim_core::movement::{apply_order, MoveMode, Order};
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

fn commit(game: &mut shipsim_core::game_state::GameState, ship: u32, weapon: &str, target: u32) {
    apply_order(
        game,
        Order::CommitFire {
            ship,
            weapon: weapon.into(),
            target,
            shield_facing: 0,
        },
    )
    .expect("commit fire");
}

/// G1: resolving a firing batch while an attacker still has `move_remaining > 0`
/// returns to a fresh movement phase with commits/ready/moved cleared.
#[test]
fn test_g1_loop_back_to_movement_when_move_remaining() {
    let mut game = load_combat();
    game.set_ship_pos(1, shipsim_core::hex::Hex::new(1, 0)).unwrap();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, shipsim_core::hex::Hex::new(0, 0)).unwrap();
    game.set_ship_facing(2, 0).unwrap();

    // ship1 keeps movement power (passes its move); ship2 has none (auto-pass).
    allocate(&mut game, 1, 2, &[("beam_1", 2)], [0; 6]);
    allocate(&mut game, 2, 0, &[], [0; 6]);

    apply_order(&mut game, Order::PassMove { ship: 1 }).expect("pass keeps move power");
    assert_eq!(StateSnapshot::from_game_state(&game).phase, "firing");

    commit(&mut game, 1, "beam_1", 2);
    apply_order(&mut game, Order::ReadyFire { ship: 1 }).unwrap();
    apply_order(&mut game, Order::ReadyFire { ship: 2 }).unwrap();

    let snapshot = StateSnapshot::from_game_state(&game);
    assert_eq!(snapshot.phase, "movement");
    assert!(snapshot.fire_commits.is_empty());
    assert!(snapshot.ships_ready_fire.is_empty());
    assert!(snapshot.ships_moved_this_phase.is_empty());
}

/// G2: resolving when no ship can move and no charged legal weapon remains ends the turn.
#[test]
fn test_g2_turn_end_when_no_actions_remain() {
    let mut game = load_combat();
    game.set_ship_pos(1, shipsim_core::hex::Hex::new(1, 0)).unwrap();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, shipsim_core::hex::Hex::new(0, 0)).unwrap();
    game.set_ship_facing(2, 0).unwrap();

    allocate(&mut game, 1, 0, &[("beam_1", 2)], [0; 6]);
    allocate(&mut game, 2, 0, &[], [3, 0, 0, 0, 0, 0]);

    commit(&mut game, 1, "beam_1", 2);
    apply_order(&mut game, Order::ReadyFire { ship: 1 }).unwrap();
    apply_order(&mut game, Order::ReadyFire { ship: 2 }).unwrap();

    assert_eq!(StateSnapshot::from_game_state(&game).phase, "turn_end");
}

/// G3: mid-turn warning is true while usable move/fire remains; EndTurn advances the turn.
#[test]
fn test_g3_end_turn_warning_true_and_end_turn_advances() {
    let mut game = load_combat();
    game.set_ship_pos(1, shipsim_core::hex::Hex::new(1, 0)).unwrap();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, shipsim_core::hex::Hex::new(0, 0)).unwrap();
    game.set_ship_facing(2, 0).unwrap();

    allocate(&mut game, 1, 2, &[("beam_1", 2)], [0; 6]);
    allocate(&mut game, 2, 0, &[], [0; 6]);

    let before = StateSnapshot::from_game_state(&game);
    assert_eq!(before.phase, "movement");
    assert!(before.end_turn_warning, "ship1 still has move power");
    assert_eq!(before.turn, 1);

    apply_order(&mut game, Order::EndTurn).expect("end turn advances");

    let after = StateSnapshot::from_game_state(&game);
    assert_eq!(after.turn, 2);
    assert_eq!(after.phase, "allocate");
}

/// G4: warning is false once no legal actions remain (turn end reached).
#[test]
fn test_g4_end_turn_warning_false_when_no_actions() {
    let mut game = load_combat();
    game.set_ship_pos(1, shipsim_core::hex::Hex::new(1, 0)).unwrap();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, shipsim_core::hex::Hex::new(0, 0)).unwrap();
    game.set_ship_facing(2, 0).unwrap();

    allocate(&mut game, 1, 0, &[("beam_1", 2)], [0; 6]);
    allocate(&mut game, 2, 0, &[], [3, 0, 0, 0, 0, 0]);

    commit(&mut game, 1, "beam_1", 2);
    apply_order(&mut game, Order::ReadyFire { ship: 1 }).unwrap();
    apply_order(&mut game, Order::ReadyFire { ship: 2 }).unwrap();

    let snapshot = StateSnapshot::from_game_state(&game);
    assert_eq!(snapshot.phase, "turn_end");
    assert!(!snapshot.end_turn_warning);
}

/// G5: destroying the terminal target with v2 fire wins the scenario.
#[test]
fn test_g5_destruction_win() {
    let mut game = load_combat();
    game.set_ship_pos(1, shipsim_core::hex::Hex::new(1, 0)).unwrap();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, shipsim_core::hex::Hex::new(0, 0)).unwrap();
    game.set_ship_facing(2, 0).unwrap();
    game.set_ship_structure(2, 1).unwrap();

    // Different movement allocations break the initiative tie so no PRNG roll is
    // consumed before to-hit: seed 4242's first d20 is 16, a hit vs beam r1 (18).
    allocate(&mut game, 1, 0, &[("beam_1", 4)], [0; 6]);
    allocate(&mut game, 2, 1, &[], [0; 6]);

    apply_order(&mut game, Order::PassMove { ship: 2 }).expect("ship2 passes its move");
    assert_eq!(StateSnapshot::from_game_state(&game).phase, "firing");

    commit(&mut game, 1, "beam_1", 2);
    apply_order(&mut game, Order::ReadyFire { ship: 1 }).unwrap();
    apply_order(&mut game, Order::ReadyFire { ship: 2 }).unwrap();

    assert_eq!(StateSnapshot::from_game_state(&game).status, ScenarioStatus::Won);
}

/// G6: a fresh turn clears allocations, resets keel to stopped, zeroes shields and
/// weapon charges, and clears weapon fired flags.
#[test]
fn test_g6_new_turn_resets_allocation() {
    let mut game = load_fleet();

    allocate(&mut game, 1, 3, &[("beam_1", 2)], [2, 0, 0, 0, 0, 0]);
    allocate(&mut game, 2, 0, &[], [0; 6]);
    allocate(&mut game, 3, 0, &[], [0; 6]);
    allocate(&mut game, 4, 0, &[], [0; 6]);

    assert_eq!(StateSnapshot::from_game_state(&game).phase, "movement");
    // ship1 moves forward -> keel becomes forward, spends move power.
    apply_order(
        &mut game,
        Order::Move {
            ship: 1,
            mode: MoveMode::Forward,
        },
    )
    .expect("ship1 forward");

    apply_order(&mut game, Order::EndTurn).expect("end turn resets");

    let snapshot = StateSnapshot::from_game_state(&game);
    assert_eq!(snapshot.turn, 2);
    assert_eq!(snapshot.phase, "allocate");
    for ship in &snapshot.ships {
        assert_eq!(ship.movement_allocated, 0, "ship {}", ship.id);
        assert_eq!(ship.move_remaining, 0, "ship {}", ship.id);
        assert_eq!(ship.keel, "stopped", "ship {}", ship.id);
        assert_eq!(ship.shields_powered, [0; 6], "ship {}", ship.id);
        assert_eq!(ship.shields_remaining, [0; 6], "ship {}", ship.id);
        for weapon in &ship.weapons {
            assert_eq!(weapon.charge, 0, "ship {} weapon {}", ship.id, weapon.id);
            assert!(!weapon.fired, "ship {} weapon {}", ship.id, weapon.id);
        }
    }
}
