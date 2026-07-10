//! Game aggregate for Combat v2 (ADR-0019).

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Allocate,
    Movement,
    Firing,
    TurnEnd,
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FireCommit {
    pub ship: u32,
    pub weapon: String,
    pub target: u32,
    pub shield_facing: u8,
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
    status: ScenarioStatus,
    phase: Phase,
    move_order: Vec<u32>,
    allocated_this_turn: HashSet<u32>,
    moved_this_phase: HashSet<u32>,
    fire_commits: Vec<FireCommit>,
    ready_fire: HashSet<u32>,
    /// Keys are (ship_id, weapon_id) for multi-firer safety (TS2).
    fired_weapons_this_turn: HashSet<(u32, String)>,
    /// In-flight seeking munitions (D5a drones).
    seeking: Vec<SeekingMunition>,
    next_seeking_id: u32,
    /// Hits applied this turn (cleared with turn ephemera). AS3 combat logging.
    combat_log: Vec<CombatLogEvent>,
    /// Non-player controllers (scripted waypoints or AI). BTreeMap = deterministic order (T1).
    npcs: BTreeMap<u32, NpcController>,
}

#[derive(Debug, Clone)]
pub struct CombatLogEvent {
    pub attacker: u32,
    pub target: u32,
    pub shield: usize,
    pub damage: u32,
    pub kind: String,
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
            status: ScenarioStatus::InProgress,
            phase: Phase::Allocate,
            move_order: Vec::new(),
            allocated_this_turn: HashSet::new(),
            moved_this_phase: HashSet::new(),
            fire_commits: Vec::new(),
            ready_fire: HashSet::new(),
            fired_weapons_this_turn: HashSet::new(),
            seeking: Vec::new(),
            next_seeking_id: 1,
            combat_log: Vec::new(),
            npcs,
        };
        state.reset_all_power();
        state.refresh_status();
        state
    }

    // ----- public reads -----

    pub fn status(&self) -> ScenarioStatus {
        self.status
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn move_order(&self) -> &[u32] {
        &self.move_order
    }

    pub fn moved_this_phase(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.moved_this_phase.iter().copied().collect();
        ids.sort_unstable();
        ids
    }

    pub fn fire_commits(&self) -> &[FireCommit] {
        &self.fire_commits
    }

    pub fn ready_fire(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.ready_fire.iter().copied().collect();
        ids.sort_unstable();
        ids
    }

    /// In-flight seeking munitions (D5a drones).
    pub fn seeking_munitions(&self) -> &[SeekingMunition] {
        &self.seeking
    }

    pub fn turn_number(&self) -> u32 {
        self.turn.number()
    }


    /// Recompute the action order. v2 computes `action_order` on demand, so this
    /// is a no-op kept for construction compatibility.
    fn rebuild_action_order(&mut self) {}

    /// Record that `weapon` on `ship` has fired this turn (TS2 multi-firer safety).
    fn mark_weapon_fired(&mut self, ship: u32, weapon: &str) {
        self.fired_weapons_this_turn.insert((ship, weapon.to_string()));
    }

    pub fn reset_all_power(&mut self) {
        for s in &mut self.ships {
            if !s.destroyed {
                s.power_remaining = s.effective_power();
                s.reset_v2_allocation();
            }
        }
        self.phase = Phase::Allocate;
        self.move_order.clear();
        self.allocated_this_turn.clear();
        self.moved_this_phase.clear();
        self.fire_commits.clear();
        self.ready_fire.clear();
        self.fired_weapons_this_turn.clear();
        self.combat_log.clear();
    }

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
        if self.allocated_this_turn.contains(&ship_id) {
            return Err(crate::movement::OrderError::AlreadyAllocated(ship_id));
        }

        let ship = self
            .ship(ship_id)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship_id))?;
        if ship.destroyed {
            return Err(crate::movement::OrderError::ShipNotFound(ship_id));
        }

        for (weapon_id, charge) in &weapons {
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

        let weapon_power: u32 = weapons.values().copied().sum();
        let shield_power: u32 = shields.iter().copied().sum();
        let total = movement
            .saturating_add(weapon_power)
            .saturating_add(shield_power);
        let available = ship.effective_power();
        if total > available {
            return Err(crate::movement::OrderError::OverAllocated {
                ship: ship_id,
                total,
                available,
            });
        }

        let ship = self
            .ship_mut(ship_id)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship_id))?;
        ship.movement_allocated = movement;
        ship.move_remaining = movement;
        ship.weapon_charges = weapons;
        ship.shields_powered = shields;
        ship.shields_remaining = shields;
        ship.keel = crate::momentum::Keel::Stopped;
        self.allocated_this_turn.insert(ship_id);

        if self.all_living_allocated() {
            self.build_v2_move_order();
            self.begin_v2_movement_phase();
        }
        Ok(())
    }

    fn all_living_allocated(&self) -> bool {
        self.ships
            .iter()
            .filter(|ship| !ship.destroyed)
            .all(|ship| self.allocated_this_turn.contains(&ship.id))
    }

    fn build_v2_move_order(&mut self) {
        let mut ids: Vec<u32> = self
            .ships
            .iter()
            .filter(|s| !s.destroyed)
            .map(|s| s.id)
            .collect();
        ids.sort_by(|a, b| {
            let ma = self.ship(*a).map(|s| s.movement_allocated).unwrap_or(0);
            let mb = self.ship(*b).map(|s| s.movement_allocated).unwrap_or(0);
            mb.cmp(&ma).then_with(|| a.cmp(b))
        });

        let mut start = 0;
        while start < ids.len() {
            let movement = self
                .ship(ids[start])
                .map(|s| s.movement_allocated)
                .unwrap_or(0);
            let mut end = start + 1;
            while end < ids.len()
                && self
                    .ship(ids[end])
                    .map(|s| s.movement_allocated)
                    .unwrap_or(0)
                    == movement
            {
                end += 1;
            }
            if end - start > 1 {
                for i in (start + 1..end).rev() {
                    let offset = self.prng.roll((i - start + 1) as u32) as usize - 1;
                    ids.swap(i, start + offset);
                }
            }
            start = end;
        }
        self.move_order = ids;
    }

    pub fn active_v2_mover(&self) -> Option<u32> {
        self.move_order.iter().copied().find(|id| {
            !self.moved_this_phase.contains(id)
                && self
                    .ship(*id)
                    .is_some_and(|ship| !ship.destroyed && ship.move_remaining > 0)
        })
    }

    pub fn has_moved_this_phase(&self, ship: u32) -> bool {
        self.moved_this_phase.contains(&ship)
    }

    pub fn mark_v2_move_decision(&mut self, ship: u32) {
        self.moved_this_phase.insert(ship);
        if self.v2_movement_phase_complete() {
            self.phase = Phase::Firing;
            self.moved_this_phase.clear();
            self.ready_fire.clear();
            self.fire_commits.clear();
        }
    }

    fn v2_movement_phase_complete(&self) -> bool {
        self.move_order.iter().all(|id| {
            self.ship(*id)
                .is_none_or(|ship| ship.destroyed || ship.move_remaining == 0)
                || self.moved_this_phase.contains(id)
        })
    }

    pub fn spend_v2_move_power(
        &mut self,
        ship: u32,
        cost: u32,
    ) -> Result<(), crate::movement::OrderError> {
        let s = self
            .ship_mut(ship)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship))?;
        if s.move_remaining < cost {
            return Err(crate::movement::OrderError::InsufficientMovePower {
                ship,
                need: cost,
                have: s.move_remaining,
            });
        }
        s.move_remaining -= cost;
        Ok(())
    }

    pub(crate) fn set_v2_keel(
        &mut self,
        ship: u32,
        keel: crate::momentum::Keel,
    ) -> Result<(), StateError> {
        let s = self.ship_mut(ship).ok_or(StateError::ShipNotFound(ship))?;
        s.keel = keel;
        Ok(())
    }

    #[doc(hidden)]
    pub fn start_next_movement_phase_for_tests(&mut self) {
        self.begin_v2_movement_phase();
    }

    fn begin_v2_movement_phase(&mut self) {
        self.phase = Phase::Movement;
        self.moved_this_phase.clear();
        if self.v2_movement_phase_complete() {
            self.phase = Phase::Firing;
        }
    }

    pub fn commit_fire_v2(
        &mut self,
        commit: FireCommit,
    ) -> Result<(), crate::movement::OrderError> {
        self.validate_fire_commit_v2(&commit)?;
        if self
            .fire_commits
            .iter()
            .any(|existing| existing.ship == commit.ship && existing.weapon == commit.weapon)
        {
            return Err(crate::movement::OrderError::WeaponAlreadyCommitted {
                ship: commit.ship,
                weapon: commit.weapon,
            });
        }
        self.fire_commits.push(commit);
        Ok(())
    }

    pub fn ready_fire_v2(&mut self, ship: u32) -> Result<(), crate::movement::OrderError> {
        if self.phase != Phase::Firing {
            return Err(crate::movement::OrderError::WrongPhase {
                expected: "firing",
                actual: self.phase_name(),
            });
        }
        if self.ship(ship).is_none_or(|s| s.destroyed) {
            return Err(crate::movement::OrderError::ShipNotFound(ship));
        }
        self.ready_fire.insert(ship);
        if self.all_living_ready_fire() {
            self.resolve_fire_phase_v2()?;
        }
        Ok(())
    }

    fn all_living_ready_fire(&self) -> bool {
        self.ships
            .iter()
            .filter(|ship| !ship.destroyed)
            .all(|ship| self.ready_fire.contains(&ship.id))
    }

    fn validate_fire_commit_v2(
        &self,
        commit: &FireCommit,
    ) -> Result<(), crate::movement::OrderError> {
        if self.phase != Phase::Firing {
            return Err(crate::movement::OrderError::WrongPhase {
                expected: "firing",
                actual: self.phase_name(),
            });
        }
        if self.ready_fire.contains(&commit.ship) {
            return Err(crate::movement::OrderError::FireAlreadyReady(commit.ship));
        }
        let attacker = self
            .ship(commit.ship)
            .ok_or(crate::movement::OrderError::ShipNotFound(commit.ship))?;
        if attacker.destroyed {
            return Err(crate::movement::OrderError::ShipNotFound(commit.ship));
        }
        let target = self
            .ship(commit.target)
            .ok_or(crate::movement::OrderError::TargetNotFound(commit.target))?;
        if target.destroyed {
            return Err(crate::movement::OrderError::TargetNotFound(commit.target));
        }
        if commit.ship == commit.target {
            return Err(crate::movement::OrderError::FireAtSelf(commit.target));
        }
        let weapon = attacker
            .weapon(&commit.weapon)
            .ok_or_else(|| crate::movement::OrderError::WeaponNotFound(commit.weapon.clone()))?;
        if self.weapon_fired_this_turn(commit.ship, &commit.weapon) {
            return Err(crate::movement::OrderError::WeaponAlreadyFired {
                ship: commit.ship,
                weapon: commit.weapon.clone(),
            });
        }
        let charge = attacker
            .weapon_charges
            .get(&commit.weapon)
            .copied()
            .unwrap_or(0);
        if charge == 0 {
            return Err(crate::movement::OrderError::WeaponNotCharged {
                ship: commit.ship,
                weapon: commit.weapon.clone(),
            });
        }
        let kind = weapon
            .v2_kind
            .ok_or_else(|| crate::movement::OrderError::WeaponNotFound(commit.weapon.clone()))?;
        let range = attacker.pos.distance(target.pos);
        let max_range = crate::combat_tables::max_range(kind);
        if range > max_range {
            return Err(crate::movement::OrderError::OutOfRange {
                weapon: commit.weapon.clone(),
                range,
                max_range,
            });
        }
        if kind == crate::combat_tables::WeaponKind::Beam
            && crate::combat_tables::beam_damage(charge, range).is_none()
        {
            return Err(crate::movement::OrderError::NoDamage {
                weapon: commit.weapon.clone(),
                range,
                charge,
            });
        }
        if let Some(mount) = weapon.mount {
            if !crate::arc::in_arc(mount, attacker.facing, attacker.pos, target.pos) {
                return Err(crate::movement::OrderError::OutOfArc {
                    weapon: commit.weapon.clone(),
                    target: commit.target,
                });
            }
        }
        let legal_facings =
            crate::arc::legal_shield_facings(attacker.pos, target.pos, target.facing);
        if !legal_facings.contains(&commit.shield_facing) {
            return Err(crate::movement::OrderError::IllegalShieldFacing {
                requested: commit.shield_facing,
                legal: legal_facings,
            });
        }
        Ok(())
    }

    fn resolve_fire_phase_v2(&mut self) -> Result<(), crate::movement::OrderError> {
        let mut commits = self.fire_commits.clone();
        commits.sort_by(|a, b| a.ship.cmp(&b.ship).then_with(|| a.weapon.cmp(&b.weapon)));
        let snapshot = self.ships.clone();
        let mut results = Vec::new();

        for commit in &commits {
            self.validate_fire_commit_against_v2_snapshot(commit, &snapshot)?;
            let attacker = snapshot
                .iter()
                .find(|ship| ship.id == commit.ship)
                .ok_or(crate::movement::OrderError::ShipNotFound(commit.ship))?;
            let target = snapshot
                .iter()
                .find(|ship| ship.id == commit.target)
                .ok_or(crate::movement::OrderError::TargetNotFound(commit.target))?;
            let weapon = attacker.weapon(&commit.weapon).ok_or_else(|| {
                crate::movement::OrderError::WeaponNotFound(commit.weapon.clone())
            })?;
            let kind = weapon.v2_kind.expect("validated v2 weapon");
            let range = attacker.pos.distance(target.pos);
            let threshold =
                crate::combat_tables::to_hit_threshold(kind, range).ok_or_else(|| {
                    crate::movement::OrderError::OutOfRange {
                        weapon: commit.weapon.clone(),
                        range,
                        max_range: crate::combat_tables::max_range(kind),
                    }
                })?;
            let roll = self.prng.roll(20);
            let hit = roll <= threshold as u32;
            let damage = if hit {
                self.v2_projected_damage(attacker, &commit.weapon, kind, range)?
            } else {
                0
            };
            results.push((commit.clone(), hit, damage, roll));
        }

        for (commit, hit, damage, _roll) in results {
            if let Some(attacker) = self.ship_mut(commit.ship) {
                attacker.weapon_charges.insert(commit.weapon.clone(), 0);
            }
            self.mark_weapon_fired(commit.ship, &commit.weapon);
            if hit && damage > 0 {
                self.apply_v2_damage(commit.target, commit.shield_facing, damage);
            }
            self.combat_log.push(CombatLogEvent {
                attacker: commit.ship,
                target: commit.target,
                shield: commit.shield_facing as usize,
                damage,
                kind: if hit { "hit".into() } else { "miss".into() },
            });
        }
        self.fire_commits.clear();
        self.ready_fire.clear();
        self.refresh_status();
        // Turn-loop decision (frozen state machine): once a batch resolves, return to a
        // fresh movement phase if anyone can still move or fire legally; otherwise the turn
        // ends. A finished scenario simply parks at TurnEnd.
        if self.status == ScenarioStatus::Won {
            self.phase = Phase::TurnEnd;
        } else if self.can_any_move() || self.can_any_legal_fire() {
            self.begin_v2_movement_phase();
        } else {
            self.phase = Phase::TurnEnd;
        }
        Ok(())
    }

    /// Any living ship still has movement power (a turn-in-place is always a legal move).
    pub fn can_any_move(&self) -> bool {
        self.ships
            .iter()
            .any(|ship| !ship.destroyed && ship.move_remaining > 0)
    }

    /// Any living ship has a charged, unfired v2 weapon with at least one currently-legal shot.
    pub fn can_any_legal_fire(&self) -> bool {
        self.ships
            .iter()
            .filter(|ship| !ship.destroyed)
            .any(|attacker| {
                attacker
                    .weapons
                    .iter()
                    .any(|weapon| self.weapon_has_legal_shot(attacker, weapon))
            })
    }

    fn weapon_has_legal_shot(&self, attacker: &Ship, weapon: &combat::Weapon) -> bool {
        self.ships
            .iter()
            .filter(|target| !target.destroyed && target.id != attacker.id)
            .any(|target| self.v2_shot_shield_facing(attacker, weapon, target).is_some())
    }

    /// Shared v2 fire-legality predicate for AI + advisory queries. Mirrors commit-time
    /// legality (operational + charged + unfired weapon, in range, beam deals >= 1, in arc)
    /// and, when legal, returns the first geometry-legal shield facing for the shot.
    /// Returns `None` when the shot is illegal. Keep in sync with `validate_fire_commit_v2`.
    pub(crate) fn v2_shot_shield_facing(
        &self,
        attacker: &Ship,
        weapon: &combat::Weapon,
        target: &Ship,
    ) -> Option<u8> {
        if target.destroyed || target.id == attacker.id {
            return None;
        }
        let kind = weapon.v2_kind?;
        attacker.weapon(&weapon.id)?; // SSD-destroyed weapon
        let charge = attacker
            .weapon_charges
            .get(&weapon.id)
            .copied()
            .unwrap_or(0);
        if charge == 0 || self.weapon_fired_this_turn(attacker.id, &weapon.id) {
            return None;
        }
        let range = attacker.pos.distance(target.pos);
        if range > crate::combat_tables::max_range(kind) {
            return None;
        }
        if kind == crate::combat_tables::WeaponKind::Beam
            && crate::combat_tables::beam_damage(charge, range).is_none()
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

    /// Advisory for the UI: true iff some living ship could still move or fire legally.
    /// Never blocks EndTurn — the client owns any confirm dialog.
    pub fn end_turn_warning(&self) -> bool {
        self.can_any_move() || self.can_any_legal_fire()
    }

    /// Combat v2 EndTurn: always advances to the next turn's allocation. Legal in any phase
    /// after allocation. Pending commits/ready/move decisions are discarded (consistent with
    /// the turn reset).
    pub fn end_turn_v2(&mut self) -> Result<(), crate::movement::OrderError> {
        if self.phase == Phase::Allocate {
            return Err(crate::movement::OrderError::EndTurnDuringAllocation);
        }
        self.fire_commits.clear();
        self.ready_fire.clear();
        self.moved_this_phase.clear();
        self.advance_turn_counter();
        self.reset_all_power();
        self.refresh_status();
        Ok(())
    }

    fn validate_fire_commit_against_v2_snapshot(
        &self,
        commit: &FireCommit,
        ships: &[Ship],
    ) -> Result<(), crate::movement::OrderError> {
        let attacker = ships
            .iter()
            .find(|ship| ship.id == commit.ship)
            .ok_or(crate::movement::OrderError::ShipNotFound(commit.ship))?;
        if attacker.destroyed {
            return Err(crate::movement::OrderError::ShipNotFound(commit.ship));
        }
        let target = ships
            .iter()
            .find(|ship| ship.id == commit.target)
            .ok_or(crate::movement::OrderError::TargetNotFound(commit.target))?;
        if target.destroyed {
            return Err(crate::movement::OrderError::TargetNotFound(commit.target));
        }
        if commit.ship == commit.target {
            return Err(crate::movement::OrderError::FireAtSelf(commit.target));
        }
        let weapon = attacker
            .weapon(&commit.weapon)
            .ok_or_else(|| crate::movement::OrderError::WeaponNotFound(commit.weapon.clone()))?;
        if self.weapon_fired_this_turn(commit.ship, &commit.weapon) {
            return Err(crate::movement::OrderError::WeaponAlreadyFired {
                ship: commit.ship,
                weapon: commit.weapon.clone(),
            });
        }
        let charge = attacker
            .weapon_charges
            .get(&commit.weapon)
            .copied()
            .unwrap_or(0);
        if charge == 0 {
            return Err(crate::movement::OrderError::WeaponNotCharged {
                ship: commit.ship,
                weapon: commit.weapon.clone(),
            });
        }
        let kind = weapon
            .v2_kind
            .ok_or_else(|| crate::movement::OrderError::WeaponNotFound(commit.weapon.clone()))?;
        let range = attacker.pos.distance(target.pos);
        let max_range = crate::combat_tables::max_range(kind);
        if range > max_range {
            return Err(crate::movement::OrderError::OutOfRange {
                weapon: commit.weapon.clone(),
                range,
                max_range,
            });
        }
        if kind == crate::combat_tables::WeaponKind::Beam
            && crate::combat_tables::beam_damage(charge, range).is_none()
        {
            return Err(crate::movement::OrderError::NoDamage {
                weapon: commit.weapon.clone(),
                range,
                charge,
            });
        }
        if let Some(mount) = weapon.mount {
            if !crate::arc::in_arc(mount, attacker.facing, attacker.pos, target.pos) {
                return Err(crate::movement::OrderError::OutOfArc {
                    weapon: commit.weapon.clone(),
                    target: commit.target,
                });
            }
        }
        let legal_facings =
            crate::arc::legal_shield_facings(attacker.pos, target.pos, target.facing);
        if !legal_facings.contains(&commit.shield_facing) {
            return Err(crate::movement::OrderError::IllegalShieldFacing {
                requested: commit.shield_facing,
                legal: legal_facings,
            });
        }
        Ok(())
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
                crate::combat_tables::beam_damage(charge, range).ok_or_else(|| {
                    crate::movement::OrderError::NoDamage {
                        weapon: weapon_id.to_string(),
                        range,
                        charge,
                    }
                })
            }
            crate::combat_tables::WeaponKind::Plasma => crate::combat_tables::plasma_damage(range)
                .ok_or_else(|| crate::movement::OrderError::OutOfRange {
                    weapon: weapon_id.to_string(),
                    range,
                    max_range: crate::combat_tables::max_range(kind),
                }),
            crate::combat_tables::WeaponKind::Torp => crate::combat_tables::torp_damage(range)
                .ok_or_else(|| crate::movement::OrderError::OutOfRange {
                    weapon: weapon_id.to_string(),
                    range,
                    max_range: crate::combat_tables::max_range(kind),
                }),
        }
    }

    fn apply_v2_damage(&mut self, target: u32, shield_facing: u8, damage: u32) {
        let Some(ship) = self.ship_mut(target) else {
            return;
        };
        let facing = (shield_facing % 6) as usize;
        let absorbed = ship.shields_remaining[facing].min(damage);
        ship.shields_remaining[facing] -= absorbed;
        let overflow = damage - absorbed;
        if overflow > 0 {
            ship.ssd.apply_internal(overflow);
            ship.destroyed = ship.ssd.is_destroyed();
        }
    }

    pub fn phase_name(&self) -> &'static str {
        match self.phase {
            Phase::Allocate => "allocate",
            Phase::Movement => "movement",
            Phase::Firing => "firing",
            Phase::TurnEnd => "turn_end",
        }
    }

    /// True iff `ship_id` is a greedy-seek AI ship (the v2 driver's remit).
    fn is_v2_ai(&self, ship_id: u32) -> bool {
        matches!(self.npc(ship_id), Some(NpcController::GreedySeek))
    }

    /// True iff some living ship is not a v2 AI ship (a human must drive it).
    fn v2_has_living_human(&self) -> bool {
        self.ships
            .iter()
            .any(|ship| !ship.destroyed && !self.is_v2_ai(ship.id))
    }

    /// Combat v2 NPC auto-play. Plays greedy AI ships through the v2 phase machine:
    /// allocate un-allocated AI ships, emit the active AI mover's move/pass, commit each
    /// AI ship's legal shots then ready it, and (when only AI ships remain) end the turn to
    /// advance. Stops when a human ship must act, when the scenario is decided, or at a
    /// bounded step cap so it can never spin forever.
    pub fn resolve_v2_npc_actions(&mut self) {
        use crate::movement::{self, Order};
        const STEP_CAP: usize = 8192;

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
                                && !self.allocated_this_turn.contains(&ship.id)
                        })
                        .map(|ship| ship.id);
                    match next {
                        Some(id) => {
                            let (movement, weapons, shields) = crate::ai::v2_allocation(self, id)
                                .unwrap_or((0, BTreeMap::new(), [0; 6]));
                            let _ = self.allocate_v2(id, movement, weapons, shields);
                        }
                        // Still allocating with no AI ship left => a human must allocate.
                        None => break,
                    }
                }
                Phase::Movement => match self.active_v2_mover() {
                    Some(id) if self.is_v2_ai(id) => {
                        if let Some(mode) = crate::ai::v2_move_decision(self, id) {
                            let _ = movement::apply_order(self, Order::Move { ship: id, mode });
                        } else {
                            let _ = movement::apply_order(self, Order::PassMove { ship: id });
                        }
                    }
                    // Human mover (or, defensively, nothing to move) => hand back control.
                    _ => break,
                },
                Phase::Firing => {
                    let next = self
                        .ships
                        .iter()
                        .find(|ship| {
                            !ship.destroyed
                                && self.is_v2_ai(ship.id)
                                && !self.ready_fire.contains(&ship.id)
                        })
                        .map(|ship| ship.id);
                    match next {
                        Some(id) => {
                            for (weapon, target, shield_facing) in
                                crate::ai::v2_fire_commits(self, id)
                            {
                                let _ = self.commit_fire_v2(FireCommit {
                                    ship: id,
                                    weapon,
                                    target,
                                    shield_facing,
                                });
                            }
                            // May auto-resolve the batch when the last living ship readies.
                            let _ = self.ready_fire_v2(id);
                        }
                        // Every AI ship is ready but the phase has not resolved => a human
                        // still owes a ReadyFire.
                        None => break,
                    }
                }
                Phase::TurnEnd => {
                    if self.v2_has_living_human() {
                        break;
                    }
                    if self.end_turn_v2().is_err() {
                        break;
                    }
                }
            }
        }
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

    /// Snapshot / UI label: `player`, `ai`, or `scripted` (ADR-0018).
    pub fn controller_label(&self, ship_id: u32) -> &'static str {
        match self.npc(ship_id) {
            None => "player",
            Some(NpcController::GreedySeek) => "ai",
            Some(NpcController::Scripted(_)) => "scripted",
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


    pub fn set_ship_structure(&mut self, id: u32, structure: u32) -> Result<(), StateError> {
        let ship = self.ship_mut(id).ok_or(StateError::ShipNotFound(id))?;
        ship.set_structure(structure);
        Ok(())
    }


    // ----- crate-internal -----

    pub(crate) fn ship_mut(&mut self, id: u32) -> Option<&mut Ship> {
        self.ships.iter_mut().find(|ship| ship.id == id)
    }


    pub(crate) fn advance_turn_counter(&mut self) {
        self.turn.advance();
    }


    /// D4 floating map: translate all units so the formation stays on the board.
    pub(crate) fn maybe_float_recenter(&mut self) {
        if self.board.mode != crate::board::MapMode::Floating {
            return;
        }
        let mut positions: Vec<Hex> = self.ships.iter().map(|s| s.pos).collect();
        positions.extend(self.seeking.iter().map(|m| m.pos));
        if positions.is_empty() {
            return;
        }
        let (dq, dr) = Board::float_delta(&positions, self.board.width, self.board.height);
        if dq == 0 && dr == 0 {
            // Still clamp any outliers if formation larger than map.
            self.clamp_all_to_board();
            return;
        }
        for ship in &mut self.ships {
            ship.pos.q += dq;
            ship.pos.r += dr;
        }
        for m in &mut self.seeking {
            m.pos.q += dq;
            m.pos.r += dr;
        }
        self.clamp_all_to_board();
    }

    fn clamp_all_to_board(&mut self) {
        let w = self.board.width as i32;
        let h = self.board.height as i32;
        let clamp = |hex: &mut Hex| {
            hex.q = hex.q.clamp(0, w.saturating_sub(1).max(0));
            hex.r = hex.r.clamp(0, h.saturating_sub(1).max(0));
        };
        for ship in &mut self.ships {
            clamp(&mut ship.pos);
        }
        for m in &mut self.seeking {
            clamp(&mut m.pos);
        }
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


    pub(crate) fn clear_combat_log(&mut self) {
        self.combat_log.clear();
    }

    pub fn combat_log(&self) -> &[CombatLogEvent] {
        &self.combat_log
    }


    pub(crate) fn npc(&self, ship_id: u32) -> Option<&NpcController> {
        self.npcs.get(&ship_id)
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
