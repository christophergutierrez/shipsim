use serde::Serialize;

use crate::arc::Mount;
use crate::combat::{Arc, Weapon};
use crate::combat_tables;
use crate::game_state::{FireOpportunity, GameState, ScenarioStatus};
use crate::path_resolve::PathResult;

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
    pub size: u32,
    /// `player`, `ai`, or `scripted` (ADR-0018).
    pub controller: String,
    pub q: i32,
    pub r: i32,
    pub facing: u8,
    pub power: u32,
    #[serde(skip_serializing_if = "is_zero_u8")]
    pub attack_accuracy_bonus: u8,
    pub power_available: u32,
    pub movement_allocated: u32,
    pub shields_powered: [u32; 6],
    pub shields_remaining: [u32; 6],
    pub max_shield_per_facing: u32,
    pub structure: u32,
    pub engine: u32,
    pub power_sys: u32,
    pub bridge: u32,
    pub weapon_boxes: Vec<u32>,
    pub destroyed: bool,
    pub weapons: Vec<WeaponSnapshot>,
    pub max_maneuver_actions: u8,
    pub thrust_per_power: u32,
    pub power_per_thrust: u32,
    /// Usable motion points during movement stage (0 after resolution / other stages).
    pub motion_available: u32,
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
    /// Living ships that have completed (or staged) allocation this turn.
    pub ships_allocated_this_turn: Vec<u32>,
    /// Living ships that have committed a path this movement stage.
    pub ships_committed_path: Vec<u32>,
    /// Living ships that have committed a volley this firing stage.
    pub ships_committed_volley: Vec<u32>,
    pub seed: u64,
    pub prng_state: u64,
    pub map: MapSnapshot,
    pub objective: Option<HexSnapshot>,
    pub ships: Vec<ShipSnapshot>,
    pub combat_log: Vec<CombatLogEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fire_opportunity: Option<FireOpportunity>,
    /// Path resolution telemetry from the last movement stage.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_results: Vec<PathResult>,
    pub rules_id: String,
    pub rules_fingerprint: String,
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
            ships_allocated_this_turn: game.allocated_this_turn(),
            ships_committed_path: game.ships_committed_path(),
            ships_committed_volley: game.ships_committed_volley(),
            seed: game.seed(),
            prng_state: game.prng_state(),
            map: MapSnapshot {
                width: game.board().width,
                height: game.board().height,
                mode: match game.board().mode {
                    crate::board::MapMode::Hard => "hard".into(),
                    crate::board::MapMode::Floating => "floating".into(),
                    crate::board::MapMode::Unbounded => "unbounded".into(),
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
                    power: ship.power,
                    attack_accuracy_bonus: ship.attack_accuracy_bonus,
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
                    max_maneuver_actions: ship.max_maneuver_actions,
                    thrust_per_power: ship.thrust_conversion.thrust_per_power,
                    power_per_thrust: ship.thrust_conversion.power_per_thrust,
                    motion_available: ship.motion_available,
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
            fire_opportunity: game.fire_opportunity(),
            path_results: game.path_results().to_vec(),
            rules_id: game.rules_id().to_string(),
            rules_fingerprint: game.rules_fingerprint().to_string(),
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

fn is_zero_u8(value: &u8) -> bool {
    *value == 0
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
