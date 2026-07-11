//! M6 gates: greedy v2 AI + multi-ship scenarios driven through the v2 phase
//! machine. See `docs/PRD.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use shipsim_core::game_state::ScenarioStatus;
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario;
use shipsim_core::snapshot::StateSnapshot;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn load_duel() -> shipsim_core::game_state::GameState {
    load_scenario(&manifest_path("scenarios/v2_duel.toml")).expect("v2_duel loads")
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

/// G1: an AI-only v2 duel, driven entirely by the v2 NPC driver, completes at
/// least three turns (or reaches a decisive win) without panicking.
#[test]
fn test_g1_ai_duel_runs_three_turns_without_panic() {
    let mut game = load_duel();

    let mut guard = 0;
    loop {
        game.resolve_v2_npc_actions();
        let snapshot = StateSnapshot::from_game_state(&game);
        if snapshot.turn >= 3 || snapshot.status == ScenarioStatus::Won {
            break;
        }
        guard += 1;
        assert!(guard < 100, "driver made no progress toward turn 3 / win");
    }

    let snapshot = StateSnapshot::from_game_state(&game);
    assert!(
        snapshot.turn >= 3 || snapshot.status == ScenarioStatus::Won,
        "expected turn>=3 or Won, got turn={} status={:?}",
        snapshot.turn,
        snapshot.status
    );
}

/// G2: a 2v2 fleet. After every ship allocates, `move_order` holds all four ids
/// and each of the four gets exactly one movement decision this phase.
#[test]
fn test_g2_fleet_move_order_and_decisions() {
    let mut game = load_fleet();

    // Everyone allocates some movement so all four are movers in one phase.
    allocate(&mut game, 1, 2, &[], [0; 6]);
    allocate(&mut game, 2, 2, &[], [0; 6]);
    allocate(&mut game, 3, 2, &[], [0; 6]);
    allocate(&mut game, 4, 2, &[], [0; 6]);

    let move_order = StateSnapshot::from_game_state(&game).move_order;
    assert_eq!(move_order.len(), 4, "move_order = {move_order:?}");
    for id in [1u32, 2, 3, 4] {
        assert!(move_order.contains(&id), "move_order missing {id}");
    }

    // Drive the movement phase one active mover at a time; each of the four must
    // get a decision (here a legitimate pass) before the phase closes.
    let mut decided: BTreeSet<u32> = BTreeSet::new();
    let mut guard = 0;
    while StateSnapshot::from_game_state(&game).phase == "movement" {
        guard += 1;
        assert!(guard < 20, "movement phase did not converge");
        let Some(active) = game.active_v2_mover() else {
            break;
        };
        decided.insert(active);
        apply_order(&mut game, Order::PassMove { ship: active }).expect("pass move");
    }

    assert_eq!(
        decided,
        BTreeSet::from([1, 2, 3, 4]),
        "each of the four ships should get a move decision"
    );
    assert_eq!(StateSnapshot::from_game_state(&game).phase, "firing");
}
