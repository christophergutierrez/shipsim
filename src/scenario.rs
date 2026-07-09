use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::board::Board;
use crate::combat::{Arc, Weapon, WeaponKind};
use crate::game_state::{GameState, ScriptedPlan, Terminal};
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
    #[error("ships {a} and {b} both placed on hex ({q},{r})")]
    OverlappingPlacement { a: u32, b: u32, q: i32, r: i32 },
    #[error("scenario defines both objective and destruction terminal")]
    ConflictingTerminals,
    #[error("destruction terminal missing target")]
    DestructionTargetMissing,
    #[error("unknown weapon kind {kind:?} on weapon {weapon_id}")]
    UnknownWeaponKind { kind: String, weapon_id: String },
    #[error("unknown weapon arc {arc:?} on weapon {weapon_id}")]
    UnknownWeaponArc { arc: String, weapon_id: String },
}

pub fn load_scenario(path: &Path) -> Result<GameState, LoadError> {
    let text = read_to_string(path)?;
    let def: ScenarioDef = parse_toml(path, &text)?;
    let board = Board::new(def.width, def.height);

    let has_objective = def.objective.is_some();
    let has_destruction = def
        .terminal
        .as_ref()
        .is_some_and(|t| t.terminal_type == "destruction");
    if has_objective && has_destruction {
        return Err(LoadError::ConflictingTerminals);
    }

    let terminal = if let Some(obj) = def.objective {
        let hex = Hex::new(obj.q, obj.r);
        validate_on_board(&board, hex)?;
        Some(Terminal::ReachHex(hex))
    } else if let Some(term) = def.terminal {
        if term.terminal_type == "destruction" {
            let target = term.target.ok_or(LoadError::DestructionTargetMissing)?;
            Some(Terminal::DestroyShip(target))
        } else {
            None
        }
    } else {
        None
    };

    let seed = if def.seed == 0 { 1 } else { def.seed };

    let mut ships = Vec::with_capacity(def.ships.len());
    let mut scripted_plans = BTreeMap::new();
    let mut occupied: BTreeMap<(i32, i32), u32> = BTreeMap::new();

    for placement in def.ships {
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

        let ship_def = load_ship_def(path, &placement.class)?;
        let waypoints = placement
            .waypoints
            .into_iter()
            .map(|waypoint| {
                let hex = Hex::new(waypoint.q, waypoint.r);
                validate_on_board(&board, hex)?;
                Ok(hex)
            })
            .collect::<Result<Vec<_>, LoadError>>()?;
        let is_scripted = placement.controller == "scripted" || !waypoints.is_empty();

        let power = ship_def.power.unwrap_or(ship_def.speed);
        let (turn_speed, weapons_energy, shield_reinforce) =
            crate::energy::default_buckets(power, ship_def.speed);
        ships.push(Ship {
            id: placement.id,
            class: ship_def.name,
            pos,
            facing: placement.facing,
            speed: ship_def.speed,
            power,
            turn_speed,
            weapons_energy,
            shield_reinforce,
            turn_mode: ship_def.turn_mode,
            weapons: ship_def
                .weapons
                .into_iter()
                .map(parse_weapon)
                .collect::<Result<Vec<_>, LoadError>>()?,
            shields: ship_def.shields,
            structure: ship_def.structure,
            destroyed: false,
        });
        if is_scripted {
            scripted_plans.insert(placement.id, ScriptedPlan::new(waypoints));
        }
    }

    Ok(GameState::new_with_options(
        board,
        ships,
        terminal,
        scripted_plans,
        seed,
    ))
}

fn load_ship_def(scenario_path: &Path, class: &str) -> Result<ShipDef, LoadError> {
    let root = scenario_path
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."));
    let ship_path = root
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
    if board.contains(hex) {
        Ok(())
    } else {
        Err(LoadError::OffBoard { q: hex.q, r: hex.r })
    }
}

fn parse_weapon(def: WeaponDef) -> Result<Weapon, LoadError> {
    let kind = match def.kind.as_str() {
        "phaser" => WeaponKind::Phaser,
        "disruptor" => WeaponKind::Disruptor,
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
    Ok(Weapon {
        id: def.id,
        kind,
        arc,
        max_range: def.max_range,
        damage: def.damage,
        phaser_dice_by_range: def.phaser_dice_by_range,
        to_hit_by_range: def.to_hit_by_range,
    })
}
