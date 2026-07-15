//! Keyboard input handling.
//!
//! Translates key events into app state changes and pending orders.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Confirmation, Mode};
use crate::protocol::{Maneuver, Order};
use crate::tutorial::ExpectedAction;

/// Result of handling a key.
pub enum KeyResult {
    /// Continue running.
    Continue,
    /// Quit the app.
    Quit,
    /// An order was produced — send it to the engine.
    SendOrder(Order),
}

/// Handle a key event.
pub fn handle_key(app: &mut App, key: KeyEvent) -> KeyResult {
    if let Some(confirmation) = app.confirmation {
        return handle_confirmation(app, confirmation, key);
    }
    if app.terminal_too_small && key.code != KeyCode::Char('q') {
        return KeyResult::Continue;
    }

    let tutorial_active = app
        .tutorial
        .as_ref()
        .map(|t| !t.is_complete())
        .unwrap_or(false);

    // Keep the small global escape hatch explicit. Every other key goes
    // through the lesson gate before mode handlers, so adding a new global
    // binding cannot silently bypass tutorial validation.
    if tutorial_active && !matches!(key.code, KeyCode::Char('q') | KeyCode::Esc | KeyCode::Tab) {
        if let Some(result) = tutorial_gate(app, &key) {
            return result;
        }
    }

    // Global keys
    match key.code {
        KeyCode::Char('q') => {
            if app.snap.is_none() || app.snap.as_ref().map(|s| s.is_over()).unwrap_or(false) {
                return KeyResult::Quit;
            }
            app.confirmation = Some(Confirmation::Quit);
            app.log("quit requested — press y to confirm, n/Esc to cancel");
            return KeyResult::Continue;
        }
        KeyCode::Esc => {
            if tutorial_active {
                reopen_tutorial_mode(
                    app,
                    "Esc does not cancel the lesson; the expected form is open again.",
                );
            } else {
                app.mode = Mode::Normal;
                app.last_error = None;
            }
            return KeyResult::Continue;
        }
        KeyCode::Tab => {
            if tutorial_active {
                if let Some(t) = app.tutorial.as_mut() {
                    t.set_error("Tab is disabled during the lesson; use ↓/↑ for the active form.");
                }
            } else {
                cycle_ship_focus(app);
            }
            return KeyResult::Continue;
        }
        _ => {}
    }

    // End turn is guarded because it can discard queued fire and skip useful
    // actions. The engine's end_turn_warning is shown alongside this prompt.
    if key.code == KeyCode::Char('e') && app.mode != Mode::GameOver && app.snap.is_some() {
        app.confirmation = Some(Confirmation::EndTurn);
        app.log("end turn requested — press y to confirm, n/Esc to cancel");
        return KeyResult::Continue;
    }

    match app.mode {
        Mode::Normal => handle_normal(app, key),
        Mode::Allocate => handle_allocate(app, key),
        Mode::Movement => handle_movement(app, key),
        Mode::Fire => handle_fire(app, key),
        Mode::GameOver => {
            if key.code == KeyCode::Enter {
                // Dismiss is local only (tutorial advance already done in gate).
                return KeyResult::Continue;
            }
            KeyResult::Continue
        }
    }
}

fn handle_confirmation(app: &mut App, confirmation: Confirmation, key: KeyEvent) -> KeyResult {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            app.confirmation = None;
            match confirmation {
                Confirmation::Quit => KeyResult::Quit,
                Confirmation::EndTurn => {
                    let synthetic = KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE);
                    if app.tutorial.is_some() {
                        if let Some(result) = tutorial_gate(app, &synthetic) {
                            return result;
                        }
                    }
                    app.log("end_turn");
                    emit_order(app, Order::end_turn())
                }
            }
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.confirmation = None;
            app.tutorial_order_candidate = None;
            app.tutorial_order_pending = false;
            app.log("cancelled");
            KeyResult::Continue
        }
        _ => KeyResult::Continue,
    }
}

fn reopen_tutorial_mode(app: &mut App, message: &str) {
    let Some(snap) = app.snap.clone() else { return };
    if let Some(player) = snap.player_ship() {
        app.focused_ship = Some(player.id);
    }
    match snap.phase.as_str() {
        "allocate" => {
            if app.alloc_draft.is_none() {
                app.alloc_draft = app
                    .focused_ship
                    .map(|sid| crate::app::AllocDraft::from_ship(&snap, sid));
            }
            app.fire_draft = None;
            app.mode = Mode::Allocate;
        }
        "movement" => {
            app.alloc_draft = None;
            app.fire_draft = None;
            app.mode = Mode::Movement;
        }
        "firing" => {
            app.alloc_draft = None;
            if app.fire_draft.is_none() {
                app.fire_draft = Some(crate::app::FireDraft::default());
            }
            app.mode = Mode::Fire;
        }
        _ => app.mode = Mode::Normal,
    }
    app.digit_entry = None;
    app.last_error = None;
    if let Some(t) = app.tutorial.as_mut() {
        t.set_error(message);
    }
}

fn cycle_ship_focus(app: &mut App) {
    if let Some(snap) = app.snap.clone() {
        let living: Vec<i64> = snap
            .ships
            .iter()
            .filter(|s| s.controller == "player" && !s.destroyed)
            .map(|s| s.id)
            .collect();
        if living.is_empty() {
            return;
        }
        let next = match app.focused_ship {
            Some(id) => {
                let idx = living.iter().position(|&x| x == id);
                match idx {
                    Some(i) => living[(i + 1) % living.len()],
                    None => living[0],
                }
            }
            None => living[0],
        };
        app.switch_focus(next);
    }
}

/// Tutorial gate. Returns `Some(Continue)` if the key is blocked.
/// Returns `None` if the key is allowed (and tutorial may have advanced).
fn tutorial_gate(app: &mut App, key: &KeyEvent) -> Option<KeyResult> {
    if app
        .tutorial
        .as_ref()
        .map(|t| t.is_complete())
        .unwrap_or(true)
    {
        return None;
    }

    let snap = app.snap.clone()?;
    let step_expected = app.tutorial.as_ref()?.current_step()?.expected.clone();

    // ── Allocate: ReachValue ───────────────────────────────────────────
    if let ExpectedAction::ReachValue {
        field: need_field, ..
    } = step_expected
    {
        if app.mode != Mode::Allocate {
            reopen_tutorial_mode(
                app,
                "Allocate form reopened; continue with the highlighted field.",
            );
            return Some(KeyResult::Continue);
        }
        let (field, old) = {
            let draft = app.alloc_draft.as_ref()?;
            (draft.cursor, draft.field_value())
        };

        // Allow ↓/↑ to recover if the cursor is on the wrong field.
        if field != need_field {
            match key.code {
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Up | KeyCode::Char('k') => {
                    // Fall through to allocate handler (no advance until on field).
                    app.tutorial.as_mut().unwrap().error_msg = Some(format!(
                        "Move ▶ to field {need_field} (now on {field}). ↓ next · ↑ previous."
                    ));
                    return None;
                }
                _ => {
                    app.tutorial.as_mut().unwrap().set_error(format!(
                        "▶ is on field {field}; need field {need_field}. Press ↓ or ↑."
                    ));
                    return Some(KeyResult::Continue);
                }
            }
        }

        let new = match key.code {
            KeyCode::Right => old.saturating_add(1),
            KeyCode::Left => old.saturating_sub(1),
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let d = (c as u8 - b'0') as u32;
                digit_entry(app, field, d)
            }
            KeyCode::Backspace => {
                app.digit_entry = None;
                0
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Up | KeyCode::Char('k') => {
                app.tutorial
                    .as_mut()
                    .unwrap()
                    .set_error("Stay on this field — use → / ← to set the value (↓ moves away).");
                return Some(KeyResult::Continue);
            }
            _ => {
                let hint = app
                    .tutorial
                    .as_ref()
                    .and_then(|t| t.current_step())
                    .map(|s| s.hint)
                    .unwrap_or("");
                app.tutorial
                    .as_mut()
                    .unwrap()
                    .set_error(format!("Use → to raise, ← to lower (or digits). {hint}"));
                return Some(KeyResult::Continue);
            }
        };
        let (allow, _adv) = app
            .tutorial
            .as_mut()
            .unwrap()
            .check_reach_value(field, old, new);
        if !allow {
            return Some(KeyResult::Continue);
        }
        if let Some(draft) = app.alloc_draft.as_mut() {
            draft.set_field_value(new);
        }
        return Some(KeyResult::Continue);
    }

    // ── Map key → discrete ExpectedAction ──────────────────────────────
    let action = match map_key_to_action(app, &snap, key) {
        Some(a) => a,
        None => {
            if let Some(t) = app.tutorial.as_mut() {
                if let Some(step) = t.current_step() {
                    t.set_error(format!("Expected: {}. {}", step.title, step.hint));
                }
            }
            return Some(KeyResult::Continue);
        }
    };

    let order_backed = matches!(
        action,
        ExpectedAction::CommitAllocate
            | ExpectedAction::Accel
            | ExpectedAction::TurnTo(_)
            | ExpectedAction::Coast
            | ExpectedAction::FireWeapon
            | ExpectedAction::ReadyFire
            | ExpectedAction::EndTurn
    );
    let ok = if order_backed {
        app.tutorial.as_mut().unwrap().validate_action(&action)
    } else {
        app.tutorial.as_mut().unwrap().check_action(&action)
    };
    if !ok {
        return Some(KeyResult::Continue);
    }
    if order_backed {
        app.tutorial_order_candidate = Some(action);
    }
    // Allowed — fall through to normal handlers.
    None
}

fn handle_normal(app: &mut App, key: KeyEvent) -> KeyResult {
    let phase = match app.snap.as_ref() {
        Some(s) => s.phase.clone(),
        None => return KeyResult::Continue,
    };

    match key.code {
        KeyCode::Char('a') => {
            if phase == "allocate" {
                app.open_allocate_for_focus();
            }
            KeyResult::Continue
        }
        KeyCode::Char('m') => {
            if phase == "movement" {
                app.mode = Mode::Movement;
            }
            KeyResult::Continue
        }
        KeyCode::Char('f') => {
            if phase == "firing" {
                app.open_fire_for_focus();
            }
            KeyResult::Continue
        }
        // Maneuvers also work from Normal during movement (quick keys).
        KeyCode::Char('c') if phase == "movement" => send_coast(app),
        KeyCode::Char('t') if phase == "movement" => send_accel(app),
        KeyCode::Char(c) if phase == "movement" && c.is_ascii_digit() && c <= '5' => {
            let facing = (c as u8 - b'0') as u32;
            send_turn(app, facing)
        }
        KeyCode::Char(' ') if phase == "firing" => send_ready(app),
        KeyCode::Enter => {
            match phase.as_str() {
                "allocate" => app.open_allocate_for_focus(),
                "movement" => app.mode = Mode::Movement,
                "firing" => app.open_fire_for_focus(),
                _ => {}
            }
            KeyResult::Continue
        }
        _ => KeyResult::Continue,
    }
}

fn handle_allocate(app: &mut App, key: KeyEvent) -> KeyResult {
    if app.snap.is_none() {
        return KeyResult::Continue;
    }
    let sid = match app.focused_ship {
        Some(id) => id,
        None => return KeyResult::Continue,
    };

    match key.code {
        KeyCode::Enter => {
            let draft = match &app.alloc_draft {
                Some(d) => d.clone(),
                None => return KeyResult::Continue,
            };

            let weapons_json = serde_json::to_value(&draft.weapons).unwrap_or_default();
            let shields = draft.shields.to_vec();

            app.log(format!(
                "allocate: mv={} shields={:?}",
                draft.movement, shields
            ));
            app.digit_entry = None;
            emit_order(
                app,
                Order::allocate(sid, draft.movement, weapons_json, shields),
            )
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.digit_entry = None;
            if let Some(draft) = &mut app.alloc_draft {
                let n_fields = 1 + draft.weapons.len() + 6;
                draft.cursor = (draft.cursor + 1) % n_fields.max(1);
            }
            KeyResult::Continue
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.digit_entry = None;
            if let Some(draft) = &mut app.alloc_draft {
                let n_fields = 1 + draft.weapons.len() + 6;
                draft.cursor = (draft.cursor + n_fields - 1) % n_fields.max(1);
            }
            KeyResult::Continue
        }
        KeyCode::Left => {
            app.digit_entry = None;
            if let Some(draft) = &mut app.alloc_draft {
                adjust_field(draft, -1);
            }
            KeyResult::Continue
        }
        KeyCode::Right => {
            app.digit_entry = None;
            if let Some(draft) = &mut app.alloc_draft {
                adjust_field(draft, 1);
            }
            KeyResult::Continue
        }
        KeyCode::Char(c) if c.is_ascii_digit() => {
            let cursor = app.alloc_draft.as_ref().map(|d| d.cursor);
            if let Some(cursor) = cursor {
                let d = (c as u8 - b'0') as u32;
                let new = digit_entry(app, cursor, d);
                if let Some(draft) = &mut app.alloc_draft {
                    draft.set_field_value(new);
                }
            }
            KeyResult::Continue
        }
        KeyCode::Backspace => {
            app.digit_entry = None;
            if let Some(draft) = &mut app.alloc_draft {
                draft.set_field_value(0);
            }
            KeyResult::Continue
        }
        _ => KeyResult::Continue,
    }
}

fn adjust_field(draft: &mut crate::app::AllocDraft, delta: i32) {
    let v = draft.field_value() as i32 + delta;
    draft.set_field_value(v.max(0) as u32);
}

fn digit_entry(app: &mut App, field: usize, digit: u32) -> u32 {
    let value = match app.digit_entry {
        Some((same_field, pending)) if same_field == field => {
            pending.saturating_mul(10).saturating_add(digit).min(99)
        }
        _ => digit,
    };
    app.digit_entry = Some((field, value));
    value
}

fn handle_movement(app: &mut App, key: KeyEvent) -> KeyResult {
    match key.code {
        KeyCode::Char('c') => send_coast(app),
        KeyCode::Char('t') => send_accel(app),
        // Absolute facing 0–5 (preferred for multi-hex turns).
        KeyCode::Char(c) if c.is_ascii_digit() && c <= '5' => {
            let facing = (c as u8 - b'0') as u32;
            send_turn(app, facing)
        }
        // `r` = turn +1 facing (short ring step).
        KeyCode::Char('r') => {
            let facing = app.focused().map(|s| (s.facing + 1) % 6).unwrap_or(0);
            send_turn(app, facing)
        }
        _ => KeyResult::Continue,
    }
}

fn send_coast(app: &mut App) -> KeyResult {
    let sid = match app.focused_ship {
        Some(id) => id,
        None => return KeyResult::Continue,
    };
    app.log("maneuver: coast");
    emit_order(app, Order::commit_maneuver(sid, Maneuver::Coast))
}

fn send_accel(app: &mut App) -> KeyResult {
    let sid = match app.focused_ship {
        Some(id) => id,
        None => return KeyResult::Continue,
    };
    app.log("maneuver: accel");
    emit_order(app, Order::commit_maneuver(sid, Maneuver::Accel))
}

fn send_turn(app: &mut App, facing: u32) -> KeyResult {
    let sid = match app.focused_ship {
        Some(id) => id,
        None => return KeyResult::Continue,
    };
    app.log(format!("maneuver: turn facing {facing}"));
    emit_order(app, Order::commit_maneuver(sid, Maneuver::Turn { facing }))
}

fn send_ready(app: &mut App) -> KeyResult {
    let sid = match app.focused_ship {
        Some(id) => id,
        None => return KeyResult::Continue,
    };
    app.log("ready_fire");
    emit_order(app, Order::ready_fire(sid))
}

fn emit_order(app: &mut App, order: Order) -> KeyResult {
    app.mark_tutorial_order_emitted();
    KeyResult::SendOrder(order)
}

fn handle_fire(app: &mut App, key: KeyEvent) -> KeyResult {
    let snap = match &app.snap {
        Some(s) => s.clone(),
        None => return KeyResult::Continue,
    };
    let sid = match app.focused_ship {
        Some(id) => id,
        None => return KeyResult::Continue,
    };

    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(draft) = &mut app.fire_draft {
                let ship = snap.ship(sid);
                let n_weapons = ship.map(|s| s.weapons.len()).unwrap_or(1);
                draft.weapon_idx = (draft.weapon_idx + 1) % n_weapons.max(1);
            }
            KeyResult::Continue
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(draft) = &mut app.fire_draft {
                let ship = snap.ship(sid);
                let n_weapons = ship.map(|s| s.weapons.len()).unwrap_or(1);
                draft.weapon_idx = (draft.weapon_idx + n_weapons - 1) % n_weapons.max(1);
            }
            KeyResult::Continue
        }
        KeyCode::Left => {
            if let Some(draft) = &mut app.fire_draft {
                draft.shield_facing = (draft.shield_facing + 5) % 6;
            }
            KeyResult::Continue
        }
        KeyCode::Right => {
            if let Some(draft) = &mut app.fire_draft {
                draft.shield_facing = (draft.shield_facing + 1) % 6;
            }
            KeyResult::Continue
        }
        KeyCode::Enter => {
            let draft = match &app.fire_draft {
                Some(d) => d.clone(),
                None => return KeyResult::Continue,
            };
            let ship = match snap.ship(sid) {
                Some(s) => s,
                None => return KeyResult::Continue,
            };
            let weapon = match ship.weapons.get(draft.weapon_idx) {
                Some(w) => w.id.clone(),
                None => return KeyResult::Continue,
            };
            let target = match draft.target {
                Some(t) => t,
                None => {
                    let enemy = snap
                        .ships
                        .iter()
                        .find(|s| s.controller != "player" && !s.destroyed);
                    match enemy {
                        Some(e) => e.id,
                        None => return KeyResult::Continue,
                    }
                }
            };

            app.log(format!(
                "fire: {} → #{} shield={}",
                weapon, target, draft.shield_facing
            ));
            emit_order(
                app,
                Order::commit_fire(sid, &weapon, target, draft.shield_facing),
            )
        }
        KeyCode::Char(' ') => send_ready(app),
        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
            let idx = (c as u8 - b'1') as usize;
            let enemies: Vec<i64> = snap
                .ships
                .iter()
                .filter(|s| s.controller != "player" && !s.destroyed)
                .map(|s| s.id)
                .collect();
            if let Some(&tid) = enemies.get(idx) {
                if let Some(draft) = &mut app.fire_draft {
                    draft.target = Some(tid);
                }
            }
            KeyResult::Continue
        }
        _ => KeyResult::Continue,
    }
}

/// Map a key press to an ExpectedAction for discrete tutorial steps.
fn map_key_to_action(
    app: &App,
    snap: &crate::protocol::Snapshot,
    key: &KeyEvent,
) -> Option<ExpectedAction> {
    // End-turn is global.
    if key.code == KeyCode::Char('e') && app.mode != Mode::GameOver {
        return Some(ExpectedAction::EndTurn);
    }

    match app.mode {
        Mode::Normal => match key.code {
            KeyCode::Char('c') if snap.phase == "movement" => Some(ExpectedAction::Coast),
            KeyCode::Char('t') if snap.phase == "movement" => Some(ExpectedAction::Accel),
            KeyCode::Char(c) if snap.phase == "movement" && c.is_ascii_digit() && c <= '5' => {
                Some(ExpectedAction::TurnTo((c as u8 - b'0') as u32))
            }
            KeyCode::Char(' ') if snap.phase == "firing" => Some(ExpectedAction::ReadyFire),
            KeyCode::Char('f') | KeyCode::Enter if snap.phase == "firing" => {
                Some(ExpectedAction::EnterFire)
            }
            _ => None,
        },
        Mode::Allocate => {
            let draft = app.alloc_draft.as_ref()?;
            match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    let n_fields = 1 + draft.weapons.len() + 6;
                    let next = (draft.cursor + 1) % n_fields.max(1);
                    Some(ExpectedAction::NavField(next))
                }
                KeyCode::Enter => Some(ExpectedAction::CommitAllocate),
                // Arrows/digits for ReachValue are handled in tutorial_gate.
                _ => None,
            }
        }
        Mode::Movement => match key.code {
            KeyCode::Char('c') => Some(ExpectedAction::Coast),
            KeyCode::Char('t') => Some(ExpectedAction::Accel),
            KeyCode::Char(c) if c.is_ascii_digit() && c <= '5' => {
                Some(ExpectedAction::TurnTo((c as u8 - b'0') as u32))
            }
            KeyCode::Char('r') => {
                let facing = app.focused().map(|s| (s.facing + 1) % 6).unwrap_or(0);
                Some(ExpectedAction::TurnTo(facing))
            }
            // Space during movement shouldn't appear; if phase already advanced, handle below.
            KeyCode::Char(' ') if snap.phase == "firing" => Some(ExpectedAction::ReadyFire),
            _ => None,
        },
        Mode::Fire => match key.code {
            KeyCode::Enter => Some(ExpectedAction::FireWeapon),
            KeyCode::Down | KeyCode::Char('j') => Some(ExpectedAction::TabWeapon),
            KeyCode::Char(' ') => Some(ExpectedAction::ReadyFire),
            _ => None,
        },
        Mode::GameOver => match key.code {
            KeyCode::Enter => Some(ExpectedAction::Dismiss),
            _ => None,
        },
    }
}
