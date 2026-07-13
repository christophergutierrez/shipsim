use serde::Serialize;

use crate::arc::Mount;
use crate::combat::{Arc, Weapon};
use crate::combat_tables;
use crate::game_state::{FireCommit, GameState, ScenarioStatus};

#[derive(Debug, Clone, Serialize)]
pub struct MapSnapshot {
    pub width: u32,
    pub height: u32,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HexSnapshot {
    pub q: i32,
    pub r: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShipSnapshot {
    pub id: u32,
    pub class: String,
    /// Relative target silhouette; size 2 is the neutral to-hit baseline.
    pub size: u32,
    /// `player`, `ai`, or `scripted` (ADR-0018).
    pub controller: String,
    pub q: i32,
    pub r: i32,
    pub facing: u8,
    pub speed: u32,
    pub power: u32,
    /// Effective power after power_sys damage.
    pub power_available: u32,
    pub movement_allocated: u32,
    pub shields_powered: [u32; 6],
    pub shields_remaining: [u32; 6],
    pub max_shield_per_facing: u32,
    /// Hull boxes remaining (SSD).
    pub structure: u32,
    pub engine: u32,
    pub power_sys: u32,
    pub bridge: u32,
    pub weapon_boxes: Vec<u32>,
    pub destroyed: bool,
    pub weapons: Vec<WeaponSnapshot>,
    // --- Inertial movement (ADR-0022, M2) ---
    pub max_velocity: u8,
    pub thrust_per_power: u32,
    pub power_per_thrust: u32,
    /// Current velocity speed (hexes per turn).
    pub velocity: u8,
    /// Current course (hex direction 0..=5).
    pub course: u8,
    /// Thrust reserve bought this turn.
    pub thrust_remaining: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeaponSnapshot {
    pub id: String,
    pub kind: String,
    pub arc: String,
    pub mount: Option<String>,
    pub max_range: u32,
    pub charge: u32,
    pub fired: bool,
    pub max_charge: u32,
    pub operational: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StateSnapshot {
    pub protocol_version: u32,
    pub turn: u32,
    pub status: ScenarioStatus,
    pub phase: String,
    /// 1..=4 during the movement phase (ADR-0022 M4).
    pub movement_phase: u8,
    /// Living ships that have committed a maneuver for the current movement phase.
    pub ships_committed_this_phase: Vec<u32>,
    pub ships_ready_fire: Vec<u32>,
    /// Living ships that have completed v2 power allocation this turn.
    pub ships_allocated_this_turn: Vec<u32>,
    pub seed: u64,
    /// PRNG stream position for mid-game resume (TS3).
    pub prng_state: u64,
    pub map: MapSnapshot,
    pub objective: Option<HexSnapshot>,
    pub ships: Vec<ShipSnapshot>,
    pub fire_commits: Vec<FireCommit>,
    pub combat_log: Vec<CombatLogEntry>,
    /// Advisory (never blocks EndTurn): some living ship could still move or fire legally.
    pub end_turn_warning: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CombatLogEntry {
    pub attacker: u32,
    pub target: u32,
    pub weapon: String,
    pub shield: usize,
    pub damage: u32,
    pub shield_absorbed: u32,
    pub hull_damage: u32,
    pub kind: String,
}

impl StateSnapshot {
    pub fn from_game_state(game: &GameState) -> Self {
        Self {
            protocol_version: crate::protocol::PROTOCOL_VERSION,
            turn: game.turn_number(),
            status: game.status(),
            phase: game.phase_name().to_string(),
            movement_phase: game.movement_phase(),
            ships_committed_this_phase: game.ships_committed_this_phase(),
            ships_ready_fire: game.ready_fire(),
            ships_allocated_this_turn: game.allocated_this_turn(),
            seed: game.seed(),
            prng_state: game.prng_state(),
            map: MapSnapshot {
                width: game.board().width,
                height: game.board().height,
                mode: match game.board().mode {
                    crate::board::MapMode::Hard => "hard".into(),
                    crate::board::MapMode::Floating => "floating".into(),
                },
            },
            objective: game.objective().map(|objective| HexSnapshot {
                q: objective.q,
                r: objective.r,
            }),
            ships: game
                .ships()
                .iter()
                .map(|ship| ShipSnapshot {
                    id: ship.id,
                    class: ship.class.clone(),
                    size: ship.size,
                    controller: game.controller_label(ship.id).to_string(),
                    q: ship.pos.q,
                    r: ship.pos.r,
                    facing: ship.facing,
                    speed: ship.speed,
                    power: ship.power,
                    power_available: ship.effective_power(),
                    movement_allocated: ship.movement_allocated,
                    shields_powered: ship.shields_powered,
                    shields_remaining: ship.shields_remaining,
                    max_shield_per_facing: ship.max_shield_per_facing,
                    structure: ship.structure(),
                    engine: ship.ssd.engine,
                    power_sys: ship.ssd.power_sys,
                    bridge: ship.ssd.bridge,
                    weapon_boxes: ship.ssd.weapon_boxes.clone(),
                    destroyed: ship.destroyed,
                    max_velocity: ship.max_velocity,
                    thrust_per_power: ship.thrust_conversion.thrust_per_power,
                    power_per_thrust: ship.thrust_conversion.power_per_thrust,
                    velocity: ship.velocity.speed,
                    course: ship.velocity.course,
                    thrust_remaining: ship.thrust_remaining,
                    weapons: ship
                        .weapons
                        .iter()
                        .enumerate()
                        .map(|(idx, weapon)| WeaponSnapshot {
                            id: weapon.id.clone(),
                            kind: weapon_kind_name(weapon).to_string(),
                            arc: arc_name(&weapon.arc).to_string(),
                            mount: weapon.mount.map(|mount| mount_name(mount).to_string()),
                            max_range: weapon.max_range,
                            charge: ship.weapon_charges.get(&weapon.id).copied().unwrap_or(0),
                            fired: game.weapon_fired_this_turn(ship.id, &weapon.id),
                            max_charge: weapon.max_charge,
                            operational: ship.ssd.weapon_operational(idx),
                        })
                        .collect(),
                })
                .collect(),
            fire_commits: game.fire_commits().to_vec(),
            combat_log: game
                .combat_log()
                .iter()
                .map(|e| CombatLogEntry {
                    attacker: e.attacker,
                    target: e.target,
                    weapon: e.weapon.clone(),
                    shield: e.shield,
                    damage: e.damage,
                    shield_absorbed: e.shield_absorbed,
                    hull_damage: e.hull_damage,
                    kind: e.kind.clone(),
                })
                .collect(),
            end_turn_warning: game.end_turn_warning(),
        }
    }
}

fn weapon_kind_name(weapon: &Weapon) -> &'static str {
    match weapon.kind {
        combat_tables::WeaponKind::Beam => "Beam",
        combat_tables::WeaponKind::Plasma => "Plasma",
        combat_tables::WeaponKind::Torp => "Torp",
    }
}

fn arc_name(arc: &Arc) -> &'static str {
    match arc {
        Arc::Forward => "Forward",
        Arc::Rear => "Rear",
        Arc::Left => "Left",
        Arc::Right => "Right",
        Arc::All => "All",
    }
}

fn mount_name(mount: Mount) -> &'static str {
    match mount {
        Mount::Forward => "forward",
        Mount::ForwardStarboard => "forward_starboard",
        Mount::AftStarboard => "aft_starboard",
        Mount::Aft => "aft",
        Mount::AftPort => "aft_port",
        Mount::ForwardPort => "forward_port",
    }
}
