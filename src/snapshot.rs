use serde::Serialize;

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
    pub speed_max: u32,
    pub turn_mode: u32,
}

#[derive(Debug, Serialize)]
pub struct StateSnapshot {
    pub turn: u32,
    pub status: ScenarioStatus,
    pub map: MapSnapshot,
    pub objective: HexSnapshot,
    pub ships: Vec<ShipSnapshot>,
}

impl StateSnapshot {
    pub fn from_game_state(game: &GameState) -> Self {
        Self {
            turn: game.turn.number(),
            status: game.status,
            map: MapSnapshot {
                width: game.board.width,
                height: game.board.height,
            },
            objective: HexSnapshot {
                q: game.objective.q,
                r: game.objective.r,
            },
            ships: game
                .ships
                .iter()
                .map(|ship| ShipSnapshot {
                    id: ship.id,
                    class: ship.class.clone(),
                    q: ship.pos.q,
                    r: ship.pos.r,
                    facing: ship.facing,
                    speed_max: ship.speed_max,
                    turn_mode: ship.turn_mode,
                })
                .collect(),
        }
    }
}
