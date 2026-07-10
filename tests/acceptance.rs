//! Combat v2 acceptance: allocate, move into range, commit fire, ready, end turn.

use std::collections::BTreeMap;
use std::path::PathBuf;

use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn empty_shields() -> [u32; 6] {
    [0; 6]
}

#[test]
fn test_v2_allocate_move_fire_end_turn() {
    let mut game = load_scenario(&manifest_path("scenarios/combat.toml")).expect("scenario");

    // --- Turn 1, Allocation phase ---
    // Ship 1 (heavy cruiser, power 22): allocate movement + charge beam_1.
    let mut weapons_1 = BTreeMap::new();
    weapons_1.insert("beam_1".to_string(), 1);
    apply_order(
        &mut game,
        Order::Allocate {
            ship: 1,
            movement: 4,
            weapons: weapons_1,
            shields: empty_shields(),
        },
    )
    .unwrap();

    // Ship 2 (escort, power 14): allocate movement + charge beam_1.
    let mut weapons_2 = BTreeMap::new();
    weapons_2.insert("beam_1".to_string(), 1);
    apply_order(
        &mut game,
        Order::Allocate {
            ship: 2,
            movement: 2,
            weapons: weapons_2,
            shields: empty_shields(),
        },
    )
    .unwrap();

    // --- Movement phase ---
    // Ship 1 is at (1,0) facing 3 (toward ship 2 at (0,0)). They are adjacent.
    // Move ship 1 forward (toward ship 2) — but (0,0) is occupied, so just pass.
    apply_order(&mut game, Order::PassMove { ship: 1 }).unwrap();
    apply_order(&mut game, Order::PassMove { ship: 2 }).unwrap();

    // --- Firing phase ---
    // Ship 1 fires beam_1 at ship 2 (adjacent, range 1, forward arc).
    apply_order(
        &mut game,
        Order::CommitFire {
            ship: 1,
            weapon: "beam_1".to_string(),
            target: 2,
            shield_facing: 0,
        },
    )
    .unwrap();
    apply_order(&mut game, Order::ReadyFire { ship: 1 }).unwrap();
    // Ship 2 must also ready before the batch resolves.
    apply_order(&mut game, Order::ReadyFire { ship: 2 }).unwrap();

    // The fire batch resolves when all living ships are ready; the combat log
    // captures the resolved shot. (EndTurn clears the per-turn log.)
    assert!(!game.combat_log().is_empty());

    // --- End turn ---
    apply_order(&mut game, Order::EndTurn).unwrap();
}
