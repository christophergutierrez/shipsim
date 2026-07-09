use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::board::Board;
use crate::hex::Hex;
use crate::impulse::{move_count, moves_on_impulse};
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
struct PlotState {
    path: Vec<Hex>,
    cursor: usize,
}

#[derive(Debug, Clone)]
struct PendingFire {
    weapon: String,
    target: u32,
}

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

    /// Resolve a full 32-impulse turn: auto-plot scripted ships, simultaneous movement, then fire.
    pub fn run_turn(&mut self) {
        self.ensure_scripted_plots();

        for impulse in 1u8..=32 {
            self.impulse = impulse;
            self.resolve_impulse(impulse);
        }

        self.impulse = 0;
        self.resolve_pending_fires();
        self.sync_scripted_waypoints();
        self.plots.clear();
        self.fired_weapons_this_turn.clear();
        self.pending_fires.clear();
        self.refresh_status();
        self.turn.advance();
    }

    fn resolve_impulse(&mut self, impulse: u8) {
        // Collect intents: (ship_id, next_hex, new_facing)
        let mut intents: Vec<(u32, Hex, u8)> = Vec::new();
        let ship_ids: Vec<u32> = self.ships.iter().map(|s| s.id).collect();

        for ship_id in ship_ids {
            let Some(ship) = self.ship(ship_id) else {
                continue;
            };
            if ship.destroyed {
                continue;
            }
            let speed = ship.speed.min(31) as u8;
            if !moves_on_impulse(speed, impulse) {
                continue;
            }
            let Some(plot) = self.plots.get(&ship_id) else {
                continue;
            };
            if plot.cursor >= plot.path.len() {
                continue;
            }
            let next = plot.path[plot.cursor];
            let facing = Hex::facing_between(ship.pos, next).unwrap_or(ship.facing);
            intents.push((ship_id, next, facing));
        }

        // Tentative post positions for all ships
        let mut post: HashMap<u32, Hex> = self
            .ships
            .iter()
            .filter(|s| !s.destroyed)
            .map(|s| (s.id, s.pos))
            .collect();
        for (ship_id, next, _) in &intents {
            post.insert(*ship_id, *next);
        }

        let mut occupancy: HashMap<Hex, u32> = HashMap::new();
        for hex in post.values() {
            *occupancy.entry(*hex).or_insert(0) += 1;
        }

        let mut failed: HashSet<u32> = HashSet::new();
        for (ship_id, next, _) in &intents {
            if occupancy.get(next).copied().unwrap_or(0) > 1 {
                failed.insert(*ship_id);
            }
        }

        for (ship_id, next, facing) in intents {
            if failed.contains(&ship_id) {
                // Both-stop: leave position; clear remaining plot.
                self.plots.remove(&ship_id);
                continue;
            }
            if let Some(ship) = self.ship_mut(ship_id) {
                ship.pos = next;
                ship.facing = facing;
            }
            if let Some(plot) = self.plots.get_mut(&ship_id) {
                plot.cursor += 1;
            }
        }
    }

    fn resolve_pending_fires(&mut self) {
        let pending = std::mem::take(&mut self.pending_fires);
        for fire in pending {
            // Re-validate geometry at post-movement positions; skip if now illegal.
            let Some(attacker_index) = self.weapon_owner_index(&fire.weapon) else {
                continue;
            };
            let Some(target_ship) = self.ship(fire.target).cloned() else {
                continue;
            };
            if target_ship.destroyed {
                continue;
            }
            let attacker = self.ships[attacker_index].clone();
            if attacker.id == fire.target {
                continue;
            }
            let Some(weapon_def) = attacker
                .weapons
                .iter()
                .find(|w| w.id == fire.weapon)
                .cloned()
            else {
                continue;
            };
            let range = attacker.pos.distance(target_ship.pos);
            if range > weapon_def.max_range {
                continue;
            }
            let relative_bearing =
                crate::combat::relative_bearing(attacker.facing, attacker.pos, target_ship.pos);
            if !crate::combat::arc_contains(&weapon_def.arc, relative_bearing) {
                continue;
            }
            crate::combat::resolve_fire(self, &fire.weapon, fire.target);
        }
    }

    fn ensure_scripted_plots(&mut self) {
        let scripted_ids: Vec<u32> = self.scripted_plans.keys().copied().collect();
        for ship_id in scripted_ids {
            if self.plots.contains_key(&ship_id) {
                continue;
            }
            let path = self.generate_scripted_plot(ship_id);
            // Store even if empty so we do not regenerate mid-turn.
            self.plots.insert(
                ship_id,
                PlotState {
                    path,
                    cursor: 0,
                },
            );
        }
    }

    fn generate_scripted_plot(&mut self, ship_id: u32) -> Vec<Hex> {
        let Some(ship) = self.ship(ship_id).cloned() else {
            return Vec::new();
        };
        if ship.destroyed {
            return Vec::new();
        }
        let max_steps = move_count(ship.speed.min(31) as u8) as u32;
        let mut path: Vec<Hex> = Vec::new();
        let mut pos = ship.pos;
        let mut waypoints_advanced = 0usize;

        while (path.len() as u32) < max_steps {
            let target = {
                let plan = match self.scripted_plans.get(&ship_id) {
                    Some(p) => p,
                    None => break,
                };
                let mut idx = plan.next_waypoint + waypoints_advanced;
                while plan.waypoints.get(idx).is_some_and(|w| *w == pos) {
                    waypoints_advanced += 1;
                    idx = plan.next_waypoint + waypoints_advanced;
                }
                match plan.waypoints.get(idx).copied() {
                    Some(t) => t,
                    None => break,
                }
            };

            if pos.distance(target) == 0 {
                break;
            }

            // Prefer stepping onto the waypoint if adjacent and free.
            let next = if pos.distance(target) == 1
                && self.board.contains(target)
                && !self.is_occupied_by_other(ship_id, target)
                && !path.contains(&target)
            {
                Some(target)
            } else {
                // Deterministic: lowest facing index among neighbors that reduce distance.
                let mut best: Option<(u8, Hex)> = None;
                for facing in 0u8..=5 {
                    let Some(delta) = Hex::direction(facing) else {
                        continue;
                    };
                    let candidate = pos + delta;
                    if !self.board.contains(candidate) {
                        continue;
                    }
                    if self.is_occupied_by_other(ship_id, candidate) {
                        continue;
                    }
                    if path.contains(&candidate) {
                        continue;
                    }
                    if candidate.distance(target) >= pos.distance(target) {
                        continue;
                    }
                    match best {
                        None => best = Some((facing, candidate)),
                        Some((bf, _)) if facing < bf => best = Some((facing, candidate)),
                        _ => {}
                    }
                }
                best.map(|(_, hex)| hex)
            };

            let Some(step) = next else {
                break;
            };
            path.push(step);
            if step == target {
                waypoints_advanced += 1;
            }
            pos = step;
        }

        // Shorten to longest turn-mode-valid prefix.
        while !path.is_empty() {
            if movement::validate_plot(self, ship_id, &path).is_ok() {
                break;
            }
            path.pop();
        }

        // Advance waypoint bookkeeping for steps that land on waypoints (applied after successful plot).
        // Actual mark happens as positions are reached during the turn via post-turn sync.
        // For simplicity: after generating path, update next_waypoint for any prefix of waypoints
        // that appear as terminal steps in order — done after RunTurn movement by scanning final pos.
        // Here we only return the path; waypoint index is advanced after run_turn based on final pos.
        let _ = waypoints_advanced;
        path
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
