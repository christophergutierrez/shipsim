use super::*;

pub fn new_design(id: impl Into<String>) -> Design {
    Design {
        id: id.into(),
        name: "New Ship".into(),
        group: "user".into(),
        size: 2,
        material: "titanium".into(),
        engine: "fission".into(),
        engine_size: "m".into(),
        armored: true,
        shields: [6, 4, 2, 2, 2, 4],
        weapons: vec![
            DesignWeapon {
                component: "beam".into(),
                mount: "forward".into(),
            },
            DesignWeapon {
                component: "torpedo".into(),
                mount: "forward".into(),
            },
        ],
        systems: vec![],
    }
}

pub fn weapon_skus(root: &Path) -> Result<Vec<String>, Error> {
    Ok(load_components(root)?.weapons.keys().cloned().collect())
}

pub fn system_skus(root: &Path) -> Result<Vec<String>, Error> {
    Ok(load_components(root)?.systems.keys().cloned().collect())
}


pub fn list_designs(root: &Path) -> Result<Vec<(PathBuf, Design)>, Error> {
    let dir = root.join("data/designs");
    let mut out = Vec::new();
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(source) => {
            return Err(Error::Read { path: dir, source });
        }
    };
    for entry in entries {
        let entry = entry.map_err(|source| Error::Read {
            path: dir.clone(),
            source,
        })?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        match load_design(&path) {
            Ok(design) => out.push((path, design)),
            Err(_) => continue,
        }
    }
    out.sort_by(|a, b| a.1.id.cmp(&b.1.id));
    Ok(out)
}

pub fn preview_design(root: &Path, design: &Design) -> Result<DesignPreview, Error> {
    let sizes = sizes::load(root)?;
    let hull = sizes
        .get(design.size)
        .map_err(|_| Error::UnknownSize(design.size))?
        .clone();
    let components = load_components(root)?;
    let mat = material(&design.material)?;
    let mut used = 0u64;
    let mut parts = 0u64;
    let engine = plant(&components, &design.engine, &design.engine_size)?;
    checked_count(1, engine.space, engine.cost, &mut used, &mut parts)?;
    if design.armored {
        parts = add(parts, plate_cost(hull.frame_cost, mat))?;
    }
    let (shield_space, shield_cost, faces) = shield_install(&components, design.shields)?;
    used = add(used, shield_space)?;
    parts = add(parts, shield_cost)?;
    let (_, systems_space, systems_cost) = compile_systems(&components, &design.systems)?;
    used = add(used, systems_space)?;
    parts = add(parts, systems_cost)?;
    for weapon in &design.weapons {
        let c = components
            .weapons
            .get(&weapon.component)
            .ok_or_else(|| Error::UnknownComponent(weapon.component.clone()))?;
        if !MOUNTS.contains(&weapon.mount.as_str()) {
            return Err(Error::UnknownMount(weapon.mount.clone()));
        }
        checked_count(1, c.space, c.cost, &mut used, &mut parts)?;
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
    let cost = add(half_up(f64::from(hull.frame_cost) * mat.cost_mult), parts)?;
    let power = engine.power;
    let shields = faces.iter().copied().max().unwrap_or(0);
    let structure = plated_structure(hull.base_structure, mat, design.armored);
    Ok(DesignPreview {
        cost: u32_field("cost", cost)?,
        space_used: u32_field("space", used)?,
        space_cap: hull.space,
        power: u32_field("power", power)?,
        structure: u32_field("structure", structure)?,
        shields,
        shield_faces: faces,
    })
}

pub fn save_design(root: &Path, design: &Design) -> Result<PathBuf, Error> {
    if design.id.is_empty()
        || !design
            .id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err(Error::InvalidId(design.id.clone()));
    }
    if design.name.trim().is_empty() {
        return Err(Error::EmptyName);
    }
    for (_, other) in list_designs(root)? {
        if other.id != design.id && names_collide(&other.name, &design.name) {
            return Err(Error::DuplicateName(design.name.clone()));
        }
    }
    preview_design(root, design)?;
    let dir = root.join("data/designs");
    fs::create_dir_all(&dir).map_err(|source| Error::Read {
        path: dir.clone(),
        source,
    })?;
    let path = dir.join(format!("{}.toml", design.id));
    let body = toml::to_string_pretty(design).map_err(|e| Error::Serialize(e.to_string()))?;
    fs::write(&path, body).map_err(|source| Error::Read {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

pub fn design_cost(root: &Path, path: &Path) -> Result<u32, Error> {
    let (design, hull) = validate(root, path)?;
    let components = load_components(root)?;
    let mut cost = half_up(f64::from(hull.frame_cost) * material(&design.material)?.cost_mult);
    let engine = plant(&components, &design.engine, &design.engine_size)?;
    checked_count(1, 0, engine.cost, &mut 0, &mut cost)?;
    if design.armored {
        cost = add(
            cost,
            plate_cost(hull.frame_cost, material(&design.material)?),
        )?;
    }
    let (_, shield_cost, _) = shield_install(&components, design.shields)?;
    cost = add(cost, shield_cost)?;
    let (_, _, systems_cost) = compile_systems(&components, &design.systems)?;
    cost = add(cost, systems_cost)?;
    for w in design.weapons {
        let c = components
            .weapons
            .get(&w.component)
            .ok_or(Error::UnknownComponent(w.component))?;
        checked_count(1, 0, c.cost, &mut 0, &mut cost)?;
    }
    u32_field("cost", cost)
}
