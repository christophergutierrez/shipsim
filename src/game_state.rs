use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;

use crate::board::Board;
use crate::combat::{self, WeaponKind};
use crate::hex::Hex;
use crate::prng::Prng;
use crate::ship::Ship;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ScenarioStatus {
    InProgress,
    Won,
}

/// Win condition. Objective and destruction are mutually exclusive (AS1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminal {
    ReachHex(Hex),
    DestroyShip(u32),
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

#[derive(Debug, Clone)]
struct PlotState {
    path: Vec<Hex>,
    cursor: usize,
}

#[derive(Debug, Clone)]
struct PendingFire {
    ship: u32,
    weapon: String,
    target: u32,
}

/// Encapsulated game aggregate. Storage is private; mutate via orders (`movement::apply_order`)
/// or explicit setup APIs — never by poking internal collections.
#[derive(Debug, Clone)]
pub struct GameState {
    board: Board,
    ships: Vec<Ship>,
    terminal: Option<Terminal>,
    seed: u64,
    prng: Prng,
    turn: Turn,
    /// 0 between turns; 1..=32 only during an in-progress RunTurn.
    impulse: u8,
    status: ScenarioStatus,
    /// Keys are (ship_id, weapon_id) for multi-firer safety (TS2).
    fired_weapons_this_turn: HashSet<(u32, String)>,
    plots: HashMap<u32, PlotState>,
    pending_fires: Vec<PendingFire>,
    /// BTreeMap so multi-scripted iteration is deterministic (T1).
    scripted_plans: BTreeMap<u32, ScriptedPlan>,
}

impl GameState {
    pub fn new(board: Board, ships: Vec<Ship>, objective: Hex) -> Self {
        Self::new_with_options(
            board,
            ships,
            Some(Terminal::ReachHex(objective)),
            BTreeMap::new(),
            1,
        )
    }

    pub(crate) fn new_with_options(
        board: Board,
        ships: Vec<Ship>,
        terminal: Option<Terminal>,
        scripted_plans: BTreeMap<u32, ScriptedPlan>,
        seed: u64,
    ) -> Self {
        let mut state = Self {
            board,
            ships,
            terminal,
            seed,
            prng: Prng::new(seed),
            turn: Turn::new(),
            impulse: 0,
            status: ScenarioStatus::InProgress,
            fired_weapons_this_turn: HashSet::new(),
            plots: HashMap::new(),
            pending_fires: Vec::new(),
            scripted_plans,
        };
        state.refresh_status();
        state
    }

    // ----- public reads -----

    pub fn status(&self) -> ScenarioStatus {
        self.status
    }

    pub fn impulse(&self) -> u8 {
        self.impulse
    }

    pub fn turn_number(&self) -> u32 {
        self.turn.number()
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// PRNG stream position for mid-game resume hooks (TS3).
    pub fn prng_state(&self) -> u64 {
        self.prng.state()
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
            _ => None,
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

    /// Whether `ship` owns a non-destroyed weapon with this id.
    pub fn ship_owns_weapon(&self, ship_id: u32, weapon_id: &str) -> bool {
        self.ship(ship_id)
            .is_some_and(|ship| !ship.destroyed && ship.weapon(weapon_id).is_some())
    }

    // ----- public setup mutators -----

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

    pub fn set_ship_shields(&mut self, id: u32, shields: [u32; 6]) -> Result<(), StateError> {
        let ship = self.ship_mut(id).ok_or(StateError::ShipNotFound(id))?;
        ship.shields = shields;
        Ok(())
    }

    pub fn set_ship_structure(&mut self, id: u32, structure: u32) -> Result<(), StateError> {
        let ship = self.ship_mut(id).ok_or(StateError::ShipNotFound(id))?;
        ship.structure = structure;
        ship.destroyed = structure == 0;
        Ok(())
    }

    pub fn configure_weapon_exact_damage(
        &mut self,
        ship_id: u32,
        weapon_id: &str,
        damage: u32,
    ) -> Result<(), StateError> {
        let ship = self
            .ship_mut(ship_id)
            .ok_or(StateError::ShipNotFound(ship_id))?;
        let weapon = ship.weapon_mut(weapon_id).ok_or(StateError::WeaponNotFound {
            ship: ship_id,
            weapon: weapon_id.to_string(),
        })?;
        weapon.kind = WeaponKind::Disruptor;
        weapon.damage = damage;
        weapon.to_hit_by_range = vec![6];
        weapon.phaser_dice_by_range.clear();
        Ok(())
    }

    pub fn configure_weapon_max_range(
        &mut self,
        ship_id: u32,
        weapon_id: &str,
        max_range: u32,
    ) -> Result<(), StateError> {
        let ship = self
            .ship_mut(ship_id)
            .ok_or(StateError::ShipNotFound(ship_id))?;
        let weapon = ship.weapon_mut(weapon_id).ok_or(StateError::WeaponNotFound {
            ship: ship_id,
            weapon: weapon_id.to_string(),
        })?;
        weapon.max_range = max_range;
        Ok(())
    }

    pub fn configure_weapon_as_disruptor(
        &mut self,
        ship_id: u32,
        weapon_id: &str,
        damage: u32,
        to_hit_by_range: Vec<u32>,
    ) -> Result<(), StateError> {
        let ship = self
            .ship_mut(ship_id)
            .ok_or(StateError::ShipNotFound(ship_id))?;
        let weapon = ship.weapon_mut(weapon_id).ok_or(StateError::WeaponNotFound {
            ship: ship_id,
            weapon: weapon_id.to_string(),
        })?;
        weapon.kind = WeaponKind::Disruptor;
        weapon.damage = damage;
        weapon.to_hit_by_range = to_hit_by_range;
        weapon.phaser_dice_by_range.clear();
        Ok(())
    }

    // ----- crate-internal -----

    pub(crate) fn ship_mut(&mut self, id: u32) -> Option<&mut Ship> {
        self.ships.iter_mut().find(|ship| ship.id == id)
    }

    pub(crate) fn set_impulse(&mut self, impulse: u8) {
        self.impulse = impulse;
    }

    pub(crate) fn advance_turn_counter(&mut self) {
        self.turn.advance();
    }

    pub(crate) fn ship_ids(&self) -> Vec<u32> {
        self.ships.iter().map(|s| s.id).collect()
    }

    pub(crate) fn alive_positions(&self) -> HashMap<u32, Hex> {
        self.ships
            .iter()
            .filter(|s| !s.destroyed)
            .map(|s| (s.id, s.pos))
            .collect()
    }

    pub(crate) fn apply_ship_step(&mut self, ship_id: u32, pos: Hex, facing: u8) {
        if let Some(ship) = self.ship_mut(ship_id) {
            ship.pos = pos;
            ship.facing = facing;
        }
    }

    pub(crate) fn store_plot(&mut self, ship_id: u32, path: Vec<Hex>) {
        self.plots.insert(
            ship_id,
            PlotState {
                path,
                cursor: 0,
            },
        );
    }

    pub(crate) fn queue_fire(&mut self, ship: u32, weapon: String, target: u32) {
        self.fired_weapons_this_turn
            .insert((ship, weapon.clone()));
        self.pending_fires.push(PendingFire {
            ship,
            weapon,
            target,
        });
    }

    pub(crate) fn has_plot(&self, ship_id: u32) -> bool {
        self.plots.contains_key(&ship_id)
    }

    pub(crate) fn plot_next_step(&self, ship_id: u32) -> Option<(Hex, u8)> {
        let ship = self.ship(ship_id)?;
        let plot = self.plots.get(&ship_id)?;
        if plot.cursor >= plot.path.len() {
            return None;
        }
        let next = plot.path[plot.cursor];
        let facing = Hex::facing_between(ship.pos, next).unwrap_or(ship.facing);
        Some((next, facing))
    }

    pub(crate) fn advance_plot_cursor(&mut self, ship_id: u32) {
        if let Some(plot) = self.plots.get_mut(&ship_id) {
            plot.cursor += 1;
        }
    }

    pub(crate) fn abort_plot(&mut self, ship_id: u32) {
        self.plots.remove(&ship_id);
    }

    pub(crate) fn take_pending_fires(&mut self) -> Vec<(u32, String, u32)> {
        std::mem::take(&mut self.pending_fires)
            .into_iter()
            .map(|f| (f.ship, f.weapon, f.target))
            .collect()
    }

    pub(crate) fn clear_turn_ephemera(&mut self) {
        self.plots.clear();
        self.fired_weapons_this_turn.clear();
        self.pending_fires.clear();
    }

    pub(crate) fn scripted_ship_ids(&self) -> Vec<u32> {
        self.scripted_plans.keys().copied().collect()
    }

    pub(crate) fn scripted_waypoint_view(&self, ship_id: u32) -> Option<(usize, &[Hex])> {
        let plan = self.scripted_plans.get(&ship_id)?;
        Some((plan.next_waypoint, plan.waypoints.as_slice()))
    }

    pub(crate) fn apply_fire(&mut self, ship_id: u32, weapon_id: &str, target_id: u32) {
        let Some(attacker) = self.ship(ship_id).cloned() else {
            return;
        };
        if attacker.destroyed || attacker.weapon(weapon_id).is_none() {
            return;
        }
        let Some(target) = self.ships.iter_mut().find(|ship| ship.id == target_id) else {
            return;
        };
        let _ = combat::resolve_fire(&attacker, weapon_id, target, &mut self.prng);
    }

    pub fn refresh_status(&mut self) {
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
            None => ScenarioStatus::InProgress,
        };
    }

    pub(crate) fn sync_scripted_waypoints(&mut self) {
        let scripted_ids: Vec<u32> = self.scripted_plans.keys().copied().collect();
        for ship_id in scripted_ids {
            let Some(current) = self.ship(ship_id).map(|s| s.pos) else {
                continue;
            };
            let Some(plan) = self.scripted_plans.get_mut(&ship_id) else {
                continue;
            };
            while let Some(offset) = plan.waypoints[plan.next_waypoint..]
                .iter()
                .position(|waypoint| *waypoint == current)
            {
                plan.next_waypoint += offset + 1;
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ScriptedPlan {
    waypoints: Vec<Hex>,
    next_waypoint: usize,
}

impl ScriptedPlan {
    pub(crate) fn new(waypoints: Vec<Hex>) -> Self {
        Self {
            waypoints,
            next_waypoint: 0,
        }
    }
}
