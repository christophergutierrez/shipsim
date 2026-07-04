use serde::Deserialize;
use thiserror::Error;

use crate::combat;
use crate::game_state::GameState;
use crate::hex::Hex;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Order {
    Move { ship: u32, to: Hex },
    Face { ship: u32, facing: u8 },
    Fire { weapon: String, target: u32 },
    EndTurn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclaredOrder {
    Move { ship: u32, to: Hex },
    Face { ship: u32, facing: u8 },
    Fire { weapon: String, target: u32 },
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
    #[error("weapon {0} was not found")]
    WeaponNotFound(String),
    #[error("target {0} was not found")]
    TargetNotFound(u32),
    #[error("ship {0} cannot fire at itself")]
    FireAtSelf(u32),
    #[error("weapon {weapon} is out of range {range} > {max_range}")]
    OutOfRange {
        weapon: String,
        range: u32,
        max_range: u32,
    },
    #[error("weapon {weapon} cannot bear on target {target}")]
    OutOfArc { weapon: String, target: u32 },
    #[error("weapon {0} has already fired this turn")]
    WeaponAlreadyFired(String),
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
        Order::Fire { weapon, target } => {
            let attacker_index = game
                .weapon_owner_index(&weapon)
                .ok_or_else(|| OrderError::WeaponNotFound(weapon.clone()))?;
            let target_ship = game
                .ship(target)
                .ok_or(OrderError::TargetNotFound(target))?;
            let attacker = &game.ships[attacker_index];
            if attacker.id == target {
                return Err(OrderError::FireAtSelf(target));
            }
            let weapon_def = attacker
                .weapons
                .iter()
                .find(|weapon_def| weapon_def.id == weapon)
                .expect("owner lookup proved weapon exists");
            let range = attacker.pos.distance(target_ship.pos);
            if range > weapon_def.max_range {
                return Err(OrderError::OutOfRange {
                    weapon,
                    range,
                    max_range: weapon_def.max_range,
                });
            }
            let relative_bearing =
                combat::relative_bearing(attacker.facing, attacker.pos, target_ship.pos);
            if !combat::arc_contains(&weapon_def.arc, relative_bearing) {
                return Err(OrderError::OutOfArc { weapon, target });
            }
            if game.weapon_fired_this_turn(&weapon) {
                return Err(OrderError::WeaponAlreadyFired(weapon));
            }
            Ok(DeclaredOrder::Fire { weapon, target })
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
        DeclaredOrder::Fire { weapon, target } => {
            crate::combat::resolve_fire(game, &weapon, target);
            game.record_weapon_fired(weapon);
            game.refresh_status();
        }
        DeclaredOrder::EndTurn => game.end_turn(),
    }
}
