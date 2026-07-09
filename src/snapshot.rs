use serde::Serialize;

use crate::combat::{Arc, WeaponKind};
use crate::game_state::{GameState, ScenarioStatus};

#[derive(Debug, Serialize)]
pub struct MapSnapshot {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Serialize)]
pub struct HexSnapshot {
    pub q: i32,
    pub r: i32,
}

#[derive(Debug, Serialize)]
pub struct ShipSnapshot {
    pub id: u32,
    pub class: String,
    pub q: i32,
    pub r: i32,
    pub facing: u8,
    pub speed: u32,
    pub turn_mode: u32,
    pub shields: [u32; 6],
    pub structure: u32,
    pub destroyed: bool,
    pub weapons: Vec<WeaponSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct WeaponSnapshot {
    pub id: String,
    pub kind: String,
    pub arc: String,
    pub max_range: u32,
}

#[derive(Debug, Serialize)]
pub struct StateSnapshot {
    pub turn: u32,
    pub impulse: u8,
    pub status: ScenarioStatus,
    pub seed: u64,
    /// PRNG stream position for mid-game resume (TS3).
    pub prng_state: u64,
    pub map: MapSnapshot,
    pub objective: Option<HexSnapshot>,
    pub ships: Vec<ShipSnapshot>,
}

impl StateSnapshot {
    pub fn from_game_state(game: &GameState) -> Self {
        Self {
            turn: game.turn_number(),
            impulse: game.impulse(),
            status: game.status(),
            seed: game.seed(),
            prng_state: game.prng_state(),
            map: MapSnapshot {
                width: game.board().width,
                height: game.board().height,
            },
            objective: game.objective().map(|objective| HexSnapshot {
                q: objective.q,
                r: objective.r,
            }),
            ships: game
                .ships()
                .iter()
                .map(|ship| ShipSnapshot {
                    id: ship.id,
                    class: ship.class.clone(),
                    q: ship.pos.q,
                    r: ship.pos.r,
                    facing: ship.facing,
                    speed: ship.speed,
                    turn_mode: ship.turn_mode,
                    shields: ship.shields,
                    structure: ship.structure,
                    destroyed: ship.destroyed,
                    weapons: ship
                        .weapons
                        .iter()
                        .map(|weapon| WeaponSnapshot {
                            id: weapon.id.clone(),
                            kind: weapon_kind_name(&weapon.kind).to_string(),
                            arc: arc_name(&weapon.arc).to_string(),
                            max_range: weapon.max_range,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

fn weapon_kind_name(kind: &WeaponKind) -> &'static str {
    match kind {
        WeaponKind::Phaser => "Phaser",
        WeaponKind::Disruptor => "Disruptor",
    }
}

fn arc_name(arc: &Arc) -> &'static str {
    match arc {
        Arc::Forward => "Forward",
        Arc::Rear => "Rear",
        Arc::Left => "Left",
        Arc::Right => "Right",
        Arc::All => "All",
    }
}
