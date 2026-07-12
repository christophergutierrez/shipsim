//! Combat v2 acceptance (M9).
//!
//! Two full turn cycles — allocate, move decision, commit fire, ready, end turn
//! — driven in-process via `apply_order`, plus a byte-lock harness test against
//! the golden fixture `tests/fixtures/v2/duel.jsonl`.
//!
//! Spec: `docs/PRD.md` and ADR-0020.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn empty_shields() -> [u32; 6] {
    [0; 6]
}

fn charged_beam() -> BTreeMap<String, u32> {
    let mut w = BTreeMap::new();
    w.insert("beam_1".to_string(), 1);
    w
}

/// Run one full turn cycle up to (but not including) EndTurn: allocate both
/// ships, pass-move both, ship 1 fires beam_1 at ship 2, both ready. Returns
/// the resolved combat log for this turn (EndTurn clears it).
fn run_fire_cycle(
    game: &mut shipsim_core::game_state::GameState,
) -> Vec<shipsim_core::game_state::CombatLogEvent> {
    apply_order(
        game,
        Order::Allocate {
            ship: 1,
            movement: 4,
            weapons: charged_beam(),
            shields: empty_shields(),
        },
    )
    .unwrap();
    apply_order(
        game,
        Order::Allocate {
            ship: 2,
            movement: 2,
            weapons: charged_beam(),
            shields: empty_shields(),
        },
    )
    .unwrap();

    // Movement phase: both ships are adjacent and blocked forward, so each
    // passes its move decision. This still exercises the movement phase and
    // the active-mover rotation.
    //
    // M3: move order is now by thrust_remaining descending. Ship 2 (escort,
    // 2 power × 4 thrust/power = 8 thrust) moves before ship 1 (cruiser,
    // 4 power × 1 = 4 thrust).
    apply_order(game, Order::PassMove { ship: 2 }).unwrap();
    apply_order(game, Order::PassMove { ship: 1 }).unwrap();

    // Firing phase: ship 1 fires beam_1 at ship 2 (adjacent, forward arc).
    apply_order(
        game,
        Order::CommitFire {
            ship: 1,
            weapon: "beam_1".to_string(),
            target: 2,
            shield_facing: 0,
        },
    )
    .unwrap();
    apply_order(game, Order::ReadyFire { ship: 1 }).unwrap();
    // Ship 2 must also ready before the batch resolves.
    apply_order(game, Order::ReadyFire { ship: 2 }).unwrap();

    // The fire batch resolves when all living ships are ready; capture the
    // log before EndTurn clears it.
    let log = game.combat_log().to_vec();
    assert!(!log.is_empty(), "combat log populated after fire");
    log
}

/// Gate 1: acceptance test green and deterministic with fixed seed.
///
/// Two move cycles (two full turns) of allocate → move → fire → end turn,
/// driven in-process. The scenario seed (4242) fixes the PRNG so the combat
/// log is deterministic: turn 1 hits for 2 damage, turn 2 misses.
#[test]
fn test_v2_acceptance_two_move_cycles() {
    let mut game = load_scenario(&manifest_path("scenarios/combat.toml")).expect("scenario");
    assert_eq!(game.seed(), 4242, "scenario seed is fixed");
    let struct_before = game.ship(2).expect("ship 2").structure();

    // Turn 1 — fire resolves to a hit (damage 2).
    let log_t1 = run_fire_cycle(&mut game);
    assert_eq!(log_t1.len(), 1, "one resolved shot in turn 1");
    assert_eq!(log_t1[0].attacker, 1);
    assert_eq!(log_t1[0].target, 2);
    assert_eq!(log_t1[0].kind, "hit", "turn 1 is a hit");
    assert_eq!(log_t1[0].damage, 2, "turn 1 hit deals 2 damage");
    let struct_after_t1 = game.ship(2).expect("ship 2").structure();
    assert_eq!(
        struct_after_t1,
        struct_before - 2,
        "ship 2 took 2 damage in turn 1: {struct_before} -> {struct_after_t1}"
    );
    apply_order(&mut game, Order::EndTurn).unwrap();

    // Turn 2 — second move cycle; fire resolves to a miss (damage 0).
    let log_t2 = run_fire_cycle(&mut game);
    assert_eq!(log_t2.len(), 1, "one resolved shot in turn 2");
    assert_eq!(log_t2[0].kind, "miss", "turn 2 is a miss");
    assert_eq!(log_t2[0].damage, 0, "turn 2 miss deals 0 damage");
    let struct_after_t2 = game.ship(2).expect("ship 2").structure();
    assert_eq!(
        struct_after_t2, struct_after_t1,
        "ship 2 took no damage in turn 2 (miss): {struct_after_t1} -> {struct_after_t2}"
    );
    apply_order(&mut game, Order::EndTurn).unwrap();

    assert_eq!(game.turn_number(), 3, "advanced two turns to turn 3");
}

/// Gate 2: fixture byte-lock harness test.
///
/// Runs the shipsim binary against `tests/fixtures/v2/duel_orders.jsonl` and
/// asserts the stdout NDJSON stream is byte-identical to the golden fixture
/// `tests/fixtures/v2/duel.jsonl`. Regenerate the fixture after intentional
/// snapshot/harness changes:
///
/// ```bash
/// cargo run -q --bin shipsim -- \
///   --scenario scenarios/combat.toml \
///   --orders tests/fixtures/v2/duel_orders.jsonl \
///   > tests/fixtures/v2/duel.jsonl
/// ```
#[test]
fn test_v2_duel_fixture_byte_lock() {
    let output = Command::new(env!("CARGO_BIN_EXE_shipsim"))
        .arg("--scenario")
        .arg(manifest_path("scenarios/combat.toml"))
        .arg("--orders")
        .arg(manifest_path("tests/fixtures/v2/duel_orders.jsonl"))
        .output()
        .expect("shipsim binary");

    assert!(
        output.status.success(),
        "shipsim failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let expected = std::fs::read(manifest_path("tests/fixtures/v2/duel.jsonl"))
        .expect("golden fixture duel.jsonl");
    assert_eq!(
        output.stdout, expected,
        "duel.jsonl fixture is stale; regenerate with the command in the test doc-comment"
    );
}
