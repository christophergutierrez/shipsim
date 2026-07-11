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
    let mode = def
        .map_mode
        .as_deref()
        .map(MapMode::parse)
        .unwrap_or_default();
    let board = Board::new(def.width, def.height).with_mode(mode);

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
    let mut npcs: BTreeMap<u32, NpcController> = BTreeMap::new();
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
        let ctrl = placement.controller.to_ascii_lowercase();
        let is_ai = matches!(ctrl.as_str(), "ai" | "greedy");
        let is_scripted = !is_ai && ctrl == "scripted";

        let power = ship_def.power.unwrap_or(ship_def.speed);
        let weapons: Vec<_> = ship_def
            .weapons
            .into_iter()
            .map(parse_weapon)
            .collect::<Result<Vec<_>, LoadError>>()?;
        let ssd = crate::ssd::Ssd::new(ship_def.structure, ship_def.speed.max(1), 2, weapons.len());
        ships.push(Ship {
            id: placement.id,
            class: ship_def.name,
            pos,
            facing: placement.facing,
            speed: ship_def.speed,
            power,
            weapons,
            shields_powered: [0; 6],
            shields_remaining: [0; 6],
            max_shield_per_facing: ship_def.max_shield_per_facing,
            movement_allocated: 0,
            move_remaining: 0,
            keel: crate::momentum::Keel::Stopped,
            weapon_charges: BTreeMap::new(),
            ssd,
            destroyed: false,
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
