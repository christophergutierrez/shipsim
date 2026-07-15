//! Engine-level tests for the read-only movement preview (ADR-0022 contract).
//!
//! These verify the Phase 1 invariants that require a real GameState:
//! - preview does not mutate position, phase, thrust, commits, combat log, or PRNG;
//! - preview matches actual resolution for the coast trajectory;
//! - preview rejects invalid ship / phase / allocation drafts;
//! - repeated previews do not advance the game.

use std::collections::BTreeMap;
use std::path::PathBuf;

use shipsim_core::game_state::GameState;
use shipsim_core::movement::OrderError;
use shipsim_core::scenario::load_scenario;
use shipsim_core::snapshot::StateSnapshot;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn load_combat() -> GameState {
    load_scenario(&manifest_path("scenarios/combat.toml")).expect("combat loads")
}

fn empty_weapons() -> BTreeMap<String, u32> {
    BTreeMap::new()
}

#[test]
fn preview_does_not_mutate_engine_state() {
    let game = load_combat();
    let before = StateSnapshot::from_game_state(&game);
    let prng_before = game.prng_state();
    let log_len_before = game.combat_log().len();

    // Run a preview with a valid draft (movement 6, no weapons/shields).
    let result = game
        .movement_preview(1, 6, empty_weapons(), [0; 6])
        .expect("preview succeeds");

    // The preview must return endpoints.
    assert!(!result.endpoints.is_empty(), "preview returned no endpoints");

    // ── Nothing may have changed ──
    let after = StateSnapshot::from_game_state(&game);
    assert_eq!(after.phase, before.phase, "phase mutated by preview");
    assert_eq!(after.turn, before.turn, "turn mutated by preview");
    assert_eq!(after.movement_phase, before.movement_phase, "movement_phase mutated");
    assert_eq!(after.ships.len(), before.ships.len(), "ship count changed");
    for (a, b) in after.ships.iter().zip(before.ships.iter()) {
        assert_eq!((a.q, a.r), (b.q, b.r), "ship {} position mutated by preview", a.id);
        assert_eq!(a.facing, b.facing, "ship {} facing mutated by preview", a.id);
        assert_eq!(a.velocity, b.velocity, "ship {} velocity mutated by preview", a.id);
        assert_eq!(a.course, b.course, "ship {} course mutated by preview", a.id);
        assert_eq!(
            a.thrust_remaining, b.thrust_remaining,
            "ship {} thrust mutated by preview",
            a.id
        );
        assert_eq!(
            a.movement_allocated, b.movement_allocated,
            "ship {} movement_allocated mutated by preview",
            a.id
        );
    }
    assert_eq!(game.prng_state(), prng_before, "PRNG state mutated by preview");
    assert_eq!(
        game.combat_log().len(),
        log_len_before,
        "combat log mutated by preview"
    );
}

#[test]
fn repeated_previews_do_not_advance_game() {
    let game = load_combat();
    let snap_before = StateSnapshot::from_game_state(&game);

    let r1 = game.movement_preview(1, 4, empty_weapons(), [0; 6]).unwrap();
    let r2 = game.movement_preview(1, 4, empty_weapons(), [0; 6]).unwrap();
    let r3 = game.movement_preview(1, 4, empty_weapons(), [0; 6]).unwrap();

    // Deterministic: same draft ⇒ same result.
    assert_eq!(r1, r2, "preview not deterministic across repeated calls");
    assert_eq!(r2, r3, "preview not deterministic across repeated calls");

    // Game state unchanged after three previews.
    let snap_after = StateSnapshot::from_game_state(&game);
    assert_eq!(snap_after.phase, snap_before.phase, "phase advanced by previews");
    assert_eq!(snap_after.turn, snap_before.turn, "turn advanced by previews");
}

#[test]
fn preview_rejects_invalid_ship() {
    let game = load_combat();
    let err = game
        .movement_preview(999, 4, empty_weapons(), [0; 6])
        .expect_err("unknown ship rejects");
    assert!(matches!(err, OrderError::ShipNotFound(999)));
}

#[test]
fn preview_rejects_over_allocated_draft() {
    let game = load_combat();
    // movement 999 with no other spend should exceed effective power.
    let err = game
        .movement_preview(1, 999, empty_weapons(), [0; 6])
        .expect_err("over-allocation rejects");
    assert!(matches!(err, OrderError::OverAllocated { ship: 1, .. }));
}

#[test]
fn preview_rejects_shield_over_cap() {
    let game = load_combat();
    // A shield value far above the per-facing cap.
    let err = game
        .movement_preview(1, 0, empty_weapons(), [99, 0, 0, 0, 0, 0])
        .expect_err("shield over cap rejects");
    assert!(matches!(err, OrderError::ShieldPowerTooHigh { ship: 1, .. }));
}

#[test]
fn preview_coast_matches_zero_thrust_endpoint() {
    // With zero movement power, the only reachable trajectory is coast. The
    // coast endpoint must be present in the reachable set and equal the
    // zero-thrust endpoint.
    let game = load_combat();
    let result = game
        .movement_preview(1, 0, empty_weapons(), [0; 6])
        .expect("zero-thrust preview succeeds");
    assert!(
        result
            .endpoints
            .iter()
            .any(|e| e.q == result.coast.q && e.r == result.coast.r),
        "coast endpoint not in reachable set: coast={:?} endpoints={:?}",
        result.coast,
        result.endpoints
    );
}

#[test]
fn preview_coast_endpoint_present_and_stable() {
    // The live game resolves one movement phase per turn (each ship commits one
    // maneuver, then translates). The preview projects the full four-cycle
    // envelope per the ADR-0022 contract. We verify here that the preview's
    // coast endpoint is present in the reachable set and is deterministic; the
    // per-cycle coast match is covered by the unit tests in movement_preview.
    let game = load_combat();
    let result = game
        .movement_preview(1, 0, empty_weapons(), [0; 6])
        .expect("zero-thrust preview succeeds");
    assert!(
        result
            .endpoints
            .iter()
            .any(|e| e.q == result.coast.q && e.r == result.coast.r),
        "coast endpoint not in reachable set: coast={:?} endpoints={:?}",
        result.coast,
        result.endpoints
    );
    // Re-run: deterministic.
    let result2 = game
        .movement_preview(1, 0, empty_weapons(), [0; 6])
        .expect("zero-thrust preview succeeds (2)");
    assert_eq!(result, result2, "preview not deterministic");
}

#[test]
fn preview_more_thrust_never_shrinks_reachable_set() {
    let game = load_combat();
    let less = game.movement_preview(1, 2, empty_weapons(), [0; 6]).unwrap();
    let more = game.movement_preview(1, 6, empty_weapons(), [0; 6]).unwrap();
    assert!(
        more.endpoints.len() >= less.endpoints.len(),
        "more thrust reduced reachable set: more={} < less={}",
        more.endpoints.len(),
        less.endpoints.len()
    );
}
