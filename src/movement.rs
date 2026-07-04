use serde::Deserialize;
use thiserror::Error;

use crate::game_state::GameState;
use crate::hex::Hex;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Order {
    Move { ship: u32, to: Hex },
    Face { ship: u32, facing: u8 },
    EndTurn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclaredOrder {
    Move { ship: u32, to: Hex },
    Face { ship: u32, facing: u8 },
    EndTurn,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OrderError {
    #[error("ship {0} was not found")]
    ShipNotFound(u32),
    #[error("destination ({q},{r}) is off the map")]
    OffMap { q: i32, r: i32 },
    #[error("moving ship {ship} would exceed speed {speed_max} with attempted hex {attempted}")]
    BeyondSpeed {
        ship: u32,
        speed_max: u32,
        attempted: u32,
    },
    #[error("destination ({q},{r}) is occupied")]
    HexOccupied { q: i32, r: i32 },
    #[error("destination ({q},{r}) is not adjacent")]
    NotAdjacent { q: i32, r: i32 },
    #[error("facing {0} is not in 0..=5")]
    NotSixFacing(u8),
}

pub fn declare(game: &GameState, order: Order) -> Result<DeclaredOrder, OrderError> {
    match order {
        Order::Move { ship, to } => {
            let moving_ship = game.ship(ship).ok_or(OrderError::ShipNotFound(ship))?;
            let current = moving_ship.pos;
            if !game.board.contains(to) {
                return Err(OrderError::OffMap { q: to.q, r: to.r });
            }
            if game.is_occupied_by_other(ship, to) {
                return Err(OrderError::HexOccupied { q: to.q, r: to.r });
            }
            if current.distance(to) != 1 {
                return Err(OrderError::NotAdjacent { q: to.q, r: to.r });
            }
            let attempted = game.hexes_moved_this_turn(ship) + 1;
            if attempted > moving_ship.speed_max {
                return Err(OrderError::BeyondSpeed {
                    ship,
                    speed_max: moving_ship.speed_max,
                    attempted,
                });
            }
            Ok(DeclaredOrder::Move { ship, to })
        }
        Order::Face { ship, facing } => {
            if game.ship(ship).is_none() {
                return Err(OrderError::ShipNotFound(ship));
            }
            if facing > 5 {
                return Err(OrderError::NotSixFacing(facing));
            }
            Ok(DeclaredOrder::Face { ship, facing })
        }
        Order::EndTurn => Ok(DeclaredOrder::EndTurn),
    }
}

pub fn resolve(game: &mut GameState, order: DeclaredOrder) {
    match order {
        DeclaredOrder::Move { ship, to } => {
            if let Some(ship) = game.ship_mut(ship) {
                ship.pos = to;
            }
            game.record_hex_moved(ship);
            game.refresh_status();
        }
        DeclaredOrder::Face { ship, facing } => {
            if let Some(ship) = game.ship_mut(ship) {
                ship.facing = facing;
            }
        }
        DeclaredOrder::EndTurn => game.end_turn(),
    }
}
