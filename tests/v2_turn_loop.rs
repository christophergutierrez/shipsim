//! M5 gates: turn loop, EndTurn + end_turn_warning, multi-cycle, destruction win,
//! and per-turn reset. See `docs/PRD.md` and ADR-0022 (four-phase maneuver/fire loop).

use std::collections::BTreeMap;
use std::path::PathBuf;

use shipsim_core::game_state::ScenarioStatus;
use shipsim_core::motion::Maneuver;
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario;
use shipsim_core::snapshot::StateSnapshot;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn load_combat() -> shipsim_core::game_state::GameState {
    load_scenario(&manifest_path("scenarios/combat.toml")).expect("combat loads")
}

fn load_size_hit() -> shipsim_core::game_state::GameState {
    load_scenario(&manifest_path("scenarios/m8_size_hit.toml")).expect("size-hit loads")
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

fn commit_fire(
    game: &mut shipsim_core::game_state::GameState,
    ship: u32,
    weapon: &str,
    target: u32,
) {
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

/// Commit `Maneuver::Coast` for every listed ship, resolving the current
/// movement phase into the fire window.
fn coast_all(game: &mut shipsim_core::game_state::GameState, ships: &[u32]) {
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

fn ready_all(game: &mut shipsim_core::game_state::GameState, ships: &[u32]) {
    for &ship in ships {
        apply_order(game, Order::ReadyFire { ship }).expect("ready fire");
    }
}

/// G1: resolving a fire window always returns to the next movement phase (or
/// ends the turn after phase 4) — the frozen four-phase schedule is the only
/// termination condition, with no `move_remaining`-based re-entry heuristic.
#[test]
fn test_g1_fire_window_returns_to_next_movement_phase() {
    let mut game = load_combat();
    game.set_ship_pos(1, shipsim_core::hex::Hex::new(1, 0))
        .unwrap();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, shipsim_core::hex::Hex::new(0, 0))
        .unwrap();
    game.set_ship_facing(2, 0).unwrap();

    allocate(&mut game, 1, 2, &[("beam_1", 2)], [0; 6]);
    allocate(&mut game, 2, 0, &[], [0; 6]);
    assert_eq!(StateSnapshot::from_game_state(&game).phase, "movement");
    assert_eq!(StateSnapshot::from_game_state(&game).movement_phase, 1);

    coast_all(&mut game, &[1, 2]);
    assert_eq!(StateSnapshot::from_game_state(&game).phase, "firing");

    commit_fire(&mut game, 1, "beam_1", 2);
    ready_all(&mut game, &[1, 2]);

    let snapshot = StateSnapshot::from_game_state(&game);
    assert_eq!(snapshot.phase, "movement");
    assert_eq!(snapshot.movement_phase, 2);
    assert!(snapshot.fire_commits.is_empty());
    assert!(snapshot.ships_ready_fire.is_empty());
    assert!(snapshot.ships_committed_this_phase.is_empty());
}

/// G2: with no further actions available, the turn still runs its full four
/// movement/fire windows and then ends — the schedule terminates the turn
/// unconditionally, not because "no actions remain".
#[test]
fn test_g2_turn_end_after_four_windows() {
    let mut game = load_combat();
    game.set_ship_pos(1, shipsim_core::hex::Hex::new(1, 0))
        .unwrap();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, shipsim_core::hex::Hex::new(0, 0))
        .unwrap();
    game.set_ship_facing(2, 0).unwrap();

    allocate(&mut game, 1, 0, &[("beam_1", 2)], [0; 6]);
    allocate(&mut game, 2, 0, &[], [3, 0, 0, 0, 0, 0]);

    // Phase 1: fire the only charged weapon, then ready with nothing left to commit.
    coast_all(&mut game, &[1, 2]);
    commit_fire(&mut game, 1, "beam_1", 2);
    ready_all(&mut game, &[1, 2]);
    assert_eq!(StateSnapshot::from_game_state(&game).movement_phase, 2);

    // Phases 2-4: nothing left to fire; coast and ready through to turn end.
    for expected_phase in [2u8, 3, 4] {
        assert_eq!(
            StateSnapshot::from_game_state(&game).movement_phase,
            expected_phase
        );
        coast_all(&mut game, &[1, 2]);
        ready_all(&mut game, &[1, 2]);
    }

    assert_eq!(StateSnapshot::from_game_state(&game).phase, "turn_end");
}

/// G3: `end_turn_warning` tracks legal fire only (movement is no longer
/// optional under inertia — every ship commits a maneuver regardless); EndTurn
/// always advances the turn.
#[test]
fn test_g3_end_turn_warning_true_and_end_turn_advances() {
    let mut game = load_combat();
    game.set_ship_pos(1, shipsim_core::hex::Hex::new(1, 0))
        .unwrap();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, shipsim_core::hex::Hex::new(0, 0))
        .unwrap();
    game.set_ship_facing(2, 0).unwrap();

    allocate(&mut game, 1, 2, &[("beam_1", 2)], [0; 6]);
    allocate(&mut game, 2, 0, &[], [0; 6]);

    let before = StateSnapshot::from_game_state(&game);
    assert_eq!(before.phase, "movement");
    assert!(before.end_turn_warning, "ship1 has a legal charged shot");
    assert_eq!(before.turn, 1);

    apply_order(&mut game, Order::EndTurn).expect("end turn advances");

    let after = StateSnapshot::from_game_state(&game);
    assert_eq!(after.turn, 2);
    assert_eq!(after.phase, "allocate");
}

/// G4: warning is false once no legal fire remains (turn end reached).
#[test]
fn test_g4_end_turn_warning_false_when_no_actions() {
    let mut game = load_combat();
    game.set_ship_pos(1, shipsim_core::hex::Hex::new(1, 0))
        .unwrap();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, shipsim_core::hex::Hex::new(0, 0))
        .unwrap();
    game.set_ship_facing(2, 0).unwrap();

    allocate(&mut game, 1, 0, &[("beam_1", 2)], [0; 6]);
    allocate(&mut game, 2, 0, &[], [3, 0, 0, 0, 0, 0]);

    coast_all(&mut game, &[1, 2]);
    commit_fire(&mut game, 1, "beam_1", 2);
    ready_all(&mut game, &[1, 2]);
    for _ in 0..3 {
        coast_all(&mut game, &[1, 2]);
        ready_all(&mut game, &[1, 2]);
    }

    let snapshot = StateSnapshot::from_game_state(&game);
    assert_eq!(snapshot.phase, "turn_end");
    assert!(!snapshot.end_turn_warning);
}

/// G5: destroying the terminal target with v2 fire wins the scenario
/// immediately, without waiting for the remaining movement phases.
#[test]
fn test_g5_destruction_win() {
    let mut game = load_size_hit();
    game.set_ship_pos(1, shipsim_core::hex::Hex::new(1, 0))
        .unwrap();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, shipsim_core::hex::Hex::new(0, 0))
        .unwrap();
    game.set_ship_facing(2, 0).unwrap();
    game.set_ship_structure(2, 1).unwrap();

    allocate(&mut game, 1, 0, &[("beam_1", 4)], [0; 6]);
    allocate(&mut game, 2, 1, &[], [0; 6]);

    coast_all(&mut game, &[1, 2]);
    assert_eq!(StateSnapshot::from_game_state(&game).phase, "firing");

    commit_fire(&mut game, 1, "beam_1", 2);
    ready_all(&mut game, &[1, 2]);

    let snapshot = StateSnapshot::from_game_state(&game);
    assert_eq!(snapshot.status, ScenarioStatus::Won);
    assert_eq!(
        snapshot.phase, "turn_end",
        "a decided scenario parks at turn_end regardless of movement_phase"
    );
}

#[test]
fn test_player_fleet_destruction_is_lost() {
    let mut game = load_combat();
    game.set_ship_structure(1, 0).unwrap();
    game.refresh_status();

    assert_eq!(
        StateSnapshot::from_game_state(&game).status,
        ScenarioStatus::Lost
    );
}

/// G6: EndTurn clears thrust/shields/fired flags. Protocol 3: weapon charge
/// *carries* across turns; shields always re-buy from 0.
#[test]
fn test_g6_new_turn_resets_allocation() {
    let mut game = load_fleet();

    allocate(&mut game, 1, 3, &[("beam_1", 2)], [2, 0, 0, 0, 0, 0]);
    allocate(&mut game, 2, 0, &[], [0; 6]);
    allocate(&mut game, 3, 0, &[], [0; 6]);
    allocate(&mut game, 4, 0, &[], [0; 6]);
    assert_eq!(StateSnapshot::from_game_state(&game).phase, "movement");

    apply_order(&mut game, Order::EndTurn).expect("end turn resets");

    let snapshot = StateSnapshot::from_game_state(&game);
    assert_eq!(snapshot.turn, 2);
    assert_eq!(snapshot.phase, "allocate");
    for ship in &snapshot.ships {
        assert_eq!(ship.movement_allocated, 0, "ship {}", ship.id);
        assert_eq!(ship.thrust_remaining, 0, "ship {}", ship.id);
        assert_eq!(ship.shields_powered, [0; 6], "ship {}", ship.id);
        assert_eq!(ship.shields_remaining, [0; 6], "ship {}", ship.id);
        for weapon in &ship.weapons {
            assert!(!weapon.fired, "ship {} weapon {}", ship.id, weapon.id);
        }
    }
    // Carried charge on ship 1 beam_1.
    let beam = snapshot
        .ships
        .iter()
        .find(|s| s.id == 1)
        .unwrap()
        .weapons
        .iter()
        .find(|w| w.id == "beam_1")
        .unwrap();
    assert_eq!(beam.charge, 2);
}
