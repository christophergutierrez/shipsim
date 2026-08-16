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
    #[serde(default)]
    pub winner: Option<String>,
    #[serde(default)]
    pub credits: std::collections::BTreeMap<String, u32>,
    #[serde(default)]
    pub purchasable: Vec<PurchaseOption>,
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
    #[serde(default)]
    pub side: String,
    #[serde(default)]
    pub cost: u32,
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
    /// Design cap reduced by engine damage. `None` means the snapshot predates
    /// the field (fall back to `max_maneuver_actions`). `Some(0)` means the
    /// engines are gone — a real zero, not "absent".
    #[serde(default)]
    pub effective_max_maneuver_actions: Option<u32>,
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
    #[serde(default)]
    pub max_shields: Vec<u32>,
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
    #[serde(default)]
    pub systems: Vec<InstalledSystem>,
    /// Engine-authoritative repair ceiling. Absent means this snapshot cannot
    /// safely answer repair-capacity questions; the client must not infer it.
    #[serde(default)]
    pub repair_cap: Option<u32>,
    /// Evasive motion points declared on the last resolved path this turn.
    #[serde(default)]
    pub evasion_committed: u32,
    #[serde(default)]
    pub cloaked: bool,
    #[serde(default)]
    pub squad_id: Option<i64>,
    #[serde(default)]
    pub squad_leader: Option<i64>,
    #[serde(default)]
    pub squad_members: Vec<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PurchaseOption {
    pub class: String,
    pub cost: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InstalledSystem {
    pub kind: String,
    #[serde(default)]
    pub mk: Option<u8>,
}

impl Ship {
    pub fn has_system(&self, kind: &str) -> bool {
        self.systems.iter().any(|system| system.kind == kind)
    }
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
    #[serde(default)]
    pub ammo_remaining: Option<u32>,
    #[serde(default)]
    pub max_ammo: Option<u32>,
    #[serde(default)]
    pub accuracy_bonus: u32,
    #[serde(default)]
    pub damage_bonus: u32,
    #[serde(default)]
    pub repeat: bool,
    #[serde(default)]
    pub pierce: bool,
}

impl Weapon {
    pub fn is_pd(&self) -> bool {
        self.kind.eq_ignore_ascii_case("pd")
    }

    pub fn tags(&self) -> Vec<&'static str> {
        let mut tags = Vec::new();
        if self.repeat {
            tags.push("Repeat");
        }
        if self.pierce {
            tags.push("Pierce");
        }
        if self.is_pd() {
            tags.push("PD");
        }
        if self.kind.eq_ignore_ascii_case("graviton") {
            tags.push("hex");
        }
        tags
    }
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
    pub kind: String, // "hit" | "miss" | "pd_hit" | "pd_miss" | "graviton"
    #[serde(default)]
    pub roll: Option<u32>,
    #[serde(default)]
    pub packet: Option<u8>,
    #[serde(default)]
    pub vs_weapon: Option<String>,
}

/// One or more engine combat-log rows, collapsed for display.
pub fn format_combat_lines(events: &[CombatEvent], snap: &Snapshot, turn: u32) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < events.len() {
        let event = &events[i];
        let attacker = snap
            .ship(event.attacker)
            .map(callsign)
            .unwrap_or_else(|| format!("#{}", event.attacker));
        let target = snap
            .ship(event.target)
            .map(callsign)
            .unwrap_or_else(|| format!("#{}", event.target));
        if event.packet.is_some() {
            let mut end = i + 1;
            let mut hits = u32::from(event.kind == "hit");
            let mut damage = event.damage;
            let mut absorbed = event.shield_absorbed;
            let mut hull = event.hull_damage;
            while end < events.len()
                && events[end].packet.is_some()
                && events[end].attacker == event.attacker
                && events[end].target == event.target
                && events[end].weapon == event.weapon
            {
                hits += u32::from(events[end].kind == "hit");
                damage += events[end].damage;
                absorbed += events[end].shield_absorbed;
                hull += events[end].hull_damage;
                end += 1;
            }
            let n = end - i;
            out.push(format!(
                "T{turn} {attacker} {}>{target} {hits}/{n} HIT +{damage} sh-{absorbed} int-{hull}",
                event.weapon
            ));
            i = end;
            continue;
        }
        out.push(format_one_combat_line(event, &attacker, &target, turn));
        i += 1;
    }
    out
}

fn format_one_combat_line(event: &CombatEvent, attacker: &str, target: &str, turn: u32) -> String {
    match event.kind.as_str() {
        "pd_hit" => format!(
            "T{turn} {attacker} {} INTERCEPT {}",
            event.weapon,
            event.vs_weapon.as_deref().unwrap_or("ordnance")
        ),
        "pd_miss" => format!(
            "T{turn} {attacker} {} PD-MISS vs {}",
            event.weapon,
            event.vs_weapon.as_deref().unwrap_or("ordnance")
        ),
        "graviton" => format!(
            "T{turn} {attacker} {}>{target} GRAV int-{}",
            event.weapon, event.hull_damage
        ),
        "hit" => format!(
            "T{turn} {attacker} {}>{target} HIT +{} sh-{} int-{}",
            event.weapon, event.damage, event.shield_absorbed, event.hull_damage
        ),
        _ => format!("T{turn} {attacker} {}>{target} MISS", event.weapon),
    }
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
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        cloak: bool,
        #[serde(default, skip_serializing_if = "is_zero_u32")]
        repair: u32,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        unsquad: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        squad_leader: Option<i64>,
    },
    CommitPath {
        ship: i64,
        actions: Vec<String>,
        #[serde(default, skip_serializing_if = "is_zero_u32")]
        evasive: u32,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        follow: bool,
    },
    CommitVolley {
        ship: i64,
        shots: Vec<VolleyShot>,
    },
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
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
                cloak: false,
                repair: 0,
                unsquad: false,
                squad_leader: None,
            },
        }
    }

    pub fn allocate_with_systems(
        ship: i64,
        movement: u32,
        weapons: serde_json::Value,
        shields: Vec<u32>,
        cloak: bool,
        repair: u32,
        unsquad: bool,
    ) -> Self {
        Self::allocate_full(
            ship, movement, weapons, shields, cloak, repair, unsquad, None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn allocate_full(
        ship: i64,
        movement: u32,
        weapons: serde_json::Value,
        shields: Vec<u32>,
        cloak: bool,
        repair: u32,
        unsquad: bool,
        squad_leader: Option<i64>,
    ) -> Self {
        Order {
            protocol_version: PROTOCOL_VERSION,
            body: OrderBody::Allocate {
                ship,
                movement,
                weapons: Some(weapons),
                shields: Some(shields),
                cloak,
                repair,
                unsquad,
                squad_leader,
            },
        }
    }

    pub fn commit_path(ship: i64, actions: Vec<String>) -> Self {
        Self::commit_path_with_evasive(ship, actions, 0)
    }

    pub fn commit_path_with_evasive(ship: i64, actions: Vec<String>, evasive: u32) -> Self {
        Self::commit_path_full(ship, actions, evasive, false)
    }

    pub fn commit_path_full(ship: i64, actions: Vec<String>, evasive: u32, follow: bool) -> Self {
        Order {
            protocol_version: PROTOCOL_VERSION,
            body: OrderBody::CommitPath {
                ship,
                actions,
                evasive,
                follow,
            },
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
    /// Assumes a facing-0 start; `turn_left` is the +1 (counterclockwise) step,
    /// so `facing` of them reach the absolute facing from 0.
    pub fn commit_maneuver(ship: i64, maneuver: Maneuver) -> Self {
        let actions = match maneuver {
            Maneuver::Coast => Vec::new(),
            Maneuver::Accel => vec!["move_f".into()],
            Maneuver::Turn { facing } => {
                std::iter::repeat_n("turn_left".to_string(), facing as usize).collect()
            }
            Maneuver::TurnAccel { facing } => {
                let mut actions: Vec<String> =
                    std::iter::repeat_n("turn_left".to_string(), facing as usize).collect();
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
                cloak: false,
                repair: 0,
                unsquad: false,
                squad_leader: None,
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

impl Ship {
    pub fn shield_cap(&self, facing: usize) -> u32 {
        self.max_shields
            .get(facing)
            .copied()
            .filter(|&cap| cap > 0 || self.max_shields.len() == 6)
            .unwrap_or(self.max_shield_per_facing)
    }

    /// Motion cap the engine will actually enforce this turn.
    ///
    /// `Some(n)` (including `Some(0)` — engines destroyed) is authoritative.
    /// `None` means a pre-field snapshot: fall back to the design cap so an
    /// older engine degrades to "clamp at the undamaged hull" rather than
    /// "no clamp".
    pub fn motion_cap(&self) -> u32 {
        self.effective_max_maneuver_actions
            .unwrap_or(self.max_maneuver_actions)
    }

    /// Most movement power worth spending: the least power that reaches
    /// `motion_cap`. Beyond this the engine truncates the surplus
    /// (`path::usable_motion`) and the power is destroyed, not refunded.
    ///
    /// `Some(0)` is a real ceiling (dead engines). `None` means the ratio is
    /// unusable (immobile / missing) so callers keep the power-pool bound.
    pub fn movement_power_cap(&self) -> Option<u32> {
        if self.thrust_per_power == 0 || self.power_per_thrust == 0 {
            return None;
        }
        Some(shipsim_core::thrust::power_for_thrust(
            self.motion_cap(),
            self.thrust_per_power,
            self.power_per_thrust,
        ))
    }
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
        matches!(self.status.as_str(), "Won" | "Lost" | "Draw")
    }
}
