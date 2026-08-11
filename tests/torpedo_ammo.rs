//! Torpedo ammo cap (finite magazine per torpedo-kind weapon).

use std::collections::BTreeMap;
use std::path::PathBuf;

use shipsim_core::game_state::Phase;
use shipsim_core::movement::{apply_order, Order, OrderError, VolleyShot};
use shipsim_core::scenario::{load_scenario, load_scenario_def, load_ship_def};
use shipsim_core::schema::ScenarioDef;
use shipsim_core::snapshot::StateSnapshot;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn empty_path(ship: u32) -> Order {
    Order::CommitPath {
        ship,
        actions: Vec::new(),
        evasive: 0,
    }
}

#[test]
fn size_formula_defaults_and_explicit_override() {
    let dir = tempfile::tempdir().unwrap();
    let ships = dir.path().join("data/ships");
    std::fs::create_dir_all(&ships).unwrap();
    // Copy rules so load_scenario can find them.
    let rules_src = root().join("data/rules/default.toml");
    let rules_dst = dir.path().join("data/rules");
    std::fs::create_dir_all(&rules_dst).unwrap();
    std::fs::copy(&rules_src, rules_dst.join("default.toml")).unwrap();

    std::fs::write(
        ships.join("ammo_test.toml"),
        r#"
id = "ammo_test"
name = "Ammo Test"
size = 2
max_maneuver_actions = 4
power = 20
max_shield_per_facing = 4
structure = 8
power_sys = 2
engine_boxes = 2
thrust_per_power = 1
power_per_thrust = 1
weapon_boxes = 2

[[weapons]]
id = "torp_default"
kind = "torp"
arc = "forward"
mount = "forward"
max_range = 12
max_charge = 1

[[weapons]]
id = "torp_override"
kind = "torp"
arc = "forward"
mount = "forward"
max_range = 12
max_charge = 1
max_ammo = 1
"#,
    )
    .unwrap();

    let def: ScenarioDef = toml::from_str(
        r#"
width = 8
height = 8
seed = 1
[[ships]]
id = 1
class = "ammo_test"
q = 0
r = 0
facing = 0
controller = "player"
"#,
    )
    .unwrap();
    let game = load_scenario_def(&def, dir.path()).expect("load scenario");
    let ship = game.ship(1).unwrap();
    // size 2 → 3+2 = 5 default
    assert_eq!(ship.weapon_ammo.get("torp_default"), Some(&5));
    assert_eq!(ship.weapon_ammo.get("torp_override"), Some(&1));
    let w_default = ship
        .weapons
        .iter()
        .find(|w| w.id == "torp_default")
        .unwrap();
    let w_override = ship
        .weapons
        .iter()
        .find(|w| w.id == "torp_override")
        .unwrap();
    assert_eq!(w_default.max_ammo, Some(5));
    assert_eq!(w_override.max_ammo, Some(1));

    // Schema round-trip: load_ship_def parses max_ammo
    let parsed = load_ship_def(dir.path(), "ammo_test").unwrap();
    assert_eq!(
        parsed
            .weapons
            .iter()
            .find(|w| w.id == "torp_override")
            .unwrap()
            .max_ammo,
        Some(1)
    );
    assert_eq!(
        parsed
            .weapons
            .iter()
            .find(|w| w.id == "torp_default")
            .unwrap()
            .max_ammo,
        None
    );
}

#[test]
fn fire_countdown_and_reject_charge_at_zero() {
    let dir = tempfile::tempdir().unwrap();
    let ships = dir.path().join("data/ships");
    std::fs::create_dir_all(&ships).unwrap();
    let rules_src = root().join("data/rules/default.toml");
    let rules_dst = dir.path().join("data/rules");
    std::fs::create_dir_all(&rules_dst).unwrap();
    std::fs::copy(&rules_src, rules_dst.join("default.toml")).unwrap();

    std::fs::write(
        ships.join("one_shot.toml"),
        r#"
id = "one_shot"
name = "One Shot"
size = 2
max_maneuver_actions = 4
power = 20
max_shield_per_facing = 4
structure = 20
power_sys = 2
engine_boxes = 2
thrust_per_power = 1
power_per_thrust = 1
weapon_boxes = 1

[[weapons]]
id = "torp_1"
kind = "torp"
arc = "forward"
mount = "forward"
max_range = 12
max_charge = 1
max_ammo = 2
"#,
    )
    .unwrap();
    std::fs::write(
        ships.join("target.toml"),
        r#"
id = "target"
name = "Target"
size = 2
max_maneuver_actions = 0
power = 4
max_shield_per_facing = 1
structure = 30
power_sys = 1
engine_boxes = 1
thrust_per_power = 0
power_per_thrust = 1
weapon_boxes = 1

[[weapons]]
id = "beam_1"
kind = "beam"
arc = "all"
max_range = 10
max_charge = 1
"#,
    )
    .unwrap();

    let def: ScenarioDef = toml::from_str(
        r#"
width = 10
height = 10
seed = 42
[[ships]]
id = 1
class = "one_shot"
q = 0
r = 0
facing = 0
controller = "player"
[[ships]]
id = 2
class = "target"
q = 2
r = 0
facing = 3
controller = "scripted"
"#,
    )
    .unwrap();
    let mut game = load_scenario_def(&def, dir.path()).expect("load");

    for shot_n in 0..2 {
        assert_eq!(
            game.ship(1).unwrap().weapon_ammo.get("torp_1"),
            Some(&(2 - shot_n))
        );
        apply_order(
            &mut game,
            Order::Allocate {
                ship: 1,
                movement: 0,
                weapons: BTreeMap::from([("torp_1".into(), 1)]),
                shields: [0; 6],
            },
        )
        .expect("allocate torp");
        apply_order(
            &mut game,
            Order::Allocate {
                ship: 2,
                movement: 0,
                weapons: BTreeMap::new(),
                shields: [0; 6],
            },
        )
        .expect("target allocate");
        assert_eq!(game.phase(), Phase::Movement);

        apply_order(&mut game, empty_path(1)).unwrap();
        apply_order(&mut game, empty_path(2)).unwrap();
        assert_eq!(game.phase(), Phase::Firing);

        apply_order(
            &mut game,
            Order::CommitVolley {
                ship: 1,
                shots: vec![VolleyShot {
                    weapon: "torp_1".into(),
                    target: 2,
                    shield_facing: 0,
                }],
            },
        )
        .unwrap();
        apply_order(
            &mut game,
            Order::CommitVolley {
                ship: 2,
                shots: Vec::new(),
            },
        )
        .unwrap();
        // Auto-advance to next allocate
        assert_eq!(game.phase(), Phase::Allocate);
    }

    assert_eq!(game.ship(1).unwrap().weapon_ammo.get("torp_1"), Some(&0));
    let snap = StateSnapshot::from_game_state(&game);
    let w = snap.ships[0]
        .weapons
        .iter()
        .find(|w| w.id == "torp_1")
        .unwrap();
    assert_eq!(w.ammo_remaining, Some(0));
    assert_eq!(w.max_ammo, Some(2));

    let err = apply_order(
        &mut game,
        Order::Allocate {
            ship: 1,
            movement: 0,
            weapons: BTreeMap::from([("torp_1".into(), 1)]),
            shields: [0; 6],
        },
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            OrderError::WeaponOutOfAmmo {
                ship: 1,
                weapon: ref w
            } if w == "torp_1"
        ),
        "expected WeaponOutOfAmmo, got {err:?}"
    );
}

#[test]
fn catalog_ships_get_size_formula_ammo() {
    let game = load_scenario(&root().join("scenarios/ai.toml")).expect("ai.toml");
    let hc = game.ship(1).expect("heavy cruiser");
    // size 2 → 5 default for torp_1
    if let Some(w) = hc
        .weapons
        .iter()
        .find(|w| w.kind == shipsim_core::combat_tables::WeaponKind::Torp)
    {
        assert_eq!(w.max_ammo, Some(5));
        assert_eq!(hc.weapon_ammo.get(&w.id), Some(&5));
    }
}

/// Integration regression for plan 1d-2: pure-AI match with a dry torpedo
/// magazine must keep progressing through `resolve_v2_npc_actions` (no barrier
/// soft-lock). Uses `max_ammo = 0` so the magazine is empty from load without
/// needing crate-private `ship_mut`.
#[test]
fn ai_npc_barrier_continues_with_dry_torpedo_magazine() {
    let dir = tempfile::tempdir().unwrap();
    let ships = dir.path().join("data/ships");
    std::fs::create_dir_all(&ships).unwrap();
    let rules_dst = dir.path().join("data/rules");
    std::fs::create_dir_all(&rules_dst).unwrap();
    std::fs::copy(
        root().join("data/rules/default.toml"),
        rules_dst.join("default.toml"),
    )
    .unwrap();

    std::fs::write(
        ships.join("ai_torp.toml"),
        r#"
id = "ai_torp"
name = "AI Torp"
size = 2
max_maneuver_actions = 4
power = 16
max_shield_per_facing = 4
structure = 12
power_sys = 2
engine_boxes = 2
thrust_per_power = 1
power_per_thrust = 1
weapon_boxes = 1
[[weapons]]
id = "torp_1"
kind = "torp"
arc = "forward"
mount = "forward"
max_range = 12
max_charge = 1
max_ammo = 0
"#,
    )
    .unwrap();
    std::fs::write(
        ships.join("ai_beam.toml"),
        r#"
id = "ai_beam"
name = "AI Beam"
size = 2
max_maneuver_actions = 4
power = 12
max_shield_per_facing = 4
structure = 20
power_sys = 2
engine_boxes = 2
thrust_per_power = 1
power_per_thrust = 1
weapon_boxes = 1
[[weapons]]
id = "beam_1"
kind = "beam"
arc = "forward"
mount = "forward"
max_range = 10
max_charge = 2
"#,
    )
    .unwrap();

    let def: ScenarioDef = toml::from_str(
        r#"
width = 12
height = 12
seed = 7
[[ships]]
id = 1
class = "ai_torp"
q = 0
r = 0
facing = 0
controller = "ai"
[[ships]]
id = 2
class = "ai_beam"
q = 3
r = 0
facing = 3
controller = "ai"
"#,
    )
    .unwrap();
    let mut game = load_scenario_def(&def, dir.path()).expect("load pure-AI scenario");
    assert_eq!(
        game.ship(1).unwrap().weapon_ammo.get("torp_1"),
        Some(&0),
        "fixture must start with a dry torpedo magazine"
    );

    let start_turn = game.turn_number();
    // One call to resolve_v2_npc_actions only drains one collection stage's AI
    // commits; loop to advance through multiple full turns.
    for _ in 0..12 {
        let applied = game.resolve_v2_npc_actions();
        assert!(
            !applied.is_empty()
                || game.status() != shipsim_core::game_state::ScenarioStatus::InProgress,
            "NPC barrier stalled at turn {} phase {:?}",
            game.turn_number(),
            game.phase()
        );
        if game.status() != shipsim_core::game_state::ScenarioStatus::InProgress {
            break;
        }
        if game.turn_number() > start_turn + 2 {
            break;
        }
    }
    assert!(
        game.turn_number() > start_turn
            || game.status() != shipsim_core::game_state::ScenarioStatus::InProgress,
        "turn must advance (or match end) with a dry AI torpedo magazine; start={start_turn} now={}",
        game.turn_number()
    );
}
