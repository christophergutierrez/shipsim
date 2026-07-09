use std::collections::HashMap;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::board::Board;
use crate::combat::{Arc, Weapon, WeaponKind};
use crate::game_state::{GameState, ScriptedPlan};
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
}

pub fn load_scenario(path: &Path) -> Result<GameState, LoadError> {
    let text = read_to_string(path)?;
    let def: ScenarioDef = parse_toml(path, &text)?;
    let board = Board::new(def.width, def.height);
    let objective = def
        .objective
        .map(|objective| Hex::new(objective.q, objective.r));
    if let Some(objective) = objective {
        validate_on_board(&board, objective)?;
    }
    let destruction_target = def.terminal.as_ref().and_then(|terminal| {
        if terminal.terminal_type == "destruction" {
            terminal.target
        } else {
            None
        }
    });
    let seed = if def.seed == 0 { 1 } else { def.seed };

    let mut ships = Vec::with_capacity(def.ships.len());
    let mut scripted_plans = HashMap::new();
    for placement in def.ships {
        if placement.facing > 5 {
            return Err(LoadError::InvalidFacing {
                facing: placement.facing,
            });
        }

        let pos = Hex::new(placement.q, placement.r);
        validate_on_board(&board, pos)?;
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

        ships.push(Ship {
            id: placement.id,
            class: ship_def.name,
            pos,
            facing: placement.facing,
            speed: ship_def.speed,
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
        objective,
        destruction_target,
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
    Ok(Weapon {
        id: def.id,
        kind: match def.kind.as_str() {
            "phaser" => WeaponKind::Phaser,
            "disruptor" => WeaponKind::Disruptor,
            _ => WeaponKind::Phaser,
        },
        arc: match def.arc.as_str() {
            "forward" => Arc::Forward,
            "rear" => Arc::Rear,
            "left" => Arc::Left,
            "right" => Arc::Right,
            "all" => Arc::All,
            _ => Arc::Forward,
        },
        max_range: def.max_range,
        damage: def.damage,
        phaser_dice_by_range: def.phaser_dice_by_range,
        to_hit_by_range: def.to_hit_by_range,
    })
}
