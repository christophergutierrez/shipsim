use crate::{
    schema::{ShipDef, SystemDef, SystemKind, WeaponDef},
    sizes::{self, HullSize, SizeError},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

mod naming;
mod catalog;
mod parse;
mod spec;
mod compile;
pub use naming::{allocate_id, is_generated_class_name, names_collide, unique_class_name};
pub use catalog::{design_cost, list_designs, new_design, preview_design, save_design, system_skus, weapon_skus};
pub use parse::{load_design, validate, validate_design};
pub use spec::{engine_key, engine_spec, material, system_spec, weapon_headline, weapon_headline_from_spec, weapon_spec};
pub use compile::{check, check_all, compile, generated_path, render};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Design {
    pub id: String,
    pub name: String,
    pub size: u32,
    pub material: String,
    #[serde(default)]
    pub engine: String,
    #[serde(default)]
    pub engine_size: String,
    /// Exterior plate. No interior space; 1.5× hull HP; costs half the frame.
    #[serde(default)]
    pub armored: bool,
    /// Banks on F, FR, RR, R, RL, FL. One bank = one power on that face.
    #[serde(default)]
    pub shields: [u64; 6],
    #[serde(default)]
    pub weapons: Vec<DesignWeapon>,
    #[serde(default)]
    pub systems: Vec<DesignSystem>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DesignWeapon {
    pub component: String,
    pub mount: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DesignSystem {
    pub component: String,
}

/// Design ids that exist to hold one weapon SKU constant for balance
/// comparison (`simulation/suites/weapon_quality_matched.toml`), not as a
/// player-facing standard. Hidden from the interactive shipyard browser only —
/// `list_designs`, `compile`, `check_all`, and the CLI still see them, since
/// hiding here must not change what gets compiled, checked, or drift-audited.
/// Editing one silently invalidates that suite's controlled comparison, which
/// is exactly the mistake the catalog-remediation review locked
/// these roles against; keeping them out of the interactive picker is cheaper
/// than relying on everyone re-deriving that history from the plan.
pub const QUALITY_FIXTURE_IDS: &[&str] = &[
    "yard_baseline",
    "yard_compact",
    "yard_potent",
    "yard_precise",
];

/// Player-facing stock classes. These remain visible in the yard, but are
/// reference material rather than editable user designs.
pub const STANDARD_CLASS_IDS: &[&str] = &[
    "yard_swarm",
    "yard_destroyer",
    "yard_light_cruiser",
    "yard_heavy_cruiser",
    "yard_battleship",
    "yard_dreadnought",
    "yard_capital",
];

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Components {
    engines: BTreeMap<String, EngineComponent>,
    shield_bank: Component,
    weapons: BTreeMap<String, WeaponComponent>,
    #[serde(default)]
    systems: BTreeMap<String, SystemComponent>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct EngineComponent {
    power: u64,
    space: u64,
    cost: u64,
    #[serde(default)]
    thrust_step: i8,
}

/// Public view of one discrete engine plant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineSpec {
    pub id: String,
    pub kind: String,
    pub size: String,
    pub power: u32,
    pub space: u32,
    pub cost: u32,
    pub thrust_step: i8,
}

/// Public view of one weapon SKU from the component catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeaponSpec {
    pub id: String,
    pub kind: String,
    pub space: u32,
    pub cost: u32,
    pub max_charge: u32,
    pub max_range: u32,
    pub max_ammo: Option<u32>,
    pub accuracy_bonus: u8,
    pub damage_bonus: u32,
    pub repeat: bool,
    pub pierce: bool,
}

impl WeaponSpec {
    /// Quality tags from component fields, never from the SKU name string.
    pub fn quality_tags(&self) -> Vec<String> {
        let mut tags = Vec::new();
        if self.accuracy_bonus > 0 {
            tags.push(format!("Precise +{}", self.accuracy_bonus));
        }
        if self.damage_bonus > 0 {
            tags.push(format!("Potent +{}", self.damage_bonus));
        }
        if self.repeat {
            tags.push("Repeat".into());
        }
        if self.pierce {
            tags.push("Pierce".into());
        }
        if let Some(ammo) = self.max_ammo {
            tags.push(format!("Ammo {ammo}"));
        }
        tags
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Component {
    space: u64,
    cost: u64,
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
    max_ammo: Option<u32>,
    #[serde(default)]
    accuracy_bonus: u8,
    #[serde(default)]
    damage_bonus: u32,
    #[serde(default)]
    repeat: bool,
    #[serde(default)]
    pierce: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SystemComponent {
    kind: String,
    #[serde(default)]
    mk: Option<u8>,
    space: u64,
    cost: u64,
}

/// Hull plating. Structure and frame cost use the same multiplier (MOO armor).
/// Tech is display-only; every material is always available.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub tech: u8,
    pub structure_mult: f64,
    pub cost_mult: f64,
}

pub const MATERIALS: &[MaterialSpec] = &[
    MaterialSpec {
        id: "titanium",
        name: "Titanium",
        tech: 1,
        structure_mult: 1.0,
        cost_mult: 1.0,
    },
    MaterialSpec {
        id: "duralloy",
        name: "Duralloy",
        tech: 9,
        structure_mult: 1.5,
        cost_mult: 1.5,
    },
    MaterialSpec {
        id: "zortrium",
        name: "Zortrium",
        tech: 17,
        structure_mult: 2.0,
        cost_mult: 2.0,
    },
    MaterialSpec {
        id: "andrium",
        name: "Andrium",
        tech: 26,
        structure_mult: 2.5,
        cost_mult: 2.5,
    },
    MaterialSpec {
        id: "tritanium",
        name: "Tritanium",
        tech: 34,
        structure_mult: 3.0,
        cost_mult: 3.0,
    },
    MaterialSpec {
        id: "adamantium",
        name: "Adamantium",
        tech: 42,
        structure_mult: 3.5,
        cost_mult: 3.5,
    },
    MaterialSpec {
        id: "neutronium",
        name: "Neutronium",
        tech: 50,
        structure_mult: 4.0,
        cost_mult: 4.0,
    },
];

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
    #[error("class name {0:?} is already used")]
    DuplicateName(String),
    #[error("unknown hull size {0}")]
    UnknownSize(u32),
    #[error("unknown material {0:?}")]
    UnknownMaterial(String),
    #[error("unknown component {0:?}")]
    UnknownComponent(String),
    #[error("unknown system kind {0:?}")]
    UnknownSystemKind(String),
    #[error("system {0:?} may be installed only once")]
    DuplicateSystem(String),
    #[error("computer system mk must be 1, 2, or 3")]
    InvalidComputerMark,
    #[error("weapon component {0:?} has incompatible repeat and pierce flags")]
    IncompatibleWeaponFlags(String),
    #[error("weapon component {0:?} has an unknown kind")]
    UnknownWeaponKind(String),
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
    #[error("generated ship file is missing: {0:?}")]
    GeneratedMissing(PathBuf),
    #[error("generated ship file is out of date: {0:?}")]
    GeneratedDrift(PathBuf),
    #[error("size table: {0}")]
    Sizes(#[from] SizeError),
    #[error("combat rules: {0}")]
    Rules(String),
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
pub const ENGINE_KINDS: &[&str] = &["fission", "fusion", "antimatter"];
pub const ENGINE_SIZES: &[&str] = &["s", "m", "l", "h"];
pub const SHIELD_FACES: &[&str] = &["F", "FR", "RR", "R", "RL", "FL"];
/// Exterior armor multiplies hull base HP and costs this fraction of frame cost.
pub const ARMOR_HP_MULT: f64 = 1.5;
pub const ARMOR_FRAME_COST_MULT: f64 = 0.5;

fn plate_cost(frame_cost: u32, mat: &MaterialSpec) -> u64 {
    half_up(f64::from(frame_cost) * mat.cost_mult * ARMOR_FRAME_COST_MULT)
}

fn plated_structure(base: u32, mat: &MaterialSpec, armored: bool) -> u64 {
    let hp = if armored { ARMOR_HP_MULT } else { 1.0 };
    half_up(f64::from(base) * mat.structure_mult * hp)
}

const THRUST_LADDER: &[(u32, u32)] = &[
    (4, 1),
    (3, 1),
    (2, 1),
    (1, 1),
    (1, 2),
    (1, 3),
    (1, 4),
    (1, 5),
    (1, 6),
];


fn apply_thrust_step(thrust_per_power: u32, power_per_thrust: u32, step: i8) -> (u32, u32) {
    let idx = THRUST_LADDER
        .iter()
        .position(|&pair| pair == (thrust_per_power, power_per_thrust))
        .unwrap_or(3);
    let next = (idx as i32 - i32::from(step)).clamp(0, THRUST_LADDER.len() as i32 - 1) as usize;
    THRUST_LADDER[next]
}

fn shield_install(components: &Components, banks: [u64; 6]) -> Result<(u64, u64, [u32; 6]), Error> {
    let unit = &components.shield_bank;
    let mut count = 0u64;
    let mut faces = [0u32; 6];
    for (i, banks_here) in banks.iter().copied().enumerate() {
        count = add(count, banks_here)?;
        faces[i] = u32_field("shields", mul(banks_here, unit.max_shield_per_facing)?)?;
    }
    Ok((mul(count, unit.space)?, mul(count, unit.cost)?, faces))
}

fn plant<'a>(
    components: &'a Components,
    kind: &str,
    size: &str,
) -> Result<&'a EngineComponent, Error> {
    if kind.is_empty() || size.is_empty() {
        return Err(Error::InsufficientReactor { power: 0 });
    }
    let key = engine_key(kind, size);
    components
        .engines
        .get(&key)
        .ok_or(Error::UnknownComponent(key))
}

fn compile_systems(
    components: &Components,
    requested: &[DesignSystem],
) -> Result<(Vec<SystemDef>, u64, u64), Error> {
    let mut systems = Vec::with_capacity(requested.len());
    let mut used = 0u64;
    let mut cost = 0u64;
    let mut seen = BTreeMap::<String, ()>::new();
    for design in requested {
        let component = components
            .systems
            .get(&design.component)
            .ok_or_else(|| Error::UnknownComponent(design.component.clone()))?;
        let kind = match component.kind.as_str() {
            "computer" => {
                let mk = component.mk.ok_or(Error::InvalidComputerMark)?;
                if !(1..=3).contains(&mk) {
                    return Err(Error::InvalidComputerMark);
                }
                SystemKind::Computer { mk }
            }
            "cloak" => SystemKind::Cloak,
            "repair" => SystemKind::Repair,
            "ecm" => SystemKind::Ecm,
            other => return Err(Error::UnknownSystemKind(other.to_string())),
        };
        let key = match kind {
            SystemKind::Computer { .. } => "computer",
            SystemKind::Cloak => "cloak",
            SystemKind::Repair => "repair",
            SystemKind::Ecm => "ecm",
        };
        if seen.insert(key.to_string(), ()).is_some() {
            return Err(Error::DuplicateSystem(key.to_string()));
        }
        used = add(used, component.space)?;
        cost = add(cost, component.cost)?;
        systems.push(SystemDef { kind });
    }
    Ok((systems, used, cost))
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


/// Live cost/space/combat totals for a design, without writing files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignPreview {
    pub cost: u32,
    pub space_used: u32,
    pub space_cap: u32,
    pub power: u32,
    pub structure: u32,
    pub shields: u32,
    pub shield_faces: [u32; 6],
}

pub const MOUNTS: &[&str] = &[
    "forward",
    "forward_starboard",
    "aft_starboard",
    "aft",
    "aft_port",
    "forward_port",
];



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
material = "titanium"
engine = "fission"
engine_size = "m"
armored = true
shields = [6, 4, 2, 2, 2, 4]

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
                    material: "titanium".into(),
                    engine: "fission".into(),
                    engine_size: "s".into(),
                    armored: false,
                    shields: [0; 6],
                    weapons: vec![],
                    systems: vec![]
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
        assert_eq!(ship.structure, 9);
        assert_eq!(ship.power, 14);
        assert_eq!(ship.max_shield_per_facing, 6);
        assert_eq!(ship.max_shields, Some([6, 4, 2, 2, 2, 4]));
        // frame 9 + plate 5 + fission_m 36 + banks 20 + beam 16
        assert_eq!(ship.cost, 86);
        assert_eq!(ship.thrust_per_power, 2);
        assert_eq!(ship.power_per_thrust, 1);
        let preview = preview_design(root.path(), &load_design(&path).unwrap()).unwrap();
        assert_eq!(preview.cost, ship.cost);
        assert_eq!(preview.power, 14);
        assert_eq!(preview.structure, 9);
        assert_eq!(preview.shields, 6);
        assert_eq!(preview.shield_faces, [6, 4, 2, 2, 2, 4]);
    }

    #[test]
    fn list_and_save_keep_cost_on_the_design() {
        let root = fixture_root();
        let design = new_design("listed");
        save_design(root.path(), &design).unwrap();
        let listed = list_designs(root.path()).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].1.id, "listed");
        let preview = preview_design(root.path(), &listed[0].1).unwrap();
        assert!(preview.cost > 0);
        assert_eq!(
            preview.cost,
            preview_design(root.path(), &design).unwrap().cost
        );
    }

    #[test]
    fn moo_armor_scales_structure_and_frame_cost() {
        assert_eq!(MATERIALS.len(), 7);
        assert_eq!(MATERIALS[0].id, "titanium");
        assert_eq!(MATERIALS[0].tech, 1);
        assert_eq!(MATERIALS[6].id, "neutronium");
        assert_eq!(MATERIALS[6].structure_mult, 4.0);
        let root = fixture_root();
        let mut design = new_design("mat");
        design.material = "titanium".into();
        let titanium = preview_design(root.path(), &design).unwrap();
        design.material = "neutronium".into();
        let neutronium = preview_design(root.path(), &design).unwrap();
        assert_eq!(neutronium.structure, titanium.structure * 4);
        assert!(neutronium.cost > titanium.cost);
        assert!(material("exotic").is_err());
    }

    #[test]
    fn compact_sku_compiles_without_quality_modifiers() {
        let root = fixture_root();
        let path = write_design(
            root.path(),
            "compact.toml",
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
        );
        let output = compile(root.path(), &path).expect("compile compact");
        let ship: ShipDef = toml::from_str(&fs::read_to_string(output).unwrap()).unwrap();
        assert_eq!(ship.weapons[0].accuracy_bonus, 0);
        assert_eq!(ship.weapons[0].damage_bonus, 0);
    }

    #[test]
    fn typed_combat_kit_round_trips_through_ship_loader() {
        let root = fixture_root();
        let path = write_design(
            root.path(),
            "kit.toml",
            r#"
id = "kit"
name = "Typed Kit"
size = 3
material = "titanium"
engine = "fission"
engine_size = "l"
shields = [4, 2, 0, 0, 0, 2]

[[weapons]]
component = "beam_repeat"
mount = "forward"

[[weapons]]
component = "beam_pierce"
mount = "forward_port"

[[weapons]]
component = "pd"
mount = "aft"

[[weapons]]
component = "graviton"
mount = "forward_starboard"

[[weapons]]
component = "missile"
mount = "aft_port"

[[systems]]
component = "computer_mk3"

[[systems]]
component = "cloak"

[[systems]]
component = "repair"

[[systems]]
component = "ecm"
"#,
        );
        let output = compile(root.path(), &path).expect("compile typed kit");
        let ship: ShipDef = toml::from_str(&fs::read_to_string(output).unwrap()).unwrap();
        assert_eq!(ship.weapons.len(), 5);
        assert!(ship.weapons[0].repeat);
        assert!(ship.weapons[1].pierce);
        assert_eq!(ship.systems.len(), 4);
        assert_eq!(
            ship.systems[0],
            SystemDef {
                kind: SystemKind::Computer { mk: 3 }
            }
        );

        let def: ScenarioDef = toml::from_str(
            r#"
width = 8
height = 8
[[ships]]
id = 1
class = "kit"
q = 0
r = 0
facing = 0
"#,
        )
        .unwrap();
        let game = load_scenario_def(&def, root.path()).expect("load typed kit");
        let snapshot = crate::snapshot::StateSnapshot::from_game_state(&game);
        assert_eq!(snapshot.ships[0].systems.len(), 4);
        assert!(snapshot.ships[0].weapons[0].repeat);
        assert!(snapshot.ships[0].weapons[1].pierce);
        assert_eq!(game.ships()[0].ssd.hull_max, ship.structure);
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
                    cloak: false,
                    repair: 0,
                    unsquad: false,
                    squad_leader: None,
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
                    follow: false,
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
material = "titanium"
engine = "fission"
engine_size = "m"
armored = true
shields = [1, 1, 1, 1, 1, 1]
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
    fn public_weapon_spec_and_headline_use_catalog_and_rules() {
        let root = fixture_root();
        let rules = crate::rules::Ruleset::load(root.path()).expect("rules");
        let combat = rules.combat();
        let precise = weapon_spec(root.path(), "beam_precise").expect("precise SKU");
        assert_eq!(precise.kind, "beam");
        assert_eq!(precise.accuracy_bonus, 2);
        assert_eq!(precise.damage_bonus, 0);
        assert_eq!(precise.quality_tags(), vec!["Precise +2".to_string()]);

        let beam = weapon_spec(root.path(), "beam").expect("beam");
        let beam_dmg = crate::combat_tables::beam_damage(combat, beam.max_charge, 1).unwrap();
        assert_eq!(
            weapon_headline(root.path(), "beam").unwrap(),
            format!("dmg {beam_dmg} @ r1")
        );
        assert!(beam.quality_tags().is_empty());

        let potent = weapon_spec(root.path(), "beam_potent").expect("potent");
        let potent_dmg = beam_dmg.saturating_add(potent.damage_bonus);
        assert_eq!(
            weapon_headline(root.path(), "beam_potent").unwrap(),
            format!("dmg {potent_dmg} @ r1")
        );
        assert_eq!(potent.quality_tags(), vec!["Potent +2".to_string()]);

        let torp = crate::combat_tables::torp_damage(combat, 1).unwrap();
        assert_eq!(
            weapon_headline(root.path(), "torpedo").unwrap(),
            format!("dmg {torp}")
        );
        assert_eq!(weapon_headline(root.path(), "missile").unwrap(), "dmg 2");
        assert_eq!(
            weapon_headline(root.path(), "pd").unwrap(),
            "intercept only"
        );
        assert_eq!(
            weapon_headline(root.path(), "graviton").unwrap(),
            "dmg = your size − target size"
        );
    }

    #[test]
    fn public_system_spec_names_computer_accuracy() {
        let root = fixture_root();
        let mk2 = system_spec(root.path(), "computer_mk2").expect("mk2");
        assert_eq!(mk2.kind, "computer");
        assert_eq!(mk2.mk, Some(2));
        assert_eq!(mk2.headline(), "to-hit +2");
        let ids = system_skus(root.path()).expect("skus");
        assert!(ids.iter().any(|id| id == "computer_mk1"));
        assert!(ids.iter().any(|id| id == "cloak"));
    }

    #[test]
    fn render_omits_empty_systems_and_is_deterministic() {
        let root = fixture_root();
        let path = write_design(
            root.path(),
            "rendered.toml",
            &WORKED.replace("worked", "rendered"),
        );
        let first = render(root.path(), &path).unwrap();
        let second = render(root.path(), &path).unwrap();
        assert_eq!(first, second);
        let text = String::from_utf8(first).unwrap();
        assert!(text.starts_with("# generated by shipsim-yard\n"));
        assert!(!text.contains("systems = []"));
    }

    #[test]
    fn check_detects_missing_and_stale_outputs_without_writing() {
        let root = fixture_root();
        let path = write_design(
            root.path(),
            "checked.toml",
            &WORKED.replace("worked", "checked"),
        );
        assert!(matches!(
            check(root.path(), &path),
            Err(Error::GeneratedMissing(_))
        ));
        let output = compile(root.path(), &path).unwrap();
        assert_eq!(check(root.path(), &path).unwrap(), output);
        let mut stale = fs::read_to_string(&output).unwrap();
        stale.push_str("\n# stale\n");
        fs::write(&output, stale).unwrap();
        let stale_bytes = fs::read(&output).unwrap();
        assert!(matches!(
            check(root.path(), &path),
            Err(Error::GeneratedDrift(_))
        ));
        assert_eq!(fs::read(&output).unwrap(), stale_bytes);
    }

    #[test]
    fn validation_reports_capacity_and_reactor_errors() {
        let root = fixture_root();
        let over = write_design(
            root.path(),
            "over.toml",
            "id = \"over\"\nname = \"Over\"\nsize = 1\nmaterial = \"titanium\"\nengine = \"fission\"\nengine_size = \"h\"\n",
        );
        assert!(matches!(
            validate(root.path(), &over),
            Err(Error::OverCapacity { .. })
        ));
        let still = write_design(
            root.path(),
            "still.toml",
            "id = \"still\"\nname = \"Still\"\nsize = 2\nmaterial = \"titanium\"\nengine = \"\"\nengine_size = \"\"\n",
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
                "id = \"{name}\"\nname = \"{name}\"\nsize = 2\nmaterial = \"titanium\"\nengine = \"fission\"\nengine_size = \"m\"\n[[weapons]]\n{field}\n"
            );
            let path = write_design(root.path(), &format!("{name}.toml"), &body);
            let error = validate(root.path(), &path).expect_err("invalid input rejected");
            assert!(error.to_string().contains(expected));
        }
        let material = write_design(
            root.path(),
            "material.toml",
            "id = \"material\"\nname = \"Material\"\nsize = 2\nmaterial = \"exotic\"\nengine = \"fission\"\nengine_size = \"m\"\n",
        );
        assert!(matches!(
            validate(root.path(), &material),
            Err(Error::UnknownMaterial(_))
        ));

        let unknown = write_design(
            root.path(),
            "unknown.toml",
            "id = \"unknown\"\nname = \"Unknown\"\nsize = 2\nmaterial = \"titanium\"\nengine = \"fission\"\nengine_size = \"m\"\nextra = true\n",
        );
        assert!(matches!(
            validate(root.path(), &unknown),
            Err(Error::Parse { .. })
        ));

        let components = fs::read_to_string(root.path().join("data/components.toml")).unwrap();
        fs::write(
            root.path().join("data/components.toml"),
            components.replacen(
                "[engines.fission_m]",
                "[engines.fission_m]\nextra = true",
                1,
            ),
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
[engines.fission_m]
power = 1
space = 0
cost = 5000000000
thrust_step = 0
[shield_bank]
space = 0
cost = 0
max_shield_per_facing = 0
[weapons]
"#,
        )
        .expect("huge-cost engine fixture");
        let path = write_design(
            root.path(),
            "huge.toml",
            "id = \"huge\"\nname = \"Huge\"\nsize = 2\nmaterial = \"titanium\"\nengine = \"fission\"\nengine_size = \"m\"\n",
        );
        assert!(matches!(
            validate(root.path(), &path),
            Err(Error::OutOfRange { field: "cost", .. })
        ));

        fs::write(
            root.path().join("data/components.toml"),
            r#"
[engines.fission_m]
power = 1
space = 0
cost = 1
thrust_step = 0
[shield_bank]
space = 9223372036854775807
cost = 0
max_shield_per_facing = 0
[weapons]
"#,
        )
        .expect("overflow engine fixture");
        let overflow = write_design(
            root.path(),
            "overflow.toml",
            "id = \"overflow\"\nname = \"Overflow\"\nsize = 2\nmaterial = \"titanium\"\nengine = \"fission\"\nengine_size = \"m\"\nshields = [3, 0, 0, 0, 0, 0]\n",
        );
        assert!(matches!(
            validate(root.path(), &overflow),
            Err(Error::Overflow)
        ));
    }

    #[test]
    fn discrete_engine_costs_space_and_can_step_thrust() {
        let root = fixture_root();
        let mut small = new_design("eng");
        small.engine = "fission".into();
        small.engine_size = "s".into();
        let mut large = small.clone();
        large.engine_size = "m".into();
        let ps = preview_design(root.path(), &small).unwrap();
        let pl = preview_design(root.path(), &large).unwrap();
        assert!(pl.power > ps.power);
        assert!(pl.space_used > ps.space_used);
        assert!(pl.cost > ps.cost);
        let fission = write_design(root.path(), "worked.toml", WORKED);
        let fusion_body = WORKED
            .replace("id = \"worked\"", "id = \"hot\"")
            .replace("engine = \"fission\"", "engine = \"fusion\"");
        let fusion = write_design(root.path(), "hot.toml", &fusion_body);
        let cool = compile(root.path(), &fission).unwrap();
        let hot = compile(root.path(), &fusion).unwrap();
        let cool_ship: ShipDef = toml::from_str(&fs::read_to_string(cool).unwrap()).unwrap();
        let hot_ship: ShipDef = toml::from_str(&fs::read_to_string(hot).unwrap()).unwrap();
        assert_eq!(
            (cool_ship.thrust_per_power, cool_ship.power_per_thrust),
            (2, 1)
        );
        assert_eq!(
            (hot_ship.thrust_per_power, hot_ship.power_per_thrust),
            (3, 1)
        );
        assert!(hot_ship.power > cool_ship.power);
    }

    #[test]
    fn exterior_armor_is_a_hull_wrap() {
        let root = fixture_root();
        let mut bare = new_design("plate");
        bare.armored = false;
        let mut plated = bare.clone();
        plated.armored = true;
        let off = preview_design(root.path(), &bare).unwrap();
        let on = preview_design(root.path(), &plated).unwrap();
        assert_eq!(on.space_used, off.space_used);
        assert_eq!(
            on.structure,
            half_up(off.structure as f64 * ARMOR_HP_MULT) as u32
        );
        assert!(on.cost > off.cost);
        plated.size = 7;
        let titan = preview_design(root.path(), &plated).unwrap();
        plated.armored = false;
        let titan_bare = preview_design(root.path(), &plated).unwrap();
        assert!(titan.cost - titan_bare.cost > on.cost - off.cost);
    }

    #[test]
    fn class_ids_are_short_tokens_and_names_do_not_collide() {
        let a = allocate_id(std::iter::empty());
        let b = allocate_id(std::iter::once(a.as_str()));
        assert_ne!(a, b);
        assert_eq!(a.len(), 8);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        let first = unique_class_name("Destroyer", std::iter::empty());
        assert_eq!(first, "Basic Destroyer");
        let second = unique_class_name("Destroyer", std::iter::once("Basic Destroyer"));
        assert_eq!(second, "Basic Destroyer 2");
        assert!(names_collide("Basic Destroyer", "basic  destroyer"));
        assert!(is_generated_class_name(
            "Basic Light Cruiser 3",
            &["Light Cruiser"]
        ));
        let root = fixture_root();
        let mut one = new_design("n1");
        one.name = "Repeater".into();
        save_design(root.path(), &one).unwrap();
        let mut two = new_design("n2");
        two.name = "repeater".into();
        assert!(matches!(
            save_design(root.path(), &two),
            Err(Error::DuplicateName(_))
        ));
    }
}
