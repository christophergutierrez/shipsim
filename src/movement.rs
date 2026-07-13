//! Orders for Combat v2 play (ADR-0019).
//! Allocate → (Maneuver × 4) → Fire → Ready → EndTurn phase machine (ADR-0022 M4).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

use crate::game_state::GameState;
use crate::motion::Maneuver;

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
    /// Retired in M4 (ADR-0022): the single-active-mover legacy movement model is gone.
    /// Kept in the enum only so old protocol-v1 payloads still deserialize; always rejected
    /// by `apply_order`. `mode` is untyped since the legacy `MoveMode` enum is deleted.
    Move {
        ship: u32,
        mode: String,
    },
    /// Commit one maneuver (or `Maneuver::Coast`) for `ship` during the current movement
    /// phase (ADR-0022 M4). Resolution is deferred until every living ship has committed.
    CommitManeuver {
        ship: u32,
        maneuver: Maneuver,
    },
    /// Retired in M6 (ADR-0022): the external contract now uses maneuver commitment
    /// semantics (`CommitManeuver` with `Maneuver::Coast`). Kept in the enum only so old
    /// protocol-v1 payloads still deserialize; always rejected by `apply_order`.
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
    #[error("ship {ship} lacks power (need {need}, have {have})")]
    InsufficientPower { ship: u32, need: u32, have: u32 },
    #[error("ship {ship} lacks thrust (need {need}, have {have})")]
    InsufficientThrust { ship: u32, need: u32, have: u32 },
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
    #[error("ship {0} has already committed a maneuver this movement phase")]
    AlreadyCommittedThisPhase(u32),
    #[error("ship {ship} cannot perform this maneuver: {reason}")]
    IllegalManeuver { ship: u32, reason: String },
    #[error("the Move order was removed in M4 (ADR-0022); submit CommitManeuver instead")]
    MoveOrderRetired,
    #[error(
        "the PassMove order was retired in M6 (ADR-0022); submit CommitManeuver with maneuver \"coast\" instead"
    )]
    PassMoveOrderRetired,
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
    #[error(
        "weapon {weapon} on ship {ship} already has charge {have}; cannot strip to {want} (charge carries across turns)"
    )]
    CannotStripWeaponCharge {
        ship: u32,
        weapon: String,
        have: u32,
        want: u32,
    },
}

pub fn apply_order(game: &mut GameState, order: Order) -> Result<(), OrderError> {
    match order {
        Order::Allocate {
            ship,
            movement,
            weapons,
            shields,
        } => game.allocate_v2(ship, movement, weapons, shields),
        Order::Move { .. } => Err(OrderError::MoveOrderRetired),
        Order::CommitManeuver { ship, maneuver } => game.commit_maneuver_v2(ship, maneuver),
        Order::PassMove { .. } => Err(OrderError::PassMoveOrderRetired),
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
