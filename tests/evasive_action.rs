//! Evasive Action: motion spent on commit_path reduces incoming to-hit threshold.

use std::collections::BTreeMap;
use std::path::PathBuf;

use shipsim_core::combat_tables::{final_to_hit_threshold, WeaponKind};
use shipsim_core::game_state::Phase;
use shipsim_core::movement::{apply_order, Order, OrderError};
use shipsim_core::path::PathAction;
use shipsim_core::rules::Ruleset;
use shipsim_core::scenario::load_scenario_def;
use shipsim_core::schema::ScenarioDef;
use shipsim_core::snapshot::StateSnapshot;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn write_minimal_pair(dir: &std::path::Path) {
    let ships = dir.join("data/ships");
    std::fs::create_dir_all(&ships).unwrap();
    let rules_dst = dir.join("data/rules");
    std::fs::create_dir_all(&rules_dst).unwrap();
    std::fs::copy(
        root().join("data/rules/default.toml"),
        rules_dst.join("default.toml"),
    )
    .unwrap();
    for (name, body) in [
        (
            "mobile.toml",
            r#"
id = "mobile"
name = "Mobile"
size = 2
max_maneuver_actions = 6
power = 20
max_shield_per_facing = 4
structure = 20
power_sys = 2
engine_boxes = 3
thrust_per_power = 1
power_per_thrust = 1
weapon_boxes = 1
[[weapons]]
id = "beam_1"
kind = "beam"
arc = "forward"
mount = "forward"
max_range = 10
max_charge = 4
"#,
        ),
        (
            "gunner.toml",
            r#"
id = "gunner"
name = "Gunner"
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
id = "beam_1"
kind = "beam"
arc = "forward"
mount = "forward"
max_range = 10
max_charge = 4
"#,
        ),
    ] {
        std::fs::write(ships.join(name), body).unwrap();
    }
}

fn load_pair(dir: &std::path::Path) -> shipsim_core::game_state::GameState {
    let def: ScenarioDef = toml::from_str(
        r#"
width = 12
height = 12
seed = 99
[[ships]]
id = 1
class = "mobile"
q = 0
r = 0
facing = 0
controller = "player"
[[ships]]
id = 2
class = "gunner"
q = 3
r = 0
facing = 3
controller = "player"
"#,
    )
    .unwrap();
    load_scenario_def(&def, dir).expect("load pair")
}

#[test]
fn evasion_lowers_threshold_and_can_flip_hit() {
    let rules = Ruleset::builtin();
    let combat = rules.combat();
    let base = final_to_hit_threshold(combat, WeaponKind::Beam, 3, 2, 0, 0).unwrap();
    let reduced = final_to_hit_threshold(combat, WeaponKind::Beam, 3, 2, 0, 3).unwrap();
    assert_eq!(
        reduced,
        base.saturating_sub(3 * combat.accuracy().evasion_per_point())
            .max(1)
    );
}

#[test]
fn commit_path_validates_budget_and_allows_pure_jink() {
    let dir = tempfile::tempdir().unwrap();
    write_minimal_pair(dir.path());
    let mut game = load_pair(dir.path());

    // Buy motion for ship 1
    apply_order(
        &mut game,
        Order::Allocate {
            ship: 1,
            movement: 4,
            weapons: BTreeMap::new(),
            shields: [0; 6],
            cloak: false,
            repair: 0,
            unsquad: false,
            squad_leader: None,
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::Allocate {
            ship: 2,
            movement: 0,
            weapons: BTreeMap::new(),
            shields: [0; 6],
            cloak: false,
            repair: 0,
            unsquad: false,
            squad_leader: None,
        },
    )
    .unwrap();
    assert_eq!(game.phase(), Phase::Movement);
    let motion = game.ship(1).unwrap().motion_available;
    assert!(motion >= 4);

    // Over budget: path cost 3 + evasive 2 with motion 4
    let err = apply_order(
        &mut game,
        Order::CommitPath {
            ship: 1,
            actions: vec![PathAction::MoveF, PathAction::MoveF, PathAction::MoveF],
            evasive: 2,
            follow: false,
        },
    )
    .unwrap_err();
    assert!(
        matches!(err, OrderError::IllegalPath { .. })
            || matches!(err, OrderError::InsufficientMotion { .. }),
        "expected over-budget reject, got {err:?}"
    );

    // Pure jink: empty path, full motion as evasive
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 1,
            actions: Vec::new(),
            evasive: motion,
            follow: false,
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 2,
            actions: Vec::new(),
            evasive: 0,
            follow: false,
        },
    )
    .unwrap();
    assert_eq!(game.phase(), Phase::Firing);
    assert_eq!(game.ship(1).unwrap().evasion_committed, motion);
    let snap = StateSnapshot::from_game_state(&game);
    assert_eq!(
        snap.ships
            .iter()
            .find(|s| s.id == 1)
            .unwrap()
            .evasion_committed,
        motion
    );
}

#[test]
fn evasion_resets_on_next_allocate() {
    let dir = tempfile::tempdir().unwrap();
    write_minimal_pair(dir.path());
    let mut game = load_pair(dir.path());

    apply_order(
        &mut game,
        Order::Allocate {
            ship: 1,
            movement: 3,
            weapons: BTreeMap::from([("beam_1".into(), 1)]),
            shields: [0; 6],
            cloak: false,
            repair: 0,
            unsquad: false,
            squad_leader: None,
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::Allocate {
            ship: 2,
            movement: 0,
            weapons: BTreeMap::from([("beam_1".into(), 1)]),
            shields: [0; 6],
            cloak: false,
            repair: 0,
            unsquad: false,
            squad_leader: None,
        },
    )
    .unwrap();

    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 1,
            actions: Vec::new(),
            evasive: 2,
            follow: false,
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 2,
            actions: Vec::new(),
            evasive: 0,
            follow: false,
        },
    )
    .unwrap();
    assert_eq!(game.ship(1).unwrap().evasion_committed, 2);

    // Hold fire both to advance turn
    apply_order(
        &mut game,
        Order::CommitVolley {
            ship: 1,
            shots: Vec::new(),
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
    assert_eq!(game.phase(), Phase::Allocate);
    assert_eq!(
        game.ship(1).unwrap().evasion_committed,
        0,
        "evasion must reset before next path commit"
    );
    let snap = StateSnapshot::from_game_state(&game);
    assert_eq!(
        snap.ships
            .iter()
            .find(|s| s.id == 1)
            .unwrap()
            .evasion_committed,
        0
    );
}

#[test]
fn fire_preview_uses_target_evasion() {
    let dir = tempfile::tempdir().unwrap();
    write_minimal_pair(dir.path());
    let mut game = load_pair(dir.path());

    apply_order(
        &mut game,
        Order::Allocate {
            ship: 1,
            movement: 2,
            weapons: BTreeMap::new(),
            shields: [0; 6],
            cloak: false,
            repair: 0,
            unsquad: false,
            squad_leader: None,
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
            cloak: false,
            repair: 0,
            unsquad: false,
            squad_leader: None,
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 1,
            actions: Vec::new(),
            evasive: 2,
            follow: false,
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 2,
            actions: Vec::new(),
            evasive: 0,
            follow: false,
        },
    )
    .unwrap();

    let preview = game
        .fire_decision_preview(2, "beam_1", 1)
        .expect("fire preview");
    let rules = Ruleset::builtin();
    let expected =
        final_to_hit_threshold(rules.combat(), WeaponKind::Beam, preview.range, 2, 0, 2).unwrap();
    assert_eq!(preview.threshold, expected);
}
