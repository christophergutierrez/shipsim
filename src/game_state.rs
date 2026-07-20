//! Game aggregate for protocol v4 simplified simultaneous turns (ADR-0025).

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
    phase: Phase,
    /// Applied allocation this turn (after barrier) and staged commits.
    allocated_this_turn: HashSet<u32>,
    staged_allocations: BTreeMap<u32, StagedAllocation>,
    path_commits: BTreeMap<u32, Vec<PathAction>>,
    volley_commits: BTreeMap<u32, Vec<VolleyShot>>,
    /// Weapons that fired this turn (ship, weapon_id).
    fired_weapons_this_turn: HashSet<(u32, String)>,
    combat_log: Vec<CombatLogEvent>,
    npcs: BTreeMap<u32, NpcController>,
    path_results: Vec<PathResult>,
    rules: Arc<Ruleset>,
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
}

impl GameState {
    pub(crate) fn new_with_options(
        board: Board,
        ships: Vec<Ship>,
        terminal: Option<Terminal>,
        npcs: BTreeMap<u32, NpcController>,
        seed: u64,
        rules: Arc<Ruleset>,
    ) -> Self {
        let mut state = Self {
            board,
            ships,
            terminal,
            seed,
            prng: Prng::new(seed),
            turn: Turn::new(),
            status: ScenarioStatus::InProgress,
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
        };
        state.reset_all_power();
        state.refresh_status();
        state
    }

    pub fn status(&self) -> ScenarioStatus {
        self.status
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

    fn mark_weapon_fired(&mut self, ship: u32, weapon: &str) {
        self.fired_weapons_this_turn
            .insert((ship, weapon.to_string()));
    }

    pub fn reset_all_power(&mut self) {
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

    /// Validate allocation draft without mutation (shared by stage + apply).
    fn validate_allocation_draft(
        &self,
        ship: &Ship,
        ship_id: u32,
        movement: u32,
        weapons: &BTreeMap<String, u32>,
        shields: &[u32; 6],
    ) -> Result<(u32, BTreeMap<String, u32>), crate::movement::OrderError> {
        let mut weapon_increases: u32 = 0;
        let mut merged_charges = ship.weapon_charges.clone();
        for (weapon_id, charge) in weapons {
            let weapon = ship
                .weapon(weapon_id)
                .ok_or_else(|| crate::movement::OrderError::WeaponNotFound(weapon_id.clone()))?;
            if *charge > weapon.max_charge {
                return Err(crate::movement::OrderError::WeaponChargeTooHigh {
                    ship: ship_id,
                    weapon: weapon_id.clone(),
                    charge: *charge,
                    max: weapon.max_charge,
                });
            }
            let have = ship.weapon_charges.get(weapon_id).copied().unwrap_or(0);
            if *charge < have {
                return Err(crate::movement::OrderError::CannotStripWeaponCharge {
                    ship: ship_id,
                    weapon: weapon_id.clone(),
                    have,
                    want: *charge,
                });
            }
            weapon_increases = weapon_increases.saturating_add(charge - have);
            merged_charges.insert(weapon_id.clone(), *charge);
        }
        for (facing, power) in shields.iter().copied().enumerate() {
            if power > ship.max_shield_per_facing {
                return Err(crate::movement::OrderError::ShieldPowerTooHigh {
                    ship: ship_id,
                    facing: facing as u8,
                    power,
                    max: ship.max_shield_per_facing,
                });
            }
        }
        let shield_power: u32 = shields.iter().copied().sum();
        let total = movement
            .saturating_add(weapon_increases)
            .saturating_add(shield_power);
        let available = ship.effective_power();
        if total > available {
            return Err(crate::movement::OrderError::OverAllocated {
                ship: ship_id,
                total,
                available,
            });
        }
        Ok((movement, merged_charges))
    }

    /// Stage one allocation. Applied only when every living ship has staged.
    pub fn allocate_v2(
        &mut self,
        ship_id: u32,
        movement: u32,
        weapons: BTreeMap<String, u32>,
        shields: [u32; 6],
    ) -> Result<(), crate::movement::OrderError> {
        if self.phase != Phase::Allocate {
            return Err(crate::movement::OrderError::WrongPhase {
                expected: "allocate",
                actual: self.phase_name(),
            });
        }
        if self.staged_allocations.contains_key(&ship_id)
            || self.allocated_this_turn.contains(&ship_id)
        {
            return Err(crate::movement::OrderError::AlreadyAllocated(ship_id));
        }

        let ship = self
            .ship(ship_id)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship_id))?;
        if ship.destroyed {
            return Err(crate::movement::OrderError::ShipNotFound(ship_id));
        }

        self.validate_allocation_draft(ship, ship_id, movement, &weapons, &shields)?;
        // Stage only — do not mutate public ship state yet.
        self.staged_allocations.insert(
            ship_id,
            StagedAllocation {
                movement,
                weapons,
                shields,
            },
        );

        if self.all_living_staged_allocation() {
            self.apply_all_allocations();
        }
        Ok(())
    }

    fn all_living_staged_allocation(&self) -> bool {
        self.ships
            .iter()
            .filter(|s| !s.destroyed)
            .all(|s| self.staged_allocations.contains_key(&s.id))
    }

    fn apply_all_allocations(&mut self) {
        let staged: Vec<(u32, StagedAllocation)> = self
            .staged_allocations
            .iter()
            .map(|(id, a)| (*id, a.clone()))
            .collect();
        for (ship_id, alloc) in staged {
            let Some(ship) = self.ship(ship_id).cloned() else {
                continue;
            };
            let Ok((_, merged_charges)) = self.validate_allocation_draft(
                &ship,
                ship_id,
                alloc.movement,
                &alloc.weapons,
                &alloc.shields,
            ) else {
                continue;
            };
            let Some(ship) = self.ship_mut(ship_id) else {
                continue;
            };
            ship.movement_allocated = alloc.movement;
            let (converted, _rem) = ship.thrust_conversion.convert(alloc.movement);
            let cap = ship.effective_max_maneuver_actions();
            ship.motion_available = path::usable_motion(converted, cap);
            ship.weapon_charges = merged_charges;
            ship.shields_powered = alloc.shields;
            ship.shields_remaining = alloc.shields;
            self.allocated_this_turn.insert(ship_id);
        }
        self.staged_allocations.clear();
        self.phase = Phase::Movement;
        self.path_commits.clear();
        self.path_results.clear();
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

    /// Commit one complete path for `ship`. Resolves when all living ships commit.
    pub fn commit_path(
        &mut self,
        ship_id: u32,
        actions: Vec<PathAction>,
    ) -> Result<(), crate::movement::OrderError> {
        if self.phase != Phase::Movement {
            return Err(crate::movement::OrderError::WrongPhase {
                expected: "movement",
                actual: self.phase_name(),
            });
        }
        let ship = self
            .ship(ship_id)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship_id))?;
        if ship.destroyed {
            return Err(crate::movement::OrderError::ShipNotFound(ship_id));
        }
        if self.path_commits.contains_key(&ship_id) {
            return Err(crate::movement::OrderError::AlreadyCommittedPath(ship_id));
        }
        let start = PathState {
            pos: ship.pos,
            facing: ship.facing,
        };
        let budget = ship.motion_available;
        path::trace_path(start, &actions, budget, self.hard_bounds()).map_err(|err| {
            crate::movement::OrderError::IllegalPath {
                ship: ship_id,
                reason: err.to_string(),
            }
        })?;
        self.path_commits.insert(ship_id, actions);
        if self.all_living_path_committed() {
            self.resolve_paths_phase();
        }
        Ok(())
    }

    fn all_living_path_committed(&self) -> bool {
        self.ships
            .iter()
            .filter(|s| !s.destroyed)
            .all(|s| self.path_commits.contains_key(&s.id))
    }

    fn resolve_paths_phase(&mut self) {
        let bounds = self.hard_bounds();
        let mut claims = Vec::new();
        for ship in self.ships.iter().filter(|s| !s.destroyed) {
            let actions = self.path_commits.get(&ship.id).cloned().unwrap_or_default();
            let start = PathState {
                pos: ship.pos,
                facing: ship.facing,
            };
            let budget = ship.motion_available;
            if let Ok(trace) = path::trace_path(start, &actions, budget, bounds) {
                claims.push(PathClaim {
                    ship: ship.id,
                    trace,
                });
            }
        }
        let results = path_resolve::resolve_paths(&claims, &mut self.prng);
        for result in &results {
            if let Some(ship) = self.ship_mut(result.ship) {
                ship.pos = Hex::new(result.final_q, result.final_r);
                ship.facing = result.final_facing;
                ship.motion_available = 0;
            }
        }
        self.path_results = results;
        self.path_commits.clear();
        self.refresh_status();
        if self.status != ScenarioStatus::InProgress {
            return;
        }
        self.phase = Phase::Firing;
        self.volley_commits.clear();
    }

    /// Commit one complete volley. Resolves when all living ships commit.
    pub fn commit_volley(
        &mut self,
        ship_id: u32,
        shots: Vec<VolleyShot>,
    ) -> Result<(), crate::movement::OrderError> {
        if self.phase != Phase::Firing {
            return Err(crate::movement::OrderError::WrongPhase {
                expected: "firing",
                actual: self.phase_name(),
            });
        }
        let ship = self
            .ship(ship_id)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship_id))?;
        if ship.destroyed {
            return Err(crate::movement::OrderError::ShipNotFound(ship_id));
        }
        if self.volley_commits.contains_key(&ship_id) {
            return Err(crate::movement::OrderError::AlreadyCommittedVolley(ship_id));
        }

        // Validate against current (pre-fire) geometry; each weapon at most once.
        let mut seen = HashSet::new();
        for shot in &shots {
            if !seen.insert(shot.weapon.clone()) {
                return Err(crate::movement::OrderError::WeaponAlreadyInVolley {
                    ship: ship_id,
                    weapon: shot.weapon.clone(),
                });
            }
            self.validate_shot(ship_id, shot)?;
        }

        self.volley_commits.insert(ship_id, shots);
        if self.all_living_volley_committed() {
            self.resolve_volley_phase()?;
        }
        Ok(())
    }

    fn all_living_volley_committed(&self) -> bool {
        self.ships
            .iter()
            .filter(|s| !s.destroyed)
            .all(|s| self.volley_commits.contains_key(&s.id))
    }

    fn validate_shot(
        &self,
        ship_id: u32,
        shot: &VolleyShot,
    ) -> Result<(), crate::movement::OrderError> {
        let attacker = self
            .ship(ship_id)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship_id))?;
        if attacker.destroyed {
            return Err(crate::movement::OrderError::ShipNotFound(ship_id));
        }
        let target = self
            .ship(shot.target)
            .ok_or(crate::movement::OrderError::TargetNotFound(shot.target))?;
        if target.destroyed {
            return Err(crate::movement::OrderError::TargetNotFound(shot.target));
        }
        if ship_id == shot.target {
            return Err(crate::movement::OrderError::FireAtSelf(shot.target));
        }
        let weapon = attacker
            .weapon(&shot.weapon)
            .ok_or_else(|| crate::movement::OrderError::WeaponNotFound(shot.weapon.clone()))?;
        if self.weapon_fired_this_turn(ship_id, &shot.weapon) {
            return Err(crate::movement::OrderError::WeaponAlreadyFired {
                ship: ship_id,
                weapon: shot.weapon.clone(),
            });
        }
        let charge = attacker
            .weapon_charges
            .get(&shot.weapon)
            .copied()
            .unwrap_or(0);
        if charge == 0 {
            return Err(crate::movement::OrderError::WeaponNotCharged {
                ship: ship_id,
                weapon: shot.weapon.clone(),
            });
        }
        let kind = weapon.kind;
        let range = attacker.pos.distance(target.pos);
        if range == 0 {
            return Err(crate::movement::OrderError::TooClose {
                weapon: shot.weapon.clone(),
                range,
                min_range: 1,
            });
        }
        let max_range = self.effective_weapon_max_range(weapon);
        if range > max_range {
            return Err(crate::movement::OrderError::OutOfRange {
                weapon: shot.weapon.clone(),
                range,
                max_range,
            });
        }
        if kind == crate::combat_tables::WeaponKind::Beam
            && crate::combat_tables::beam_damage(self.rules.combat(), charge, range).is_none()
        {
            return Err(crate::movement::OrderError::NoDamage {
                weapon: shot.weapon.clone(),
                range,
                charge,
            });
        }
        if let Some(mount) = weapon.mount {
            if !crate::arc::in_arc(mount, attacker.facing, attacker.pos, target.pos) {
                return Err(crate::movement::OrderError::OutOfArc {
                    weapon: shot.weapon.clone(),
                    target: shot.target,
                });
            }
        }
        let legal_facings =
            crate::arc::legal_shield_facings(attacker.pos, target.pos, target.facing);
        if !legal_facings.contains(&shot.shield_facing) {
            return Err(crate::movement::OrderError::IllegalShieldFacing {
                requested: shot.shield_facing,
                legal: legal_facings,
            });
        }
        Ok(())
    }

    fn resolve_volley_phase(&mut self) -> Result<(), crate::movement::OrderError> {
        // Clear previous turn's combat log only when a new volley resolves so
        // the post-resolution allocate snapshot still exposes those events.
        self.combat_log.clear();

        // Freeze participants and geometry at fire start.
        let snapshot = self.ships.clone();
        let alive_at_start: HashSet<u32> = snapshot
            .iter()
            .filter(|s| !s.destroyed)
            .map(|s| s.id)
            .collect();

        // Flatten volleys into canonical (attacker, weapon, target) order.
        let mut shots: Vec<(u32, VolleyShot)> = Vec::new();
        for (ship_id, volley) in &self.volley_commits {
            if !alive_at_start.contains(ship_id) {
                continue;
            }
            for shot in volley {
                shots.push((*ship_id, shot.clone()));
            }
        }
        shots.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.weapon.cmp(&b.1.weapon))
                .then_with(|| a.1.target.cmp(&b.1.target))
        });

        let mut results = Vec::new();
        for (attacker_id, shot) in &shots {
            // Attacker must have been alive at start; still fire even if destroyed mid-phase.
            if !alive_at_start.contains(attacker_id) {
                continue;
            }
            // Target must have been alive at start; overkill still executes.
            if !alive_at_start.contains(&shot.target) {
                continue;
            }
            let attacker = snapshot
                .iter()
                .find(|s| s.id == *attacker_id)
                .ok_or(crate::movement::OrderError::ShipNotFound(*attacker_id))?;
            let target = snapshot
                .iter()
                .find(|s| s.id == shot.target)
                .ok_or(crate::movement::OrderError::TargetNotFound(shot.target))?;
            // Re-validate geometry from frozen snapshot (not live destroyed state).
            let weapon = attacker
                .weapon(&shot.weapon)
                .ok_or_else(|| crate::movement::OrderError::WeaponNotFound(shot.weapon.clone()))?;
            let kind = weapon.kind;
            let range = attacker.pos.distance(target.pos);
            let threshold = crate::combat_tables::final_to_hit_threshold(
                self.rules.combat(),
                kind,
                range,
                target.size,
                attacker.attack_accuracy_bonus,
            )
            .ok_or_else(|| crate::movement::OrderError::OutOfRange {
                weapon: shot.weapon.clone(),
                range,
                max_range: self.effective_weapon_max_range(weapon),
            })?;
            let roll = self.prng.roll(u32::from(self.rules.combat().die_sides()));
            let hit = roll <= threshold as u32;
            let damage = if hit {
                self.v2_projected_damage(attacker, &shot.weapon, kind, range)?
            } else {
                0
            };
            results.push((
                *attacker_id,
                shot.weapon.clone(),
                shot.target,
                shot.shield_facing,
                hit,
                damage,
            ));
        }

        for (attacker_id, weapon, target, shield_facing, hit, damage) in results {
            if let Some(att) = self.ship_mut(attacker_id) {
                att.weapon_charges.insert(weapon.clone(), 0);
            }
            self.mark_weapon_fired(attacker_id, &weapon);
            let (shield_absorbed, hull_damage) = if hit && damage > 0 {
                self.apply_v2_damage(target, shield_facing, damage)
            } else {
                (0, 0)
            };
            self.combat_log.push(CombatLogEvent {
                attacker: attacker_id,
                target,
                weapon,
                shield: shield_facing as usize,
                damage,
                shield_absorbed,
                hull_damage,
                kind: if hit { "hit".into() } else { "miss".into() },
            });
        }

        self.volley_commits.clear();
        self.refresh_status();
        if self.status == ScenarioStatus::InProgress {
            // Automatic turn advance to next allocate.
            self.advance_turn_counter();
            self.reset_all_power();
            self.refresh_status();
        }
        Ok(())
    }

    pub fn path_preview(
        &self,
        ship_id: u32,
        actions: &[PathAction],
    ) -> Result<PathPreview, crate::movement::OrderError> {
        let ship = self
            .ship(ship_id)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship_id))?;
        if ship.destroyed {
            return Err(crate::movement::OrderError::ShipNotFound(ship_id));
        }
        let start = PathState {
            pos: ship.pos,
            facing: ship.facing,
        };
        let budget = ship.motion_available;
        match path::trace_path(start, actions, budget, self.hard_bounds()) {
            Ok(trace) => Ok(PathPreview {
                ship: ship_id,
                cost: trace.cost,
                remaining_motion: budget.saturating_sub(trace.cost),
                final_q: trace.final_state.pos.q,
                final_r: trace.final_state.pos.r,
                final_facing: trace.final_state.facing,
                steps: trace
                    .steps
                    .iter()
                    .map(|s| PathPreviewStep {
                        action: s.action.as_str().to_string(),
                        q: s.pos.q,
                        r: s.pos.r,
                        facing: s.facing,
                    })
                    .collect(),
                error_index: None,
                error: None,
            }),
            Err(err) => {
                let index = match &err {
                    path::PathError::OverBudget { index, .. } => Some(*index),
                    path::PathError::OffBoard { index, .. } => Some(*index),
                    path::PathError::InvalidFacingAt { index, .. } => Some(*index),
                    _ => None,
                };
                Ok(PathPreview {
                    ship: ship_id,
                    cost: actions.len() as u32,
                    remaining_motion: 0,
                    final_q: ship.pos.q,
                    final_r: ship.pos.r,
                    final_facing: ship.facing,
                    steps: Vec::new(),
                    error_index: index,
                    error: Some(err.to_string()),
                })
            }
        }
    }

    /// BFS reachable endpoints up to motion budget (for shading).
    ///
    /// `budget` is clamped to the authoritative usable motion pool for the ship
    /// (converted allocation when available, else effective maneuver cap). This
    /// prevents unbounded-world graph searches from hostile or accidental values.
    pub fn reach_preview(
        &self,
        ship_id: u32,
        budget: Option<u32>,
    ) -> Result<Vec<ReachEndpoint>, crate::movement::OrderError> {
        let ship = self
            .ship(ship_id)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship_id))?;
        if ship.destroyed {
            return Err(crate::movement::OrderError::ShipNotFound(ship_id));
        }
        // During movement, zero is meaningful: it means the ship bought no
        // motion and must not receive a fabricated reach cloud. During
        // allocation, preview the largest motion purchase possible from the
        // current power pool, capped by the damaged engine and hull limits.
        let max_budget = match self.phase {
            Phase::Movement => ship.motion_available,
            Phase::Allocate => {
                let (converted, _) = ship.thrust_conversion.convert(ship.effective_power());
                converted.min(ship.effective_max_maneuver_actions())
            }
            Phase::Firing => 0,
        };
        let budget = budget.unwrap_or(max_budget).min(max_budget);
        let start = PathState {
            pos: ship.pos,
            facing: ship.facing,
        };
        let bounds = self.hard_bounds();
        let mut best: BTreeMap<(i32, i32, u8), u32> = BTreeMap::new();
        best.insert((start.pos.q, start.pos.r, start.facing), 0);
        let mut frontier = vec![(start, 0u32)];
        while let Some((state, cost)) = frontier.pop() {
            if cost >= budget {
                continue;
            }
            for action in PathAction::ALL {
                let Ok(next) = path::apply_action(state, action) else {
                    continue;
                };
                if let Some(b) = bounds {
                    if action.translates() && !b.contains(next.pos) {
                        continue;
                    }
                }
                let nc = cost + 1;
                let key = (next.pos.q, next.pos.r, next.facing);
                if best.get(&key).is_some_and(|&c| c <= nc) {
                    continue;
                }
                best.insert(key, nc);
                frontier.push((next, nc));
            }
        }
        let mut out: Vec<ReachEndpoint> = best
            .into_iter()
            .map(|((q, r, facing), min_cost)| ReachEndpoint {
                q,
                r,
                facing,
                min_cost,
            })
            .collect();
        out.sort_by_key(|e| (e.min_cost, e.q, e.r, e.facing));
        Ok(out)
    }

    pub fn fire_decision_preview(
        &self,
        ship_id: u32,
        weapon_id: &str,
        target_id: u32,
    ) -> Result<FireDecisionPreview, crate::movement::OrderError> {
        let attacker = self
            .ship(ship_id)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship_id))?;
        let target = self
            .ship(target_id)
            .ok_or(crate::movement::OrderError::TargetNotFound(target_id))?;
        let legal_shield_facings =
            crate::arc::legal_shield_facings(attacker.pos, target.pos, target.facing);
        let shield_facing = legal_shield_facings.first().copied().unwrap_or(0);
        let shot = VolleyShot {
            weapon: weapon_id.to_string(),
            target: target_id,
            shield_facing,
        };
        if self.phase == Phase::Firing {
            self.validate_shot(ship_id, &shot)?;
        } else {
            // Soft validation for UI during other phases.
            let _ = shot;
        }

        let weapon = attacker
            .weapon(weapon_id)
            .ok_or_else(|| crate::movement::OrderError::WeaponNotFound(weapon_id.to_string()))?;
        let range = attacker.pos.distance(target.pos);
        let threshold = crate::combat_tables::final_to_hit_threshold(
            self.rules.combat(),
            weapon.kind,
            range,
            target.size,
            attacker.attack_accuracy_bonus,
        )
        .ok_or_else(|| crate::movement::OrderError::OutOfRange {
            weapon: weapon_id.to_string(),
            range,
            max_range: self.effective_weapon_max_range(weapon),
        })?;
        let die_sides = self.rules.combat().die_sides();
        let projected_damage = self.v2_projected_damage(attacker, weapon_id, weapon.kind, range)?;

        Ok(FireDecisionPreview {
            ship: ship_id,
            weapon: weapon_id.to_string(),
            target: target_id,
            range,
            threshold,
            die_sides,
            hit_percent: u32::from(threshold) * 100 / u32::from(die_sides),
            projected_damage,
            legal_shield_facings,
        })
    }

    pub fn can_any_legal_fire(&self) -> bool {
        self.fire_opportunity().is_some()
    }

    pub fn fire_opportunity(&self) -> Option<FireOpportunity> {
        let mut attackers: Vec<&Ship> = self
            .ships
            .iter()
            .filter(|s| !s.destroyed && self.controller_label(s.id) == "player")
            .filter(|s| !self.volley_commits.contains_key(&s.id))
            .collect();
        attackers.sort_by_key(|s| s.id);
        for attacker in attackers {
            for weapon in &attacker.weapons {
                if !self.weapon_has_legal_shot(attacker, weapon) {
                    continue;
                }
                let mut targets: Vec<&Ship> = self
                    .ships
                    .iter()
                    .filter(|t| !t.destroyed && self.controller_label(t.id) != "player")
                    .collect();
                targets.sort_by_key(|t| t.id);
                for target in targets {
                    let facings = self.v2_legal_shield_facings(attacker, weapon, target);
                    if facings.is_empty() {
                        continue;
                    }
                    return Some(FireOpportunity {
                        ship: attacker.id,
                        weapon: weapon.id.clone(),
                        target: target.id,
                        legal_shield_facings: facings,
                    });
                }
            }
        }
        None
    }

    fn weapon_has_legal_shot(&self, attacker: &Ship, weapon: &combat::Weapon) -> bool {
        self.ships
            .iter()
            .filter(|target| !target.destroyed && self.controller_label(target.id) != "player")
            .any(|target| {
                self.v2_shot_shield_facing(attacker, weapon, target)
                    .is_some()
            })
    }

    fn v2_legal_shield_facings(
        &self,
        attacker: &Ship,
        weapon: &combat::Weapon,
        target: &Ship,
    ) -> Vec<u8> {
        if self
            .v2_shot_shield_facing(attacker, weapon, target)
            .is_none()
        {
            return Vec::new();
        }
        crate::arc::legal_shield_facings(attacker.pos, target.pos, target.facing)
    }

    pub fn v2_shot_shield_facing(
        &self,
        attacker: &Ship,
        weapon: &combat::Weapon,
        target: &Ship,
    ) -> Option<u8> {
        if target.destroyed || target.id == attacker.id {
            return None;
        }
        let kind = weapon.kind;
        attacker.weapon(&weapon.id)?;
        let charge = attacker
            .weapon_charges
            .get(&weapon.id)
            .copied()
            .unwrap_or(0);
        if charge == 0 || self.weapon_fired_this_turn(attacker.id, &weapon.id) {
            return None;
        }
        let range = attacker.pos.distance(target.pos);
        if range == 0 {
            return None;
        }
        if range > self.effective_weapon_max_range(weapon) {
            return None;
        }
        if kind == crate::combat_tables::WeaponKind::Beam
            && crate::combat_tables::beam_damage(self.rules.combat(), charge, range).is_none()
        {
            return None;
        }
        if let Some(mount) = weapon.mount {
            if !crate::arc::in_arc(mount, attacker.facing, attacker.pos, target.pos) {
                return None;
            }
        }
        crate::arc::legal_shield_facings(attacker.pos, target.pos, target.facing)
            .into_iter()
            .next()
    }

    fn v2_projected_damage(
        &self,
        attacker: &Ship,
        weapon_id: &str,
        kind: crate::combat_tables::WeaponKind,
        range: u32,
    ) -> Result<u32, crate::movement::OrderError> {
        let charge = attacker.weapon_charges.get(weapon_id).copied().unwrap_or(0);
        match kind {
            crate::combat_tables::WeaponKind::Beam => {
                crate::combat_tables::beam_damage(self.rules.combat(), charge, range).ok_or_else(
                    || crate::movement::OrderError::NoDamage {
                        weapon: weapon_id.to_string(),
                        range,
                        charge,
                    },
                )
            }
            crate::combat_tables::WeaponKind::Plasma => {
                crate::combat_tables::plasma_damage(self.rules.combat(), range).ok_or_else(|| {
                    crate::movement::OrderError::OutOfRange {
                        weapon: weapon_id.to_string(),
                        range,
                        max_range: attacker.weapon(weapon_id).map_or_else(
                            || self.rules.max_range(kind),
                            |weapon| self.effective_weapon_max_range(weapon),
                        ),
                    }
                })
            }
            crate::combat_tables::WeaponKind::Torp => {
                crate::combat_tables::torp_damage(self.rules.combat(), range).ok_or_else(|| {
                    crate::movement::OrderError::OutOfRange {
                        weapon: weapon_id.to_string(),
                        range,
                        max_range: attacker.weapon(weapon_id).map_or_else(
                            || self.rules.max_range(kind),
                            |weapon| self.effective_weapon_max_range(weapon),
                        ),
                    }
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

    /// Combat v4 NPC auto-play through collection stages.
    ///
    /// Returns every order the AI successfully applied, in application order.
    /// Harnesses must append these to the save stream so barrier-stage AI
    /// commits are replayed exactly (player-only saves cannot reconstruct them).
    pub fn resolve_v2_npc_actions(&mut self) -> Vec<crate::movement::Order> {
        use crate::movement::Order;

        const STEP_CAP: usize = 8192;
        let mut applied = Vec::new();

        for _ in 0..STEP_CAP {
            if self.status != ScenarioStatus::InProgress {
                break;
            }
            match self.phase {
                Phase::Allocate => {
                    let next = self
                        .ships
                        .iter()
                        .find(|ship| {
                            !ship.destroyed
                                && self.is_v2_ai(ship.id)
                                && !self.staged_allocations.contains_key(&ship.id)
                                && !self.allocated_this_turn.contains(&ship.id)
                        })
                        .map(|ship| ship.id);
                    match next {
                        Some(id) => {
                            let (movement, weapons, shields) = crate::ai::v2_allocation(self, id)
                                .unwrap_or((0, BTreeMap::new(), [0; 6]));
                            let order = Order::Allocate {
                                ship: id,
                                movement,
                                weapons: weapons.clone(),
                                shields,
                            };
                            if self.allocate_v2(id, movement, weapons, shields).is_ok() {
                                applied.push(order);
                            }
                        }
                        None => break,
                    }
                }
                Phase::Movement => {
                    let next = self
                        .ships
                        .iter()
                        .find(|ship| {
                            !ship.destroyed
                                && self.is_v2_ai(ship.id)
                                && !self.has_committed_path(ship.id)
                        })
                        .map(|ship| ship.id);
                    match next {
                        Some(id) => {
                            let actions = crate::ai::v2_path_decision(self, id).unwrap_or_default();
                            let order = Order::CommitPath {
                                ship: id,
                                actions: actions.clone(),
                            };
                            if self.commit_path(id, actions).is_ok() {
                                applied.push(order);
                            }
                        }
                        None => break,
                    }
                }
                Phase::Firing => {
                    let next = self
                        .ships
                        .iter()
                        .find(|ship| {
                            !ship.destroyed
                                && self.is_v2_ai(ship.id)
                                && !self.has_committed_volley(ship.id)
                        })
                        .map(|ship| ship.id);
                    match next {
                        Some(id) => {
                            let shots = crate::ai::v2_volley(self, id);
                            let order = Order::CommitVolley {
                                ship: id,
                                shots: shots.clone(),
                            };
                            if self.commit_volley(id, shots).is_ok() {
                                applied.push(order);
                            }
                        }
                        None => break,
                    }
                }
            }
        }
        applied
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
        let player_ships: Vec<_> = self
            .ships
            .iter()
            .filter(|ship| !self.npcs.contains_key(&ship.id))
            .collect();
        if !player_ships.is_empty() && !player_ships.iter().any(|ship| !ship.destroyed) {
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
