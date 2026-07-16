//! Game aggregate for Combat v2 (ADR-0020).

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;

use crate::board::Board;
use crate::combat;
use crate::hex::Hex;
use crate::prng::Prng;
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
    TurnEnd,
}

/// Win condition. Objective, single-target destruction, and annihilation are
/// mutually exclusive (AS1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminal {
    ReachHex(Hex),
    DestroyShip(u32),
    /// Player wins when every non-player (NPC/scripted) ship is destroyed.
    /// Used for multi-ship fleet engagements.
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FireCommit {
    pub ship: u32,
    pub weapon: String,
    pub target: u32,
    pub shield_facing: u8,
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
    /// 1..=4: the current movement/translation phase within the turn (ADR-0022 M4).
    movement_phase: u8,
    allocated_this_turn: HashSet<u32>,
    /// Maneuvers committed by living ships for the current movement phase, keyed by ship id
    /// for deterministic iteration. Resolution is deferred until every living ship has
    /// committed, so the result is independent of commit order (ADR-0022 M4).
    maneuver_commits: BTreeMap<u32, crate::motion::Maneuver>,
    fire_commits: Vec<FireCommit>,
    ready_fire: HashSet<u32>,
    /// Keys are (ship_id, weapon_id) for multi-firer safety (TS2).
    fired_weapons_this_turn: HashSet<(u32, String)>,
    /// Hits applied this turn (cleared with turn ephemera). AS3 combat logging.
    combat_log: Vec<CombatLogEvent>,
    /// Non-player controller labels and AI behavior. BTreeMap keeps iteration deterministic.
    npcs: BTreeMap<u32, NpcController>,
    /// Per-resolution outcome for eligible scheduled translations. The value is
    /// true only when the ship actually moved; this survives floating-map
    /// recentering without relying on absolute snapshot coordinates.
    last_translation_outcomes: BTreeMap<u32, bool>,
}

#[derive(Debug, Clone)]
pub struct CombatLogEvent {
    pub attacker: u32,
    pub target: u32,
    pub weapon: String,
    pub shield: usize,
    pub damage: u32,
    /// Damage removed from the selected shield facing before it reached the ship.
    pub shield_absorbed: u32,
    /// Damage that overflowed shields and was applied to the SSD.
    pub hull_damage: u32,
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
            movement_phase: 0,
            allocated_this_turn: HashSet::new(),
            maneuver_commits: BTreeMap::new(),
            fire_commits: Vec::new(),
            ready_fire: HashSet::new(),
            fired_weapons_this_turn: HashSet::new(),
            combat_log: Vec::new(),
            npcs,
            last_translation_outcomes: BTreeMap::new(),
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

    /// 1..=4 during `Phase::Movement`; the current movement/translation phase (ADR-0022 M4).
    pub fn movement_phase(&self) -> u8 {
        self.movement_phase
    }

    /// Ships that have committed a maneuver for the current movement phase (sorted).
    pub fn ships_committed_this_phase(&self) -> Vec<u32> {
        self.maneuver_commits.keys().copied().collect()
    }

    pub fn fire_commits(&self) -> &[FireCommit] {
        &self.fire_commits
    }

    pub fn ready_fire(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.ready_fire.iter().copied().collect();
        ids.sort_unstable();
        ids
    }

    /// Ships that have finished allocate this turn (sorted for deterministic snapshots).
    pub fn allocated_this_turn(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.allocated_this_turn.iter().copied().collect();
        ids.sort_unstable();
        ids
    }

    pub fn turn_number(&self) -> u32 {
        self.turn.number()
    }

    /// Record that `weapon` on `ship` has fired this turn (TS2 multi-firer safety).
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
        self.movement_phase = 0;
        self.allocated_this_turn.clear();
        self.maneuver_commits.clear();
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

        // Weapon charge carries across turns. The allocate map lists *desired*
        // totals; power is spent only on *increases*. You may not strip charge
        // from a weapon that still holds last turn's charge.
        let mut weapon_increases: u32 = 0;
        let mut merged_charges = ship.weapon_charges.clone();
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

        // Pre-charged weapons do not free power: the pool is still full, but
        // only *new* weapon charge, shields, and engine spend from it.
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

        let ship = self
            .ship_mut(ship_id)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship_id))?;
        ship.movement_allocated = movement;
        // Engine power → thrust via hull conversion. Velocity persists; thrust is
        // re-bought each turn.
        let (thrust, _remainder) = ship.thrust_conversion.convert(movement);
        ship.thrust_remaining = thrust;
        ship.weapon_charges = merged_charges;
        // Shields always start this turn from the allocate order (0 if unspent).
        ship.shields_powered = shields;
        ship.shields_remaining = shields;
        self.allocated_this_turn.insert(ship_id);

        if self.all_living_allocated() {
            self.begin_v2_movement_phase();
        }
        Ok(())
    }

    /// Read-only four-cycle movement preview (ADR-0022 preview contract).
    ///
    /// Computes the reachable endpoint set for `ship_id` over the four movement
    /// phases of the current turn, given a *complete* allocation draft
    /// (movement power, weapons, shields). The engine — not the TUI — owns
    /// power accounting: it validates the draft exactly as `allocate_v2` would
    /// (weapon charge carry rules, shield caps, total <= effective power) and
    /// converts movement power → thrust via the ship's hull conversion, then
    /// runs the pure projection in `movement_preview`.
    ///
    /// This is **read-only**: it takes `&self`, never mutates position, phase,
    /// thrust, commits, the combat log, or the PRNG, and is excluded from
    /// save/replay. Enemy ships remain at their current positions for display;
    /// their future movement is unknown and is not predicted. Currently
    /// occupied destinations are reported separately, not removed from the
    /// geometric envelope.
    pub fn movement_preview(
        &self,
        ship_id: u32,
        movement: u32,
        weapons: BTreeMap<String, u32>,
        shields: [u32; 6],
    ) -> Result<crate::movement_preview::PreviewResult, crate::movement::OrderError> {
        // Phase guard: preview is meaningful during allocation (the draft is
        // being built). We also allow it during the movement phase so a client
        // can preview remaining cycles, but never during firing/turn-end.
        if !matches!(self.phase, Phase::Allocate | Phase::Movement) {
            return Err(crate::movement::OrderError::WrongPhase {
                expected: "allocate or movement",
                actual: self.phase_name(),
            });
        }

        let ship = self
            .ship(ship_id)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship_id))?;
        if ship.destroyed {
            return Err(crate::movement::OrderError::ShipNotFound(ship_id));
        }

        // ── Validate the draft exactly as allocate_v2 would (no mutation) ──
        // Weapon charge carries across turns; power is spent only on increases.
        let mut weapon_increases: u32 = 0;
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

        // ── Convert movement power → thrust (engine owns this) ──
        let (thrust, _remainder) = ship.thrust_conversion.convert(movement);

        // ── Assemble preview inputs from the ship's CURRENT live state ──
        // During the movement phase, thrust_remaining reflects what was bought
        // this turn minus what was already spent on committed maneuvers. The
        // draft's movement power is only authoritative during allocation; once
        // movement has begun, the preview reflects the remaining thrust.
        let (start_thrust, start_pos, start_facing, start_velocity) = match self.phase {
            Phase::Allocate => (thrust, ship.pos, ship.facing, ship.velocity),
            // During movement, the ship has already spent some thrust; preview
            // the *remaining* cycles from the current state with remaining thrust.
            _ => (ship.thrust_remaining, ship.pos, ship.facing, ship.velocity),
        };

        // Occupied hexes = every other living ship's current position.
        let occupied_hexes: Vec<crate::hex::Hex> = self
            .ships
            .iter()
            .filter(|s| s.id != ship_id && !s.destroyed)
            .map(|s| s.pos)
            .collect();

        let inputs = crate::movement_preview::PreviewInputs {
            start: start_pos,
            facing: start_facing,
            velocity: start_velocity,
            max_velocity: ship.max_velocity,
            thrust_remaining: start_thrust,
            occupied_hexes,
            map_mode: self.board.mode,
            board_width: self.board.width,
            board_height: self.board.height,
        };

        crate::movement_preview::preview(inputs)
            .map_err(|e| crate::movement::OrderError::PreviewFailed(e.to_string()))
    }

    /// Clamp movement power to what the ship can actually afford (ADR-0022).
    ///
    /// Given a draft allocation, returns the movement power after reserving
    /// weapon top-ups and shield power against the ship's effective power pool.
    /// Weapons and shields are honored first (they are committed costs); only
    /// movement — the residual — is clamped down. If weapons + shields alone
    /// already exceed available power, movement clamps to 0.
    ///
    /// This is the live-preview companion to `allocate_v2`'s hard reject: while
    /// a player drags allocation sliders, the TUI asks for a clamped preview so
    /// the reachable set shrinks smoothly instead of snapping to an error.
    ///
    /// Read-only: takes `&self`, never mutates. Weapon/shield validity is *not*
    /// re-checked here (the caller may want a preview even with an invalid
    /// shield facing); only the power budget is clamped.
    pub fn clamp_movement_power(
        &self,
        ship_id: u32,
        movement: u32,
        weapons: &BTreeMap<String, u32>,
        shields: &[u32; 6],
    ) -> Result<u32, crate::movement::OrderError> {
        let ship = self
            .ship(ship_id)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship_id))?;
        if ship.destroyed {
            return Err(crate::movement::OrderError::ShipNotFound(ship_id));
        }

        // Weapon cost = top-ups only (carried charge does not re-spend).
        let mut weapon_increases: u32 = 0;
        for (weapon_id, charge) in weapons {
            let have = ship.weapon_charges.get(weapon_id).copied().unwrap_or(0);
            // Clamp the top-up at 0: a draft that tries to strip charge still
            // costs nothing extra (the engine would reject it on allocate, but
            // the preview should not panic on overflow).
            weapon_increases = weapon_increases.saturating_add(charge.saturating_sub(have));
        }
        let shield_power: u32 = shields.iter().copied().sum();
        let available = ship.effective_power();

        // Reserve weapons + shields first; movement gets the remainder.
        let reserved = weapon_increases.saturating_add(shield_power);
        let movement_budget = available.saturating_sub(reserved);
        Ok(movement.min(movement_budget))
    }

    /// Clamped movement preview (ADR-0022).
    ///
    /// Like `movement_preview`, but instead of hard-rejecting an over-allocated
    /// draft, clamps movement power down to the affordable residual and returns
    /// the reachable set for that clamped thrust. Weapon/shield *validity*
    /// (charge caps, per-facing caps, no-stripping) is still enforced — only
    /// the *total* power budget is relaxed. This gives the TUI a smooth preview
    /// while a player drags the movement slider past the affordable limit.
    pub fn movement_preview_clamped(
        &self,
        ship_id: u32,
        movement: u32,
        weapons: BTreeMap<String, u32>,
        shields: [u32; 6],
    ) -> Result<crate::movement_preview::PreviewResult, crate::movement::OrderError> {
        if !matches!(self.phase, Phase::Allocate | Phase::Movement) {
            return Err(crate::movement::OrderError::WrongPhase {
                expected: "allocate or movement",
                actual: self.phase_name(),
            });
        }

        let ship = self
            .ship(ship_id)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship_id))?;
        if ship.destroyed {
            return Err(crate::movement::OrderError::ShipNotFound(ship_id));
        }

        // Enforce weapon/shield validity (caps, no-stripping) exactly as
        // allocate_v2 would — only the total budget is relaxed, not field rules.
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
            let have = ship.weapon_charges.get(weapon_id).copied().unwrap_or(0);
            if *charge < have {
                return Err(crate::movement::OrderError::CannotStripWeaponCharge {
                    ship: ship_id,
                    weapon: weapon_id.clone(),
                    have,
                    want: *charge,
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

        // Clamp movement to the affordable residual and delegate to the strict
        // preview (which will now pass the budget check).
        let clamped = self.clamp_movement_power(ship_id, movement, &weapons, &shields)?;
        self.movement_preview(ship_id, clamped, weapons, shields)
    }

    fn all_living_allocated(&self) -> bool {
        self.ships
            .iter()
            .filter(|ship| !ship.destroyed)
            .all(|ship| self.allocated_this_turn.contains(&ship.id))
    }

    /// True iff `ship` has already committed a maneuver for the current movement phase.
    pub fn has_committed_this_phase(&self, ship: u32) -> bool {
        self.maneuver_commits.contains_key(&ship)
    }

    fn begin_v2_movement_phase(&mut self) {
        self.phase = Phase::Movement;
        self.movement_phase = 1;
        self.maneuver_commits.clear();
    }

    /// Commit one maneuver for `ship` during the current movement phase (ADR-0022 M4).
    /// Validates legality and thrust affordability against the ship's *current* state
    /// without mutating anything; the maneuver is only applied once every living ship
    /// has committed (see `resolve_movement_phase`), so results never depend on commit
    /// order.
    pub fn commit_maneuver_v2(
        &mut self,
        ship_id: u32,
        maneuver: crate::motion::Maneuver,
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
        if self.maneuver_commits.contains_key(&ship_id) {
            return Err(crate::movement::OrderError::AlreadyCommittedThisPhase(
                ship_id,
            ));
        }
        let result = crate::motion::resolve_maneuver(
            ship.velocity,
            ship.facing,
            ship.max_velocity,
            maneuver,
        )
        .map_err(|err| crate::movement::OrderError::IllegalManeuver {
            ship: ship_id,
            reason: err.to_string(),
        })?;
        if result.thrust_cost > ship.thrust_remaining {
            return Err(crate::movement::OrderError::InsufficientThrust {
                ship: ship_id,
                need: result.thrust_cost,
                have: ship.thrust_remaining,
            });
        }

        self.maneuver_commits.insert(ship_id, maneuver);
        if self.all_living_committed() {
            self.resolve_movement_phase();
        }
        Ok(())
    }

    fn all_living_committed(&self) -> bool {
        self.ships
            .iter()
            .filter(|ship| !ship.destroyed)
            .all(|ship| self.maneuver_commits.contains_key(&ship.id))
    }

    /// Resolve every committed maneuver simultaneously, then slide each ship
    /// `speed` hexes along course (protocol 3 constant-rate translation), then
    /// open the fire window (or TurnEnd after phase 4 / scenario end).
    fn resolve_movement_phase(&mut self) {
        self.last_translation_outcomes.clear();
        // Step 1: apply each ship's committed maneuver (independent).
        let commits: Vec<(u32, crate::motion::Maneuver)> = self
            .maneuver_commits
            .iter()
            .map(|(id, m)| (*id, *m))
            .collect();
        for (ship_id, maneuver) in &commits {
            let Some(ship) = self.ship_mut(*ship_id) else {
                continue;
            };
            if let Ok(result) = crate::motion::resolve_maneuver(
                ship.velocity,
                ship.facing,
                ship.max_velocity,
                *maneuver,
            ) {
                ship.thrust_remaining = ship.thrust_remaining.saturating_sub(result.thrust_cost);
                ship.velocity = result.velocity;
                ship.facing = result.facing;
            }
        }

        // Step 2: lockstep multi-hex slide — each ship with speed S moves S hexes
        // along course this cycle (unless blocked). Steps are simultaneous.
        let max_step = self
            .ships
            .iter()
            .filter(|s| !s.destroyed)
            .map(|s| s.velocity.speed)
            .max()
            .unwrap_or(0);
        for step in 1..=max_step {
            self.resolve_one_hex_translation_step(step);
        }

        // Step 3: objective / phase advance.
        self.refresh_status();
        self.maneuver_commits.clear();
        self.fire_commits.clear();
        self.ready_fire.clear();
        if self.status != ScenarioStatus::InProgress {
            self.phase = Phase::TurnEnd;
            return;
        }
        self.phase = Phase::Firing;
    }

    /// One simultaneous hex of translation for every ship whose speed >= `step`.
    fn resolve_one_hex_translation_step(&mut self, step: u8) {
        let hard_map = self.board.mode.blocks_edges();
        let mut destination: BTreeMap<u32, Hex> = BTreeMap::new();
        let mut active: HashSet<u32> = HashSet::new();
        for ship in self
            .ships
            .iter()
            .filter(|s| !s.destroyed && s.velocity.speed >= step)
        {
            let Some(delta) = Hex::direction(ship.velocity.course) else {
                continue;
            };
            let dest = ship.pos + delta;
            if hard_map && !self.board.contains(dest) {
                self.last_translation_outcomes.insert(ship.id, false);
                continue;
            }
            destination.insert(ship.id, dest);
            active.insert(ship.id);
        }

        // Course lookup for pass-through (opposite courses sharing a hex).
        let course_of: BTreeMap<u32, u8> = self
            .ships
            .iter()
            .filter(|s| active.contains(&s.id))
            .map(|s| (s.id, s.velocity.course))
            .collect();

        loop {
            let mut changed = false;
            let mut claims: HashMap<Hex, Vec<u32>> = HashMap::new();
            for id in &active {
                claims.entry(destination[id]).or_default().push(*id);
            }
            for claimants in claims.values() {
                if claimants.len() <= 1 {
                    continue;
                }
                // Two ships on opposite courses claiming the same hex pass through
                // (head-on mid-hex meet). Other multi-claims still block everyone.
                if claimants.len() == 2 {
                    let c0 = course_of.get(&claimants[0]).copied().unwrap_or(0);
                    let c1 = course_of.get(&claimants[1]).copied().unwrap_or(0);
                    if crate::motion::opposite_dir(c0) == c1 {
                        continue;
                    }
                }
                for id in claimants {
                    if active.remove(id) {
                        changed = true;
                    }
                }
            }
            let stationary_positions: HashSet<Hex> = self
                .ships
                .iter()
                .filter(|s| !s.destroyed && !active.contains(&s.id))
                .map(|s| s.pos)
                .collect();
            let blocked_now: Vec<u32> = active
                .iter()
                .filter(|id| stationary_positions.contains(&destination[id]))
                .copied()
                .collect();
            for id in blocked_now {
                if active.remove(&id) {
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        for id in destination.keys() {
            // Last step's outcome wins for snapshot diagnostics.
            self.last_translation_outcomes
                .insert(*id, active.contains(id));
        }
        for id in &active {
            if let Some(ship) = self.ship_mut(*id) {
                ship.pos = destination[id];
            }
        }
        if !active.is_empty() {
            self.maybe_float_recenter();
        }
    }

    /// Translation outcomes from the most recently resolved movement phase.
    /// Entries exist only for ships that were scheduled to attempt translation.
    pub fn last_translation_outcomes(&self) -> &BTreeMap<u32, bool> {
        &self.last_translation_outcomes
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
        let kind = weapon.kind;
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
            let kind = weapon.kind;
            let range = attacker.pos.distance(target.pos);
            let threshold = crate::combat_tables::final_to_hit_threshold(
                kind,
                range,
                target.size,
                attacker.attack_accuracy_bonus,
            )
            .ok_or_else(|| crate::movement::OrderError::OutOfRange {
                weapon: commit.weapon.clone(),
                range,
                max_range: crate::combat_tables::max_range(kind),
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

        // Miss and hit both consume charge and mark the weapon fired this turn (ADR-0020 /
        // maintainer decision: a miss still uses the weapon).
        for (commit, hit, damage, _roll) in results {
            if let Some(attacker) = self.ship_mut(commit.ship) {
                attacker.weapon_charges.insert(commit.weapon.clone(), 0);
            }
            self.mark_weapon_fired(commit.ship, &commit.weapon);
            let (shield_absorbed, hull_damage) = if hit && damage > 0 {
                self.apply_v2_damage(commit.target, commit.shield_facing, damage)
            } else {
                (0, 0)
            };
            self.combat_log.push(CombatLogEvent {
                attacker: commit.ship,
                target: commit.target,
                weapon: commit.weapon.clone(),
                shield: commit.shield_facing as usize,
                damage,
                shield_absorbed,
                hull_damage,
                kind: if hit { "hit".into() } else { "miss".into() },
            });
        }

        self.fire_commits.clear();
        self.ready_fire.clear();
        self.refresh_status();
        // Turn-loop decision (ADR-0022 M5, frozen four-phase schedule): a finished scenario
        // parks at TurnEnd immediately; otherwise fire always returns to the next movement
        // phase, or ends the turn after phase 4. There is no re-entry heuristic — the phase
        // counter is the only termination condition.
        if self.status != ScenarioStatus::InProgress {
            self.phase = Phase::TurnEnd;
        } else if self.movement_phase < 4 {
            self.movement_phase += 1;
            self.phase = Phase::Movement;
            self.maneuver_commits.clear();
        } else {
            self.phase = Phase::TurnEnd;
        }
        Ok(())
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
            .any(|target| {
                self.v2_shot_shield_facing(attacker, weapon, target)
                    .is_some()
            })
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
        let kind = weapon.kind;
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

    /// Advisory for the UI: true iff some living ship could still fire legally. Movement
    /// is no longer optional under inertia (every living ship commits a maneuver each
    /// phase regardless), so this only tracks fire.
    /// Never blocks EndTurn — the client owns any confirm dialog.
    pub fn end_turn_warning(&self) -> bool {
        self.can_any_legal_fire()
    }

    /// Combat v2 EndTurn: always advances to the next turn's allocation. Legal in any phase
    /// after allocation. Pending commits/ready/maneuver decisions are discarded (consistent
    /// with the turn reset).
    pub fn end_turn_v2(&mut self) -> Result<(), crate::movement::OrderError> {
        if self.phase == Phase::Allocate {
            return Err(crate::movement::OrderError::EndTurnDuringAllocation);
        }
        self.fire_commits.clear();
        self.ready_fire.clear();
        self.maneuver_commits.clear();
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
        let kind = weapon.kind;
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

    fn apply_v2_damage(&mut self, target: u32, shield_facing: u8, damage: u32) -> (u32, u32) {
        let Some(ship) = self.ship_mut(target) else {
            return (0, 0);
        };
        let facing = (shield_facing % 6) as usize;
        let absorbed = ship.shields_remaining[facing].min(damage);
        ship.shields_remaining[facing] -= absorbed;
        let overflow = damage - absorbed;
        if overflow > 0 {
            ship.ssd.apply_internal(overflow);
            ship.destroyed = ship.ssd.is_destroyed();
        }
        (absorbed, overflow)
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

    /// True if this scenario has at least one ship that is not AI-controlled,
    /// living or not. Used to detect scenarios that were authored for headless
    /// self-play and should not be driven interactively.
    pub fn has_player_ship(&self) -> bool {
        self.ships.iter().any(|ship| !self.is_v2_ai(ship.id))
    }

    /// Combat v2 NPC auto-play. Plays greedy AI ships through the v2 phase machine:
    /// allocate un-allocated AI ships, emit the active AI mover's move/pass, commit each
    /// AI ship's legal shots then ready it, and (when only AI ships remain) end the turn to
    /// advance. Stops when a human ship must act, when the scenario is decided, or at a
    /// bounded step cap so it can never spin forever.
    pub fn resolve_v2_npc_actions(&mut self) {
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
                Phase::Movement => {
                    let next = self
                        .ships
                        .iter()
                        .find(|ship| {
                            !ship.destroyed
                                && self.is_v2_ai(ship.id)
                                && !self.has_committed_this_phase(ship.id)
                        })
                        .map(|ship| ship.id);
                    match next {
                        Some(id) => {
                            if let Some(maneuver) = crate::ai::v2_move_decision(self, id) {
                                let _ = self.commit_maneuver_v2(id, maneuver);
                            } else {
                                let _ = self.commit_maneuver_v2(id, crate::motion::Maneuver::Coast);
                            }
                        }
                        // A living non-AI ship still owes a maneuver commitment.
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

    /// Snapshot / UI label: `player`, `ai`, or `scripted` (ADR-0018).
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
        let positions: Vec<Hex> = self.ships.iter().map(|s| s.pos).collect();
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
pub(crate) enum NpcController {
    Scripted,
    GreedySeek,
}
