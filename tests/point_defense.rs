use std::collections::BTreeMap;

use shipsim_core::game_state::Phase;
use shipsim_core::movement::{apply_order, Order, VolleyShot};
use shipsim_core::scenario::load_scenario_def;
use shipsim_core::schema::ScenarioDef;

fn setup(attacker_mount: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("data/rules")).unwrap();
    std::fs::create_dir_all(dir.path().join("data/ships")).unwrap();
    std::fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/data/rules/default.toml"),
        dir.path().join("data/rules/default.toml"),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("data/ships/attacker.toml"),
        r#"
id = "attacker"
name = "Attacker"
size = 2
max_maneuver_actions = 4
power = 20
max_shield_per_facing = 4
structure = 20
power_sys = 2
engine_boxes = 2
weapon_boxes = 1
thrust_per_power = 1
power_per_thrust = 1
[[weapons]]
id = "torp_1"
kind = "torp"
arc = "forward"
mount = "forward"
max_range = 12
max_charge = 1
"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("data/ships/defender.toml"),
        format!(
            r#"
id = "defender"
name = "Defender"
size = 2
max_maneuver_actions = 4
power = 20
max_shield_per_facing = 4
structure = 20
power_sys = 2
engine_boxes = 2
weapon_boxes = 1
[[weapons]]
id = "pd_1"
kind = "pd"
arc = "rear"
mount = "{attacker_mount}"
max_range = 10
max_charge = 1
"#
        ),
    )
    .unwrap();
    dir
}

fn firing(dir: &std::path::Path) -> shipsim_core::game_state::GameState {
    let def: ScenarioDef = toml::from_str(
        r#"
width = 12
height = 12
seed = 99
[[ships]]
id = 1
class = "attacker"
q = 0
r = 0
facing = 0
controller = "player"
[[ships]]
id = 2
class = "defender"
q = 3
r = 0
facing = 0
controller = "player"
"#,
    )
    .unwrap();
    let mut game = load_scenario_def(&def, dir).unwrap();
    for (ship, weapons) in [
        (1, BTreeMap::from([("torp_1".into(), 1)])),
        (2, BTreeMap::from([("pd_1".into(), 1)])),
    ] {
        apply_order(
            &mut game,
            Order::Allocate {
                ship,
                movement: 0,
                weapons,
                shields: [0; 6],
                cloak: false,
                repair: 0,
                unsquad: false,
                squad_leader: None,
            },
        )
        .unwrap();
    }
    for ship in [1, 2] {
        apply_order(
            &mut game,
            Order::CommitPath {
                ship,
                actions: vec![],
                evasive: 0,
                follow: false,
            },
        )
        .unwrap();
    }
    assert_eq!(game.phase(), Phase::Firing);
    game
}

fn fire_torp(mut game: shipsim_core::game_state::GameState) -> shipsim_core::game_state::GameState {
    apply_order(
        &mut game,
        Order::CommitVolley {
            ship: 2,
            shots: vec![],
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::CommitVolley {
            ship: 1,
            shots: vec![VolleyShot {
                weapon: "torp_1".into(),
                target: 2,
                shield_facing: 3,
            }],
        },
    )
    .unwrap();
    game
}

#[test]
fn in_arc_pd_rolls_and_spends_charge() {
    let dir = setup("aft");
    let game = fire_torp(firing(dir.path()));
    assert_eq!(game.ship(2).unwrap().weapon_charges["pd_1"], 0);
    assert!(game
        .combat_log()
        .iter()
        .any(|event| event.kind == "pd_hit" || event.kind == "pd_miss"));
}

#[test]
fn out_of_arc_pd_keeps_charge_and_does_not_log() {
    let dir = setup("forward");
    let game = fire_torp(firing(dir.path()));
    assert_eq!(game.ship(2).unwrap().weapon_charges["pd_1"], 1);
    assert!(!game
        .combat_log()
        .iter()
        .any(|event| event.kind == "pd_hit" || event.kind == "pd_miss"));
}
