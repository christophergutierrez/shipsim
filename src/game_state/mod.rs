//! Game aggregate for protocol v4 simplified simultaneous turns (ADR-0025).

mod allocation;
mod movement;
#[path = "combat.rs"]
mod combat_ops;
mod preview;
mod ai;

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use serde::Serialize;

use crate::board::Board;
use crate::combat;
use crate::hex::Hex;
use crate::movement::VolleyShot;
use crate::path::{self, MapBounds, PathAction, PathState};
use crate::path_resolve::{self, PathClaim, PathResult};
use crate::prng::Prng;
use crate::rules::Ruleset;
use crate::ship::Ship;
use crate::schema::SideId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ScenarioStatus {
    InProgress,
    Won,
    Lost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Allocate,
    Movement,
    Firing,
}

/// Win condition. Objective, single-target destruction, and annihilation are
/// mutually exclusive (AS1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminal {
    ReachHex(Hex),
    DestroyShip(u32),
    DestroyShips { side_a_target: u32, side_b_target: u32 },
    /// Player wins when every non-player (NPC/scripted) ship is destroyed.
    AnnihilateEnemies,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    ShipNotFound(u32),
    WeaponNotFound { ship: u32, weapon: String },
    InvalidFacing(u8),
}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateError::ShipNotFound(id) => write!(f, "ship {id} was not found"),
            StateError::WeaponNotFound { ship, weapon } => {
                write!(f, "weapon {weapon} not found on ship {ship}")
            }
            StateError::InvalidFacing(facing) => write!(f, "facing {facing} is not in 0..=5"),
        }
    }
}

impl std::error::Error for StateError {}

#[derive(Debug, Clone)]
pub struct Turn {
    number: u32,
}

impl Turn {
    pub fn new() -> Self {
        Self { number: 1 }
    }

    pub fn number(&self) -> u32 {
        self.number
    }

    pub fn advance(&mut self) {
        self.number += 1;
    }
}

impl Default for Turn {
    fn default() -> Self {
        Self::new()
    }
}

/// Staged allocation payload (not public until all living ships commit).
#[derive(Debug, Clone, PartialEq, Eq)]
struct StagedAllocation {
    movement: u32,
    weapons: BTreeMap<String, u32>,
    shields: [u32; 6],
    cloak: bool,
    repair: u32,
    unsquad: bool,
    squad_leader: Option<u32>,
}

/// Optional systems and squad changes accompanying an allocation.
#[derive(Debug, Clone, Copy, Default)]
pub struct AllocationOptions {
    pub cloak: bool,
    pub repair: u32,
    pub unsquad: bool,
    pub squad_leader: Option<u32>,
}

/// One engine-authoritative legal fire opportunity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FireOpportunity {
    pub ship: u32,
    pub weapon: String,
    pub target: u32,
    pub legal_shield_facings: Vec<u8>,
}

/// Read-only fire decision data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FireDecisionPreview {
    pub ship: u32,
    pub weapon: String,
    pub target: u32,
    pub range: u32,
    pub threshold: u8,
    pub die_sides: u8,
    pub hit_percent: u32,
    pub projected_damage: u32,
    pub legal_shield_facings: Vec<u8>,
}

/// Path preview result for clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PathPreview {
    pub ship: u32,
    pub cost: u32,
    pub remaining_motion: u32,
    pub final_q: i32,
    pub final_r: i32,
    pub final_facing: u8,
    pub steps: Vec<PathPreviewStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PathPreviewStep {
    pub action: String,
    pub q: i32,
    pub r: i32,
    pub facing: u8,
}

/// Reachable endpoint/facing after spending up to `budget` motion points.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReachEndpoint {
    pub q: i32,
    pub r: i32,
    pub facing: u8,
    pub min_cost: u32,
}

#[derive(Debug, Clone)]
pub struct GameState {
    board: Board,
    ships: Vec<Ship>,
    terminal: Option<Terminal>,
    seed: u64,
    prng: Prng,
    turn: Turn,
    status: ScenarioStatus,
    winner: Option<SideId>,
    credits: BTreeMap<SideId, u32>,
    catalog: BTreeMap<String, Ship>,
    phase: Phase,
    /// Applied allocation this turn (after barrier) and staged commits.
    allocated_this_turn: HashSet<u32>,
    staged_allocations: BTreeMap<u32, StagedAllocation>,
    path_commits: BTreeMap<u32, PathCommit>,
    volley_commits: BTreeMap<u32, Vec<VolleyShot>>,
    /// Weapons that fired this turn (ship, weapon_id).
    fired_weapons_this_turn: HashSet<(u32, String)>,
    combat_log: Vec<CombatLogEvent>,
    npcs: BTreeMap<u32, NpcController>,
    path_results: Vec<PathResult>,
    rules: Arc<Ruleset>,
    squads: BTreeMap<u32, SquadState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SquadState {
    pub leader: u32,
    pub members: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct CombatLogEvent {
    pub attacker: u32,
    pub target: u32,
    pub weapon: String,
    pub shield: usize,
    pub damage: u32,
    pub shield_absorbed: u32,
    pub hull_damage: u32,
    pub kind: String,
    pub packet: Option<u8>,
    pub vs_weapon: Option<String>,
}

fn computer_bonus(systems: &[crate::schema::SystemKind]) -> u8 {
    systems
        .iter()
        .find_map(|system| match system {
            crate::schema::SystemKind::Computer { mk } => Some(*mk),
            _ => None,
        })
        .unwrap_or(0)
}

fn ecm_penalty(
    systems: &[crate::schema::SystemKind],
    kind: crate::combat_tables::WeaponKind,
) -> u8 {
    if kind == crate::combat_tables::WeaponKind::Missile
        && systems
            .iter()
            .any(|system| matches!(system, crate::schema::SystemKind::Ecm))
    {
        2
    } else {
        0
    }
}

/// Single construction point for [`crate::combat_tables::ToHitModifiers`].
/// `resolve_volley_phase` and `fire_decision_preview` read from two different
/// snapshot/ship types but must derive identical modifiers for the same
/// attacker/weapon/target — resolution and its preview silently disagreeing is
/// the one failure mode that must not be possible here. Route both through
/// this so a future field is wired once, not once-per-call-site-remembered.
#[allow(clippy::too_many_arguments)]
fn to_hit_modifiers(
    attack_accuracy_bonus: u8,
    weapon_accuracy_bonus: u8,
    attacker_systems: &[crate::schema::SystemKind],
    target_cloaked: bool,
    target_evasion: u32,
    target_systems: &[crate::schema::SystemKind],
    kind: crate::combat_tables::WeaponKind,
) -> crate::combat_tables::ToHitModifiers {
    crate::combat_tables::ToHitModifiers {
        attack_accuracy_bonus,
        weapon_accuracy_bonus,
        computer_accuracy_bonus: computer_bonus(attacker_systems),
        defender_cloaked: target_cloaked,
        defender_evasion: target_evasion,
        defender_ecm_penalty: ecm_penalty(target_systems, kind),
    }
}

/// Staged path commitment during the movement collection stage.
#[derive(Debug, Clone)]
struct PathCommit {
    actions: Vec<PathAction>,
    evasive: u32,
}

impl GameState {
    pub(crate) fn new_with_squads(
        board: Board,
        ships: Vec<Ship>,
        terminal: Option<Terminal>,
        npcs: BTreeMap<u32, NpcController>,
        seed: u64,
        rules: Arc<Ruleset>,
        squad_defs: BTreeMap<u32, (u32, Vec<u32>)>,
    ) -> Self {
        let catalog = ships
            .iter()
            .filter(|ship| ship.class_id != "shipyard")
            .map(|ship| (ship.class_id.clone(), ship.clone()))
            .collect();
        let mut state = Self {
            board,
            ships,
            terminal,
            seed,
            prng: Prng::new(seed),
            turn: Turn::new(),
            status: ScenarioStatus::InProgress,
            winner: None,
            credits: BTreeMap::from([(SideId::A, 100), (SideId::B, 100)]),
            catalog,
            phase: Phase::Allocate,
            allocated_this_turn: HashSet::new(),
            staged_allocations: BTreeMap::new(),
            path_commits: BTreeMap::new(),
            volley_commits: BTreeMap::new(),
            fired_weapons_this_turn: HashSet::new(),
            combat_log: Vec::new(),
            npcs,
            path_results: Vec::new(),
            rules,
            squads: squad_defs
                .into_iter()
                .map(|(id, (leader, members))| (id, SquadState { leader, members }))
                .collect(),
        };
        state.reset_all_power();
        state.refresh_status();
        state
    }

    pub fn status(&self) -> ScenarioStatus {
        self.status
    }

    pub fn winner(&self) -> Option<SideId> {
        self.winner
    }

    pub fn credits(&self) -> BTreeMap<SideId, u32> {
        self.credits.clone()
    }

    pub fn purchase_catalog(&self) -> Vec<crate::snapshot::PurchaseOption> {
        self.catalog
            .iter()
            .map(|(class, ship)| crate::snapshot::PurchaseOption { class: class.clone(), cost: ship.cost })
            .collect()
    }

    pub fn purchase(
        &mut self,
        side: SideId,
        class: &str,
    ) -> Result<(), crate::movement::OrderError> {
        if self.phase != Phase::Allocate {
            return Err(crate::movement::OrderError::PurchaseWrongPhase);
        }
        if class == "shipyard" {
            return Err(crate::movement::OrderError::NotPurchasable(class.to_string()));
        }
        let template = self
            .catalog
            .get(class)
            .cloned()
            .ok_or_else(|| crate::movement::OrderError::UnknownPurchaseClass(class.to_string()))?;
        let have = self.credits.get(&side).copied().unwrap_or(0);
        let cost = template.cost;
        if have < cost {
            return Err(crate::movement::OrderError::InsufficientCredits {
                side,
                class: class.to_string(),
                need: cost,
                have,
            });
        }
        let yard = self
            .ships
            .iter()
            .find(|ship| ship.side == side && ship.class_id == "shipyard" && !ship.destroyed)
            .map(|ship| ship.pos)
            .ok_or(crate::movement::OrderError::SpawnBlocked { side })?;
        let center = Hex::new(self.board.width as i32 / 2, self.board.height as i32 / 2);
        let spawn = std::iter::once(yard)
            .chain(yard.neighbors())
            .filter(|hex| self.board.mode == crate::board::MapMode::Unbounded || self.board.contains(*hex))
            .find(|hex| !self.is_occupied_by_other(0, *hex))
            .filter(|hex| *hex != yard)
            .ok_or(crate::movement::OrderError::SpawnBlocked { side })?;
        let mut ship = template;
        ship.id = self.ships.iter().map(|s| s.id).max().unwrap_or(0).saturating_add(1);
        ship.side = side;
        ship.pos = spawn;
        ship.facing = Hex::facing_between(spawn, center).unwrap_or(0);
        ship.reset_v2_allocation();
        self.ships.push(ship);
        self.credits.insert(side, have - cost);
        if side == SideId::B {
            self.npcs.insert(self.ships.last().unwrap().id, NpcController::GreedySeek);
        }
        Ok(())
    }

    pub fn rules_fingerprint(&self) -> &str {
        self.rules.fingerprint()
    }

    pub fn rules_id(&self) -> &str {
        self.rules.id()
    }

    /// The immutable ruleset governing this match.
    pub fn rules(&self) -> &Ruleset {
        &self.rules
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn ships_committed_path(&self) -> Vec<u32> {
        self.path_commits.keys().copied().collect()
    }

    pub fn ships_committed_volley(&self) -> Vec<u32> {
        self.volley_commits.keys().copied().collect()
    }

    pub fn allocated_this_turn(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.allocated_this_turn.iter().copied().collect();
        ids.sort_unstable();
        // Also count staged-but-not-yet-applied during allocate barrier.
        for id in self.staged_allocations.keys() {
            if !ids.contains(id) {
                ids.push(*id);
            }
        }
        ids.sort_unstable();
        ids
    }

    pub fn turn_number(&self) -> u32 {
        self.turn.number()
    }

    pub fn path_results(&self) -> &[PathResult] {
        &self.path_results
    }


    pub fn reset_all_power(&mut self) {
        if self.turn.number() > 1 {
            for side in [SideId::A, SideId::B] {
                let current = self.credits.get(&side).copied().unwrap_or(0);
                self.credits.insert(side, current.saturating_add(100));
            }
        }
        for s in &mut self.ships {
            if !s.destroyed {
                s.reset_v2_allocation();
            }
        }
        self.phase = Phase::Allocate;
        self.allocated_this_turn.clear();
        self.staged_allocations.clear();
        self.path_commits.clear();
        self.volley_commits.clear();
        self.fired_weapons_this_turn.clear();
        // Retain `combat_log` until the next volley resolution so clients and
        // simulation metrics can observe ordinary hit/miss events after the
        // automatic turn advance (ADR-0025 observability).
    }

    pub fn squads(&self) -> &BTreeMap<u32, SquadState> {
        &self.squads
    }

    pub fn has_committed_path(&self, ship: u32) -> bool {
        self.path_commits.contains_key(&ship)
    }

    pub fn has_committed_volley(&self, ship: u32) -> bool {
        self.volley_commits.contains_key(&ship)
    }

    fn hard_bounds(&self) -> Option<MapBounds> {
        if self.board.mode.blocks_edges() {
            Some(MapBounds {
                width: self.board.width,
                height: self.board.height,
            })
        } else {
            None
        }
    }

    fn v2_projected_damage(
        &self,
        attacker: &Ship,
        weapon_id: &str,
        _kind: crate::combat_tables::WeaponKind,
        range: u32,
    ) -> Result<u32, crate::movement::OrderError> {
        let weapon = attacker
            .weapon(weapon_id)
            .ok_or_else(|| crate::movement::OrderError::WeaponNotFound(weapon_id.to_string()))?;
        let charge = attacker.weapon_charges.get(weapon_id).copied().unwrap_or(0);
        if weapon.repeat {
            return Ok((0..charge)
                .map(|_| self.v2_packet_damage(attacker, weapon, range, 1))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .sum());
        }
        self.v2_packet_damage(attacker, weapon, range, charge)
    }

    fn v2_packet_damage(
        &self,
        attacker: &Ship,
        weapon: &combat::Weapon,
        range: u32,
        charge: u32,
    ) -> Result<u32, crate::movement::OrderError> {
        match weapon.kind {
            crate::combat_tables::WeaponKind::Beam => {
                crate::combat_tables::beam_damage(self.rules.combat(), charge, range)
                    .map(|damage| {
                        if weapon.pierce {
                            damage.div_ceil(2)
                        } else {
                            damage
                        }
                        .saturating_add(weapon.damage_bonus)
                    })
                    .ok_or_else(|| crate::movement::OrderError::NoDamage {
                        weapon: weapon.id.clone(),
                        range,
                        charge,
                    })
            }
            crate::combat_tables::WeaponKind::Plasma => {
                crate::combat_tables::plasma_damage(self.rules.combat(), range)
                    .map(|damage| damage.saturating_add(weapon.damage_bonus))
                    .ok_or_else(|| crate::movement::OrderError::OutOfRange {
                        weapon: weapon.id.clone(),
                        range,
                        max_range: attacker.weapon(&weapon.id).map_or_else(
                            || self.rules.max_range(weapon.kind),
                            |installed| self.effective_weapon_max_range(installed),
                        ),
                    })
            }
            crate::combat_tables::WeaponKind::Torp => {
                crate::combat_tables::torp_damage(self.rules.combat(), range)
                    .map(|damage| damage.saturating_add(weapon.damage_bonus))
                    .ok_or_else(|| crate::movement::OrderError::OutOfRange {
                        weapon: weapon.id.clone(),
                        range,
                        max_range: attacker.weapon(&weapon.id).map_or_else(
                            || self.rules.max_range(weapon.kind),
                            |installed| self.effective_weapon_max_range(installed),
                        ),
                    })
            }
            crate::combat_tables::WeaponKind::Missile => {
                Ok(2u32.saturating_add(weapon.damage_bonus))
            }
            crate::combat_tables::WeaponKind::Pd | crate::combat_tables::WeaponKind::Graviton => {
                Err(crate::movement::OrderError::OutOfRange {
                    weapon: weapon.id.clone(),
                    range,
                    max_range: attacker.weapon(&weapon.id).map_or_else(
                        || self.rules.max_range(weapon.kind),
                        |installed| self.effective_weapon_max_range(installed),
                    ),
                })
            }
        }
    }

    fn apply_v2_damage(&mut self, target: u32, shield_facing: u8, damage: u32) -> (u32, u32) {
        let rules = self.rules.clone();
        let Some(ship) = self.ship_mut(target) else {
            return (0, 0);
        };
        let facing = (shield_facing % 6) as usize;
        let absorbed = ship.shields_remaining[facing].min(damage);
        ship.shields_remaining[facing] -= absorbed;
        let overflow = damage - absorbed;
        if overflow > 0 {
            ship.ssd.apply_internal(overflow, rules.dac());
            ship.destroyed = ship.ssd.is_destroyed();
        }
        (absorbed, overflow)
    }

    fn apply_v2_pierce_damage(&mut self, target: u32, damage: u32) -> (u32, u32) {
        let rules = self.rules.clone();
        let Some(ship) = self.ship_mut(target) else {
            return (0, 0);
        };
        let before = ship.ssd.hull;
        ship.ssd.apply_internal(damage, rules.dac());
        ship.destroyed = ship.ssd.is_destroyed();
        (0, before.saturating_sub(ship.ssd.hull))
    }

    fn effective_weapon_max_range(&self, weapon: &combat::Weapon) -> u32 {
        self.rules.max_range(weapon.kind).min(weapon.max_range)
    }

    pub fn phase_name(&self) -> &'static str {
        match self.phase {
            Phase::Allocate => "allocate",
            Phase::Movement => "movement",
            Phase::Firing => "firing",
        }
    }

    fn is_v2_ai(&self, ship_id: u32) -> bool {
        matches!(self.npc(ship_id), Some(NpcController::GreedySeek))
    }

    pub fn has_player_ship(&self) -> bool {
        self.ships.iter().any(|ship| !self.is_v2_ai(ship.id))
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn prng_state(&self) -> u64 {
        self.prng.state()
    }

    pub(crate) fn reseed(&mut self, seed: u64) {
        self.seed = seed;
        self.prng = Prng::new(seed);
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn terminal(&self) -> Option<Terminal> {
        self.terminal
    }

    pub fn objective(&self) -> Option<Hex> {
        match self.terminal {
            Some(Terminal::ReachHex(hex)) => Some(hex),
            _ => None,
        }
    }

    pub fn destruction_target(&self) -> Option<u32> {
        match self.terminal {
            Some(Terminal::DestroyShip(id)) => Some(id),
            Some(Terminal::DestroyShips { side_a_target, .. }) => Some(side_a_target),
            _ => None,
        }
    }

    pub fn controller_label(&self, ship_id: u32) -> &'static str {
        match self.npc(ship_id) {
            None => "player",
            Some(NpcController::GreedySeek) => "ai",
            Some(NpcController::Scripted) => "scripted",
        }
    }

    pub fn ship(&self, id: u32) -> Option<&Ship> {
        self.ships.iter().find(|ship| ship.id == id)
    }

    pub fn ships(&self) -> &[Ship] {
        &self.ships
    }

    pub fn is_occupied_by_other(&self, moving_ship: u32, hex: Hex) -> bool {
        self.ships
            .iter()
            .any(|ship| ship.id != moving_ship && !ship.destroyed && ship.pos == hex)
    }

    pub fn weapon_fired_this_turn(&self, ship: u32, weapon_id: &str) -> bool {
        self.fired_weapons_this_turn
            .contains(&(ship, weapon_id.to_string()))
    }

    pub fn ship_owns_weapon(&self, ship_id: u32, weapon_id: &str) -> bool {
        self.ship(ship_id)
            .is_some_and(|ship| !ship.destroyed && ship.weapon(weapon_id).is_some())
    }

    pub fn set_ship_pos(&mut self, id: u32, pos: Hex) -> Result<(), StateError> {
        let ship = self.ship_mut(id).ok_or(StateError::ShipNotFound(id))?;
        ship.pos = pos;
        Ok(())
    }

    pub fn set_ship_facing(&mut self, id: u32, facing: u8) -> Result<(), StateError> {
        if !Hex::is_valid_facing(facing) {
            return Err(StateError::InvalidFacing(facing));
        }
        let ship = self.ship_mut(id).ok_or(StateError::ShipNotFound(id))?;
        ship.facing = facing;
        Ok(())
    }

    pub fn set_ship_structure(&mut self, id: u32, structure: u32) -> Result<(), StateError> {
        let ship = self.ship_mut(id).ok_or(StateError::ShipNotFound(id))?;
        ship.set_structure(structure);
        Ok(())
    }

    pub(crate) fn ship_mut(&mut self, id: u32) -> Option<&mut Ship> {
        self.ships.iter_mut().find(|ship| ship.id == id)
    }

    pub(crate) fn advance_turn_counter(&mut self) {
        self.turn.advance();
    }

    pub fn combat_log(&self) -> &[CombatLogEvent] {
        &self.combat_log
    }

    pub(crate) fn npc(&self, ship_id: u32) -> Option<&NpcController> {
        self.npcs.get(&ship_id)
    }

    pub fn refresh_status(&mut self) {
        let destroyed: std::collections::HashSet<u32> = self
            .ships
            .iter()
            .filter(|ship| ship.destroyed)
            .map(|ship| ship.id)
            .collect();
        let empty_squads: Vec<u32> = self
            .squads
            .iter_mut()
            .filter_map(|(id, squad)| {
                squad.members.retain(|member| !destroyed.contains(member));
                if squad.members.is_empty() {
                    Some(*id)
                } else {
                    if !squad.members.contains(&squad.leader) {
                        squad.leader = *squad.members.iter().min().unwrap();
                    }
                    None
                }
            })
            .collect();
        for id in empty_squads {
            self.squads.remove(&id);
        }
        let player_ships: Vec<_> = self
            .ships
            .iter()
            .filter(|ship| !self.npcs.contains_key(&ship.id))
            .collect();
        if !player_ships.is_empty()
            && !player_ships.iter().any(|ship| !ship.destroyed)
            && !matches!(self.terminal, Some(Terminal::DestroyShips { .. }))
        {
            self.status = ScenarioStatus::Lost;
            return;
        }
        self.status = match self.terminal {
            Some(Terminal::ReachHex(objective)) => {
                if self.ships.iter().any(|ship| ship.pos == objective) {
                    ScenarioStatus::Won
                } else {
                    ScenarioStatus::InProgress
                }
            }
            Some(Terminal::DestroyShip(target)) => {
                if self
                    .ships
                    .iter()
                    .any(|ship| ship.id == target && ship.destroyed)
                {
                    ScenarioStatus::Won
                } else {
                    ScenarioStatus::InProgress
                }
            }
            Some(Terminal::DestroyShips { side_a_target, side_b_target }) => {
                let a_dead = self.ships.iter().any(|s| s.id == side_a_target && s.destroyed);
                let b_dead = self.ships.iter().any(|s| s.id == side_b_target && s.destroyed);
                self.winner = if b_dead { Some(SideId::A) } else if a_dead { Some(SideId::B) } else { None };
                if self.winner.is_some() { ScenarioStatus::Won } else { ScenarioStatus::InProgress }
            }
            Some(Terminal::AnnihilateEnemies) => {
                let enemies: Vec<_> = self
                    .ships
                    .iter()
                    .filter(|ship| self.npcs.contains_key(&ship.id))
                    .collect();
                if !enemies.is_empty() && enemies.iter().all(|ship| ship.destroyed) {
                    ScenarioStatus::Won
                } else {
                    ScenarioStatus::InProgress
                }
            }
            None => ScenarioStatus::InProgress,
        };
    }
}

#[derive(Debug, Clone)]
pub enum NpcController {
    Scripted,
    GreedySeek,
}

#[cfg(test)]
mod ammo_ai_tests {
    use super::*;
    use crate::scenario::load_scenario_def;
    use crate::schema::ScenarioDef;
    use std::path::PathBuf;

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    /// Dry torpedo magazines must not soft-lock the AI barrier loop.
    #[test]
    fn resolve_v2_npc_actions_continues_with_dry_torpedo() {
        let dir = tempfile::tempdir().unwrap();
        let ships = dir.path().join("data/ships");
        std::fs::create_dir_all(&ships).unwrap();
        let rules_dst = dir.path().join("data/rules");
        std::fs::create_dir_all(&rules_dst).unwrap();
        std::fs::copy(
            root().join("data/rules/default.toml"),
            rules_dst.join("default.toml"),
        )
        .unwrap();
        std::fs::write(
            ships.join("ai_torp.toml"),
            r#"
id = "ai_torp"
name = "AI Torp"
size = 2
max_maneuver_actions = 4
power = 16
max_shield_per_facing = 4
structure = 12
power_sys = 2
engine_boxes = 2
thrust_per_power = 1
power_per_thrust = 1
weapon_boxes = 1
[[weapons]]
id = "torp_1"
kind = "torp"
arc = "forward"
mount = "forward"
max_range = 12
max_charge = 1
max_ammo = 1
"#,
        )
        .unwrap();
        std::fs::write(
            ships.join("ai_target.toml"),
            r#"
id = "ai_target"
name = "AI Target"
size = 2
max_maneuver_actions = 4
power = 12
max_shield_per_facing = 4
structure = 20
power_sys = 2
engine_boxes = 2
thrust_per_power = 1
power_per_thrust = 1
weapon_boxes = 1
[[weapons]]
id = "beam_1"
kind = "beam"
arc = "forward"
mount = "forward"
max_range = 10
max_charge = 2
"#,
        )
        .unwrap();

        let def: ScenarioDef = toml::from_str(
            r#"
width = 12
height = 12
seed = 7
[[ships]]
id = 1
class = "ai_torp"
q = 0
r = 0
facing = 0
controller = "ai"
[[ships]]
id = 2
class = "ai_target"
q = 3
r = 0
facing = 3
controller = "ai"
"#,
        )
        .unwrap();
        let mut game = load_scenario_def(&def, dir.path()).expect("load");
        // Force the soft-lock case: magazine already empty before first allocate.
        game.ship_mut(1)
            .unwrap()
            .weapon_ammo
            .insert("torp_1".into(), 0);

        let start_turn = game.turn_number();
        for _ in 0..8 {
            let applied = game.resolve_v2_npc_actions();
            assert!(
                !applied.is_empty() || game.status != ScenarioStatus::InProgress,
                "stalled at turn {}",
                game.turn_number()
            );
            if game.status != ScenarioStatus::InProgress {
                break;
            }
        }
        assert!(
            game.turn_number() > start_turn,
            "turn must advance with dry magazines"
        );
    }
}

#[cfg(test)]
mod economy_tests {
    use crate::movement::{apply_order, Order, OrderError};
    use crate::scenario::load_scenario;
    use crate::schema::SideId;
    use crate::snapshot::StateSnapshot;
    use std::path::Path;

    #[test]
    fn credits_start_at_one_income_and_accrue_at_turn_start() {
        let mut game = load_scenario(Path::new("scenarios/shipyard_assault.toml")).unwrap();
        assert_eq!(game.credits().get(&SideId::A), Some(&100));
        apply_order(&mut game, Order::Purchase { side: SideId::A, class: "basic_swarm".into() }).unwrap();
        assert_eq!(game.credits().get(&SideId::A), Some(&56));
        game.advance_turn_counter();
        game.reset_all_power();
        assert_eq!(game.credits().get(&SideId::A), Some(&156));
    }

    #[test]
    fn purchase_catalog_is_snapshot_data_and_excludes_shipyard() {
        let game = load_scenario(Path::new("scenarios/shipyard_assault.toml")).unwrap();
        let snapshot = StateSnapshot::from_game_state(&game);
        assert!(snapshot.purchasable.iter().any(|entry| entry.class == "basic_swarm"));
        assert!(!snapshot.purchasable.iter().any(|entry| entry.class == "shipyard"));
        let mut game2 = game;
        let error = apply_order(&mut game2, Order::Purchase { side: SideId::A, class: "not_a_class".into() }).unwrap_err();
        assert!(matches!(error, OrderError::UnknownPurchaseClass(_)));
    }
}
