use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::board::Board;
use crate::hex::Hex;
use crate::movement::{self, Order, OrderError};
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
pub struct GameState {
    pub board: Board,
    pub ships: Vec<Ship>,
    pub objective: Option<Hex>,
    pub seed: u64,
    pub(crate) prng: Prng,
    pub turn: Turn,
    pub status: ScenarioStatus,
    moves_this_turn: HashMap<u32, u32>,
    fired_weapons_this_turn: HashSet<String>,
    scripted_plans: HashMap<u32, ScriptedPlan>,
}

impl GameState {
    pub fn new(board: Board, ships: Vec<Ship>, objective: Hex) -> Self {
        Self::new_with_options(board, ships, Some(objective), HashMap::new(), 1)
    }

    pub(crate) fn new_with_options(
        board: Board,
        ships: Vec<Ship>,
        objective: Option<Hex>,
        scripted_plans: HashMap<u32, ScriptedPlan>,
        seed: u64,
    ) -> Self {
        let mut state = Self {
            board,
            ships,
            objective,
            seed,
            prng: Prng::new(seed),
            turn: Turn::new(),
            status: ScenarioStatus::InProgress,
            moves_this_turn: HashMap::new(),
            fired_weapons_this_turn: HashSet::new(),
            scripted_plans,
        };
        state.refresh_status();
        state
    }

    pub fn apply_order(&mut self, order: Order) -> Result<(), OrderError> {
        let declared = movement::declare(self, order)?;
        movement::resolve(self, declared);
        Ok(())
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

    pub fn fire_attacker_index(&self, weapon_id: &str, target_id: u32) -> Option<usize> {
        self.ships.iter().position(|ship| {
            ship.id != target_id
                && !ship.destroyed
                && ship.weapons.iter().any(|weapon| weapon.id == weapon_id)
        })
    }

    pub fn is_occupied_by_other(&self, moving_ship: u32, hex: Hex) -> bool {
        self.ships
            .iter()
            .any(|ship| ship.id != moving_ship && ship.pos == hex)
    }

    pub fn weapon_fired_this_turn(&self, weapon_id: &str) -> bool {
        self.fired_weapons_this_turn.contains(weapon_id)
    }

    pub fn record_weapon_fired(&mut self, weapon_id: String) {
        self.fired_weapons_this_turn.insert(weapon_id);
    }

    pub fn hexes_moved_this_turn(&self, ship: u32) -> u32 {
        self.moves_this_turn.get(&ship).copied().unwrap_or(0)
    }

    pub fn record_hex_moved(&mut self, ship: u32) {
        *self.moves_this_turn.entry(ship).or_insert(0) += 1;
    }

    pub fn end_turn(&mut self) {
        self.advance_scripted_ships();
        self.turn.advance();
        self.moves_this_turn.clear();
        self.fired_weapons_this_turn.clear();
        self.refresh_status();
    }

    pub fn refresh_status(&mut self) {
        self.status = match self.objective {
            Some(objective) if self.ships.iter().any(|ship| ship.pos == objective) => {
                ScenarioStatus::Won
            }
            _ => ScenarioStatus::InProgress,
        };
    }

    fn advance_scripted_ships(&mut self) {
        let scripted_ids: Vec<u32> = self.scripted_plans.keys().copied().collect();
        for ship_id in scripted_ids {
            let Some(target) = self.next_scripted_target(ship_id) else {
                continue;
            };
            let Some(current) = self.ship(ship_id).map(|ship| ship.pos) else {
                continue;
            };
            if current.distance(target) == 1
                && self.board.contains(target)
                && !self.is_occupied_by_other(ship_id, target)
            {
                if let Some(ship) = self.ship_mut(ship_id) {
                    ship.pos = target;
                }
                self.mark_scripted_target_reached(ship_id, target);
            }
        }
    }

    fn next_scripted_target(&mut self, ship_id: u32) -> Option<Hex> {
        let current = self.ship(ship_id)?.pos;
        let plan = self.scripted_plans.get_mut(&ship_id)?;
        while plan
            .waypoints
            .get(plan.next_waypoint)
            .is_some_and(|waypoint| *waypoint == current)
        {
            plan.next_waypoint += 1;
        }
        plan.waypoints.get(plan.next_waypoint).copied()
    }

    fn mark_scripted_target_reached(&mut self, ship_id: u32, target: Hex) {
        let Some(plan) = self.scripted_plans.get_mut(&ship_id) else {
            return;
        };
        if plan
            .waypoints
            .get(plan.next_waypoint)
            .is_some_and(|waypoint| *waypoint == target)
        {
            plan.next_waypoint += 1;
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
