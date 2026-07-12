use serde::Serialize;

use crate::arc::Mount;
use crate::combat::{Arc, Weapon};
use crate::combat_tables;
use crate::game_state::{FireCommit, GameState, ScenarioStatus};

#[derive(Debug, Serialize)]
pub struct MapSnapshot {
    pub width: u32,
    pub height: u32,
    pub mode: String,
}

#[derive(Debug, Serialize)]
pub struct HexSnapshot {
    pub q: i32,
    pub r: i32,
}

#[derive(Debug, Serialize)]
pub struct ShipSnapshot {
    pub id: u32,
    pub class: String,
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
    pub move_remaining: u32,
    pub keel: String,
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
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
pub struct StateSnapshot {
    pub protocol_version: u32,
    pub turn: u32,
    /// Ship that may move now (v2 active mover), or `None` outside the movement phase.
    pub active_ship: Option<u32>,
    pub status: ScenarioStatus,
    pub phase: String,
    pub move_order: Vec<u32>,
    pub ships_moved_this_phase: Vec<u32>,
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
    pub kind: String,
}

impl StateSnapshot {
    pub fn from_game_state(game: &GameState) -> Self {
        Self {
            protocol_version: crate::protocol::PROTOCOL_VERSION,
            turn: game.turn_number(),
            active_ship: game.active_v2_mover(),
            status: game.status(),
            phase: game.phase_name().to_string(),
            move_order: game.move_order().to_vec(),
            ships_moved_this_phase: game.moved_this_phase(),
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
                    controller: game.controller_label(ship.id).to_string(),
                    q: ship.pos.q,
                    r: ship.pos.r,
                    facing: ship.facing,
                    speed: ship.speed,
                    power: ship.power,
                    power_available: ship.effective_power(),
                    movement_allocated: ship.movement_allocated,
                    move_remaining: ship.move_remaining,
                    keel: format!("{:?}", ship.keel).to_ascii_lowercase(),
                    shields_powered: ship.shields_powered,
                    shields_remaining: ship.shields_remaining,
                    max_shield_per_facing: ship.max_shield_per_facing,
                    structure: ship.structure(),
                    engine: ship.ssd.engine,
                    power_sys: ship.ssd.power_sys,
                    bridge: ship.ssd.bridge,
                    weapon_boxes: ship.ssd.weapon_boxes.clone(),
                    destroyed: ship.destroyed,
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
