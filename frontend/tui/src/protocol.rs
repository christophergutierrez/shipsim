//! NDJSON protocol model for shipsim v3.
//!
//! These types mirror the JSON shapes produced by the `shipsim` binary
//! (`docs/PROTOCOL.md`). The TUI never recomputes rules — it only reads
//! snapshots and builds orders.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

// ── Snapshot ──────────────────────────────────────────────────────────────

/// A full game-state snapshot, one per NDJSON line from the engine.
#[derive(Debug, Clone, Deserialize)]
pub struct Snapshot {
    pub protocol_version: u32,
    pub turn: u32,
    pub status: String,
    pub phase: String,
    #[serde(default)]
    pub movement_phase: u32,
    #[serde(default)]
    pub ships_committed_this_phase: Vec<i64>,
    #[serde(default)]
    pub ships_ready_fire: Vec<i64>,
    #[serde(default)]
    pub ships_allocated_this_turn: Vec<i64>,
    #[serde(default)]
    pub seed: u64,
    #[serde(default)]
    pub prng_state: u64,
    pub map: GameMap,
    #[serde(default)]
    pub objective: Option<serde_json::Value>,
    pub ships: Vec<Ship>,
    #[serde(default)]
    pub combat_log: Vec<CombatEvent>,
    #[serde(default)]
    pub fire_commits: Vec<FireCommit>,
    #[serde(default)]
    pub end_turn_warning: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GameMap {
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub mode: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Ship {
    pub id: i64,
    pub class: String,
    pub size: u32,
    pub controller: String,
    pub q: i32,
    pub r: i32,
    pub facing: u32,
    #[serde(default)]
    pub speed: u32,
    #[serde(default)]
    pub power: u32,
    #[serde(default)]
    pub power_available: u32,
    #[serde(default)]
    pub movement_allocated: u32,
    #[serde(default)]
    pub shields_powered: Vec<u32>,
    #[serde(default)]
    pub shields_remaining: Vec<u32>,
    #[serde(default)]
    pub max_shield_per_facing: u32,
    pub structure: u32,
    #[serde(default)]
    pub engine: u32,
    #[serde(default)]
    pub power_sys: u32,
    #[serde(default)]
    pub bridge: u32,
    #[serde(default)]
    pub weapon_boxes: Vec<u32>,
    pub destroyed: bool,
    pub weapons: Vec<Weapon>,
    /// Protocol 3 motion — optional so older fixtures still parse.
    #[serde(default)]
    pub max_velocity: u32,
    #[serde(default)]
    pub velocity: u32,
    #[serde(default)]
    pub course: u32,
    #[serde(default)]
    pub thrust_remaining: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Weapon {
    pub id: String,
    pub kind: String,
    pub arc: String,
    #[serde(default)]
    pub mount: String,
    pub max_range: u32,
    #[serde(default)]
    pub charge: u32,
    #[serde(default)]
    pub fired: bool,
    #[serde(default)]
    pub max_charge: u32,
    #[serde(default)]
    pub operational: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CombatEvent {
    pub attacker: i64,
    pub target: i64,
    pub weapon: String,
    #[serde(default)]
    pub shield: u32,
    #[serde(default)]
    pub damage: u32,
    #[serde(default)]
    pub shield_absorbed: u32,
    #[serde(default)]
    pub hull_damage: u32,
    pub kind: String, // "hit" | "miss"
    #[serde(default)]
    pub roll: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FireCommit {
    pub ship: i64,
    pub weapon: String,
    pub target: i64,
    #[serde(default)]
    pub shield_facing: u32,
}

// ── Soft error ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ErrorResponse {
    #[serde(rename = "type")]
    pub kind: String, // "error"
    pub ok: bool,
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub order: Option<serde_json::Value>,
}

// ── Orders ────────────────────────────────────────────────────────────────

/// Every order carries `protocol_version: 3`.
pub const PROTOCOL_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize)]
pub struct Order {
    pub protocol_version: u32,
    #[serde(flatten)]
    pub body: OrderBody,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OrderBody {
    Allocate {
        ship: i64,
        movement: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        weapons: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        shields: Option<Vec<u32>>,
    },
    CommitManeuver {
        ship: i64,
        maneuver: Maneuver,
    },
    PassMove { ship: i64 },
    CommitFire {
        ship: i64,
        weapon: String,
        target: i64,
        shield_facing: u32,
    },
    ReadyFire { ship: i64 },
    EndTurn,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Maneuver {
    Coast,
    Accel,
    Turn { facing: u32 },
}

impl Order {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }
}

// ── Convenience constructors ──────────────────────────────────────────────

impl Order {
    pub fn allocate(ship: i64, movement: u32, weapons: serde_json::Value, shields: Vec<u32>) -> Self {
        Order {
            protocol_version: PROTOCOL_VERSION,
            body: OrderBody::Allocate {
                ship,
                movement,
                weapons: Some(weapons),
                shields: Some(shields),
            },
        }
    }

    pub fn commit_maneuver(ship: i64, maneuver: Maneuver) -> Self {
        Order {
            protocol_version: PROTOCOL_VERSION,
            body: OrderBody::CommitManeuver { ship, maneuver },
        }
    }

    pub fn pass_move(ship: i64) -> Self {
        Order {
            protocol_version: PROTOCOL_VERSION,
            body: OrderBody::PassMove { ship },
        }
    }

    pub fn commit_fire(ship: i64, weapon: &str, target: i64, shield_facing: u32) -> Self {
        Order {
            protocol_version: PROTOCOL_VERSION,
            body: OrderBody::CommitFire {
                ship,
                weapon: weapon.into(),
                target,
                shield_facing,
            },
        }
    }

    pub fn ready_fire(ship: i64) -> Self {
        Order {
            protocol_version: PROTOCOL_VERSION,
            body: OrderBody::ReadyFire { ship },
        }
    }

    pub fn end_turn() -> Self {
        Order {
            protocol_version: PROTOCOL_VERSION,
            body: OrderBody::EndTurn,
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

/// Callsign for a ship: A# for player, B# for ai, C# for scripted.
pub fn callsign(ship: &Ship) -> String {
    let prefix = match ship.controller.as_str() {
        "player" => "A",
        "ai" => "B",
        _ => "C",
    };
    format!("{prefix}{}", ship.id)
}

/// Facing arrows, board-aligned (0 = +q → right on q→/r↓ maps).
pub const FACING_ARROWS: [&str; 6] = ["→", "↗", "↖", "←", "↙", "↘"];

pub fn facing_arrow(facing: u32) -> &'static str {
    FACING_ARROWS.get(facing as usize).copied().unwrap_or("?")
}

/// Shield face labels (ship-relative): 0:F 1:FR 2:RR 3:R 4:RL 5:FL
pub const SHIELD_LABELS: [&str; 6] = ["F", "FR", "RR", "R", "RL", "FL"];

pub fn shield_label(face: u32) -> &'static str {
    SHIELD_LABELS.get(face as usize).copied().unwrap_or("?")
}

impl Snapshot {
    /// Find a ship by id.
    pub fn ship(&self, id: i64) -> Option<&Ship> {
        self.ships.iter().find(|s| s.id == id)
    }

    /// The player-controlled ship (first non-ai, non-destroyed).
    pub fn player_ship(&self) -> Option<&Ship> {
        self.ships
            .iter()
            .find(|s| s.controller == "player" && !s.destroyed)
    }

    /// True if the game is over.
    pub fn is_over(&self) -> bool {
        matches!(self.status.as_str(), "Won" | "Lost")
    }
}
