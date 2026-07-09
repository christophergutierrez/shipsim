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

/// In-flight seeking munition (D5a drone).
#[derive(Debug, Clone)]
pub struct SeekingMunition {
    pub id: u32,
    pub owner: u32,
    pub weapon_id: String,
    pub pos: Hex,
    pub target: u32,
    pub damage: u32,
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
    seeking: Vec<SeekingMunition>,
    next_seeking_id: u32,
    /// Non-player controllers (scripted waypoints or AI). BTreeMap = deterministic order (T1).
    npcs: BTreeMap<u32, NpcController>,
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
        npcs: BTreeMap<u32, NpcController>,
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
            seeking: Vec::new(),
            next_seeking_id: 1,
            npcs,
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
        ship.set_structure(structure);
        Ok(())
    }

    /// Test/scenario helper: set remaining SSD boxes for a weapon by id.
    pub fn set_weapon_boxes(
        &mut self,
        ship_id: u32,
        weapon_id: &str,
        boxes: u32,
    ) -> Result<(), StateError> {
        let ship = self
            .ship_mut(ship_id)
            .ok_or(StateError::ShipNotFound(ship_id))?;
        let idx = ship
            .weapons
            .iter()
            .position(|w| w.id == weapon_id)
            .ok_or(StateError::WeaponNotFound {
                ship: ship_id,
                weapon: weapon_id.to_string(),
            })?;
        if let Some(slot) = ship.ssd.weapon_boxes.get_mut(idx) {
            *slot = boxes;
        }
        Ok(())
    }

    /// Test helper: raise max speed / power and re-default this turn's energy buckets.
    pub fn set_ship_power_profile(
        &mut self,
        id: u32,
        max_speed: u32,
        power: u32,
    ) -> Result<(), StateError> {
        let ship = self.ship_mut(id).ok_or(StateError::ShipNotFound(id))?;
        ship.speed = max_speed;
        ship.power = power;
        ship.reset_turn_energy();
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

    pub(crate) fn allocate_energy(
        &mut self,
        ship_id: u32,
        movement: u32,
        weapons: u32,
        shields: u32,
    ) {
        if let Some(ship) = self.ship_mut(ship_id) {
            ship.apply_allocation(movement, weapons, shields);
        }
    }

    pub(crate) fn reset_all_turn_energy(&mut self) {
        for ship in &mut self.ships {
            ship.reset_turn_energy();
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
        if let Some(s) = self.ship_mut(ship) {
            // Energy was checked at declare; spend on resolve.
            let _ = s.spend_fire_energy();
        }
        self.fired_weapons_this_turn
            .insert((ship, weapon.clone()));

        // Seeking weapons launch a munition immediately; direct-fire waits for IFF windows.
        let is_seek = self
            .ship(ship)
            .and_then(|s| s.weapons.iter().find(|w| w.id == weapon))
            .map(|w| combat::is_seeking(&w.kind))
            .unwrap_or(false);
        if is_seek {
            self.launch_seeking(ship, weapon, target);
        } else {
            self.pending_fires.push(PendingFire {
                ship,
                weapon,
                target,
            });
        }
    }

    pub fn seeking_munitions(&self) -> &[SeekingMunition] {
        &self.seeking
    }

    fn launch_seeking(&mut self, owner: u32, weapon_id: String, target: u32) {
        let Some(launcher) = self.ship(owner) else {
            return;
        };
        let Some(w) = launcher.weapons.iter().find(|w| w.id == weapon_id) else {
            return;
        };
        let damage = w.damage;
        let pos = launcher.pos;
        let id = self.next_seeking_id;
        self.next_seeking_id = self.next_seeking_id.saturating_add(1);
        self.seeking.push(SeekingMunition {
            id,
            owner,
            weapon_id,
            pos,
            target,
            damage,
        });
    }

    /// Move each seeking munition one hex toward its target; impact applies damage.
    pub(crate) fn advance_seeking_munitions(&mut self) {
        let mut next: Vec<SeekingMunition> = Vec::new();
        let munitions = std::mem::take(&mut self.seeking);
        for mut m in munitions {
            let Some(target) = self.ship(m.target).cloned() else {
                continue; // target gone
            };
            if target.destroyed {
                continue;
            }
            if m.pos == target.pos {
                // Already on hex (rare): detonate.
                let facing = 0usize;
                if let Some(t) = self.ship_mut(m.target) {
                    t.apply_hit(facing, m.damage);
                }
                continue;
            }
            // Greedy one-hex step toward target.
            let mut best: Option<(u8, Hex)> = None;
            for facing in 0u8..=5 {
                let Some(delta) = Hex::direction(facing) else {
                    continue;
                };
                let candidate = m.pos + delta;
                if !self.board.contains(candidate) {
                    continue;
                }
                if candidate.distance(target.pos) >= m.pos.distance(target.pos) {
                    continue;
                }
                match best {
                    None => best = Some((facing, candidate)),
                    Some((bf, _)) if facing < bf => best = Some((facing, candidate)),
                    _ => {}
                }
            }
            let Some((approach_facing, step)) = best else {
                // Stuck: keep munition for a later impulse (target may move).
                next.push(m);
                continue;
            };
            m.pos = step;
            if m.pos == target.pos {
                let shield = approach_facing as usize;
                if let Some(t) = self.ship_mut(m.target) {
                    t.apply_hit(shield, m.damage);
                }
                // munition expended
            } else {
                next.push(m);
            }
        }
        self.seeking = next;
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

    /// Drain pending fires whose weapon class may fire on `impulse` (D1-fire).
    /// Others remain pending for a later impulse.
    pub(crate) fn drain_fires_for_impulse(&mut self, impulse: u8) -> Vec<(u32, String, u32)> {
        let pending = std::mem::take(&mut self.pending_fires);
        let mut ready = Vec::new();
        let mut keep = Vec::new();
        for fire in pending {
            let can = self
                .ship(fire.ship)
                .and_then(|s| s.weapon(&fire.weapon))
                .map(|w| combat::fires_on_impulse(&w.kind, impulse))
                .unwrap_or(false);
            if can {
                ready.push((fire.ship, fire.weapon, fire.target));
            } else {
                keep.push(fire);
            }
        }
        self.pending_fires = keep;
        ready
    }

    pub(crate) fn discard_pending_fires(&mut self) {
        self.pending_fires.clear();
    }

    pub(crate) fn clear_turn_ephemera(&mut self) {
        self.plots.clear();
        self.fired_weapons_this_turn.clear();
        self.pending_fires.clear();
    }

    pub(crate) fn npc_ids(&self) -> Vec<u32> {
        self.npcs.keys().copied().collect()
    }

    pub(crate) fn npc(&self, ship_id: u32) -> Option<&NpcController> {
        self.npcs.get(&ship_id)
    }

    pub(crate) fn scripted_waypoint_view(&self, ship_id: u32) -> Option<(usize, &[Hex])> {
        match self.npcs.get(&ship_id)? {
            NpcController::Scripted(plan) => {
                Some((plan.next_waypoint, plan.waypoints.as_slice()))
            }
            NpcController::GreedySeek => None,
        }
    }

    /// D2-fire: resolve a batch of ready shots simultaneously.
    ///
    /// 1. Sort by ship id (then weapon, target) for deterministic PRNG order.
    /// 2. Legality + damage rolls use a frozen pre-fire ship snapshot (mutual kill possible).
    /// 3. Apply all hits after computing every shot.
    pub(crate) fn resolve_simultaneous_fires(&mut self, mut ready: Vec<(u32, String, u32)>) {
        ready.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
        });

        let snapshot: std::collections::HashMap<u32, Ship> =
            self.ships.iter().map(|s| (s.id, s.clone())).collect();

        let mut hits: Vec<combat::FireHit> = Vec::new();
        for (ship_id, weapon_id, target_id) in ready {
            let Some(attacker) = snapshot.get(&ship_id) else {
                continue;
            };
            // Destroyed before this fire phase cannot shoot; mutual fire still allowed.
            if attacker.destroyed {
                continue;
            }
            let Some(target) = snapshot.get(&target_id) else {
                continue;
            };
            if combat::fire_legality(attacker, &weapon_id, target).is_err() {
                continue;
            }
            if let Some(hit) =
                combat::compute_fire(attacker, &weapon_id, target, &mut self.prng)
            {
                hits.push(hit);
            }
        }

        for hit in hits {
            if let Some(target) = self.ships.iter_mut().find(|s| s.id == hit.target) {
                target.apply_hit(hit.shield, hit.damage);
            }
        }
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
        let scripted_ids: Vec<u32> = self
            .npcs
            .iter()
            .filter_map(|(id, c)| match c {
                NpcController::Scripted(_) => Some(*id),
                NpcController::GreedySeek => None,
            })
            .collect();
        for ship_id in scripted_ids {
            let Some(current) = self.ship(ship_id).map(|s| s.pos) else {
                continue;
            };
            let Some(NpcController::Scripted(plan)) = self.npcs.get_mut(&ship_id) else {
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
pub(crate) enum NpcController {
    Scripted(ScriptedPlan),
    GreedySeek,
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
