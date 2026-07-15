//! Application state for the TUI.
//!
//! The TUI is a thin client: it holds the latest snapshot from the engine,
//! tracks which ship is focused and what input mode the player is in, and
//! builds orders to send back. It never recomputes rules.

#![allow(dead_code)]

use crate::protocol::{self, Order, Snapshot};

/// Which input panel is active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Browsing — no input panel focused. Shows help.
    Normal,
    /// Allocate phase: drafting power distribution.
    Allocate,
    /// Movement phase: choosing a maneuver.
    Movement,
    /// Fire phase: choosing a weapon + target.
    Fire,
    /// Game over screen.
    GameOver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    Quit,
    EndTurn,
}

/// A local allocate draft (not yet sent to the engine).
#[derive(Debug, Clone, Default)]
pub struct AllocDraft {
    pub movement: u32,
    /// weapon_id → desired charge
    pub weapons: std::collections::BTreeMap<String, u32>,
    /// 6 shield facings, desired power each
    pub shields: [u32; 6],
    /// Which sub-field is being edited: 0=movement, 1..=n_weapons=weapons, then shields
    pub cursor: usize,
}

impl AllocDraft {
    pub fn from_ship(snap: &Snapshot, ship_id: i64) -> Self {
        let ship = match snap.ship(ship_id) {
            Some(s) => s,
            None => return Self::default(),
        };
        let mut weapons = std::collections::BTreeMap::new();
        for w in &ship.weapons {
            weapons.insert(w.id.clone(), w.charge);
        }
        let shields = if ship.shields_powered.len() == 6 {
            let mut s = [0u32; 6];
            s.copy_from_slice(&ship.shields_powered);
            s
        } else {
            [0u32; 6]
        };
        AllocDraft {
            movement: ship.movement_allocated,
            weapons,
            shields,
            cursor: 0,
        }
    }

    /// Sum of all draft field values (desired totals, not engine cost).
    pub fn power_spent(&self) -> u32 {
        let weapons: u32 = self.weapons.values().sum();
        let shields: u32 = self.shields.iter().sum();
        self.movement + weapons + shields
    }

    /// Engine allocate cost: movement + shield power + weapon *top-ups* only
    /// (carried charge does not re-spend).
    pub fn power_cost(&self, ship: &protocol::Ship) -> u32 {
        let weapons: u32 = self
            .weapons
            .iter()
            .map(|(id, desired)| {
                let current = ship
                    .weapons
                    .iter()
                    .find(|w| &w.id == id)
                    .map(|w| w.charge)
                    .unwrap_or(0);
                desired.saturating_sub(current)
            })
            .sum();
        let shields: u32 = self.shields.iter().sum();
        self.movement + weapons + shields
    }

    /// Current value of the field under `cursor` (flat: mov, weapons…, shields…).
    pub fn field_value(&self) -> u32 {
        let n_weapons = self.weapons.len();
        if self.cursor == 0 {
            self.movement
        } else if self.cursor <= n_weapons {
            self.weapons
                .values()
                .nth(self.cursor - 1)
                .copied()
                .unwrap_or(0)
        } else {
            let idx = self.cursor - 1 - n_weapons;
            self.shields.get(idx).copied().unwrap_or(0)
        }
    }

    /// Set the field under `cursor` to an absolute value (clamped ≥ 0).
    pub fn set_field_value(&mut self, value: u32) {
        let n_weapons = self.weapons.len();
        if self.cursor == 0 {
            self.movement = value;
        } else if self.cursor <= n_weapons {
            if let Some(k) = self.weapons.keys().nth(self.cursor - 1).cloned() {
                self.weapons.insert(k, value);
            }
        } else {
            let idx = self.cursor - 1 - n_weapons;
            if idx < 6 {
                self.shields[idx] = value;
            }
        }
    }
}

/// A local fire draft.
#[derive(Debug, Clone, Default)]
pub struct FireDraft {
    /// Index into the focused ship's weapons list.
    pub weapon_idx: usize,
    /// Target ship id.
    pub target: Option<i64>,
    /// Shield facing to hit (0..5).
    pub shield_facing: u32,
}

/// The full application state.
pub struct App {
    pub snap: Option<Snapshot>,
    pub mode: Mode,
    pub focused_ship: Option<i64>,
    pub alloc_draft: Option<AllocDraft>,
    pub fire_draft: Option<FireDraft>,
    /// Last soft error message from the engine (shown briefly).
    pub last_error: Option<String>,
    /// Recent combat events for the events panel.
    pub recent_events: Vec<String>,
    /// Session combat history, retained after the engine clears turn-scoped
    /// combat_log at end-turn.
    pub combat_history: Vec<String>,
    last_combat_snapshot: Vec<String>,
    /// Pending orders to send (built by input handlers, drained by main loop).
    pub pending_order: Option<Order>,
    /// True if the engine subprocess has exited.
    pub engine_dead: bool,
    /// Message log lines (for the log panel).
    pub log: Vec<String>,
    /// Active tutorial controller (None in free play).
    pub tutorial: Option<crate::tutorial::Tutorial>,
    /// An order-backed tutorial step is held here until the engine accepts it.
    pub tutorial_order_pending: bool,
    /// Validated tutorial action waiting for the input handler to emit its order.
    pub tutorial_order_candidate: Option<crate::tutorial::ExpectedAction>,
    /// Confirmation dialog for destructive global actions.
    pub confirmation: Option<Confirmation>,
    /// Digits typed into the current allocate field, shown as a fresh entry.
    pub digit_entry: Option<(usize, u32)>,
    /// Set by rendering when the terminal is below the Small-tier floor.
    pub terminal_too_small: bool,
}

impl App {
    pub fn new() -> Self {
        App {
            snap: None,
            mode: Mode::Normal,
            focused_ship: None,
            alloc_draft: None,
            fire_draft: None,
            last_error: None,
            recent_events: Vec::new(),
            combat_history: Vec::new(),
            last_combat_snapshot: Vec::new(),
            pending_order: None,
            engine_dead: false,
            log: Vec::new(),
            tutorial: None,
            tutorial_order_pending: false,
            tutorial_order_candidate: None,
            confirmation: None,
            digit_entry: None,
            terminal_too_small: false,
        }
    }

    /// Create a new App with tutorial mode enabled.
    pub fn new_with_tutorial() -> Self {
        let mut app = Self::new();
        app.tutorial = Some(crate::tutorial::Tutorial::new());
        app
    }

    /// Called when a new snapshot arrives from the engine.
    pub fn update_snapshot(&mut self, snap: Snapshot) {
        self.digit_entry = None;
        // Auto-focus the player ship on the first snapshot.
        if self.focused_ship.is_none() {
            self.focused_ship = snap.player_ship().map(|s| s.id);
        }

        // Update mode based on phase.
        if snap.is_over() {
            self.mode = Mode::GameOver;
        } else if self.mode == Mode::GameOver {
            // stay in game over
        } else {
            // Auto-switch to the phase-appropriate mode if in Normal.
            if self.mode == Mode::Normal {
                self.mode = match snap.phase.as_str() {
                    "allocate" => Mode::Allocate,
                    "movement" => Mode::Movement,
                    "firing" => Mode::Fire,
                    _ => Mode::Normal,
                };
            }
            // If the phase changed, reset drafts.
            if let Some(ref s) = self.snap {
                if s.phase != snap.phase {
                    self.alloc_draft = None;
                    self.fire_draft = None;
                    if snap.phase == "allocate" {
                        if let Some(sid) = self.focused_ship {
                            self.alloc_draft = Some(AllocDraft::from_ship(&snap, sid));
                            self.mode = Mode::Allocate;
                        }
                    } else if snap.phase == "firing" {
                        self.fire_draft = Some(FireDraft::default());
                        self.mode = Mode::Fire;
                    } else if snap.phase == "movement" {
                        self.mode = Mode::Movement;
                    } else {
                        self.mode = Mode::Normal;
                    }
                }
            } else {
                // First snapshot: set up drafts.
                if snap.phase == "allocate" {
                    if let Some(sid) = self.focused_ship {
                        self.alloc_draft = Some(AllocDraft::from_ship(&snap, sid));
                        self.mode = Mode::Allocate;
                    }
                } else if snap.phase == "firing" {
                    self.fire_draft = Some(FireDraft::default());
                    self.mode = Mode::Fire;
                } else if snap.phase == "movement" {
                    self.mode = Mode::Movement;
                }
            }
        }

        // Keep the complete resolution visible, including the player's own
        // volley. The old UI only retained the first line in a tiny panel.
        self.recent_events.clear();
        let mut current_events = Vec::new();
        for e in &snap.combat_log {
            let atk = snap
                .ship(e.attacker)
                .map(protocol::callsign)
                .unwrap_or_else(|| format!("#{}", e.attacker));
            let tgt = snap
                .ship(e.target)
                .map(protocol::callsign)
                .unwrap_or_else(|| format!("#{}", e.target));
            let tag = if e.kind == "hit" { "HIT" } else { "MISS" };
            let result = if e.kind == "hit" {
                format!(
                    " +{} sh-{} hull-{}",
                    e.damage, e.shield_absorbed, e.hull_damage
                )
            } else {
                String::new()
            };
            current_events.push(format!("{atk} {}>{tgt} {tag}{result}", e.weapon));
        }
        self.recent_events = current_events.clone();
        if current_events != self.last_combat_snapshot {
            let common = self
                .last_combat_snapshot
                .iter()
                .zip(&current_events)
                .take_while(|(old, new)| old == new)
                .count();
            self.combat_history
                .extend(current_events.iter().skip(common).cloned());
            if self.combat_history.len() > 200 {
                let drop = self.combat_history.len() - 200;
                self.combat_history.drain(..drop);
            }
            self.last_combat_snapshot = current_events;
        }

        self.snap = Some(snap);
        self.confirm_tutorial_order();
    }

    /// Record a soft error from the engine.
    pub fn record_error(&mut self, err: &crate::protocol::ErrorResponse) {
        self.last_error = Some(format!("{}: {}", err.code, err.message));
        self.log.push(format!("ERROR: {}", err.message));
        self.tutorial_order_pending = false;
        self.tutorial_order_candidate = None;
        if let Some(t) = self.tutorial.as_mut() {
            t.set_error(format!("Engine rejected that order: {}", err.message));
        }
    }

    /// Commit a tutorial step only after the corresponding order produced a
    /// snapshot rather than an engine error.
    pub fn confirm_tutorial_order(&mut self) {
        if self.tutorial_order_pending {
            if let Some(t) = self.tutorial.as_mut() {
                t.advance();
            }
            self.tutorial_order_pending = false;
        }
    }

    pub fn mark_tutorial_order_emitted(&mut self) {
        if self.tutorial_order_candidate.take().is_some() {
            self.tutorial_order_pending = true;
        }
    }

    /// Push a log line.
    pub fn log(&mut self, msg: impl Into<String>) {
        self.log.push(msg.into());
        if self.log.len() > 100 {
            self.log.remove(0);
        }
    }

    /// The currently focused ship, if any.
    pub fn focused(&self) -> Option<&protocol::Ship> {
        self.snap
            .as_ref()
            .and_then(|s| self.focused_ship.and_then(|id| s.ship(id)))
    }
}
