//! Protocol v4 simplified simultaneous turns (ADR-0025).

use std::collections::BTreeMap;
use std::path::PathBuf;

use shipsim_core::game_state::Phase;
use shipsim_core::movement::{apply_order, Order, OrderError, VolleyShot};
use shipsim_core::path::PathAction;
use shipsim_core::protocol::PROTOCOL_VERSION;
use shipsim_core::scenario::load_scenario;
use shipsim_core::snapshot::StateSnapshot;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn load_ai() -> shipsim_core::game_state::GameState {
    load_scenario(&root().join("scenarios/ai.toml")).expect("load ai.toml")
}

fn allocate_both(game: &mut shipsim_core::game_state::GameState) {
    // Player ship 1
    apply_order(
        game,
        Order::Allocate {
            ship: 1,
            movement: 4,
            weapons: BTreeMap::from([
                ("beam_1".into(), 4),
                ("torp_1".into(), 1),
                ("plasma_1".into(), 1),
            ]),
            shields: [2, 0, 0, 0, 0, 2],
        },
    )
    .expect("player allocate");
    // AI may auto-resolve after player; if still allocate, drive NPC.
    game.resolve_v2_npc_actions();
    if game.phase() == Phase::Allocate {
        apply_order(
            game,
            Order::Allocate {
                ship: 2,
                movement: 2,
                weapons: BTreeMap::from([("beam_1".into(), 2)]),
                shields: [1, 0, 0, 0, 0, 1],
            },
        )
        .expect("escort allocate");
    }
}

#[test]
fn protocol_version_is_four() {
    assert_eq!(PROTOCOL_VERSION, 4);
    let game = load_ai();
    let snap = StateSnapshot::from_game_state(&game);
    assert_eq!(snap.protocol_version, 4);
    assert_eq!(snap.phase, "allocate");
    assert!(snap.ships_committed_path.is_empty());
    assert!(snap.ships_committed_volley.is_empty());
}

#[test]
fn reach_preview_respects_zero_motion_in_movement() {
    let mut game = load_ai();
    apply_order(
        &mut game,
        Order::Allocate {
            ship: 1,
            movement: 0,
            weapons: BTreeMap::new(),
            shields: [0; 6],
        },
    )
    .expect("player zero-motion allocate");
    game.resolve_v2_npc_actions();
    assert_eq!(game.phase(), Phase::Movement);

    let endpoints = game
        .reach_preview(1, Some(u32::MAX))
        .expect("reach preview");
    assert_eq!(endpoints.len(), 1);
    assert_eq!(endpoints[0].min_cost, 0);
    assert_eq!(endpoints[0].q, game.ship(1).unwrap().pos.q);
    assert_eq!(endpoints[0].r, game.ship(1).unwrap().pos.r);
}

#[test]
fn flow_allocate_movement_firing_allocate_next_turn() {
    let mut game = load_ai();
    assert_eq!(game.phase(), Phase::Allocate);
    assert_eq!(game.turn_number(), 1);

    allocate_both(&mut game);
    assert_eq!(game.phase(), Phase::Movement);
    // Partial allocation must not leave shields if we only staged one — after both, applied.
    let s1 = game.ship(1).unwrap();
    assert_eq!(s1.shields_powered[0], 2);
    assert!(s1.motion_available > 0);

    // Paths: empty path for both is legal.
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 1,
            actions: vec![PathAction::MoveF, PathAction::MoveF],
        },
    )
    .expect("path 1");
    game.resolve_v2_npc_actions();
    if game.phase() == Phase::Movement {
        apply_order(
            &mut game,
            Order::CommitPath {
                ship: 2,
                actions: vec![],
            },
        )
        .expect("path 2 empty");
    }
    assert_eq!(game.phase(), Phase::Firing);
    assert!(!game.path_results().is_empty());

    // Empty volleys advance turn automatically.
    apply_order(
        &mut game,
        Order::CommitVolley {
            ship: 1,
            shots: vec![],
        },
    )
    .expect("volley 1");
    game.resolve_v2_npc_actions();
    if game.phase() == Phase::Firing {
        apply_order(
            &mut game,
            Order::CommitVolley {
                ship: 2,
                shots: vec![],
            },
        )
        .expect("volley 2");
    }
    assert_eq!(game.phase(), Phase::Allocate);
    assert_eq!(game.turn_number(), 2);
    // Motion does not carry; shields reset.
    let s1 = game.ship(1).unwrap();
    assert_eq!(s1.motion_available, 0);
    assert_eq!(s1.shields_powered, [0; 6]);
}

#[test]
fn partial_allocation_does_not_mutate_public_state() {
    let mut game = load_ai();
    apply_order(
        &mut game,
        Order::Allocate {
            ship: 1,
            movement: 4,
            weapons: BTreeMap::new(),
            shields: [3, 0, 0, 0, 0, 0],
        },
    )
    .expect("stage player only");
    assert_eq!(game.phase(), Phase::Allocate);
    let s1 = game.ship(1).unwrap();
    assert_eq!(s1.shields_powered, [0; 6]);
    assert_eq!(s1.movement_allocated, 0);
    assert_eq!(s1.motion_available, 0);
    // Commitment progress visible
    assert!(game.allocated_this_turn().contains(&1));
}

#[test]
fn empty_path_and_empty_volley_are_legal() {
    let mut game = load_ai();
    allocate_both(&mut game);
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 1,
            actions: vec![],
        },
    )
    .unwrap();
    game.resolve_v2_npc_actions();
    if game.phase() == Phase::Movement {
        apply_order(
            &mut game,
            Order::CommitPath {
                ship: 2,
                actions: vec![],
            },
        )
        .unwrap();
    }
    assert_eq!(game.phase(), Phase::Firing);
    apply_order(
        &mut game,
        Order::CommitVolley {
            ship: 1,
            shots: vec![],
        },
    )
    .unwrap();
    game.resolve_v2_npc_actions();
    if game.phase() == Phase::Firing {
        apply_order(
            &mut game,
            Order::CommitVolley {
                ship: 2,
                shots: vec![],
            },
        )
        .unwrap();
    }
    assert_eq!(game.turn_number(), 2);
}

#[test]
fn over_budget_path_rejected_with_index() {
    let mut game = load_ai();
    allocate_both(&mut game);
    let budget = game.ship(1).unwrap().motion_available;
    let mut actions = vec![PathAction::TurnRight; (budget as usize) + 1];
    let err = apply_order(
        &mut game,
        Order::CommitPath {
            ship: 1,
            actions: std::mem::take(&mut actions),
        },
    )
    .unwrap_err();
    match err {
        OrderError::IllegalPath { reason, .. } => {
            assert!(
                reason.contains("budget") || reason.contains("motion"),
                "{reason}"
            );
        }
        other => panic!("unexpected {other}"),
    }
}

#[test]
fn weapon_charge_carries_shields_reset() {
    let mut game = load_ai();
    allocate_both(&mut game);
    let charge_after = game
        .ship(1)
        .unwrap()
        .weapon_charges
        .get("beam_1")
        .copied()
        .unwrap_or(0);
    assert!(charge_after >= 1);

    // Empty path + empty volley → turn 2
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 1,
            actions: vec![],
        },
    )
    .unwrap();
    game.resolve_v2_npc_actions();
    if game.phase() == Phase::Movement {
        apply_order(
            &mut game,
            Order::CommitPath {
                ship: 2,
                actions: vec![],
            },
        )
        .unwrap();
    }
    apply_order(
        &mut game,
        Order::CommitVolley {
            ship: 1,
            shots: vec![],
        },
    )
    .unwrap();
    game.resolve_v2_npc_actions();
    if game.phase() == Phase::Firing {
        apply_order(
            &mut game,
            Order::CommitVolley {
                ship: 2,
                shots: vec![],
            },
        )
        .unwrap();
    }

    let s1 = game.ship(1).unwrap();
    assert_eq!(
        s1.weapon_charges.get("beam_1").copied().unwrap_or(0),
        charge_after
    );
    assert_eq!(s1.shields_powered, [0; 6]);
}

#[test]
fn snapshot_hides_staged_path_payload() {
    let mut game = load_ai();
    allocate_both(&mut game);
    // Stage player path only — do not let the AI resolve yet.
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 1,
            actions: vec![PathAction::MoveF],
        },
    )
    .unwrap();
    // AI may already have staged its path via allocate_both's NPC pump; if both
    // are committed, resolution already ran. Otherwise we still see ship 1 staged.
    let snap = StateSnapshot::from_game_state(&game);
    if game.phase() == Phase::Movement {
        assert!(snap.ships_committed_path.contains(&1) || snap.ships_committed_path.contains(&2));
    }
    let json = serde_json::to_string(&snap).unwrap();
    assert!(!json.contains("\"velocity\""));
    assert!(!json.contains("movement_phase"));
    assert!(!json.contains("ready_fire"));
    assert!(!json.contains("ships_ready_fire"));
    // Staged action lists are never in the snapshot JSON.
    assert!(!json.contains("\"actions\""));
}

#[test]
fn path_preview_matches_commit_without_conflict() {
    let mut game = load_ai();
    allocate_both(&mut game);
    // Empty enemy path so player endpoint is uncontested.
    // First ensure AI path is not already resolving into our destination:
    // commit empty paths for both in a controlled order.
    let actions = vec![PathAction::MoveF, PathAction::TurnRight];
    let preview = game.path_preview(1, &actions).unwrap();
    assert!(preview.error.is_none(), "{:?}", preview.error);
    assert_eq!(preview.cost, 2);

    // Force both empty-or-known paths without AI greedy paths:
    // if AI already committed, resolve may have fired; re-enter via fresh game.
    let mut game = load_ai();
    apply_order(
        &mut game,
        Order::Allocate {
            ship: 1,
            movement: 4,
            weapons: BTreeMap::from([("beam_1".into(), 4)]),
            shields: [0; 6],
        },
    )
    .unwrap();
    // Manually allocate ship 2 without NPC path planner.
    apply_order(
        &mut game,
        Order::Allocate {
            ship: 2,
            movement: 0,
            weapons: BTreeMap::from([("beam_1".into(), 1)]),
            shields: [0; 6],
        },
    )
    .unwrap();
    assert_eq!(game.phase(), Phase::Movement);
    let preview = game.path_preview(1, &actions).unwrap();
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 1,
            actions: actions.clone(),
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 2,
            actions: vec![],
        },
    )
    .unwrap();
    let s1 = game.ship(1).unwrap();
    assert_eq!(s1.pos.q, preview.final_q);
    assert_eq!(s1.pos.r, preview.final_r);
    assert_eq!(s1.facing, preview.final_facing);
}

#[test]
fn simultaneous_volley_with_legal_shot() {
    let mut game = load_ai();
    // Manual allocate (no NPC) so we control geometry.
    apply_order(
        &mut game,
        Order::Allocate {
            ship: 1,
            movement: 0,
            weapons: BTreeMap::from([
                ("beam_1".into(), 4),
                ("torp_1".into(), 1),
                ("plasma_1".into(), 1),
            ]),
            shields: [0; 6],
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::Allocate {
            ship: 2,
            movement: 0,
            weapons: BTreeMap::from([("beam_1".into(), 2)]),
            shields: [0; 6],
        },
    )
    .unwrap();
    // Stationary paths keep start hexes (0,4) and (8,4) — in arc for forward beam?
    // Range 8 may be ok for beam max 10. Facing 0 toward east.
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 1,
            actions: vec![],
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 2,
            actions: vec![],
        },
    )
    .unwrap();
    assert_eq!(game.phase(), Phase::Firing);

    let (weapon_id, facing) = {
        let attacker = game.ship(1).unwrap();
        let target = game.ship(2).unwrap();
        let weapon = attacker.weapons.iter().find(|w| w.id == "beam_1").unwrap();
        let facing = game
            .v2_shot_shield_facing(attacker, weapon, target)
            .expect("legal shot geometry at range 8");
        (weapon.id.clone(), facing)
    };

    apply_order(
        &mut game,
        Order::CommitVolley {
            ship: 1,
            shots: vec![VolleyShot {
                weapon: weapon_id,
                target: 2,
                shield_facing: facing,
            }],
        },
    )
    .expect("volley");
    let structure_before = game.ship(2).unwrap().structure();
    apply_order(
        &mut game,
        Order::CommitVolley {
            ship: 2,
            shots: vec![],
        },
    )
    .unwrap();
    // Volley resolution auto-advances the turn but retains combat_log until the
    // next volley resolves so clients can observe ordinary hit/miss events.
    assert!(
        game.turn_number() >= 2
            || game.status() != shipsim_core::game_state::ScenarioStatus::InProgress
    );
    assert!(
        !game.combat_log().is_empty(),
        "combat_log must remain visible after auto turn advance"
    );
    let charge = game
        .ship(1)
        .unwrap()
        .weapon_charges
        .get("beam_1")
        .copied()
        .unwrap_or(0);
    assert_eq!(charge, 0, "volley consumes charge even on miss");
    let _ = structure_before;
}

#[test]
fn mutual_destruction_both_volleys_resolve() {
    // Two player ships face each other with charged beams; both volleys fire
    // even if the first canonical shot would kill its target/attacker.
    use shipsim_core::game_state::ScenarioStatus;
    use shipsim_core::hex::Hex;

    let mut game = load_scenario(&root().join("scenarios/v4_conflict.toml")).unwrap();
    // Place close, face each other, full charge both beams.
    game.set_ship_pos(1, Hex::new(3, 4)).unwrap();
    game.set_ship_pos(2, Hex::new(4, 4)).unwrap();
    game.set_ship_facing(1, 0).unwrap();
    game.set_ship_facing(2, 3).unwrap();

    apply_order(
        &mut game,
        Order::Allocate {
            ship: 1,
            movement: 0,
            weapons: BTreeMap::from([("beam_1".into(), 4)]),
            shields: [0; 6],
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::Allocate {
            ship: 2,
            movement: 0,
            weapons: BTreeMap::from([("beam_1".into(), 4)]),
            shields: [0; 6],
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 1,
            actions: vec![],
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 2,
            actions: vec![],
        },
    )
    .unwrap();
    assert_eq!(game.phase(), Phase::Firing);

    let (w1, f1) = {
        let a = game.ship(1).unwrap();
        let t = game.ship(2).unwrap();
        let w = a.weapons.iter().find(|w| w.id == "beam_1").unwrap();
        let f = game.v2_shot_shield_facing(a, w, t).expect("1→2 legal");
        (w.id.clone(), f)
    };
    let (w2, f2) = {
        let a = game.ship(2).unwrap();
        let t = game.ship(1).unwrap();
        let w = a.weapons.iter().find(|w| w.id == "beam_1").unwrap();
        let f = game.v2_shot_shield_facing(a, w, t).expect("2→1 legal");
        (w.id.clone(), f)
    };

    apply_order(
        &mut game,
        Order::CommitVolley {
            ship: 1,
            shots: vec![VolleyShot {
                weapon: w1,
                target: 2,
                shield_facing: f1,
            }],
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::CommitVolley {
            ship: 2,
            shots: vec![VolleyShot {
                weapon: w2,
                target: 1,
                shield_facing: f2,
            }],
        },
    )
    .unwrap();

    // Both ships' shots appear in the combat log regardless of kill order.
    let log = game.combat_log();
    assert!(
        log.iter().any(|e| e.attacker == 1 && e.target == 2),
        "attacker 1 must resolve: {log:?}"
    );
    assert!(
        log.iter().any(|e| e.attacker == 2 && e.target == 1),
        "attacker 2 must resolve even if destroyed mid-phase: {log:?}"
    );
    let _ = ScenarioStatus::InProgress;
}

#[test]
fn path_resolve_cascade_does_not_panic_on_chain() {
    // Engine-level reproduction of the three-ship cascade case via pure module
    // is in path_resolve tests; this asserts the game can load conflict scenario
    // and resolve equal-cost head-ons without panic.
    let mut game = load_scenario(&root().join("scenarios/v4_conflict.toml")).unwrap();
    apply_order(
        &mut game,
        Order::Allocate {
            ship: 1,
            movement: 2,
            weapons: BTreeMap::new(),
            shields: [0; 6],
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::Allocate {
            ship: 2,
            movement: 2,
            weapons: BTreeMap::new(),
            shields: [0; 6],
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 1,
            actions: vec![PathAction::MoveF],
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 2,
            actions: vec![PathAction::MoveF],
        },
    )
    .unwrap();
    let mut positions = std::collections::BTreeSet::new();
    for ship in game.ships().iter().filter(|s| !s.destroyed) {
        assert!(
            positions.insert((ship.pos.q, ship.pos.r)),
            "duplicate occupancy after path resolve"
        );
    }
}

#[test]
fn floating_map_preserves_unique_world_endpoints_beyond_nominal_bounds() {
    use shipsim_core::scenario::load_scenario_def;
    use shipsim_core::schema::{ScenarioDef, ShipPlacementDef};

    let placement = |id, q, facing| ShipPlacementDef {
        id,
        class: "escort".into(),
        q,
        r: 1,
        facing,
        controller: "player".into(),
        power: None,
        structure: None,
        max_shield_per_facing: None,
    };
    let scenario = ScenarioDef {
        width: 3,
        height: 3,
        seed: 7,
        map_mode: Some("floating".into()),
        objective: None,
        terminal: None,
        ships: vec![placement(1, 0, 3), placement(2, 1, 0), placement(3, 2, 0)],
    };
    let mut game = load_scenario_def(&scenario, &root()).expect("floating scenario");
    for id in 1..=3 {
        apply_order(
            &mut game,
            Order::Allocate {
                ship: id,
                movement: 1,
                weapons: BTreeMap::new(),
                shields: [0; 6],
            },
        )
        .expect("allocation");
    }
    for id in 1..=3 {
        apply_order(
            &mut game,
            Order::CommitPath {
                ship: id,
                actions: vec![PathAction::MoveF, PathAction::MoveF],
            },
        )
        .expect("path");
    }

    let positions: std::collections::BTreeSet<_> = game
        .ships()
        .iter()
        .map(|ship| (ship.pos.q, ship.pos.r))
        .collect();
    assert_eq!(
        positions,
        std::collections::BTreeSet::from([(-2, 1), (3, 1), (4, 1)])
    );
}
