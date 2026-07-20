//! Orders for protocol v4 simplified simultaneous turns (ADR-0025).
//! Collection stages: allocate → commit_path → commit_volley (auto next turn).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

use crate::game_state::GameState;
use crate::path::PathAction;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Order {
    /// Allocate motion power, weapon charge totals, and shield power for one ship.
    /// Staged until every living ship commits; applied together.
    Allocate {
        ship: u32,
        movement: u32,
        /// Desired charge totals. Omitted or empty leaves carried charge untouched
        /// (only listed weapons are considered for increases).
        #[serde(default)]
        weapons: BTreeMap<String, u32>,
        shields: [u32; 6],
    },
    /// One complete path for `ship` during the movement collection stage.
    CommitPath { ship: u32, actions: Vec<PathAction> },
    /// One complete volley for `ship` during the firing collection stage.
    /// Empty `shots` is an explicit hold-fire.
    CommitVolley { ship: u32, shots: Vec<VolleyShot> },
    // --- Retired v3 orders (deserialize for clear rejection only) ---
    #[serde(other)]
    RetiredUnknown,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct VolleyShot {
    pub weapon: String,
    pub target: u32,
    pub shield_facing: u8,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OrderError {
    #[error("ship {0} was not found")]
    ShipNotFound(u32),
    #[error("ship {ship} lacks power (need {need}, have {have})")]
    InsufficientPower { ship: u32, need: u32, have: u32 },
    #[error("ship {ship} lacks motion (need {need}, have {have})")]
    InsufficientMotion { ship: u32, need: u32, have: u32 },
    #[error("weapon {0} was not found")]
    WeaponNotFound(String),
    #[error("target {0} was not found")]
    TargetNotFound(u32),
    #[error("ship {0} cannot fire at itself")]
    FireAtSelf(u32),
    #[error("weapon {weapon} cannot fire at range {range}; minimum range is {min_range}")]
    TooClose {
        weapon: String,
        range: u32,
        min_range: u32,
    },
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
    #[error("weapon {weapon} on ship {ship} appears more than once in the volley")]
    WeaponAlreadyInVolley { ship: u32, weapon: String },
    #[error("weapon {weapon} on ship {ship} is not charged")]
    WeaponNotCharged { ship: u32, weapon: String },
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
    #[error("ship {0} has already committed a path this turn")]
    AlreadyCommittedPath(u32),
    #[error("ship {0} has already committed a volley this turn")]
    AlreadyCommittedVolley(u32),
    #[error("ship {ship} path is illegal: {reason}")]
    IllegalPath { ship: u32, reason: String },
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
    #[error("path preview failed: {0}")]
    PreviewFailed(String),
    #[error(
        "retired protocol-v3 order is not accepted under protocol v4 (use commit_path / commit_volley; no ready_fire or end_turn)"
    )]
    RetiredV3Order,
}

pub fn apply_order(game: &mut GameState, order: Order) -> Result<(), OrderError> {
    match order {
        Order::Allocate {
            ship,
            movement,
            weapons,
            shields,
        } => game.allocate_v2(ship, movement, weapons, shields),
        Order::CommitPath { ship, actions } => game.commit_path(ship, actions),
        Order::CommitVolley { ship, shots } => game.commit_volley(ship, shots),
        Order::RetiredUnknown => Err(OrderError::RetiredV3Order),
    }
}
