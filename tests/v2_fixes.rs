//! Regression tests for v2 engine fixes E1–E4.
//! Mirrors construction style of tests/v2_turn_loop.rs, v2_allocate, v2_ai, v2_fire.
//!
//!   E1 - scripted-controller ships must wait for external orders (not NPC-driven).
//!   E2 - a no-progress move/fire cycle must reach TurnEnd.
//!   E3 - `ai::seek_target` is side-aware (no friendly fire on same side).
//!   E4 - snapshot exposes `power_available` (= Ship::effective_power()).

use std::collections::BTreeMap;
use std::path::PathBuf;

use shipsim_core::hex::Hex;
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

/// E1: `resolve_v2_npc_actions` must NOT auto-advance `Scripted` ships.
/// "Scripted" means externally driven by an order script — see
/// tests/fixtures/v2/duel_orders.jsonl, which drives a scripted ship's turn
/// explicitly via its own `allocate` / `pass_move` / `ready_fire` orders. If
/// the engine auto-allocates a zeroed turn for a scripted ship the moment the
/// player acts, a script that later tries to allocate that same ship gets
/// rejected as a duplicate of the auto-zero (`AlreadyAllocated`), and any
/// move/fire intent the script wanted for that ship is silently dropped.
///
/// The interactive-deadlock problem this was originally meant to solve (a
/// human player stuck because nobody drives the scripted ship) is being fixed
/// client-side instead, since the client is the order source in interactive
/// play. The engine's contract stays simple: Scripted ships wait for whoever
/// owns their orders — the v2 NPC driver's remit is GreedySeek ships only.
#[test]
fn e1_scripted_ships_wait_for_external_orders() {
    let mut game = load_combat();
    // scenarios/combat.toml: ship 1 = player, ship 2 = scripted.
    allocate(&mut game, 1, 2, &[("beam_1", 2)], [0; 6]);

    game.resolve_v2_npc_actions();
    let after = StateSnapshot::from_game_state(&game);
    assert_eq!(
        after.phase, "allocate",
        "resolve_v2_npc_actions must not auto-advance a scripted ship past \
         Allocate; got phase={:?} ships_allocated_this_turn={:?}",
        after.phase, after.ships_allocated_this_turn
    );
    assert!(
        !after.ships_allocated_this_turn.contains(&2),
        "scripted ship 2 should still be pending its own external allocate \
         order, not auto-allocated by the NPC driver; ships_allocated_this_turn={:?}",
        after.ships_allocated_this_turn
    );
}

// E2 ("no-progress move/fire cycle must reach TurnEnd") tested the
// `can_any_move()` re-entry heuristic directly. That heuristic is deleted in
// M4 (ADR-0022): the turn now terminates after a fixed four-phase schedule by
// construction, with no re-entry condition to regress. The equivalent
// guarantee is covered by `tests/inertial_movement.rs`'s coasting-ship
// termination test.

/// E3: `ai::seek_target` (src/ai.rs:7) picks the nearest *other* ship with no
/// notion of side, so in a scenario with two AI ships closer to each other than
/// to the player, each AI ship's greedy fire logic targets its own ally
/// instead of the player. Side rule (per the fix ticket): player-controlled
/// ships (no NPC controller) are one side; all NPC ships (ai/scripted) are the
/// other.
///
/// Built from scenarios/fleet.toml (ships 1,2 = player; 3,4 = ai): ship 2 is
/// removed from play (destroyed) so the field is exactly one player ship (1)
/// plus two allied AI ships (3, 4) placed adjacent to and facing each other —
/// closer to one another (distance 1) than to the player (distance 4, still
/// well inside beam range 10).
///
/// Ship 1 (player), ship 4, and ship 3 are placed colinear (all at r = 3,
/// decreasing q) precisely so ship 3's forward arc bears on *both* — this
/// keeps the shot legal regardless of which one `seek_target` resolves to, so
/// the test isolates targeting rather than arc geometry. Verified against
/// src/arc.rs: `bearing_to((5,3), (1,3))` has a unique nearest neighbor at
/// facing 3 (no tie), giving `relative_bearing` 0 (dead ahead, in the Forward
/// mount's `[0]` arc) at range 4, where `beam_damage(4, 4) = Some(6)`.
#[test]
fn e3_ai_never_targets_own_side() {
    let mut game = load_fleet();
    game.set_ship_structure(2, 0).unwrap(); // remove ship 2 from play

    game.set_ship_pos(3, Hex::new(5, 3)).unwrap();
    game.set_ship_facing(3, 3).unwrap();
    game.set_ship_pos(4, Hex::new(4, 3)).unwrap();
    game.set_ship_facing(4, 0).unwrap();
    // Ship 1 (player) stays at its scenario default (1, 3).

    allocate(&mut game, 1, 0, &[], [0; 6]);
    allocate(&mut game, 3, 0, &[("beam_1", 4)], [0; 6]);
    allocate(&mut game, 4, 0, &[("beam_1", 4)], [0; 6]);
    // Coast every living ship through the current movement phase (ADR-0022 M4:
    // every living ship must commit exactly once) — Coast makes no facing/course
    // change, isolating the firing-phase target selection under test here.
    for id in [1u32, 3, 4] {
        apply_order(
            &mut game,
            Order::CommitManeuver {
                ship: id,
                maneuver: Maneuver::Coast,
            },
        )
        .expect("coast commits");
    }
    assert_eq!(StateSnapshot::from_game_state(&game).phase, "firing");

    game.resolve_v2_npc_actions();

    let snapshot = StateSnapshot::from_game_state(&game);
    assert!(
        !snapshot.fire_commits.is_empty(),
        "expected the AI driver to commit at least one shot"
    );
    let is_npc = |id: u32| game.controller_label(id) != "player";
    for fire_commit in &snapshot.fire_commits {
        assert!(
            !(is_npc(fire_commit.ship) && is_npc(fire_commit.target)),
            "AI ship {} illegally targeted fellow NPC ship {} (side-blind seek_target); \
             all fire_commits={:?}",
            fire_commit.ship,
            fire_commit.target,
            snapshot.fire_commits
        );
    }
}

/// E4: `Ship::effective_power()` (src/ship.rs:50) accounts for power_sys
/// damage, but `snapshot.rs` only ever serializes the static `power` design
/// pool, so clients can show a phantom power budget after damage (live bug:
/// engine rejected "allocated 14 power, only 11 available" while the client
/// showed pool=22). Intended: the snapshot's ship JSON gains a
/// `power_available` field equal to `effective_power()`.
///
/// `ShipSnapshot` has no such field today, so this asserts against the
/// serialized JSON directly (via serde_json, already a crate dependency)
/// rather than a struct field, to fail with a clean assertion instead of a
/// compile error.
#[test]
fn e4_snapshot_exposes_power_available() {
    let game = load_combat();
    // ship 1 = heavy_cruiser, design power = 22 (data/ships/heavy_cruiser.toml),
    // undamaged, so power_available should equal power exactly.
    let snapshot = StateSnapshot::from_game_state(&game);
    let json = serde_json::to_value(&snapshot).expect("snapshot serializes to JSON");
    let ship1_json = json["ships"]
        .as_array()
        .expect("ships array")
        .iter()
        .find(|s| s["id"].as_u64() == Some(1))
        .expect("ship 1 present in snapshot");
    assert!(
        ship1_json.get("power_available").is_some(),
        "snapshot ship JSON is missing `power_available` (Ship::effective_power() is not \
         exposed to clients); ship1 json = {ship1_json}"
    );
    assert_eq!(
        ship1_json["power_available"].as_u64(),
        Some(22),
        "undamaged ship: power_available should equal the design power pool (22)"
    );
}

/// E4 continued: after power_sys damage (via real combat overflow), the snapshot
/// must reflect the reduced pool, not just the static one. Split out from
/// `e4_snapshot_exposes_power_available` because reaching that damage requires
/// the firing phase, which is unreachable from Movement in M4 (ADR-0022) until
/// M5 interleaves fire back in.
#[test]
fn e4b_snapshot_power_available_drops_after_damage() {
    let mut game = load_size_hit();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, Hex::new(0, 0)).unwrap();
    game.set_ship_facing(2, 0).unwrap();
    // Seed 7's first d20 is 8, a hit vs the Escort's size-1 threshold of 9.
    allocate(&mut game, 1, 0, &[("beam_1", 3)], [0; 6]);
    allocate(&mut game, 2, 1, &[], [0; 6]); // zero shields: overflow = full damage
    for id in [1u32, 2] {
        apply_order(
            &mut game,
            Order::CommitManeuver {
                ship: id,
                maneuver: Maneuver::Coast,
            },
        )
        .expect("coast commits");
    }
    assert_eq!(StateSnapshot::from_game_state(&game).phase, "firing");

    commit(&mut game, 1, "beam_1", 2);
    apply_order(&mut game, Order::ReadyFire { ship: 1 }).unwrap();
    apply_order(&mut game, Order::ReadyFire { ship: 2 }).unwrap();

    // beam charge 3 at range 1 = half_up(3 * 2.0) = 6 damage; with shields at 0
    // the full 6 lands as hull overflow, walking the DAC (src/ssd.rs) through
    // Hull, Hull, Engine, Weapon, Hull, Power — exactly one power_sys hit
    // without reaching Bridge (which would destroy the ship), so ship 2
    // survives with power_sys reduced from 2 to 1.
    let damaged = StateSnapshot::from_game_state(&game);
    let ship2 = damaged.ships.iter().find(|s| s.id == 2).unwrap();
    assert!(!ship2.destroyed, "ship 2 should survive this hit");
    assert_eq!(
        ship2.power, 14,
        "static power pool (escort) is unchanged by damage"
    );

    let damaged_json = serde_json::to_value(&damaged).expect("snapshot serializes to JSON");
    let ship2_json = damaged_json["ships"]
        .as_array()
        .expect("ships array")
        .iter()
        .find(|s| s["id"].as_u64() == Some(2))
        .expect("ship 2 present in snapshot");
    let power_available = ship2_json
        .get("power_available")
        .and_then(|v| v.as_u64())
        .expect("power_available field present after damage");
    assert!(
        power_available < 14,
        "power_available should drop below the static pool (14) after power_sys damage, got {power_available}"
    );
}
