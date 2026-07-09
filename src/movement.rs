use serde::Deserialize;
use thiserror::Error;

use crate::combat::{self, FireIllegal};
use crate::game_state::GameState;
use crate::hex::Hex;
use crate::energy;
use crate::impulse;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Order {
    Plot { ship: u32, path: Vec<Hex> },
    /// Multi-bucket energy allocation for this turn (D7).
    /// `movement` may be provided as JSON alias `speed` for older clients.
    Allocate {
        ship: u32,
        #[serde(alias = "speed")]
        movement: u32,
        #[serde(default)]
        weapons: u32,
        #[serde(default)]
        shields: u32,
    },
    /// `ship` is the firer (TS2 multi-firer: not inferred from weapon id alone).
    Fire { ship: u32, weapon: String, target: u32 },
    RunTurn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclaredOrder {
    Plot { ship: u32, path: Vec<Hex> },
    Allocate {
        ship: u32,
        movement: u32,
        weapons: u32,
        shields: u32,
    },
    Fire { ship: u32, weapon: String, target: u32 },
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
    #[error("weapon {weapon} on ship {ship} has already fired this turn")]
    WeaponAlreadyFired { ship: u32, weapon: String },
    #[error(
        "ship {ship} illegal allocation movement={movement} weapons={weapons} shields={shields} (power {power}, max speed {max_speed})"
    )]
    IllegalAllocation {
        ship: u32,
        movement: u32,
        weapons: u32,
        shields: u32,
        power: u32,
        max_speed: u32,
    },
    #[error("ship {ship} lacks weapon energy to fire (need {need}, have {have})")]
    InsufficientWeaponEnergy { ship: u32, need: u32, have: u32 },
}

/// Validate a plot path (adjacency, board, occupancy at submit, turn-mode, length).
pub fn validate_plot(game: &GameState, ship_id: u32, path: &[Hex]) -> Result<(), OrderError> {
    let ship = game
        .ship(ship_id)
        .ok_or(OrderError::ShipNotFound(ship_id))?;
    if ship.destroyed {
        return Err(OrderError::ShipNotFound(ship_id));
    }

    let max_steps = impulse::max_plot_steps(ship.turn_speed);
    if path.len() as u32 > max_steps {
        return Err(OrderError::PlotTooLong {
            ship: ship_id,
            speed: ship.turn_speed,
            path_len: path.len() as u32,
            max_steps,
        });
    }

    let turn_mode = ship.turn_mode;
    let mut prev = ship.pos;
    let mut prev_dir: Option<u8> = None;
    let mut straight: u32 = 0;

    for (step_index, step) in path.iter().enumerate() {
        if !game.board().contains(*step) {
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
        Order::Allocate {
            ship,
            movement,
            weapons,
            shields,
        } => {
            let s = game.ship(ship).ok_or(OrderError::ShipNotFound(ship))?;
            if s.destroyed {
                return Err(OrderError::ShipNotFound(ship));
            }
            if !energy::is_legal_multi_allocation(s.power, s.speed, movement, weapons, shields)
            {
                return Err(OrderError::IllegalAllocation {
                    ship,
                    movement,
                    weapons,
                    shields,
                    power: s.power,
                    max_speed: s.speed,
                });
            }
            Ok(DeclaredOrder::Allocate {
                ship,
                movement,
                weapons,
                shields,
            })
        }
        Order::Fire {
            ship,
            weapon,
            target,
        } => {
            if game.ship(ship).is_none() {
                return Err(OrderError::ShipNotFound(ship));
            }
            if !game.ship_owns_weapon(ship, &weapon) {
                return Err(OrderError::WeaponNotFound(weapon));
            }
            let target_ship = game
                .ship(target)
                .ok_or(OrderError::TargetNotFound(target))?;
            let attacker = game.ship(ship).expect("checked above");
            if !attacker.can_afford_fire() {
                return Err(OrderError::InsufficientWeaponEnergy {
                    ship,
                    need: energy::fire_energy_cost(),
                    have: attacker.weapons_energy,
                });
            }
            match combat::fire_legality(attacker, &weapon, target_ship) {
                Ok(_) => {}
                Err(FireIllegal::WeaponNotFound) => {
                    return Err(OrderError::WeaponNotFound(weapon));
                }
                Err(FireIllegal::TargetDestroyed) => {
                    return Err(OrderError::TargetNotFound(target));
                }
                Err(FireIllegal::FireAtSelf) => {
                    return Err(OrderError::FireAtSelf(target));
                }
                Err(FireIllegal::OutOfRange { range, max_range }) => {
                    return Err(OrderError::OutOfRange {
                        weapon,
                        range,
                        max_range,
                    });
                }
                Err(FireIllegal::OutOfArc) => {
                    return Err(OrderError::OutOfArc { weapon, target });
                }
            }
            if game.weapon_fired_this_turn(ship, &weapon) {
                return Err(OrderError::WeaponAlreadyFired { ship, weapon });
            }
            Ok(DeclaredOrder::Fire {
                ship,
                weapon,
                target,
            })
        }
        Order::RunTurn => Ok(DeclaredOrder::RunTurn),
    }
}

pub fn resolve(game: &mut GameState, order: DeclaredOrder) {
    match order {
        DeclaredOrder::Plot { ship, path } => {
            game.store_plot(ship, path);
        }
        DeclaredOrder::Allocate {
            ship,
            movement,
            weapons,
            shields,
        } => {
            game.allocate_energy(ship, movement, weapons, shields);
        }
        DeclaredOrder::Fire {
            ship,
            weapon,
            target,
        } => {
            game.queue_fire(ship, weapon, target);
        }
        DeclaredOrder::RunTurn => {
            crate::turn::run_turn(game);
        }
    }
}

/// Apply a wire order through declare/resolve (orchestration entrypoint).
pub fn apply_order(game: &mut GameState, order: Order) -> Result<(), OrderError> {
    let declared = declare(game, order)?;
    resolve(game, declared);
    Ok(())
}
