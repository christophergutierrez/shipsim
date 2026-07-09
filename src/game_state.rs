use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::board::Board;
use crate::hex::Hex;
use crate::prng::Prng;
use crate::ship::Ship;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ScenarioStatus {
    InProgress,
    Won,
}

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
    weapon: String,
    target: u32,
}

/// Passive game aggregate: board, ships, plots, pending fire, terminals.
/// Order application lives in `movement`; turn resolution lives in `turn`.
#[derive(Debug, Clone)]
pub struct GameState {
    pub board: Board,
    pub ships: Vec<Ship>,
    pub objective: Option<Hex>,
    pub destruction_target: Option<u32>,
    pub seed: u64,
    pub(crate) prng: Prng,
    pub turn: Turn,
    /// 0 between turns; 1..=32 only during an in-progress RunTurn (atomic, so tests see 0 after).
    pub impulse: u8,
    pub status: ScenarioStatus,
    fired_weapons_this_turn: HashSet<String>,
    plots: HashMap<u32, PlotState>,
    pending_fires: Vec<PendingFire>,
    scripted_plans: HashMap<u32, ScriptedPlan>,
}

impl GameState {
    pub fn new(board: Board, ships: Vec<Ship>, objective: Hex) -> Self {
        Self::new_with_options(board, ships, Some(objective), None, HashMap::new(), 1)
    }

    pub(crate) fn new_with_options(
        board: Board,
        ships: Vec<Ship>,
        objective: Option<Hex>,
        destruction_target: Option<u32>,
        scripted_plans: HashMap<u32, ScriptedPlan>,
        seed: u64,
    ) -> Self {
        let mut state = Self {
            board,
            ships,
            objective,
            destruction_target,
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

    pub fn ship(&self, id: u32) -> Option<&Ship> {
        self.ships.iter().find(|ship| ship.id == id)
    }

    pub fn ship_mut(&mut self, id: u32) -> Option<&mut Ship> {
        self.ships.iter_mut().find(|ship| ship.id == id)
    }

    pub fn ship_index(&self, id: u32) -> Option<usize> {
        self.ships.iter().position(|ship| ship.id == id)
    }

    pub fn weapon_owner_index(&self, weapon_id: &str) -> Option<usize> {
        self.ships.iter().position(|ship| {
            !ship.destroyed && ship.weapons.iter().any(|weapon| weapon.id == weapon_id)
        })
    }

    pub fn is_occupied_by_other(&self, moving_ship: u32, hex: Hex) -> bool {
        self.ships
            .iter()
            .any(|ship| ship.id != moving_ship && !ship.destroyed && ship.pos == hex)
    }

    pub fn weapon_fired_this_turn(&self, weapon_id: &str) -> bool {
        self.fired_weapons_this_turn.contains(weapon_id)
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

    pub(crate) fn queue_fire(&mut self, weapon: String, target: u32) {
        self.fired_weapons_this_turn.insert(weapon.clone());
        self.pending_fires.push(PendingFire { weapon, target });
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

    pub(crate) fn take_pending_fires(&mut self) -> Vec<(String, u32)> {
        std::mem::take(&mut self.pending_fires)
            .into_iter()
            .map(|f| (f.weapon, f.target))
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

    pub fn refresh_status(&mut self) {
        self.status = if let Some(objective) = self.objective {
            if self.ships.iter().any(|ship| ship.pos == objective) {
                ScenarioStatus::Won
            } else {
                ScenarioStatus::InProgress
            }
        } else if let Some(target) = self.destruction_target {
            if self
                .ships
                .iter()
                .any(|ship| ship.id == target && ship.destroyed)
            {
                ScenarioStatus::Won
            } else {
                ScenarioStatus::InProgress
            }
        } else {
            ScenarioStatus::InProgress
        };
    }

    /// After movement, advance scripted waypoint indices based on current position.
    ///
    /// If the ship ends on a later waypoint in the list, skip intermediate waypoints
    /// already passed during the same turn (multi-step plots).
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
