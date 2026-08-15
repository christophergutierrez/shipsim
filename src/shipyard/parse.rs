use super::*;

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
    let engine = plant(&components, &design.engine, &design.engine_size)?;
    checked_count(1, engine.space, engine.cost, &mut used, &mut cost)?;
    if design.armored {
        cost = add(cost, plate_cost(hull.frame_cost, _material))?;
    }
    let (shield_space, shield_cost, _) = shield_install(&components, design.shields)?;
    used = add(used, shield_space)?;
    cost = add(cost, shield_cost)?;
    let (_, systems_space, systems_cost) = compile_systems(&components, &design.systems)?;
    used = add(used, systems_space)?;
    cost = add(cost, systems_cost)?;
    for weapon in &design.weapons {
        let c = components
            .weapons
            .get(&weapon.component)
            .ok_or_else(|| Error::UnknownComponent(weapon.component.clone()))?;
        if c.repeat && c.pierce {
            return Err(Error::IncompatibleWeaponFlags(weapon.component.clone()));
        }
        if (c.repeat || c.pierce) && c.kind != "beam" {
            return Err(Error::IncompatibleWeaponFlags(weapon.component.clone()));
        }
        if !matches!(
            c.kind.as_str(),
            "beam" | "plasma" | "torp" | "missile" | "pd" | "graviton"
        ) {
            return Err(Error::UnknownWeaponKind(c.kind.clone()));
        }
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
    let (thrust_per_power, power_per_thrust) = apply_thrust_step(
        hull.thrust_per_power,
        hull.power_per_thrust,
        engine.thrust_step,
    );
    let motion = engine
        .power
        .checked_mul(u64::from(thrust_per_power))
        .ok_or(Error::Overflow)?
        .checked_div(u64::from(power_per_thrust))
        .ok_or(Error::InvalidThrustConversion {
            thrust_per_power,
            power_per_thrust,
        })?;
    if hull.max_maneuver_actions > 0 && motion < 1 {
        return Err(Error::InsufficientReactor {
            power: engine.power,
        });
    }
    u32_field("space", used)?;
    u32_field("cost", cost)?;
    Ok((design, hull))
}

