//! Weapon quality modifiers (Potent / Precise / Compact) through compile and combat.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use shipsim_core::combat_tables::{
    final_to_hit_threshold, final_to_hit_threshold_with_weapon_bonus, WeaponKind,
};
use shipsim_core::game_state::Phase;
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::rules::Ruleset;
use shipsim_core::scenario::load_scenario_def;
use shipsim_core::schema::{ScenarioDef, ShipDef};
use shipsim_core::shipyard;
use shipsim_core::snapshot::StateSnapshot;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn write_rules(dir: &Path) {
    let rules = dir.join("data/rules");
    std::fs::create_dir_all(&rules).unwrap();
    std::fs::copy(
        repo().join("data/rules/default.toml"),
        rules.join("default.toml"),
    )
    .unwrap();
}

fn write_ship(dir: &Path, class: &str, body: &str) {
    let ships = dir.join("data/ships");
    std::fs::create_dir_all(&ships).unwrap();
    std::fs::write(ships.join(format!("{class}.toml")), body).unwrap();
}

fn load_pair(dir: &Path, attacker: &str, target: &str) -> shipsim_core::game_state::GameState {
    let def: ScenarioDef = toml::from_str(&format!(
        r#"
width = 12
height = 12
seed = 99
[[ships]]
id = 1
class = "{attacker}"
q = 0
r = 0
facing = 0
controller = "player"
[[ships]]
id = 2
class = "{target}"
q = 3
r = 0
facing = 3
controller = "player"
"#
    ))
    .unwrap();
    load_scenario_def(&def, dir).expect("load pair")
}

fn baseline_ship(size: u32, attack_accuracy_bonus: u8, extra_weapon_fields: &str) -> String {
    format!(
        r#"
id = "attacker"
name = "Attacker"
size = {size}
max_maneuver_actions = 6
power = 20
max_shield_per_facing = 4
structure = 20
power_sys = 2
engine_boxes = 3
thrust_per_power = 1
power_per_thrust = 1
weapon_boxes = 1
attack_accuracy_bonus = {attack_accuracy_bonus}
[[weapons]]
id = "beam_1"
kind = "beam"
arc = "forward"
mount = "forward"
max_range = 10
max_charge = 4
{extra_weapon_fields}
"#
    )
}

fn target_ship(size: u32) -> String {
    format!(
        r#"
id = "target"
name = "Target"
size = {size}
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
"#
    )
}

fn enter_firing(game: &mut shipsim_core::game_state::GameState) {
    apply_order(
        game,
        Order::Allocate {
            ship: 1,
            movement: 0,
            weapons: BTreeMap::from([("beam_1".into(), 4)]),
            shields: [0; 6],
            cloak: false,
            repair: 0,
            unsquad: false,
            squad_leader: None,
        },
    )
    .unwrap();
    apply_order(
        game,
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
    apply_order(
        game,
        Order::CommitPath {
            ship: 1,
            actions: Vec::new(),
            evasive: 0,
            follow: false,
        },
    )
    .unwrap();
    apply_order(
        game,
        Order::CommitPath {
            ship: 2,
            actions: Vec::new(),
            evasive: 0,
            follow: false,
        },
    )
    .unwrap();
    assert_eq!(game.phase(), Phase::Firing);
}

#[test]
fn quality_modifiers_reach_authoritative_fire_preview() {
    let baseline_dir = tempfile::tempdir().unwrap();
    write_rules(baseline_dir.path());
    write_ship(baseline_dir.path(), "attacker", &baseline_ship(2, 0, ""));
    write_ship(baseline_dir.path(), "target", &target_ship(2));
    let mut baseline = load_pair(baseline_dir.path(), "attacker", "target");
    enter_firing(&mut baseline);
    let baseline_preview = baseline.fire_decision_preview(1, "beam_1", 2).unwrap();

    let potent_dir = tempfile::tempdir().unwrap();
    write_rules(potent_dir.path());
    write_ship(
        potent_dir.path(),
        "attacker",
        &baseline_ship(2, 0, "damage_bonus = 2"),
    );
    write_ship(potent_dir.path(), "target", &target_ship(2));
    let mut potent = load_pair(potent_dir.path(), "attacker", "target");
    enter_firing(&mut potent);
    let potent_preview = potent.fire_decision_preview(1, "beam_1", 2).unwrap();
    assert_eq!(potent_preview.threshold, baseline_preview.threshold);
    assert_eq!(
        potent_preview.projected_damage,
        baseline_preview.projected_damage + 2
    );

    let precise_dir = tempfile::tempdir().unwrap();
    write_rules(precise_dir.path());
    write_ship(
        precise_dir.path(),
        "attacker",
        &baseline_ship(2, 0, "accuracy_bonus = 2"),
    );
    write_ship(precise_dir.path(), "target", &target_ship(2));
    let mut precise = load_pair(precise_dir.path(), "attacker", "target");
    enter_firing(&mut precise);
    let precise_preview = precise.fire_decision_preview(1, "beam_1", 2).unwrap();
    assert_eq!(precise_preview.threshold, baseline_preview.threshold + 2);
    assert!(precise_preview.threshold < precise_preview.die_sides - 1);
}

#[test]
fn compiled_compact_sku_has_no_combat_modifiers() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("data/designs")).unwrap();
    std::fs::create_dir_all(dir.path().join("data/ships")).unwrap();
    std::fs::copy(
        repo().join("data/sizes.toml"),
        dir.path().join("data/sizes.toml"),
    )
    .unwrap();
    std::fs::copy(
        repo().join("data/components.toml"),
        dir.path().join("data/components.toml"),
    )
    .unwrap();
    write_rules(dir.path());
    let design = dir.path().join("data/designs/compact.toml");
    std::fs::write(
        &design,
        r#"
id = "compact"
name = "Compact"
size = 2
material = "titanium"
engine = "fission"
engine_size = "m"
armored = true
shields = [6, 4, 2, 2, 2, 4]
[[weapons]]
component = "beam_compact"
mount = "forward"
"#,
    )
    .unwrap();
    let output = shipyard::compile(dir.path(), &design).expect("compile compact");
    let ship: ShipDef = toml::from_str(&std::fs::read_to_string(output).unwrap()).unwrap();
    assert_eq!(ship.weapons[0].id, "beam_1");
    assert_eq!(ship.weapons[0].kind, "beam");
    assert_eq!(ship.weapons[0].accuracy_bonus, 0);
    assert_eq!(ship.weapons[0].damage_bonus, 0);

    write_ship(dir.path(), "target", &target_ship(2));
    let mut game = load_pair(dir.path(), "compact", "target");
    enter_firing(&mut game);
    let preview = game.fire_decision_preview(1, "beam_1", 2).unwrap();
    let rules = Ruleset::builtin();
    let expected =
        final_to_hit_threshold(rules.combat(), WeaponKind::Beam, preview.range, 2, 0, 0).unwrap();
    assert_eq!(preview.threshold, expected);
    assert_eq!(
        preview.projected_damage,
        shipsim_core::combat_tables::beam_damage(rules.combat(), 4, preview.range).unwrap()
    );
}

#[test]
fn weapon_accuracy_applies_at_non_baseline_size_while_hull_fire_control_does_not() {
    let dir = tempfile::tempdir().unwrap();
    write_rules(dir.path());
    // Huge hull fire-control would cap the shot if it applied at size 3.
    write_ship(
        dir.path(),
        "attacker",
        &baseline_ship(2, 10, "accuracy_bonus = 2"),
    );
    write_ship(dir.path(), "target", &target_ship(3));
    let mut game = load_pair(dir.path(), "attacker", "target");
    enter_firing(&mut game);
    let preview = game.fire_decision_preview(1, "beam_1", 2).unwrap();
    let rules = Ruleset::builtin();
    let expected = final_to_hit_threshold_with_weapon_bonus(
        rules.combat(),
        WeaponKind::Beam,
        preview.range,
        3,
        10,
        2,
        0,
    )
    .unwrap();
    let hull_only =
        final_to_hit_threshold(rules.combat(), WeaponKind::Beam, preview.range, 3, 10, 0).unwrap();
    assert_eq!(preview.threshold, expected);
    assert_eq!(preview.threshold, hull_only + 2);
    assert_ne!(
        preview.threshold,
        final_to_hit_threshold_with_weapon_bonus(
            rules.combat(),
            WeaponKind::Beam,
            preview.range,
            3,
            10,
            0,
            0,
        )
        .unwrap()
    );
}

#[test]
fn stock_snapshot_omits_zero_quality_fields() {
    let dir = tempfile::tempdir().unwrap();
    write_rules(dir.path());
    write_ship(dir.path(), "attacker", &baseline_ship(2, 0, ""));
    write_ship(dir.path(), "target", &target_ship(2));
    let game = load_pair(dir.path(), "attacker", "target");
    let json = serde_json::to_value(StateSnapshot::from_game_state(&game)).unwrap();
    let weapon = &json["ships"][0]["weapons"][0];
    assert!(weapon.get("accuracy_bonus").is_none());
    assert!(weapon.get("damage_bonus").is_none());
}

#[test]
fn repeat_beam_emits_one_log_row_per_charge_packet() {
    let dir = tempfile::tempdir().unwrap();
    write_rules(dir.path());
    write_ship(
        dir.path(),
        "attacker",
        &baseline_ship(2, 0, "repeat = true"),
    );
    write_ship(dir.path(), "target", &target_ship(2));
    let mut game = load_pair(dir.path(), "attacker", "target");
    enter_firing(&mut game);
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
            shots: vec![shipsim_core::movement::VolleyShot {
                weapon: "beam_1".into(),
                target: 2,
                shield_facing: 0,
            }],
        },
    )
    .unwrap();
    let packets: Vec<_> = game
        .combat_log()
        .iter()
        .filter_map(|event| event.packet)
        .collect();
    assert_eq!(packets, vec![0, 1, 2, 3]);
}

#[test]
fn pierce_beam_preview_halves_before_damage_bonus() {
    let dir = tempfile::tempdir().unwrap();
    write_rules(dir.path());
    write_ship(
        dir.path(),
        "attacker",
        &baseline_ship(2, 0, "pierce = true\ndamage_bonus = 2"),
    );
    write_ship(dir.path(), "target", &target_ship(2));
    let mut game = load_pair(dir.path(), "attacker", "target");
    enter_firing(&mut game);
    let preview = game.fire_decision_preview(1, "beam_1", 2).unwrap();
    let normal =
        shipsim_core::combat_tables::beam_damage(Ruleset::builtin().combat(), 4, preview.range)
            .unwrap();
    assert_eq!(preview.projected_damage, normal.div_ceil(2) + 2);
}
