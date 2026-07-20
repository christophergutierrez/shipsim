//! NDJSON protocol model for shipsim v4.
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
    /// `allocate` | `movement` | `firing`.
    pub phase: String,
    /// Ships that have committed allocate this turn.
    #[serde(default)]
    pub ships_allocated_this_turn: Vec<i64>,
    /// Ships that have committed a path this turn.
    #[serde(default)]
    pub ships_committed_path: Vec<i64>,
    /// Ships that have committed a volley this turn.
    #[serde(default)]
    pub ships_committed_volley: Vec<i64>,
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
    /// Engine-authoritative legal fire opportunity (additive protocol field).
    #[serde(default)]
    pub fire_opportunity: Option<FireOpportunity>,
    /// Structured movement-resolution telemetry from the last resolved path stage.
    #[serde(default)]
    pub path_results: Vec<PathResult>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FireOpportunity {
    pub ship: i64,
    pub weapon: String,
    pub target: i64,
    #[serde(default)]
    pub legal_shield_facings: Vec<u32>,
}

/// One ship's movement-resolution telemetry from the last resolved path stage
/// (protocol v4 `path_results` entry).
#[derive(Debug, Clone, Deserialize)]
pub struct PathResult {
    pub ship: i64,
    /// Motion points the submitted path cost.
    #[serde(default)]
    pub submitted_cost: u32,
    /// Steps actually applied after simultaneous resolution.
    #[serde(default)]
    pub translated_steps: u32,
    #[serde(default)]
    pub final_q: i32,
    #[serde(default)]
    pub final_r: i32,
    #[serde(default)]
    pub final_facing: u32,
    /// Steps the ship was forced to give back (fell short of the submitted path).
    #[serde(default)]
    pub fallback_steps: u32,
    /// Why the ship fell short, if it did: `edge` | `occupied` | `contested`.
    #[serde(default)]
    pub blocked_kind: Option<String>,
    /// Other ships implicated in a block/contest.
    #[serde(default)]
    pub conflicting_ships: Vec<i64>,
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
    pub power: u32,
    #[serde(default)]
    pub attack_accuracy_bonus: u32,
    #[serde(default)]
    pub power_available: u32,
    #[serde(default)]
    pub movement_allocated: u32,
    /// Usable motion points in the movement stage (v4).
    #[serde(default)]
    pub motion_available: u32,
    /// Hull cap on path length (v4).
    #[serde(default)]
    pub max_maneuver_actions: u32,
    /// Motion points produced per unit of engine power (v4).
    #[serde(default)]
    pub thrust_per_power: u32,
    /// Engine power consumed per motion point (v4).
    #[serde(default)]
    pub power_per_thrust: u32,
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

// ── Fire preview (read-only query response) ───────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct FireDecisionPreview {
    #[serde(rename = "type")]
    pub kind: String,
    pub ok: bool,
    pub legal: bool,
    pub ship: i64,
    pub weapon: String,
    pub target: i64,
    #[serde(default)]
    pub range: Option<u32>,
    #[serde(default)]
    pub threshold: Option<u32>,
    #[serde(default)]
    pub die_sides: Option<u32>,
    #[serde(default)]
    pub hit_percent: Option<u32>,
    #[serde(default)]
    pub projected_damage: Option<u32>,
    #[serde(default)]
    pub legal_shield_facings: Vec<u32>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HexCoord {
    pub q: i32,
    pub r: i32,
}

// ── Path preview (read-only v4 query response) ────────────────────────────

/// One traced step of a previewed path.
#[derive(Debug, Clone, Deserialize)]
pub struct PathStep {
    #[serde(default)]
    pub action: String,
    pub q: i32,
    pub r: i32,
    #[serde(default)]
    pub facing: u32,
}

/// The `path_preview` response envelope (`type: "path_preview"`).
#[derive(Debug, Clone, Deserialize)]
pub struct PathPreview {
    #[serde(rename = "type")]
    pub kind: String,
    pub ok: bool,
    pub ship: i64,
    #[serde(default)]
    pub cost: u32,
    #[serde(default)]
    pub remaining_motion: u32,
    #[serde(default)]
    pub final_q: i32,
    #[serde(default)]
    pub final_r: i32,
    #[serde(default)]
    pub final_facing: u32,
    #[serde(default)]
    pub steps: Vec<PathStep>,
    /// First illegal-action reason, if the path exceeds what the ship can do.
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub error_index: Option<usize>,
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

/// Every order carries `protocol_version: 4`.
pub const PROTOCOL_VERSION: u32 = 4;

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
    CommitPath {
        ship: i64,
        actions: Vec<String>,
    },
    CommitVolley {
        ship: i64,
        shots: Vec<VolleyShot>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct VolleyShot {
    pub weapon: String,
    pub target: i64,
    pub shield_facing: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Maneuver {
    Coast,
    Accel,
    Turn { facing: u32 },
    TurnAccel { facing: u32 },
}

impl Order {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }
}

// ── Convenience constructors ──────────────────────────────────────────────

impl Order {
    pub fn allocate(
        ship: i64,
        movement: u32,
        weapons: serde_json::Value,
        shields: Vec<u32>,
    ) -> Self {
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

    pub fn commit_path(ship: i64, actions: Vec<String>) -> Self {
        Order {
            protocol_version: PROTOCOL_VERSION,
            body: OrderBody::CommitPath { ship, actions },
        }
    }

    pub fn commit_volley(ship: i64, shots: Vec<VolleyShot>) -> Self {
        Order {
            protocol_version: PROTOCOL_VERSION,
            body: OrderBody::CommitVolley { ship, shots },
        }
    }

    /// Legacy test/helper adapter. Interactive input uses `commit_path` with
    /// the actual current facing, so absolute turns are translated there.
    pub fn commit_maneuver(ship: i64, maneuver: Maneuver) -> Self {
        let actions = match maneuver {
            Maneuver::Coast => Vec::new(),
            Maneuver::Accel => vec!["move_f".into()],
            Maneuver::Turn { facing } => {
                std::iter::repeat_n("turn_right".to_string(), facing as usize).collect()
            }
            Maneuver::TurnAccel { facing } => {
                let mut actions: Vec<String> =
                    std::iter::repeat_n("turn_right".to_string(), facing as usize).collect();
                actions.push("move_f".into());
                actions
            }
        };
        Self::commit_path(ship, actions)
    }

    pub fn pass_move(ship: i64) -> Self {
        Self::commit_path(ship, Vec::new())
    }

    /// Hold fire: an empty volley. Every living ship must commit a volley for
    /// the firing stage to resolve.
    pub fn hold_fire(ship: i64) -> Self {
        Self::commit_volley(ship, Vec::new())
    }

    /// Passive allocation for a frontend-driven scripted ship: zero engine power,
    /// zero shields, and NO weapons map (omitting it leaves carried charge intact;
    /// sending explicit zeros would be rejected as an illegal strip).
    pub fn passive_allocate(ship: i64) -> Self {
        Order {
            protocol_version: PROTOCOL_VERSION,
            body: OrderBody::Allocate {
                ship,
                movement: 0,
                weapons: None,
                shields: Some(vec![0; 6]),
            },
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
