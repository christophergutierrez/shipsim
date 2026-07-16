use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::arc::Mount;
use crate::board::{Board, MapMode};
use crate::combat::{Arc, Weapon};
use crate::combat_tables::WeaponKind;
use crate::game_state::{GameState, NpcController, Terminal};
use crate::hex::Hex;
use crate::schema::{ScenarioDef, ShipDef, WeaponDef};
use crate::ship::Ship;

#[derive(Debug, Error)]
pub enum LoadError {
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
    #[error("ship class {class:?} is missing at {path:?}")]
    MissingShipClass { class: String, path: PathBuf },
    #[error("hex ({q},{r}) is outside the scenario map")]
    OffBoard { q: i32, r: i32 },
    #[error("facing {facing} is not in 0..=5")]
    InvalidFacing { facing: u8 },
    #[error("ship class {class:?} has size 0; size must be at least 1")]
    InvalidShipSize { class: String },
    #[error("ships {a} and {b} both placed on hex ({q},{r})")]
    OverlappingPlacement { a: u32, b: u32, q: i32, r: i32 },
    #[error("scenario defines conflicting terminals (objective / destruction / annihilation)")]
    ConflictingTerminals,
    #[error("destruction terminal missing target")]
    DestructionTargetMissing,
    #[error("unknown terminal type {0:?}")]
    UnknownTerminal(String),
    #[error("unknown weapon kind {kind:?} on weapon {weapon_id}")]
    UnknownWeaponKind { kind: String, weapon_id: String },
    #[error("unknown weapon arc {arc:?} on weapon {weapon_id}")]
    UnknownWeaponArc { arc: String, weapon_id: String },
    #[error("ship class {class:?} has invalid thrust conversion: {source}")]
    InvalidThrustConversion {
        class: String,
        source: crate::thrust::ConversionError,
    },
    #[error(
        "ship class {class:?} max_velocity {max_velocity} exceeds global maximum {global_max}"
    )]
    MaxVelocityExceedsGlobal {
        class: String,
        max_velocity: u8,
        global_max: u8,
    },
    #[error("ship class {class:?} cannot buy one thrust with its design power {power} (conversion {thrust_per_power}:{power_per_thrust})")]
    MobileHullCannotBuyThrust {
        class: String,
        power: u32,
        thrust_per_power: u32,
        power_per_thrust: u32,
    },
    #[error("ship {ship_id} initial velocity {velocity} exceeds max_velocity {max_velocity}")]
    InitialVelocityExceedsMax {
        ship_id: u32,
        velocity: u8,
        max_velocity: u8,
    },
    #[error("ship {ship_id} initial course {course} is not a valid hex direction (0..=5)")]
    InvalidInitialCourse { ship_id: u32, course: u8 },
}

pub fn load_scenario(path: &Path) -> Result<GameState, LoadError> {
    let text = read_to_string(path)?;
    let def: ScenarioDef = parse_toml(path, &text)?;
    let data_root = path
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."));
    load_scenario_def(&def, data_root)
}

/// Load a scenario from an in-memory definition.
/// `data_root` is the repository root containing `data/ships/`.
pub fn load_scenario_def(def: &ScenarioDef, data_root: &Path) -> Result<GameState, LoadError> {
    let mode = def
        .map_mode
        .as_deref()
        .map(MapMode::parse)
        .unwrap_or_default();
    let board = Board::new(def.width, def.height).with_mode(mode);

    let has_objective = def.objective.is_some();
    let terminal_kind = def.terminal.as_ref().map(|t| t.terminal_type.as_str());
    let has_destruction = terminal_kind == Some("destruction");
    let has_annihilation = terminal_kind == Some("annihilation");
    let terminal_count = usize::from(has_objective)
        + usize::from(has_destruction)
        + usize::from(has_annihilation);
    if terminal_count > 1 {
        return Err(LoadError::ConflictingTerminals);
    }

    let terminal = if let Some(obj) = def.objective.as_ref() {
        let hex = Hex::new(obj.q, obj.r);
        validate_on_board(&board, hex)?;
        Some(Terminal::ReachHex(hex))
    } else if let Some(term) = def.terminal.as_ref() {
        match term.terminal_type.as_str() {
            "destruction" => {
                let target = term.target.ok_or(LoadError::DestructionTargetMissing)?;
                Some(Terminal::DestroyShip(target))
            }
            "annihilation" => Some(Terminal::AnnihilateEnemies),
            other => return Err(LoadError::UnknownTerminal(other.to_string())),
        }
    } else {
        None
    };

    let seed = if def.seed == 0 { 1 } else { def.seed };

    let mut ships = Vec::with_capacity(def.ships.len());
    let mut npcs: BTreeMap<u32, NpcController> = BTreeMap::new();
    let mut occupied: BTreeMap<(i32, i32), u32> = BTreeMap::new();

    for placement in &def.ships {
        if !Hex::is_valid_facing(placement.facing) {
            return Err(LoadError::InvalidFacing {
                facing: placement.facing,
            });
        }

        let pos = Hex::new(placement.q, placement.r);
        validate_on_board(&board, pos)?;
        if let Some(other) = occupied.insert((pos.q, pos.r), placement.id) {
            return Err(LoadError::OverlappingPlacement {
                a: other,
                b: placement.id,
                q: pos.q,
                r: pos.r,
            });
        }

        let ship_def = load_ship_def(data_root, &placement.class)?;
        if ship_def.size == 0 {
            return Err(LoadError::InvalidShipSize {
                class: placement.class.clone(),
            });
        }
        let ctrl = placement.controller.to_ascii_lowercase();
        let is_ai = matches!(ctrl.as_str(), "ai" | "greedy");
        let is_scripted = !is_ai && ctrl == "scripted";

        let power = placement
            .power
            .or(ship_def.power)
            .unwrap_or(ship_def.speed);
        let structure = placement.structure.unwrap_or(ship_def.structure);
        let max_shield_per_facing = placement
            .max_shield_per_facing
            .unwrap_or(ship_def.max_shield_per_facing);
        let weapons: Vec<_> = ship_def
            .weapons
            .into_iter()
            .map(parse_weapon)
            .collect::<Result<Vec<_>, LoadError>>()?;
        // Subsystem box counts are frame properties (docs/BALANCE-COST.md). Legacy
        // ship TOMLs omit them → engine = speed, power_sys = 2.
        let engine_boxes = ship_def
            .engine_boxes
            .unwrap_or_else(|| ship_def.speed.max(1));
        let power_sys_boxes = ship_def.power_sys.unwrap_or(2).max(1);
        let ssd = crate::ssd::Ssd::with_weapon_boxes(
            structure,
            engine_boxes,
            power_sys_boxes,
            weapons.len(),
            ship_def.weapon_boxes,
        );

        // Inertial movement: resolve the hull's design maximum velocity
        // (ADR-0022 §1). An explicit `max_velocity` overrides the legacy `speed`
        // derivation; when omitted, `max_velocity` is derived from `speed` so a
        // legacy speed-1 hull becomes max velocity 1, etc.
        let max_velocity = ship_def
            .max_velocity
            .unwrap_or_else(|| ship_def.speed.try_into().unwrap_or(u8::MAX));
        if max_velocity > crate::motion::MAX_VELOCITY {
            return Err(LoadError::MaxVelocityExceedsGlobal {
                class: placement.class.clone(),
                max_velocity,
                global_max: crate::motion::MAX_VELOCITY,
            });
        }

        let thrust_conversion = crate::thrust::ThrustConversion::new(
            ship_def.thrust_per_power,
            ship_def.power_per_thrust,
            max_velocity,
        )
        .map_err(|source| LoadError::InvalidThrustConversion {
            class: placement.class.clone(),
            source,
        })?;

        if max_velocity > 0 {
            let (thrust_at_full_power, _remainder) = thrust_conversion.convert(power);
            if thrust_at_full_power < 1 {
                return Err(LoadError::MobileHullCannotBuyThrust {
                    class: placement.class.clone(),
                    power,
                    thrust_per_power: ship_def.thrust_per_power,
                    power_per_thrust: ship_def.power_per_thrust,
                });
            }
        }

        let init_speed = placement.velocity.unwrap_or(0);
        if init_speed > max_velocity {
            return Err(LoadError::InitialVelocityExceedsMax {
                ship_id: placement.id,
                velocity: init_speed,
                max_velocity,
            });
        }
        let init_course = placement.course.unwrap_or(placement.facing);
        if init_course > 5 {
            return Err(LoadError::InvalidInitialCourse {
                ship_id: placement.id,
                course: init_course,
            });
        }
        let velocity = crate::motion::Velocity::new(init_speed, init_course).map_err(|_| {
            LoadError::InvalidInitialCourse {
                ship_id: placement.id,
                course: init_course,
            }
        })?;

        ships.push(Ship {
            id: placement.id,
            class: ship_def.name,
            size: ship_def.size,
            pos,
            facing: placement.facing,
            speed: ship_def.speed,
            power,
            attack_accuracy_bonus: ship_def.attack_accuracy_bonus,
            weapons,
            shields_powered: [0; 6],
            shields_remaining: [0; 6],
            max_shield_per_facing,
            movement_allocated: 0,
            weapon_charges: BTreeMap::new(),
            ssd,
            destroyed: false,
            max_velocity,
            thrust_conversion,
            velocity,
            thrust_remaining: 0,
        });
        if is_ai {
            npcs.insert(placement.id, NpcController::GreedySeek);
        } else if is_scripted {
            npcs.insert(placement.id, NpcController::Scripted);
        }
    }

    Ok(GameState::new_with_options(
        board, ships, terminal, npcs, seed,
    ))
}

/// Load a ship class TOML from `{data_root}/data/ships/{class}.toml`.
pub fn load_ship_def(data_root: &Path, class: &str) -> Result<ShipDef, LoadError> {
    let ship_path = data_root
        .join("data")
        .join("ships")
        .join(format!("{class}.toml"));
    if !ship_path.exists() {
        return Err(LoadError::MissingShipClass {
            class: class.to_string(),
            path: ship_path,
        });
    }

    let text = read_to_string(&ship_path)?;
    let ship_def: ShipDef = parse_toml(&ship_path, &text)?;
    Ok(ship_def)
}

fn read_to_string(path: &Path) -> Result<String, LoadError> {
    std::fs::read_to_string(path).map_err(|source| LoadError::Read {
        path: path.to_path_buf(),
        source,
    })
}

fn parse_toml<T: serde::de::DeserializeOwned>(path: &Path, text: &str) -> Result<T, LoadError> {
    toml::from_str(text).map_err(|source| LoadError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

fn validate_on_board(board: &Board, hex: Hex) -> Result<(), LoadError> {
    if board.mode == MapMode::Unbounded || board.contains(hex) {
        Ok(())
    } else {
        Err(LoadError::OffBoard { q: hex.q, r: hex.r })
    }
}

fn parse_weapon(def: WeaponDef) -> Result<Weapon, LoadError> {
    let kind = match def.kind.as_str() {
        "plasma" => WeaponKind::Plasma,
        "beam" => WeaponKind::Beam,
        "torp" => WeaponKind::Torp,
        other => {
            return Err(LoadError::UnknownWeaponKind {
                kind: other.to_string(),
                weapon_id: def.id.clone(),
            });
        }
    };
    let arc = match def.arc.as_str() {
        "forward" => Arc::Forward,
        "rear" => Arc::Rear,
        "left" => Arc::Left,
        "right" => Arc::Right,
        "all" => Arc::All,
        other => {
            return Err(LoadError::UnknownWeaponArc {
                arc: other.to_string(),
                weapon_id: def.id.clone(),
            });
        }
    };
    let mount = match def.mount.as_deref().unwrap_or(def.arc.as_str()) {
        "forward" => Some(Mount::Forward),
        "forward_starboard" => Some(Mount::ForwardStarboard),
        "aft_starboard" => Some(Mount::AftStarboard),
        "aft" | "rear" => Some(Mount::Aft),
        "aft_port" => Some(Mount::AftPort),
        "forward_port" => Some(Mount::ForwardPort),
        "left" | "right" | "all" => None,
        other => {
            return Err(LoadError::UnknownWeaponArc {
                arc: other.to_string(),
                weapon_id: def.id.clone(),
            });
        }
    };
    Ok(Weapon {
        id: def.id,
        kind,
        arc,
        mount,
        max_range: def.max_range,
        max_charge: def.max_charge,
    })
}
