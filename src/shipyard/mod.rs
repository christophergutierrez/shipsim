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
        / u64::from(hull.power_per_thrust);
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
    for (idx, w) in design.weapons.iter().enumerate() {
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
        weapons.push(WeaponDef {
            id: format!("{}_{}", c.kind, idx + 1),
            kind: c.kind.clone(),
            mount: Some(w.mount.clone()),
            arc: arc.to_string(),
            max_range: c.max_range,
            max_charge: c.max_charge,
            max_ammo: None,
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
}
