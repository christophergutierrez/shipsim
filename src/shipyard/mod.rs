use crate::{
    schema::{ShipDef, WeaponDef},
    sizes::{self, HullSize, SizeError},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Design {
    pub id: String,
    pub name: String,
    pub size: u32,
    pub material: String,
    #[serde(default)]
    pub reactor: u64,
    #[serde(default)]
    pub armor: u64,
    #[serde(default)]
    pub shield_banks: u64,
    #[serde(default)]
    pub weapons: Vec<DesignWeapon>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DesignWeapon {
    pub component: String,
    pub mount: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Components {
    reactor: Component,
    armor: Component,
    shield_bank: Component,
    weapons: BTreeMap<String, WeaponComponent>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Component {
    space: u64,
    cost: u64,
    #[serde(default)]
    power: u64,
    #[serde(default)]
    structure: u64,
    #[serde(default)]
    max_shield_per_facing: u64,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct WeaponComponent {
    kind: String,
    space: u64,
    cost: u64,
    max_charge: u32,
    max_range: u32,
    #[serde(default)]
    accuracy_bonus: u8,
    #[serde(default)]
    damage_bonus: u32,
}

#[derive(Debug, Clone, Copy)]
struct Material {
    structure_mult: f64,
    cost_mult: f64,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("cannot read {path:?}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot parse {path:?}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("cannot serialize compiled ship: {0}")]
    Serialize(String),
    #[error("invalid design id {0:?}; expected [a-z0-9_]+ and no path components")]
    InvalidId(String),
    #[error("design filename stem {stem:?} does not match id {id:?}")]
    FilenameMismatch { stem: String, id: String },
    #[error("design name must be non-empty")]
    EmptyName,
    #[error("unknown hull size {0}")]
    UnknownSize(u32),
    #[error("unknown material {0:?}")]
    UnknownMaterial(String),
    #[error("unknown component {0:?}")]
    UnknownComponent(String),
    #[error("unknown mount {0:?}")]
    UnknownMount(String),
    #[error("design is over capacity by {over} space (used {used}, hull {capacity})")]
    OverCapacity { used: u64, capacity: u64, over: u64 },
    #[error("design reactor power {power} cannot buy one motion point")]
    InsufficientReactor { power: u64 },
    #[error("hull size has invalid thrust conversion {thrust_per_power}:{power_per_thrust}")]
    InvalidThrustConversion {
        thrust_per_power: u32,
        power_per_thrust: u32,
    },
    #[error("arithmetic overflow while compiling design")]
    Overflow,
    #[error("compiled field {field}={value} does not fit u32")]
    OutOfRange { field: &'static str, value: u64 },
    #[error("refusing to overwrite unmarked ship file {0:?}")]
    RefuseOverwrite(PathBuf),
    #[error("output path {0:?} is not a regular file")]
    InvalidOutput(PathBuf),
    #[error("size table: {0}")]
    Sizes(#[from] SizeError),
}

fn add(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_add(b).ok_or(Error::Overflow)
}
fn mul(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_mul(b).ok_or(Error::Overflow)
}
fn u32_field(field: &'static str, value: u64) -> Result<u32, Error> {
    u32::try_from(value).map_err(|_| Error::OutOfRange { field, value })
}
fn material(name: &str) -> Result<Material, Error> {
    match name {
        "standard" => Ok(Material {
            structure_mult: 1.0,
            cost_mult: 1.0,
        }),
        "reinforced" => Ok(Material {
            structure_mult: 1.4,
            cost_mult: 1.6,
        }),
        _ => Err(Error::UnknownMaterial(name.to_string())),
    }
}
fn load_components(root: &Path) -> Result<Components, Error> {
    let path = root.join("data/components.toml");
    let text = fs::read_to_string(&path).map_err(|source| Error::Read {
        path: path.clone(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| Error::Parse { path, source })
}
fn half_up(value: f64) -> u64 {
    (value + 0.5).floor() as u64
}
fn checked_count(
    value: u64,
    unit_space: u64,
    unit_cost: u64,
    space: &mut u64,
    cost: &mut u64,
) -> Result<(), Error> {
    *space = add(*space, mul(value, unit_space)?)?;
    *cost = add(*cost, mul(value, unit_cost)?)?;
    Ok(())
}

pub fn load_design(path: &Path) -> Result<Design, Error> {
    let text = fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| Error::Parse {
        path: path.to_path_buf(),
        source,
    })
}

pub fn validate(root: &Path, path: &Path) -> Result<(Design, HullSize), Error> {
    let design = load_design(path)?;
    validate_design(root, path, design)
}

pub fn validate_design(
    root: &Path,
    path: &Path,
    design: Design,
) -> Result<(Design, HullSize), Error> {
    if design.id.is_empty()
        || !design
            .id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err(Error::InvalidId(design.id));
    }
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if stem != design.id {
        return Err(Error::FilenameMismatch {
            stem: stem.to_string(),
            id: design.id,
        });
    }
    if design.name.trim().is_empty() {
        return Err(Error::EmptyName);
    }
    let sizes = sizes::load(root)?;
    let hull = sizes
        .get(design.size)
        .map_err(|_| Error::UnknownSize(design.size))?
        .clone();
    let components = load_components(root)?;
    let _material = material(&design.material)?;
    let mut used = 0u64;
    let mut cost = 0u64;
    checked_count(
        design.reactor,
        components.reactor.space,
        components.reactor.cost,
        &mut used,
        &mut cost,
    )?;
    checked_count(
        design.armor,
        components.armor.space,
        components.armor.cost,
        &mut used,
        &mut cost,
    )?;
    checked_count(
        design.shield_banks,
        components.shield_bank.space,
        components.shield_bank.cost,
        &mut used,
        &mut cost,
    )?;
    for weapon in &design.weapons {
        let c = components
            .weapons
            .get(&weapon.component)
            .ok_or_else(|| Error::UnknownComponent(weapon.component.clone()))?;
        if !matches!(
            weapon.mount.as_str(),
            "forward" | "forward_starboard" | "aft_starboard" | "aft" | "aft_port" | "forward_port"
        ) {
            return Err(Error::UnknownMount(weapon.mount.clone()));
        }
        checked_count(1, c.space, c.cost, &mut used, &mut cost)?;
    }
    if used > u64::from(hull.space) {
        return Err(Error::OverCapacity {
            used,
            capacity: u64::from(hull.space),
            over: used - u64::from(hull.space),
        });
    }
    let motion = design
        .reactor
        .checked_mul(u64::from(hull.thrust_per_power))
        .ok_or(Error::Overflow)?
        .checked_div(u64::from(hull.power_per_thrust))
        .ok_or(Error::InvalidThrustConversion {
            thrust_per_power: hull.thrust_per_power,
            power_per_thrust: hull.power_per_thrust,
        })?;
    if hull.max_maneuver_actions > 0 && motion < 1 {
        return Err(Error::InsufficientReactor {
            power: design.reactor,
        });
    }
    u32_field("space", used)?;
    u32_field("cost", cost)?;
    Ok((design, hull))
}

pub fn design_cost(root: &Path, path: &Path) -> Result<u32, Error> {
    let (design, hull) = validate(root, path)?;
    let components = load_components(root)?;
    let mut cost = half_up(f64::from(hull.frame_cost) * material(&design.material)?.cost_mult);
    checked_count(
        design.reactor,
        0,
        components.reactor.cost,
        &mut 0,
        &mut cost,
    )?;
    checked_count(design.armor, 0, components.armor.cost, &mut 0, &mut cost)?;
    checked_count(
        design.shield_banks,
        0,
        components.shield_bank.cost,
        &mut 0,
        &mut cost,
    )?;
    for w in design.weapons {
        let c = components
            .weapons
            .get(&w.component)
            .ok_or(Error::UnknownComponent(w.component))?;
        checked_count(1, 0, c.cost, &mut 0, &mut cost)?;
    }
    u32_field("cost", cost)
}

pub fn compile(root: &Path, path: &Path) -> Result<PathBuf, Error> {
    let (design, hull) = validate(root, path)?;
    let components = load_components(root)?;
    let material = material(&design.material)?;
    let power = mul(design.reactor, components.reactor.power)?;
    let shields = mul(
        design.shield_banks,
        components.shield_bank.max_shield_per_facing,
    )?;
    let mut structure = add(
        u64::from(hull.base_structure),
        mul(design.armor, components.armor.structure)?,
    )?;
    let mut weapons = Vec::new();
    let mut kind_counts = BTreeMap::<String, u32>::new();
    for w in &design.weapons {
        let c = components
            .weapons
            .get(&w.component)
            .ok_or_else(|| Error::UnknownComponent(w.component.clone()))?;
        let arc = if matches!(
            w.mount.as_str(),
            "forward" | "forward_starboard" | "forward_port"
        ) {
            "forward"
        } else {
            "rear"
        };
        let count = kind_counts.entry(c.kind.clone()).or_insert(0);
        *count = count.checked_add(1).ok_or(Error::Overflow)?;
        weapons.push(WeaponDef {
            id: format!("{}_{}", c.kind, count),
            kind: c.kind.clone(),
            mount: Some(w.mount.clone()),
            arc: arc.to_string(),
            max_range: c.max_range,
            max_charge: c.max_charge,
            max_ammo: None,
            accuracy_bonus: c.accuracy_bonus,
            damage_bonus: c.damage_bonus,
        });
    }
    structure = half_up(structure as f64 * material.structure_mult);
    let cost = design_cost(root, path)?;
    let ship = ShipDef {
        id: design.id.clone(),
        name: design.name,
        size: design.size,
        max_maneuver_actions: hull.max_maneuver_actions,
        power: u32_field("power", power)?,
        max_shield_per_facing: u32_field("max_shield_per_facing", shields)?,
        structure: u32_field("structure", structure)?,
        power_sys: hull.power_sys,
        engine_boxes: hull.engine_boxes,
        weapon_boxes: 1,
        attack_accuracy_bonus: 0,
        weapons,
        thrust_per_power: hull.thrust_per_power,
        power_per_thrust: hull.power_per_thrust,
        cost,
    };
    let out = root.join("data/ships").join(format!("{}.toml", design.id));
    fs::create_dir_all(out.parent().unwrap()).map_err(|source| Error::Read {
        path: out.clone(),
        source,
    })?;
    if out.exists() {
        if !out.is_file() {
            return Err(Error::InvalidOutput(out));
        }
        let old = fs::read_to_string(&out).map_err(|source| Error::Read {
            path: out.clone(),
            source,
        })?;
        if !old
            .lines()
            .next()
            .is_some_and(|l| l.contains("generated by shipsim-yard"))
        {
            return Err(Error::RefuseOverwrite(out));
        }
    }
    let body = toml::to_string_pretty(&ship).map_err(|e| Error::Serialize(e.to_string()))?;
    let tmp = out.with_extension("toml.tmp");
    fs::write(&tmp, format!("# generated by shipsim-yard\n{body}")).map_err(|source| {
        Error::Read {
            path: tmp.clone(),
            source,
        }
    })?;
    fs::rename(&tmp, &out).map_err(|source| Error::Read {
        path: out.clone(),
        source,
    })?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movement::{apply_order, Order, VolleyShot};
    use crate::scenario::load_scenario_def;
    use crate::schema::ScenarioDef;
    use std::collections::BTreeMap;

    fn fixture_root() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("fixture tempdir");
        fs::create_dir_all(dir.path().join("data/designs")).expect("design dir");
        fs::create_dir_all(dir.path().join("data/ships")).expect("ship dir");
        fs::create_dir_all(dir.path().join("data/rules")).expect("rules dir");
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("data/sizes.toml"),
            dir.path().join("data/sizes.toml"),
        )
        .expect("sizes fixture");
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("data/components.toml"),
            dir.path().join("data/components.toml"),
        )
        .expect("components fixture");
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("data/rules/default.toml"),
            dir.path().join("data/rules/default.toml"),
        )
        .expect("rules fixture");
        dir
    }

    fn write_design(root: &Path, filename: &str, body: &str) -> PathBuf {
        let path = root.join("data/designs").join(filename);
        fs::write(&path, body).expect("design fixture");
        path
    }

    const WORKED: &str = r#"
id = "worked"
name = "Worked"
size = 2
material = "standard"
reactor = 14
armor = 2
shield_banks = 4

[[weapons]]
component = "beam"
mount = "forward"
"#;

    #[test]
    fn id_validation_rejects_paths() {
        assert!(matches!(
            validate_design(
                Path::new(env!("CARGO_MANIFEST_DIR")),
                Path::new("x.toml"),
                Design {
                    id: "../x".into(),
                    name: "x".into(),
                    size: 2,
                    material: "standard".into(),
                    reactor: 1,
                    armor: 0,
                    shield_banks: 0,
                    weapons: vec![]
                }
            ),
            Err(Error::InvalidId(_))
        ));
    }

    #[test]
    fn worked_design_projects_fields_and_cost() {
        let root = fixture_root();
        let path = write_design(root.path(), "worked.toml", WORKED);
        let output = compile(root.path(), &path).expect("compile worked design");
        let ship: ShipDef = toml::from_str(&fs::read_to_string(output).unwrap()).unwrap();
        assert_eq!(ship.id, "worked");
        assert_eq!(ship.structure, 8);
        assert_eq!(ship.power, 14);
        assert_eq!(ship.max_shield_per_facing, 4);
        assert!((96..=105).contains(&ship.cost));
    }

    #[test]
    fn compiled_ship_loads_and_completes_a_full_turn() {
        let root = fixture_root();
        let path = write_design(root.path(), "worked.toml", WORKED);
        compile(root.path(), &path).expect("compile worked design");
        let def: ScenarioDef = toml::from_str(
            r#"
width = 8
height = 8
seed = 7
[[ships]]
id = 1
class = "worked"
q = 0
r = 0
facing = 0
controller = "player"
[[ships]]
id = 2
class = "worked"
q = 3
r = 0
facing = 3
controller = "player"
"#,
        )
        .unwrap();
        let mut game = load_scenario_def(&def, root.path()).expect("load compiled ship");
        for ship in [1, 2] {
            apply_order(
                &mut game,
                Order::Allocate {
                    ship,
                    movement: 0,
                    weapons: BTreeMap::from([("beam_1".into(), 4)]),
                    shields: [0; 6],
                },
            )
            .expect("allocate compiled ship");
        }
        for ship in [1, 2] {
            apply_order(
                &mut game,
                Order::CommitPath {
                    ship,
                    actions: Vec::new(),
                    evasive: 0,
                },
            )
            .expect("path compiled ship");
        }
        apply_order(
            &mut game,
            Order::CommitVolley {
                ship: 1,
                shots: vec![VolleyShot {
                    weapon: "beam_1".into(),
                    target: 2,
                    shield_facing: 0,
                }],
            },
        )
        .expect("first volley");
        apply_order(
            &mut game,
            Order::CommitVolley {
                ship: 2,
                shots: Vec::new(),
            },
        )
        .expect("second volley");
        assert_eq!(game.turn_number(), 2);
        assert!(game.combat_log().iter().all(|event| event.attacker == 1));
    }

    #[test]
    fn mixed_weapon_skus_get_stable_per_kind_ids() {
        let root = fixture_root();
        let path = write_design(
            root.path(),
            "mixed.toml",
            r#"
id = "mixed"
name = "Mixed"
size = 2
material = "standard"
reactor = 14
armor = 1
shield_banks = 1
[[weapons]]
component = "beam"
mount = "forward"
[[weapons]]
component = "torpedo"
mount = "aft"
[[weapons]]
component = "beam_precise"
mount = "forward_port"
"#,
        );
        let output = compile(root.path(), &path).expect("compile mixed design");
        let ship: ShipDef = toml::from_str(&fs::read_to_string(output).unwrap()).unwrap();
        let ids: Vec<_> = ship
            .weapons
            .iter()
            .map(|weapon| weapon.id.as_str())
            .collect();
        assert_eq!(ids, ["beam_1", "torp_1", "beam_2"]);
        assert_eq!(ship.weapons[1].arc, "rear");
        assert_eq!(ship.weapons[2].accuracy_bonus, 2);
    }

    #[test]
    fn validation_reports_capacity_and_reactor_errors() {
        let root = fixture_root();
        let over = write_design(
            root.path(),
            "over.toml",
            "id = \"over\"\nname = \"Over\"\nsize = 1\nmaterial = \"standard\"\nreactor = 46\n",
        );
        assert!(matches!(
            validate(root.path(), &over),
            Err(Error::OverCapacity { over: 1, .. })
        ));
        let still = write_design(
            root.path(),
            "still.toml",
            "id = \"still\"\nname = \"Still\"\nsize = 2\nmaterial = \"standard\"\nreactor = 0\n",
        );
        assert!(matches!(
            validate(root.path(), &still),
            Err(Error::InsufficientReactor { power: 0 })
        ));
    }

    #[test]
    fn unknown_design_inputs_are_rejected() {
        let root = fixture_root();
        for (name, field, expected) in [
            (
                "component",
                "component = \"missing\"\nmount = \"forward\"",
                "component",
            ),
            ("mount", "component = \"beam\"\nmount = \"side\"", "mount"),
        ] {
            let body = format!(
                "id = \"{name}\"\nname = \"{name}\"\nsize = 2\nmaterial = \"standard\"\nreactor = 14\n[[weapons]]\n{field}\n"
            );
            let path = write_design(root.path(), &format!("{name}.toml"), &body);
            let error = validate(root.path(), &path).expect_err("invalid input rejected");
            assert!(error.to_string().contains(expected));
        }
        let material = write_design(
            root.path(),
            "material.toml",
            "id = \"material\"\nname = \"Material\"\nsize = 2\nmaterial = \"exotic\"\nreactor = 14\n",
        );
        assert!(matches!(
            validate(root.path(), &material),
            Err(Error::UnknownMaterial(_))
        ));

        let unknown = write_design(
            root.path(),
            "unknown.toml",
            "id = \"unknown\"\nname = \"Unknown\"\nsize = 2\nmaterial = \"standard\"\nreactor = 14\nextra = true\n",
        );
        assert!(matches!(
            validate(root.path(), &unknown),
            Err(Error::Parse { .. })
        ));

        let components = fs::read_to_string(root.path().join("data/components.toml")).unwrap();
        fs::write(
            root.path().join("data/components.toml"),
            components.replacen("[reactor]", "[reactor]\nextra = true", 1),
        )
        .unwrap();
        let valid = write_design(
            root.path(),
            "valid.toml",
            WORKED.replace("worked", "valid").as_str(),
        );
        assert!(matches!(
            validate(root.path(), &valid),
            Err(Error::Parse { .. })
        ));
    }

    #[test]
    fn invalid_filename_does_not_create_output_and_overwrite_policy_is_safe() {
        let root = fixture_root();
        let mismatch = write_design(
            root.path(),
            "wrong.toml",
            WORKED.replace("worked", "right").as_str(),
        );
        assert!(matches!(
            validate(root.path(), &mismatch),
            Err(Error::FilenameMismatch { .. })
        ));
        assert!(!root.path().join("data/ships/right.toml").exists());

        let path = write_design(root.path(), "worked.toml", WORKED);
        let output = root.path().join("data/ships/worked.toml");
        fs::write(&output, "name = \"stock\"\n").unwrap();
        assert!(matches!(
            compile(root.path(), &path),
            Err(Error::RefuseOverwrite(_))
        ));
        fs::write(&output, "# generated by shipsim-yard\nold = true\n").unwrap();
        compile(root.path(), &path).expect("marked output may be replaced");
        let first = fs::read(&output).unwrap();
        compile(root.path(), &path).expect("repeat compile");
        assert_eq!(first, fs::read(&output).unwrap());
    }

    #[test]
    fn out_of_range_counts_are_rejected_without_wrapping() {
        let root = fixture_root();
        fs::write(
            root.path().join("data/components.toml"),
            r#"
[reactor]
space = 0
cost = 1
power = 1
[armor]
space = 0
cost = 0
structure = 0
[shield_bank]
space = 0
cost = 0
max_shield_per_facing = 0
[weapons]
"#,
        )
        .expect("zero-space component fixture");
        let path = write_design(
            root.path(),
            "huge.toml",
            &format!(
                "id = \"huge\"\nname = \"Huge\"\nsize = 2\nmaterial = \"standard\"\nreactor = {}\n",
                i64::MAX
            ),
        );
        assert!(matches!(
            validate(root.path(), &path),
            Err(Error::OutOfRange { field: "cost", .. })
        ));

        fs::write(
            root.path().join("data/components.toml"),
            r#"
[reactor]
space = 9223372036854775807
cost = 1
power = 1
[armor]
space = 0
cost = 0
structure = 0
[shield_bank]
space = 0
cost = 0
max_shield_per_facing = 0
[weapons]
"#,
        )
        .expect("overflow component fixture");
        let overflow = write_design(
            root.path(),
            "overflow.toml",
            "id = \"overflow\"\nname = \"Overflow\"\nsize = 2\nmaterial = \"standard\"\nreactor = 3\n",
        );
        assert!(matches!(
            validate(root.path(), &overflow),
            Err(Error::Overflow)
        ));
    }
}
