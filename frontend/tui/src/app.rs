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
    /// Map-focus: WASD/hjkl pans the map viewport. Read-only; no orders.
    /// Entered with `v` from Normal, exited with Esc/v. Essential for the
    /// unbounded world where ships can drift to negative coordinates.
    Map,
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
    /// Desired charge per weapon, **ship order** (same as fire list / TOML).
    pub weapons: Vec<(String, u32)>,
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
        let weapons = ship
            .weapons
            .iter()
            .map(|w| (w.id.clone(), w.charge))
            .collect();
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

    pub fn weapon_charge(&self, id: &str) -> Option<u32> {
        self.weapons
            .iter()
            .find(|(wid, _)| wid == id)
            .map(|(_, c)| *c)
    }

    pub fn set_weapon_charge(&mut self, id: &str, charge: u32) {
        if let Some((_, c)) = self.weapons.iter_mut().find(|(wid, _)| wid == id) {
            *c = charge;
        }
    }

    /// Weapons map for the allocate order JSON.
    ///
    /// Omits non-operational weapons: the engine rejects unknown/destroyed
    /// weapon ids with `weapon … not found`. Dead rows stay in the draft for
    /// display only (OFFLINE).
    pub fn weapons_json(&self, ship: &protocol::Ship) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for (id, chg) in &self.weapons {
            let operational = ship
                .weapons
                .iter()
                .find(|w| &w.id == id)
                .map(|w| w.operational)
                .unwrap_or(false);
            if operational {
                map.insert(id.clone(), serde_json::json!(chg));
            }
        }
        serde_json::Value::Object(map)
    }

    /// Sum of all draft field values (desired totals, not engine cost).
    pub fn power_spent(&self) -> u32 {
        let weapons: u32 = self.weapons.iter().map(|(_, c)| c).sum();
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

    pub fn n_fields(&self) -> usize {
        1 + self.weapons.len() + 6
    }

    /// Current value of the field under `cursor` (flat: mov, weapons…, shields…).
    pub fn field_value(&self) -> u32 {
        let n_weapons = self.weapons.len();
        if self.cursor == 0 {
            self.movement
        } else if self.cursor <= n_weapons {
            self.weapons
                .get(self.cursor - 1)
                .map(|(_, c)| *c)
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
            if let Some((_, c)) = self.weapons.get_mut(self.cursor - 1) {
                *c = value;
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

impl FireDraft {
    /// New draft with `weapon_idx` snapped to the first operational weapon
    /// (if any). Avoids opening fire mode on an OFFLINE gun (Phase 2).
    pub fn for_ship(ship: &protocol::Ship) -> Self {
        let mut draft = Self::default();
        draft.snap_to_operational(ship);
        draft
    }

    /// If the selected weapon is missing or offline, move to the first
    /// operational weapon. No-op when every weapon is offline.
    pub fn snap_to_operational(&mut self, ship: &protocol::Ship) {
        let still_ok = ship
            .weapons
            .get(self.weapon_idx)
            .map(|w| w.operational)
            .unwrap_or(false);
        if still_ok {
            return;
        }
        if let Some(idx) = ship.weapons.iter().position(|w| w.operational) {
            self.weapon_idx = idx;
        }
    }
}

/// The full application state.
pub struct App {
    pub snap: Option<Snapshot>,
    pub mode: Mode,
    pub focused_ship: Option<i64>,
    /// Map viewport pan offset (q, r) of the top-left visible hex. `None`
    /// means auto-fit all living ships plus the active preview envelope.
    pub map_pan: Option<(i32, i32)>,
    /// Manual zoom override. `None` means choose the clearest zoom that keeps
    /// all ships and preview endpoints visible.
    pub map_zoom: Option<i8>,
    pub alloc_draft: Option<AllocDraft>,
    pub fire_draft: Option<FireDraft>,
    /// Drafts parked while the player inspects or commands another ship.
    alloc_drafts: std::collections::BTreeMap<i64, AllocDraft>,
    /// Fire selections parked per source ship for the same reason.
    fire_drafts: std::collections::BTreeMap<i64, FireDraft>,
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
    /// Pending movement-preview request JSON (built by input handlers, drained
    /// by main loop). Unlike `pending_order`, this is a read-only query that
    /// returns a `movement_preview` envelope, not a snapshot.
    pub pending_preview: Option<String>,
    /// Last movement-preview response (for rendering reachable endpoints on
    /// the map and the projected position in the ship status panel).
    pub movement_preview: Option<crate::protocol::MovementPreview>,
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
            map_pan: None,
            map_zoom: None,
            alloc_draft: None,
            fire_draft: None,
            alloc_drafts: std::collections::BTreeMap::new(),
            fire_drafts: std::collections::BTreeMap::new(),
            last_error: None,
            recent_events: Vec::new(),
            combat_history: Vec::new(),
            last_combat_snapshot: Vec::new(),
            pending_order: None,
            pending_preview: None,
            movement_preview: None,
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
        // A fresh accepted snapshot resolves any previous soft rejection.
        self.last_error = None;
        // Auto-focus the player ship on the first snapshot.
        if self.focused_ship.is_none() {
            self.focused_ship = snap.player_ship().map(|s| s.id);
        }

        // Update mode based on phase.
        if snap.is_over() {
            self.mode = Mode::GameOver;
            self.movement_preview = None;
            self.pending_preview = None;
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
                    self.alloc_drafts.clear();
                    self.fire_drafts.clear();
                    self.movement_preview = None;
                    self.pending_preview = None;
                    if snap.phase == "allocate" {
                        if let Some(sid) = self.focused_ship {
                            self.alloc_draft = Some(AllocDraft::from_ship(&snap, sid));
                            self.mode = Mode::Allocate;
                        }
                    } else if snap.phase == "firing" {
                        self.fire_draft = snap
                            .player_ship()
                            .or_else(|| {
                                self.focused_ship
                                    .and_then(|id| snap.ship(id))
                            })
                            .map(FireDraft::for_ship)
                            .or_else(|| Some(FireDraft::default()));
                        self.mode = Mode::Fire;
                    } else if snap.phase == "movement" {
                        self.mode = Mode::Movement;
                    } else {
                        self.mode = Mode::Normal;
                    }
                } else if snap.phase == "firing" {
                    // Same phase: keep selection valid if a weapon went offline.
                    if let Some(draft) = self.fire_draft.as_mut() {
                        if let Some(ship) = self
                            .focused_ship
                            .and_then(|id| snap.ship(id))
                        {
                            draft.snap_to_operational(ship);
                        }
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
                    self.fire_draft = snap
                        .player_ship()
                        .or_else(|| {
                            self.focused_ship
                                .and_then(|id| snap.ship(id))
                        })
                        .map(FireDraft::for_ship)
                        .or_else(|| Some(FireDraft::default()));
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
        self.focus_next_pending_ship();
        self.confirm_tutorial_order();
        self.sync_tutorial_allocate_cursor();
        self.request_movement_preview();
    }

    /// Keep allocate cursor aligned with the tutorial step.
    ///
    /// - `ReachValue`: snap ▶ onto the field being edited (so → powers the right row).
    /// - `NavField`: do **not** force the cursor (forcing it made ↓ overshoot and
    ///   blocked powering). If ▶ is already on the target field, complete the
    ///   select step automatically.
    pub fn sync_tutorial_allocate_cursor(&mut self) {
        use crate::tutorial::ExpectedAction;
        loop {
            if self.mode != Mode::Allocate {
                return;
            }
            let Some(t) = self.tutorial.as_ref() else {
                return;
            };
            if t.is_complete() {
                return;
            }
            let expected = match t.current_step() {
                Some(s) => s.expected.clone(),
                None => return,
            };
            let Some(draft) = self.alloc_draft.as_mut() else {
                return;
            };
            let max = draft.n_fields().saturating_sub(1);
            match expected {
                ExpectedAction::ReachValue { field, .. } => {
                    draft.cursor = field.min(max);
                    return;
                }
                ExpectedAction::NavField(field) => {
                    if draft.cursor == field.min(max) {
                        // Already on the row — no need to force another ↓.
                        if let Some(t) = self.tutorial.as_mut() {
                            t.advance();
                        }
                        continue;
                    }
                    return;
                }
                _ => return,
            }
        }
    }

    /// Record a soft error from the engine.
    pub fn record_error(&mut self, err: &crate::protocol::ErrorResponse) {
        self.last_error = Some(format!("{}: {}", err.code, err.message));
        self.log.push(format!("ERROR: {}", err.message));
        self.tutorial_order_pending = false;
        self.tutorial_order_candidate = None;
        if err.code == "preview_invalid" {
            self.movement_preview = None;
        }
        if let Some(t) = self.tutorial.as_mut() {
            t.set_error(format!("Engine rejected that order: {}", err.message));
        }
    }

    /// Queue a read-only `movement_preview` request for the focused ship using
    /// the current alloc draft. The main loop drains `pending_preview` and
    /// stores the response in `movement_preview`. Local input clamping keeps
    /// every sent preview draft legal, so the engine can reject stale state
    /// rather than silently changing the requested movement value.
    pub fn request_movement_preview(&mut self) {
        let snap = match &self.snap {
            Some(s) => s,
            None => return,
        };
        if snap.phase != "allocate" {
            return;
        }
        let ship_id = match self.focused_ship {
            Some(id) => id,
            None => return,
        };
        let draft = match &self.alloc_draft {
            Some(d) => d,
            None => return,
        };
        let Some(ship) = snap.ship(ship_id) else {
            return;
        };
        // Same filter as allocate commit: never send offline weapon ids.
        let weapons = draft.weapons_json(ship);
        let shields: serde_json::Value =
            draft.shields.iter().map(|s| serde_json::json!(s)).collect();
        let req = serde_json::json!({
            "protocol_version": 3,
            "request": "movement_preview",
            "ship": ship_id,
            "movement": draft.movement,
            "weapons": weapons,
            "shields": shields,
        });
        self.pending_preview = Some(req.to_string());
    }

    /// Keep only a preview that belongs to the currently focused ship. This
    /// prevents an in-flight response from drawing one ship's envelope around
    /// another after focus changes.
    pub fn accept_movement_preview(&mut self, preview: crate::protocol::MovementPreview) {
        if self.focused_ship == Some(preview.ship)
            && self
                .snap
                .as_ref()
                .is_some_and(|snap| snap.phase == "allocate")
        {
            self.movement_preview = Some(preview);
        }
    }

    pub fn movement_preview_for_focus(&self) -> Option<&crate::protocol::MovementPreview> {
        self.active_preview()
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

    /// Effective zoom for a viewport with `columns × rows` cells. Negative
    /// values zoom out by grouping world hexes; positive values zoom in by
    /// using wider map cells.
    ///
    /// Auto mode picks the **finest** scale that still frames all content
    /// (prefer 1 hex/cell). Coarsest-first packing made distant ships look
    /// adjacent (e.g. d=8 rendered as neighboring glyphs at 8 hex/cell).
    pub fn effective_map_zoom(&self, columns: i32, rows: i32) -> i8 {
        if let Some(zoom) = self.map_zoom {
            return zoom;
        }
        let Some((min_q, max_q, min_r, max_r)) = self.map_content_bounds() else {
            return 0;
        };
        let span_q = (max_q - min_q + 3).max(1);
        let span_r = (max_r - min_r + 3).max(1);
        let cols = columns.max(1);
        let rows = rows.max(1);
        // Finest first: scale 1, 2, 4, 8 → zoom 0, -1, -2, -3.
        for zoom_out in 0..=3 {
            let scale = 1_i32 << zoom_out;
            if span_q <= cols * scale && span_r <= rows * scale {
                return -(zoom_out as i8);
            }
        }
        -3
    }

    /// Effective top-left world hex for a viewport. In automatic mode, the
    /// camera frames every living ship and the focused ship's preview.
    pub fn map_origin(&self) -> (i32, i32) {
        self.map_origin_for_view(10, 10, 1)
    }

    pub fn map_origin_for_view(&self, columns: i32, rows: i32, scale: i32) -> (i32, i32) {
        if let Some(pan) = self.map_pan {
            return pan;
        }
        if self
            .snap
            .as_ref()
            .is_some_and(|snap| snap.map.mode == "hard")
        {
            return (0, 0);
        }
        let Some((min_q, max_q, min_r, max_r)) = self.map_content_bounds() else {
            return (0, 0);
        };
        let width = (columns.max(1) * scale.max(1)).max(max_q - min_q + 1);
        let height = (rows.max(1) * scale.max(1)).max(max_r - min_r + 1);
        ((min_q + max_q - width) / 2, (min_r + max_r - height) / 2)
    }

    /// Pan the map by (dq, dr). Sets the explicit offset (disabling auto-center)
    /// so the player's manual pan sticks.
    pub fn pan_map(&mut self, dq: i32, dr: i32) {
        let (q, r) = self.map_origin();
        self.map_pan = Some((q + dq, r + dr));
    }

    /// Reset the pan to auto-center (called on focus change and Map-mode entry).
    pub fn reset_map_pan(&mut self) {
        self.map_pan = None;
        self.map_zoom = None;
    }

    pub fn adjust_map_zoom(&mut self, delta: i8) {
        let zoom = self
            .map_zoom
            .unwrap_or(0)
            .saturating_add(delta)
            .clamp(-3, 3);
        self.map_zoom = Some(zoom);
    }

    /// Exit Map mode, restoring the phase-appropriate input mode.
    ///
    /// Since `v` can be pressed from Allocate/Movement/Fire (the phase
    /// auto-switch leaves the app there), exiting Map mode must return to the
    /// mode matching the current phase — not unconditionally Normal.
    pub fn exit_map_mode(&mut self) {
        let phase = self.snap.as_ref().map(|s| s.phase.as_str()).unwrap_or("");
        self.mode = match phase {
            "allocate" => Mode::Allocate,
            "movement" => Mode::Movement,
            "firing" => Mode::Fire,
            _ => Mode::Normal,
        };
    }

    /// Switch command focus without allowing a local draft to follow the
    /// previous ship. The engine still owns order legality; this only keeps
    /// the client from sending A1's draft with A2's id.
    pub fn switch_focus(&mut self, ship_id: i64) {
        if self.focused_ship == Some(ship_id) {
            return;
        }

        if let Some(previous) = self.focused_ship {
            if let Some(draft) = self.alloc_draft.take() {
                self.alloc_drafts.insert(previous, draft);
            }
            if let Some(draft) = self.fire_draft.take() {
                self.fire_drafts.insert(previous, draft);
            }
        }

        self.focused_ship = Some(ship_id);
        self.movement_preview = None;
        self.pending_preview = None;
        // Recenter the map on the newly focused ship (clears manual view state).
        self.reset_map_pan();
        match self.mode {
            Mode::Allocate => self.open_allocate_for_focus(),
            Mode::Fire => self.open_fire_for_focus(),
            Mode::Normal | Mode::Movement | Mode::GameOver | Mode::Map => {}
        }
        self.request_movement_preview();
    }

    pub fn open_allocate_for_focus(&mut self) {
        let Some(ship_id) = self.focused_ship else {
            return;
        };
        if self
            .snap
            .as_ref()
            .and_then(|snap| snap.ship(ship_id))
            .is_none_or(|ship| ship.controller != "player")
        {
            return;
        }
        let draft = self
            .alloc_draft
            .take()
            .or_else(|| self.alloc_drafts.remove(&ship_id))
            .or_else(|| {
                self.snap
                    .as_ref()
                    .map(|snap| AllocDraft::from_ship(snap, ship_id))
            });
        self.alloc_draft = draft;
        self.mode = Mode::Allocate;
        self.request_movement_preview();
    }

    pub fn open_fire_for_focus(&mut self) {
        let Some(ship_id) = self.focused_ship else {
            return;
        };
        let ship = self.snap.as_ref().and_then(|s| s.ship(ship_id)).cloned();
        let mut draft = self
            .fire_draft
            .take()
            .or_else(|| self.fire_drafts.remove(&ship_id))
            .unwrap_or_else(|| {
                ship.as_ref()
                    .map(FireDraft::for_ship)
                    .unwrap_or_default()
            });
        if let Some(ship) = ship.as_ref() {
            draft.snap_to_operational(ship);
        }
        self.fire_draft = Some(draft);
        self.mode = Mode::Fire;
    }

    fn focus_next_pending_ship(&mut self) {
        let next = {
            let Some(snap) = self.snap.as_ref() else {
                return;
            };
            let Some(current) = self.focused_ship else {
                return;
            };
            let completed = match snap.phase.as_str() {
                "allocate" => &snap.ships_allocated_this_turn,
                "movement" => &snap.ships_committed_this_phase,
                "firing" => &snap.ships_ready_fire,
                _ => return,
            };
            if !completed.contains(&current) {
                return;
            }
            snap.ships
                .iter()
                .find(|ship| {
                    ship.controller == "player" && !ship.destroyed && !completed.contains(&ship.id)
                })
                .map(|ship| ship.id)
        };

        if let Some(next) = next {
            self.switch_focus(next);
        }
    }

    fn active_preview(&self) -> Option<&crate::protocol::MovementPreview> {
        self.movement_preview
            .as_ref()
            .filter(|preview| self.focused_ship == Some(preview.ship))
    }

    fn map_content_bounds(&self) -> Option<(i32, i32, i32, i32)> {
        let snap = self.snap.as_ref()?;
        let mut points: Vec<(i32, i32)> = snap
            .ships
            .iter()
            .filter(|ship| !ship.destroyed)
            .map(|ship| (ship.q, ship.r))
            .collect();
        if let Some(preview) = self.active_preview() {
            points.extend(
                preview
                    .endpoints
                    .iter()
                    .map(|endpoint| (endpoint.q, endpoint.r)),
            );
            points.push((preview.coast.q, preview.coast.r));
        }
        let min_q = points.iter().map(|(q, _)| *q).min()?;
        let max_q = points.iter().map(|(q, _)| *q).max()?;
        let min_r = points.iter().map(|(_, r)| *r).min()?;
        let max_r = points.iter().map(|(_, r)| *r).max()?;
        Some((min_q, max_q, min_r, max_r))
    }
}
