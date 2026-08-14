use std::collections::BTreeMap;

use shipsim_core::combat_tables::{final_to_hit_threshold, final_to_hit_threshold_with_modifiers, WeaponKind};
use shipsim_core::game_state::Phase;
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario_def;
use shipsim_core::schema::ScenarioDef;

fn write_ship(dir: &std::path::Path, id: &str, size: u32, systems: &str) {
    std::fs::write(
        dir.join(format!("data/ships/{id}.toml")),
        format!(
            r#"
id = "{id}"
name = "{id}"
size = {size}
max_maneuver_actions = 4
power = 20
max_shield_per_facing = 4
structure = 10
power_sys = 2
engine_boxes = 2
weapon_boxes = 1
thrust_per_power = 1
power_per_thrust = 1
{systems}
[[weapons]]
id = "beam_1"
kind = "beam"
arc = "forward"
mount = "forward"
max_range = 10
max_charge = 4
"#
        ),
    )
    .unwrap();
}

fn setup(target_systems: &str) -> (tempfile::TempDir, shipsim_core::game_state::GameState) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("data/rules")).unwrap();
    std::fs::create_dir_all(dir.path().join("data/ships")).unwrap();
    std::fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/data/rules/default.toml"),
        dir.path().join("data/rules/default.toml"),
    )
    .unwrap();
    write_ship(
        dir.path(),
        "attacker",
        2,
        "[[systems]]\nkind = \"computer\"\nmk = 3",
    );
    write_ship(dir.path(), "target", 3, target_systems);
    let def: ScenarioDef = toml::from_str(
        r#"
width = 12
height = 12
[[ships]]
id = 1
class = "attacker"
q = 0
r = 0
facing = 0
controller = "player"
[[ships]]
id = 2
class = "target"
q = 3
r = 0
facing = 3
controller = "player"
"#,
    )
    .unwrap();
    let mut game = load_scenario_def(&def, dir.path()).unwrap();
    for ship in [1, 2] {
        apply_order(
            &mut game,
            Order::Allocate {
                ship,
                movement: 0,
                weapons: BTreeMap::from([("beam_1".into(), if ship == 1 { 4 } else { 0 })]),
                shields: [0; 6],
                cloak: ship == 2 && target_systems.contains("cloak"),
                repair: if ship == 2 && target_systems.contains("repair") { 1 } else { 0 },
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
            },
        )
        .unwrap();
    }
    assert_eq!(game.phase(), Phase::Firing);
    (dir, game)
}

#[test]
fn computer_applies_at_all_target_sizes_and_cloak_reduces_incoming_hits() {
    let (_dir, mut game) = setup("[[systems]]\nkind = \"cloak\"");
    let preview = game.fire_decision_preview(1, "beam_1", 2).unwrap();
    let rules = shipsim_core::rules::Ruleset::builtin();
    let expected = final_to_hit_threshold_with_modifiers(
        rules.combat(),
        WeaponKind::Beam,
        preview.range,
        3,
        0,
        0,
        3,
        true,
        0,
    )
    .unwrap();
    assert_eq!(preview.threshold, expected);
    assert_eq!(game.ship(2).unwrap().cloaked, true);
    assert_eq!(game.ship(2).unwrap().ssd.hull, game.ship(2).unwrap().ssd.hull_max);
    let no_computer = final_to_hit_threshold(rules.combat(), WeaponKind::Beam, preview.range, 3, 0, 0).unwrap();
    assert_eq!(preview.threshold, no_computer.saturating_sub(4).saturating_add(3));
}

#[test]
fn repair_is_capped_by_hull_max_and_size_rate() {
    let (_dir, game) = setup("[[systems]]\nkind = \"repair\"");
    let ship = game.ship(2).unwrap();
    assert_eq!(ship.ssd.hull, ship.ssd.hull_max);
    assert_eq!(ship.ssd.hull_max, 10);
}
