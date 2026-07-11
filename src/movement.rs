//! Orders for Combat v2 play (ADR-0019).
//! Allocate → Move → Fire → Ready → EndTurn phase machine.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

use crate::game_state::{GameState, Phase};
use crate::hex::Hex;
use crate::momentum;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MoveMode {
    Forward,
    Reverse,
    TurnPort,
    TurnStarboard,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Order {
    /// Combat v2: allocate movement, weapon charge, and shield power for one ship.
    Allocate {
        ship: u32,
        movement: u32,
        weapons: BTreeMap<String, u32>,
        shields: [u32; 6],
    },
    /// Spend power to move or turn (basic move).
    Move {
        ship: u32,
        mode: MoveMode,
    },
    /// Combat v2: pass this ship's movement decision for the current movement phase.
    PassMove {
        ship: u32,
    },
    CommitFire {
        ship: u32,
        weapon: String,
        target: u32,
        shield_facing: u8,
    },
    ReadyFire {
        ship: u32,
    },
    /// Combat v2: end the current turn and advance to the next turn's allocation.
    /// Legal in any phase after allocation; always advances (the UI owns any warning).
    EndTurn,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OrderError {
    #[error("ship {0} was not found")]
    ShipNotFound(u32),
    #[error("ship {ship} is not the active ship (active={active:?})")]
    NotActiveShip { ship: u32, active: Option<u32> },
    #[error("destination ({q},{r}) is off the map")]
    OffMap { q: i32, r: i32 },
    #[error("destination ({q},{r}) is occupied")]
    HexOccupied { q: i32, r: i32 },
    #[error("ship {ship} lacks power (need {need}, have {have})")]
    InsufficientPower { ship: u32, need: u32, have: u32 },
    #[error("ship {ship} lacks move power (need {need}, have {have})")]
    InsufficientMovePower { ship: u32, need: u32, have: u32 },
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
    #[error("weapon {weapon} on ship {ship} has already fired this turn")]
    WeaponAlreadyFired { ship: u32, weapon: String },
    #[error("weapon {weapon} on ship {ship} has already been committed this phase")]
    WeaponAlreadyCommitted { ship: u32, weapon: String },
    #[error("weapon {weapon} on ship {ship} is not charged")]
    WeaponNotCharged { ship: u32, weapon: String },
    #[error("ship {0} is already ready to fire")]
    FireAlreadyReady(u32),
    #[error("weapon {weapon} would deal no damage at range {range} with charge {charge}")]
    NoDamage {
        weapon: String,
        range: u32,
        charge: u32,
    },
    #[error("shield facing {requested} is not legal; legal facings: {legal:?}")]
    IllegalShieldFacing { requested: u8, legal: Vec<u8> },
    #[error("ship {0} has already allocated power this turn")]
    AlreadyAllocated(u32),
    #[error("cannot end the turn during allocation")]
    EndTurnDuringAllocation,
    #[error("ship {0} has already moved or passed this movement phase")]
    AlreadyMovedThisPhase(u32),
    #[error("order requires phase {expected}, actual phase is {actual}")]
    WrongPhase {
        expected: &'static str,
        actual: &'static str,
    },
    #[error("ship {ship} allocated {total} power, only {available} available")]
    OverAllocated {
        ship: u32,
        total: u32,
        available: u32,
    },
    #[error("weapon {weapon} on ship {ship} charged {charge}, max {max}")]
    WeaponChargeTooHigh {
        ship: u32,
        weapon: String,
        charge: u32,
        max: u32,
    },
    #[error("ship {ship} shield facing {facing} has {power}, max {max}")]
    ShieldPowerTooHigh {
        ship: u32,
        facing: u8,
        power: u32,
        max: u32,
    },
    #[error("invalid move")]
    InvalidMove,
}

pub fn apply_order(game: &mut GameState, order: Order) -> Result<(), OrderError> {
    match order {
        Order::Allocate {
            ship,
            movement,
            weapons,
            shields,
        } => game.allocate_v2(ship, movement, weapons, shields),
        Order::Move { ship, mode } => apply_v2_move(game, ship, mode),
        Order::PassMove { ship } => apply_v2_pass_move(game, ship),
        Order::CommitFire {
            ship,
            weapon,
            target,
            shield_facing,
        } => game.commit_fire_v2(crate::game_state::FireCommit {
            ship,
            weapon,
            target,
            shield_facing,
        }),
        Order::ReadyFire { ship } => game.ready_fire_v2(ship),
        Order::EndTurn => game.end_turn_v2(),
    }
}

fn require_v2_active_mover(game: &GameState, ship: u32) -> Result<(), OrderError> {
    if game.phase() != Phase::Movement {
        return Err(OrderError::WrongPhase {
            expected: "movement",
            actual: game.phase_name(),
        });
    }
    if game.ship(ship).is_none_or(|s| s.destroyed) {
        return Err(OrderError::ShipNotFound(ship));
    }
    if game.has_moved_this_phase(ship) {
        return Err(OrderError::AlreadyMovedThisPhase(ship));
    }
    let active = game.active_v2_mover();
    if active != Some(ship) {
        return Err(OrderError::NotActiveShip { ship, active });
    }
    Ok(())
}

fn apply_v2_pass_move(game: &mut GameState, ship: u32) -> Result<(), OrderError> {
    require_v2_active_mover(game, ship)?;
    game.mark_v2_move_decision(ship);
    Ok(())
}

fn apply_v2_move(game: &mut GameState, ship_id: u32, mode: MoveMode) -> Result<(), OrderError> {
    require_v2_active_mover(game, ship_id)?;

    let (pos, facing, keel) = {
        let s = game
            .ship(ship_id)
            .ok_or(OrderError::ShipNotFound(ship_id))?;
        (s.pos, s.facing, s.keel)
    };
    let v2_mode = match mode {
        MoveMode::Forward => momentum::MoveMode::Forward,
        MoveMode::Reverse => momentum::MoveMode::Reverse,
        MoveMode::TurnPort => momentum::MoveMode::TurnPort,
        MoveMode::TurnStarboard => momentum::MoveMode::TurnStarboard,
    };
    let (cost, next_keel) = momentum::move_cost(keel, v2_mode);
    let have = game.ship(ship_id).map(|s| s.move_remaining).unwrap_or(0);
    if have < cost {
        return Err(OrderError::InsufficientMovePower {
            ship: ship_id,
            need: cost,
            have,
        });
    }

    match mode {
        MoveMode::TurnPort => {
            let nf = (facing + 5) % 6;
            game.set_ship_facing(ship_id, nf)
                .map_err(|_| OrderError::ShipNotFound(ship_id))?;
        }
        MoveMode::TurnStarboard => {
            let nf = (facing + 1) % 6;
            game.set_ship_facing(ship_id, nf)
                .map_err(|_| OrderError::ShipNotFound(ship_id))?;
        }
        MoveMode::Forward | MoveMode::Reverse => {
            let direction = if mode == MoveMode::Forward {
                facing
            } else {
                (facing + 3) % 6
            };
            let next = Hex::direction(direction)
                .map(|d| pos + d)
                .ok_or(OrderError::InvalidMove)?;
            if game.board().mode == crate::board::MapMode::Hard && !game.board().contains(next) {
                return Err(OrderError::OffMap {
                    q: next.q,
                    r: next.r,
                });
            }
            if game.is_occupied_by_other(ship_id, next) {
                return Err(OrderError::HexOccupied {
                    q: next.q,
                    r: next.r,
                });
            }
            game.set_ship_pos(ship_id, next)
                .map_err(|_| OrderError::ShipNotFound(ship_id))?;
        }
    }

    game.spend_v2_move_power(ship_id, cost)?;
    game.set_v2_keel(ship_id, next_keel)
        .map_err(|_| OrderError::ShipNotFound(ship_id))?;
    game.mark_v2_move_decision(ship_id);
    game.maybe_float_recenter();
    Ok(())
}
