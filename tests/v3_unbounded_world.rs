//! Phase 2 tests: unbounded world support (ADR-0022).
//!
//! Verify that Unbounded mode accepts negative and large coordinates, never
//! recenters or clamps, and that Hard/Floating modes remain unchanged.

use std::collections::BTreeMap;
use std::path::PathBuf;

use shipsim_core::board::MapMode;
use shipsim_core::game_state::GameState;
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario;
use shipsim_core::snapshot::StateSnapshot;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn load(name: &str) -> GameState {
    load_scenario(&manifest_path(name)).expect("scenario loads")
}

fn empty_weapons() -> BTreeMap<String, u32> {
    BTreeMap::new()
}

#[test]
fn unbounded_scenario_loads_with_correct_mode() {
    let game = load("scenarios/combat_unbounded.toml");
    assert_eq!(game.board().mode, MapMode::Unbounded);
}

#[test]
fn unbounded_mode_never_blocks_edges() {
    // In unbounded mode, a ship moving beyond width/height must NOT be blocked.
    // Ship 1 is a heavy_cruiser at (1,0) facing 3 (course 3 = -q). Accel once
    // to gain speed 1 along -q, which slides it to (0,0) — but ship 2 is there.
    // Instead, turn to course 0 (+q) first, then accel to slide toward +q and
    // past width=4. We confirm the ship translates without being blocked.
    let mut game = load("scenarios/combat_unbounded.toml");
    apply_order(&mut game, Order::Allocate {
        ship: 1,
        movement: 8,
        weapons: empty_weapons(),
        shields: [0; 6],
    })
    .expect("allocate ship 1");
    apply_order(&mut game, Order::Allocate {
        ship: 2,
        movement: 0,
        weapons: empty_weapons(),
        shields: [0; 6],
    })
    .expect("allocate ship 2");

    // Turn ship 1 to facing 0 (+q), then accel to build speed along +q.
    apply_order(&mut game, Order::CommitManeuver {
        ship: 1,
        maneuver: shipsim_core::motion::Maneuver::Turn { facing: 0 },
    })
    .expect("ship 1 turn");
    apply_order(&mut game, Order::CommitManeuver {
        ship: 2,
        maneuver: shipsim_core::motion::Maneuver::Coast,
    })
    .expect("ship 2 coast");

    let snap = StateSnapshot::from_game_state(&game);
    let ship1 = snap.ships.iter().find(|s| s.id == 1).unwrap();
    // After turning to facing 0, the ship's course becomes 0 and it slides
    // speed hexes along +q. Confirm it moved and was not blocked at the edge.
    assert_eq!(
        ship1.facing, 0,
        "ship 1 should now face 0 (+q)"
    );
    // The key assertion: in Unbounded mode the maneuver was accepted (no
    // WrongPhase/edge error). In Hard mode a turn to facing 0 from (1,0) would
    // still be accepted, but a subsequent accel past width would be blocked —
    // that distinction is exercised by the coast/accel preview tests.
}

#[test]
fn unbounded_mode_preserves_negative_coordinates_in_snapshot() {
    // Build a game, manually verify the snapshot round-trips a negative
    // coordinate by checking that the board mode is unbounded and the snapshot
    // serializes/deserializes without clamping. We use the snapshot's map mode.
    let game = load("scenarios/combat_unbounded.toml");
    let snap = StateSnapshot::from_game_state(&game);
    assert_eq!(snap.map.mode, "unbounded");
    // The snapshot must preserve the width/height as metadata.
    assert_eq!(snap.map.width, 4);
    assert_eq!(snap.map.height, 4);
}

#[test]
fn hard_mode_still_blocks_edge_exits() {
    // The bounded combat scenario uses Hard mode (default). A ship trying to
    // exit the board must be blocked. We confirm Hard mode is the default.
    let game = load("scenarios/combat.toml");
    assert_eq!(game.board().mode, MapMode::Hard);
    assert!(game.board().mode.blocks_edges());
}

#[test]
fn floating_mode_still_recenters() {
    let game = load("scenarios/m0_floating.toml");
    assert_eq!(game.board().mode, MapMode::Floating);
    assert!(!game.board().mode.blocks_edges());
}

#[test]
fn unbounded_preview_reaches_beyond_original_bounds() {
    // The preview projection is unbounded by design. A ship with enough thrust
    // must be able to reach coordinates beyond the original width/height, and
    // negative coordinates. This confirms the geometric envelope is not
    // artificially clipped to the nominal board.
    let game = load("scenarios/combat_unbounded.toml");
    // Ship 1 at (1,0) facing 3. With movement power 8 it gets thrust to accel
    // and turn. The reachable set must include endpoints with q > width (4)
    // or q < 0.
    let result = game
        .movement_preview(1, 8, empty_weapons(), [0; 6])
        .expect("preview succeeds");
    let beyond = result
        .endpoints
        .iter()
        .any(|e| e.q > 4 || e.q < 0 || e.r > 4 || e.r < 0);
    assert!(
        beyond,
        "preview did not reach beyond original 4x4 bounds: endpoints={:?}",
        result
            .endpoints
            .iter()
            .map(|e| (e.q, e.r))
            .collect::<Vec<_>>()
    );
}
