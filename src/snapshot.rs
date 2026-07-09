use serde::Serialize;

use crate::combat::{Arc, WeaponKind};
use crate::game_state::{GameState, ScenarioStatus};

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
    pub q: i32,
    pub r: i32,
    pub facing: u8,
    pub speed: u32,
    pub power: u32,
    pub turn_speed: u32,
    pub weapons_energy: u32,
    pub shield_reinforce: u32,
    pub turn_mode: u32,
    pub shields: [u32; 6],
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
    pub max_range: u32,
    pub operational: bool,
}

#[derive(Debug, Serialize)]
pub struct SeekingSnapshot {
    pub id: u32,
    pub owner: u32,
    pub weapon: String,
    pub target: u32,
    pub q: i32,
    pub r: i32,
    pub damage: u32,
}

#[derive(Debug, Serialize)]
pub struct StateSnapshot {
    pub turn: u32,
    pub impulse: u8,
    pub status: ScenarioStatus,
    pub seed: u64,
    /// PRNG stream position for mid-game resume (TS3).
    pub prng_state: u64,
    pub map: MapSnapshot,
    pub objective: Option<HexSnapshot>,
    pub ships: Vec<ShipSnapshot>,
    pub seeking: Vec<SeekingSnapshot>,
    pub combat_log: Vec<CombatLogEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CombatLogEntry {
    pub attacker: u32,
    pub target: u32,
    pub shield: usize,
    pub damage: u32,
    pub kind: String,
}

impl StateSnapshot {
    pub fn from_game_state(game: &GameState) -> Self {
        Self {
            turn: game.turn_number(),
            impulse: game.impulse(),
            status: game.status(),
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
                    q: ship.pos.q,
                    r: ship.pos.r,
                    facing: ship.facing,
                    speed: ship.speed,
                    power: ship.power,
                    turn_speed: ship.turn_speed,
                    weapons_energy: ship.weapons_energy,
                    shield_reinforce: ship.shield_reinforce,
                    turn_mode: ship.turn_mode,
                    shields: ship.shields,
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
                            kind: weapon_kind_name(&weapon.kind).to_string(),
                            arc: arc_name(&weapon.arc).to_string(),
                            max_range: weapon.max_range,
                            operational: ship.ssd.weapon_operational(idx),
                        })
                        .collect(),
                })
                .collect(),
            seeking: game
                .seeking_munitions()
                .iter()
                .map(|m| SeekingSnapshot {
                    id: m.id,
                    owner: m.owner,
                    weapon: m.weapon_id.clone(),
                    target: m.target,
                    q: m.pos.q,
                    r: m.pos.r,
                    damage: m.damage,
                })
                .collect(),
            combat_log: game
                .combat_log()
                .iter()
                .map(|e| CombatLogEntry {
                    attacker: e.attacker,
                    target: e.target,
                    shield: e.shield,
                    damage: e.damage,
                    kind: e.kind.clone(),
                })
                .collect(),
        }
    }
}

fn weapon_kind_name(kind: &WeaponKind) -> &'static str {
    match kind {
        WeaponKind::Phaser => "Phaser",
        WeaponKind::Disruptor => "Disruptor",
        WeaponKind::Drone => "Drone",
        WeaponKind::Plasma => "Plasma",
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
