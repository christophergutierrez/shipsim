use super::*;

/// Public view of one system SKU from the component catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemSpec {
    pub id: String,
    pub kind: String,
    pub mk: Option<u8>,
    pub space: u32,
    pub cost: u32,
}

pub fn engine_key(kind: &str, size: &str) -> String {
    format!("{kind}_{size}")
}

pub fn material(name: &str) -> Result<&'static MaterialSpec, Error> {
    MATERIALS
        .iter()
        .find(|m| m.id == name)
        .ok_or_else(|| Error::UnknownMaterial(name.to_string()))
}

impl SystemSpec {
    pub fn headline(&self) -> String {
        match self.kind.as_str() {
            "computer" => format!("to-hit +{}", self.mk.unwrap_or(0)),
            "cloak" => "cloak −4 incoming".into(),
            "repair" => "repair hull".into(),
            "ecm" => "ECM −2 vs missiles".into(),
            other => other.to_string(),
        }
    }
}

pub fn system_spec(root: &Path, id: &str) -> Result<SystemSpec, Error> {
    let components = load_components(root)?;
    let system = components
        .systems
        .get(id)
        .ok_or_else(|| Error::UnknownComponent(id.to_string()))?;
    if system.kind == "computer" {
        let mk = system.mk.ok_or(Error::InvalidComputerMark)?;
        if !(1..=3).contains(&mk) {
            return Err(Error::InvalidComputerMark);
        }
    }
    Ok(SystemSpec {
        id: id.to_string(),
        kind: system.kind.clone(),
        mk: system.mk,
        space: u32_field("space", system.space)?,
        cost: u32_field("cost", system.cost)?,
    })
}

pub fn engine_spec(root: &Path, kind: &str, size: &str) -> Result<EngineSpec, Error> {
    let components = load_components(root)?;
    let plant = plant(&components, kind, size)?;
    Ok(EngineSpec {
        id: engine_key(kind, size),
        kind: kind.to_string(),
        size: size.to_string(),
        power: u32_field("power", plant.power)?,
        space: u32_field("space", plant.space)?,
        cost: u32_field("cost", plant.cost)?,
        thrust_step: plant.thrust_step,
    })
}

pub fn weapon_spec(root: &Path, id: &str) -> Result<WeaponSpec, Error> {
    let components = load_components(root)?;
    let weapon = components
        .weapons
        .get(id)
        .ok_or_else(|| Error::UnknownComponent(id.to_string()))?;
    Ok(WeaponSpec {
        id: id.to_string(),
        kind: weapon.kind.clone(),
        space: u32_field("space", weapon.space)?,
        cost: u32_field("cost", weapon.cost)?,
        max_charge: weapon.max_charge,
        max_range: weapon.max_range,
        max_ammo: weapon.max_ammo,
        accuracy_bonus: weapon.accuracy_bonus,
        damage_bonus: weapon.damage_bonus,
        repeat: weapon.repeat,
        pierce: weapon.pierce,
    })
}

/// A compact editor-facing summary evaluated from the authoritative rules.
/// Beam/plasma use full charge at range 1; torp/missile are flat. PD and
/// graviton name the mechanic instead of inventing a number.
pub fn weapon_headline(root: &Path, id: &str) -> Result<String, Error> {
    let spec = weapon_spec(root, id)?;
    weapon_headline_from_spec(root, &spec)
}

pub fn weapon_headline_from_spec(root: &Path, spec: &WeaponSpec) -> Result<String, Error> {
    let rules = crate::rules::Ruleset::load(root).map_err(|e| Error::Rules(e.to_string()))?;
    let combat = rules.combat();
    let bonus = spec.damage_bonus;
    match spec.kind.as_str() {
        "beam" => {
            let dmg = crate::combat_tables::beam_damage(combat, spec.max_charge, 1)
                .unwrap_or(0)
                .saturating_add(bonus);
            Ok(format!("dmg {dmg} @ r1"))
        }
        "plasma" => {
            let dmg = crate::combat_tables::plasma_damage(combat, 1)
                .unwrap_or(0)
                .saturating_add(bonus);
            Ok(format!("dmg {dmg} @ r1"))
        }
        "torp" => {
            let dmg = crate::combat_tables::torp_damage(combat, 1)
                .unwrap_or(0)
                .saturating_add(bonus);
            Ok(format!("dmg {dmg}"))
        }
        "missile" => Ok(format!("dmg {}", 2u32.saturating_add(bonus))),
        "pd" => Ok("intercept only".into()),
        "graviton" => Ok("dmg = your size − target size".into()),
        other => Err(Error::UnknownWeaponKind(other.to_string())),
    }
}
