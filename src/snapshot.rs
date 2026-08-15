use serde::Serialize;

use crate::arc::Mount;
use crate::combat::{Arc, Weapon};
use crate::combat_tables;
use crate::game_state::{FireOpportunity, GameState, ScenarioStatus};
use crate::path_resolve::PathResult;
use crate::schema::SystemKind;

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
    /// Canonical catalog key (ship-definition file stem). Additive identity
    /// field for presentation clients; distinct from numeric `id` and display
    /// `class`. Protocol remains v4. See `frontend/love/assets/ship_art/README.md`.
    pub class_id: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_shields: Option<[u32; 6]>,
    pub structure: u32,
    pub engine: u32,
    pub power_sys: u32,
    pub bridge: u32,
    pub weapon_boxes: Vec<u32>,
    pub destroyed: bool,
    pub weapons: Vec<WeaponSnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub systems: Vec<SystemKind>,
    /// Maximum repair boxes this ship may request when repair is installed.
    /// Clients must not derive this rule projection from `size`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_cap: Option<u32>,
    pub max_maneuver_actions: u8,
    /// Design cap reduced by engine damage — the ceiling the engine actually
    /// enforces (`path::usable_motion`). Distinct from `max_maneuver_actions`,
    /// which stays the undamaged design value that clients label motion against.
    /// Clients must clamp movement allocation against **this**: power that buys
    /// motion above it is truncated on resolve and silently lost.
    pub effective_max_maneuver_actions: u32,
    pub thrust_per_power: u32,
    pub power_per_thrust: u32,
    /// Usable motion points during movement stage (0 after resolution / other stages).
    /// Clients must not derive this rule projection from other snapshot fields.
    pub motion_available: u32,
    /// Evasive motion points declared on the last resolved path this turn (0 until then).
    /// Clients must not derive this rule projection from other snapshot fields.
    pub evasion_committed: u32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub cloaked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub squad_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub squad_leader: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub squad_members: Vec<u32>,
}

impl ShipSnapshot {
    /// Synthetic snapshot for tests; not a legal in-game ship.
    pub fn test_fixture(id: u32) -> Self {
        Self {
            id,
            class: String::new(),
            class_id: String::new(),
            size: 0,
            controller: "player".into(),
            q: 0,
            r: 0,
            facing: 0,
            power: 0,
            attack_accuracy_bonus: 0,
            power_available: 0,
            movement_allocated: 0,
            shields_powered: [0; 6],
            shields_remaining: [0; 6],
            max_shield_per_facing: 0,
            max_shields: None,
            structure: 0,
            engine: 0,
            power_sys: 0,
            bridge: 0,
            weapon_boxes: Vec::new(),
            destroyed: false,
            weapons: Vec::new(),
            systems: Vec::new(),
            repair_cap: None,
            max_maneuver_actions: 0,
            effective_max_maneuver_actions: 0,
            thrust_per_power: 0,
            power_per_thrust: 0,
            motion_available: 0,
            evasion_committed: 0,
            cloaked: false,
            squad_id: None,
            squad_leader: None,
            squad_members: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ShipSnapshot;

    #[test]
    fn test_fixture_has_safe_defaults() {
        let ship = ShipSnapshot::test_fixture(7);
        assert_eq!(ship.id, 7);
        assert!(!ship.destroyed);
        assert!(ship.repair_cap.is_none());
        assert!(ship.weapons.is_empty());
    }
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
    /// Remaining magazine for ammo-tracked weapons (`None` = unlimited / not tracked).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ammo_remaining: Option<u32>,
    /// Magazine capacity for ammo-tracked weapons.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_ammo: Option<u32>,
    #[serde(skip_serializing_if = "is_zero_u8")]
    pub accuracy_bonus: u8,
    #[serde(skip_serializing_if = "is_zero_u32")]
    pub damage_bonus: u32,
    #[serde(skip_serializing_if = "is_false")]
    pub repeat: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub pierce: bool,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub packet: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vs_weapon: Option<String>,
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
                    class_id: ship.class_id.clone(),
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
                    max_shields: Some(ship.max_shields),
                    structure: ship.structure(),
                    engine: ship.ssd.engine,
                    power_sys: ship.ssd.power_sys,
                    bridge: ship.ssd.bridge,
                    weapon_boxes: ship.ssd.weapon_boxes.clone(),
                    destroyed: ship.destroyed,
                    max_maneuver_actions: ship.max_maneuver_actions,
                    effective_max_maneuver_actions: ship.effective_max_maneuver_actions(),
                    thrust_per_power: ship.thrust_conversion.thrust_per_power,
                    power_per_thrust: ship.thrust_conversion.power_per_thrust,
                    motion_available: ship.motion_available,
                    evasion_committed: ship.evasion_committed,
                    cloaked: ship.cloaked,
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
                            max_ammo: weapon.max_ammo,
                            ammo_remaining: weapon
                                .max_ammo
                                .map(|_| ship.weapon_ammo.get(&weapon.id).copied().unwrap_or(0)),
                            accuracy_bonus: weapon.accuracy_bonus,
                            damage_bonus: weapon.damage_bonus,
                            repeat: weapon.repeat,
                            pierce: weapon.pierce,
                        })
                        .collect(),
                    systems: ship.systems.clone(),
                    repair_cap: ship
                        .systems
                        .iter()
                        .any(|system| matches!(system, SystemKind::Repair))
                        .then(|| ship.repair_cap()),
                    squad_id: game
                        .squads()
                        .iter()
                        .find(|(_, squad)| squad.members.contains(&ship.id))
                        .map(|(id, _)| *id),
                    squad_leader: game
                        .squads()
                        .values()
                        .find(|squad| squad.members.contains(&ship.id))
                        .map(|squad| squad.leader),
                    squad_members: game
                        .squads()
                        .values()
                        .find(|squad| squad.members.contains(&ship.id))
                        .map(|squad| squad.members.clone())
                        .unwrap_or_default(),
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
                    packet: e.packet,
                    vs_weapon: e.vs_weapon.clone(),
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
        combat_tables::WeaponKind::Missile => "Missile",
        combat_tables::WeaponKind::Pd => "Pd",
        combat_tables::WeaponKind::Graviton => "Graviton",
    }
}

fn is_zero_u8(value: &u8) -> bool {
    *value == 0
}

fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}

fn is_false(value: &bool) -> bool {
    !*value
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
