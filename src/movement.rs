use serde::Deserialize;
use thiserror::Error;

use crate::combat;
use crate::game_state::GameState;
use crate::hex::Hex;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Order {
    Plot { ship: u32, path: Vec<Hex> },
    Fire { weapon: String, target: u32 },
    RunTurn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclaredOrder {
    Plot { ship: u32, path: Vec<Hex> },
    Fire { weapon: String, target: u32 },
    RunTurn,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OrderError {
    #[error("ship {0} was not found")]
    ShipNotFound(u32),
    #[error("destination ({q},{r}) is off the map")]
    OffMap { q: i32, r: i32 },
    #[error("destination ({q},{r}) is occupied")]
    HexOccupied { q: i32, r: i32 },
    #[error("destination ({q},{r}) is not adjacent")]
    NotAdjacent { q: i32, r: i32 },
    #[error("path length {path_len} exceeds speed {speed} for ship {ship} (max {max_steps})")]
    PlotTooLong {
        ship: u32,
        speed: u32,
        path_len: u32,
        max_steps: u32,
    },
    #[error("turn-mode violation for ship {ship} at path step {step_index}")]
    TurnModeViolation { ship: u32, step_index: usize },
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

/// Validate a plot path (adjacency, board, occupancy at submit, turn-mode, length).
pub fn validate_plot(game: &GameState, ship_id: u32, path: &[Hex]) -> Result<(), OrderError> {
    let ship = game
        .ship(ship_id)
        .ok_or(OrderError::ShipNotFound(ship_id))?;
    if ship.destroyed {
        return Err(OrderError::ShipNotFound(ship_id));
    }

    let max_steps = ship.speed;
    if path.len() as u32 > max_steps {
        return Err(OrderError::PlotTooLong {
            ship: ship_id,
            speed: ship.speed,
            path_len: path.len() as u32,
            max_steps,
        });
    }

    let turn_mode = ship.turn_mode;
    let mut prev = ship.pos;
    let mut prev_dir: Option<u8> = None;
    let mut straight: u32 = 0;

    for (step_index, step) in path.iter().enumerate() {
        if !game.board.contains(*step) {
            return Err(OrderError::OffMap {
                q: step.q,
                r: step.r,
            });
        }
        if prev.distance(*step) != 1 {
            return Err(OrderError::NotAdjacent {
                q: step.q,
                r: step.r,
            });
        }
        if game.is_occupied_by_other(ship_id, *step) {
            return Err(OrderError::HexOccupied {
                q: step.q,
                r: step.r,
            });
        }

        let dir = Hex::facing_between(prev, *step).expect("adjacent hex has a unit facing");
        if let Some(previous) = prev_dir {
            if dir != previous {
                if turn_mode > 0 && straight < turn_mode {
                    return Err(OrderError::TurnModeViolation {
                        ship: ship_id,
                        step_index,
                    });
                }
                straight = 1;
                prev_dir = Some(dir);
            } else {
                straight = straight.saturating_add(1);
            }
        } else {
            // First step establishes facing; never a turn-mode violation.
            prev_dir = Some(dir);
            straight = 1;
        }
        prev = *step;
    }

    Ok(())
}

pub fn declare(game: &GameState, order: Order) -> Result<DeclaredOrder, OrderError> {
    match order {
        Order::Plot { ship, path } => {
            validate_plot(game, ship, &path)?;
            Ok(DeclaredOrder::Plot { ship, path })
        }
        Order::Fire { weapon, target } => {
            let attacker_index = game
                .weapon_owner_index(&weapon)
                .ok_or_else(|| OrderError::WeaponNotFound(weapon.clone()))?;
            let target_ship = game
                .ship(target)
                .ok_or(OrderError::TargetNotFound(target))?;
            if target_ship.destroyed {
                return Err(OrderError::TargetNotFound(target));
            }
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
        Order::RunTurn => Ok(DeclaredOrder::RunTurn),
    }
}

pub fn resolve(game: &mut GameState, order: DeclaredOrder) {
    match order {
        DeclaredOrder::Plot { ship, path } => {
            game.store_plot(ship, path);
        }
        DeclaredOrder::Fire { weapon, target } => {
            game.queue_fire(weapon, target);
        }
        DeclaredOrder::RunTurn => {
            game.run_turn();
        }
    }
}
