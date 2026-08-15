//! Keyboard input handling.
//!
//! Translates key events into app state changes and pending orders.

use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, Confirmation, Mode};
use crate::protocol::{shield_label, Order, VolleyShot};
use crate::tutorial::ExpectedAction;

/// Result of handling a key.
#[derive(Debug)]
pub enum KeyResult {
    /// Continue running.
    Continue,
    /// Quit the app.
    Quit,
    /// An order was produced — send it to the engine.
    SendOrder(Order),
}

/// Handle a key event.
pub fn handle_key(app: &mut App, mut key: KeyEvent) -> KeyResult {
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

    // With Num Lock off, terminals commonly report numpad 3 as PageDown.
    // Normalize it only for the explicit facing-3 lesson; free-play navigation
    // and bindings remain untouched.
    if tutorial_active
        && key.code == KeyCode::PageDown
        && app
            .tutorial
            .as_ref()
            .and_then(|tutorial| tutorial.current_step())
            .is_some_and(|step| matches!(step.expected, ExpectedAction::PathFace(3)))
    {
        key.code = KeyCode::Char('3');
    }

    // Keep the small global escape hatch explicit. Every other key goes
    // through the lesson gate before mode handlers, so adding a new global
    // binding cannot silently bypass tutorial validation.
    if tutorial_active && !matches!(key.code, KeyCode::Char('q') | KeyCode::Esc | KeyCode::Tab) {
        if let Some(result) = tutorial_gate(app, &key) {
            return result;
        }
    }

    if app.yard.is_some() {
        return handle_yard(app, key);
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
                return KeyResult::Continue;
            } else if app.mode == Mode::Map {
                // Let handle_map own Esc so it can restore the phase-appropriate
                // mode (not unconditionally Normal). Fall through to the mode
                // dispatch below.
            } else if app.mode == Mode::Normal {
                reopen_phase_form(app);
                return KeyResult::Continue;
            } else {
                app.mode = Mode::Normal;
                app.last_error = None;
                return KeyResult::Continue;
            }
        }
        KeyCode::Char('h') | KeyCode::Char('?')
            if matches!(app.mode, Mode::Allocate | Mode::Movement | Mode::Fire) =>
        {
            app.mode = Mode::Normal;
            app.last_error = None;
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

    // `v` enters map-focus from any active phase mode (not just Normal). The
    // phase auto-switch in update_snapshot leaves the app in Allocate/Movement/
    // Fire, so gating `v` on Normal alone would make it unreachable during
    // normal play. Map mode is read-only and exits cleanly back to the phase
    // mode via Esc/v/Enter.
    if key.code == KeyCode::Char('v')
        && app.mode != Mode::GameOver
        && app.mode != Mode::Map
        && app.confirmation.is_none()
    {
        app.enter_map_mode();
        return KeyResult::Continue;
    }

    if key.code == KeyCode::Char(' ')
        && matches!(app.mode, Mode::Allocate | Mode::Movement | Mode::Fire)
        && crate::ui::is_disabled_ship(app)
    {
        app.disabled_autopass = app.focused_ship;
    }

    match app.mode {
        Mode::Normal => handle_normal(app, key),
        Mode::Map => handle_map(app, key),
        Mode::Allocate => handle_allocate(app, key),
        Mode::Movement => handle_movement(app, key),
        Mode::Fire => handle_fire(app, key),
        Mode::GameOver => {
            if matches!(key.code, KeyCode::Enter | KeyCode::Char('q')) {
                // Dismiss is local only (tutorial advance already done in gate).
                return if key.code == KeyCode::Char('q') {
                    KeyResult::Quit
                } else {
                    KeyResult::Continue
                };
            }
            KeyResult::Continue
        }
    }
}

fn handle_yard(app: &mut App, key: KeyEvent) -> KeyResult {
    let Some(yard) = app.yard.as_mut() else {
        return KeyResult::Continue;
    };
    use crate::yard::YardScreen;
    if yard.shield_editor.is_some() {
        return handle_yard_shields(yard, key);
    }
    if yard.picker.is_some() {
        return handle_yard_picker(yard, key);
    }
    match yard.screen {
        YardScreen::Browse => match key.code {
            KeyCode::Up | KeyCode::Char('k') => { yard.cancel_pending(); yard.move_browse(-1); },
            KeyCode::Down | KeyCode::Char('j') => { yard.cancel_pending(); yard.move_browse(1); },
            KeyCode::Char('o') => { yard.cancel_pending(); yard.cycle_sort(); },
            KeyCode::Enter | KeyCode::Char(' ') => yard.open_selected(),
            KeyCode::Char('n') => yard.start_new(),
            KeyCode::Char('y') => yard.clone_selected(),
            KeyCode::Char('d') => yard.request_delete_design(),
            KeyCode::Char('q') | KeyCode::Esc => return KeyResult::Quit,
            _ => {}
        },
        YardScreen::Edit => {
            use crate::yard::EditField;
            if yard.is_readonly() {
                match key.code {
                    KeyCode::Enter => yard.open_picker(),
                    KeyCode::Esc => yard.request_exit(),
                    KeyCode::Up | KeyCode::Char('k') => yard.move_edit(-1),
                    KeyCode::Down | KeyCode::Char('j') => yard.move_edit(1),
                    KeyCode::Char('q') => {
                        return if yard.request_quit() {
                            KeyResult::Quit
                        } else {
                            KeyResult::Continue
                        };
                    }
                    KeyCode::Left
                    | KeyCode::Right
                    | KeyCode::Backspace
                    | KeyCode::Char('a' | 'c' | 'd' | 'h' | 'l' | 'm' | 's')
                    | KeyCode::Char(_) => {
                        yard.status = "standard classes are reference-only".into();
                    }
                    _ => {}
                }
                return KeyResult::Continue;
            }
            // Esc and 'd' each carry their own arm/confirm state (see
            // `request_exit` / `request_delete_weapon`) and must not be
            // cancelled before they get to check it. Every other key clears
            // any armed confirmation first, so an armed delete or discard can
            // never fire against a since-changed cursor or field.
            if yard.edit_cursor == EditField::Name {
                match key.code {
                    KeyCode::Enter => {}
                    KeyCode::Char('q') => {
                        return if yard.request_quit() { KeyResult::Quit } else { KeyResult::Continue };
                    }
                    KeyCode::Esc => yard.request_exit(),
                    KeyCode::Up => {
                        yard.cancel_pending();
                        yard.move_edit(-1);
                    }
                    KeyCode::Down => {
                        yard.cancel_pending();
                        yard.move_edit(1);
                    }
                    KeyCode::Backspace => {
                        yard.cancel_pending();
                        yard.backspace_name();
                    }
                    KeyCode::Char(ch) => {
                        yard.cancel_pending();
                        yard.type_name(ch);
                    }
                    _ => {}
                }
            } else {
                match key.code {
                    KeyCode::Enter => yard.open_picker(),
                    KeyCode::Esc => yard.request_exit(),
                    KeyCode::Char('d') => yard.request_delete_weapon(),
                    KeyCode::Up | KeyCode::Char('k') => {
                        yard.cancel_pending();
                        yard.move_edit(-1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        yard.cancel_pending();
                        yard.move_edit(1);
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        yard.cancel_pending();
                        yard.nudge(-1);
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        yard.cancel_pending();
                        yard.nudge(1);
                    }
                    KeyCode::Char('m') => {
                        yard.cancel_pending();
                        yard.cycle_weapon_mount();
                    }
                    KeyCode::Char('a') => {
                        yard.cancel_pending();
                        yard.add_weapon();
                    }
                    KeyCode::Char('i') => {
                        yard.cancel_pending();
                        yard.add_system();
                    }
                    KeyCode::Char('s') => {
                        yard.cancel_pending();
                        yard.save();
                    }
                    KeyCode::Char('c') => {
                        yard.cancel_pending();
                        yard.compile();
                    }
                    KeyCode::Char('q') => {
                        return if yard.request_quit() {
                            KeyResult::Quit
                        } else {
                            KeyResult::Continue
                        };
                    }
                    _ => {}
                }
            }
        }
    }
    KeyResult::Continue
}

fn handle_yard_picker(yard: &mut crate::yard::YardState, key: KeyEvent) -> KeyResult {
    let filtering = yard.picker.as_ref().is_some_and(|picker| picker.filtering);
    if filtering {
        match key.code {
            KeyCode::Esc => {
                yard.picker_clear_filter();
            }
            KeyCode::Backspace => yard.picker_backspace(),
            KeyCode::Char(ch) => yard.picker_type(ch),
            _ => {}
        }
        return KeyResult::Continue;
    }
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => yard.picker_move(-1),
        KeyCode::Down | KeyCode::Char('j') => yard.picker_move(1),
        KeyCode::PageUp => yard.picker_move(-8),
        KeyCode::PageDown => yard.picker_move(8),
        KeyCode::Enter => yard.picker_commit(),
        KeyCode::Esc => yard.picker_cancel(),
        KeyCode::Char('/') => {
            if let Some(picker) = yard.picker.as_mut() { picker.filtering = true; }
        }
        KeyCode::Char('m') => yard.cycle_weapon_mount(),
        _ => {}
    }
    KeyResult::Continue
}

fn handle_yard_shields(yard: &mut crate::yard::YardState, key: KeyEvent) -> KeyResult {
    match key.code {
        KeyCode::Left | KeyCode::Char('h') => yard.shield_face_move(-1),
        KeyCode::Right | KeyCode::Char('l') => yard.shield_face_move(1),
        KeyCode::Up | KeyCode::Char('k') => yard.shield_face_adjust(1),
        KeyCode::Down | KeyCode::Char('j') => yard.shield_face_adjust(-1),
        KeyCode::PageUp => yard.shield_face_adjust(5),
        KeyCode::PageDown => yard.shield_face_adjust(-5),
        KeyCode::Char('=') => {
            if let Some(face) = yard.shield_editor { yard.shield_set_all(yard.draft.shields[face]); }
        }
        KeyCode::Esc => yard.close_shield_editor(),
        _ => {}
    }
    KeyResult::Continue
}

fn handle_confirmation(app: &mut App, confirmation: Confirmation, key: KeyEvent) -> KeyResult {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            app.confirmation = None;
            match confirmation {
                Confirmation::Quit => KeyResult::Quit,
            }
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.confirmation = None;
            app.tutorial_order_candidate = None;
            app.tutorial_order_pending = false;
            app.log("cancelled");
            KeyResult::Continue
        }
        _ => {
            notice_unknown_key(app);
            KeyResult::Continue
        },
    }
}

fn notice_unknown_key(app: &mut App) {
    if app
        .log
        .last()
        .is_none_or(|line| line != "Unknown key — use the controls below")
    {
        app.log("Unknown key — use the controls below");
    }
}

fn log_once(app: &mut App, message: String) {
    if app.log.last() != Some(&message) {
        app.log(message);
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
            if app.path_draft.is_none() {
                app.path_draft = Some(crate::app::PathDraft::default());
            }
            app.mode = Mode::Movement;
        }
        "firing" => {
            app.alloc_draft = None;
            if app.fire_draft.is_none() {
                app.fire_draft = app.focused().map(crate::app::FireDraft::for_ship);
            } else if let Some(ship) = app.focused().cloned() {
                if let Some(draft) = app.fire_draft.as_mut() {
                    draft.snap_to_operational(&ship);
                }
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

fn cycle_contact_focus(app: &mut App, direction: i8) {
    let Some(snap) = app.snap.clone() else {
        return;
    };
    let living: Vec<i64> = snap
        .ships
        .iter()
        .filter(|ship| !ship.destroyed)
        .map(|ship| ship.id)
        .collect();
    if living.is_empty() {
        return;
    }
    let current = app
        .focused_ship
        .and_then(|id| living.iter().position(|candidate| *candidate == id))
        .unwrap_or(0);
    let len = living.len() as i32;
    let next = (current as i32 + direction as i32).rem_euclid(len) as usize;
    app.switch_focus(living[next]);
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
        // Ensure ▶ is on the field this step edits before reading its value.
        if let Some(draft) = app.alloc_draft.as_mut() {
            let max = draft.n_fields().saturating_sub(1);
            draft.cursor = need_field.min(max);
        }
        let (field, old) = {
            let draft = app.alloc_draft.as_ref()?;
            (draft.cursor, draft.field_value())
        };

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

    // ── Movement: PathForward — lay N forward steps ────────────────────
    if let ExpectedAction::PathForward(target) = step_expected {
        if app.mode != Mode::Movement {
            reopen_tutorial_mode(app, "Movement reopened; keep drawing the path.");
            return Some(KeyResult::Continue);
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('f') => {
                path_append(app, "move_f");
            }
            KeyCode::Backspace => {
                if let Some(d) = app.path_draft.as_mut() {
                    d.pop();
                }
                app.request_path_preview();
            }
            _ => {
                app.tutorial
                    .as_mut()
                    .unwrap()
                    .set_error("Press w to add a forward step (Backspace to undo).");
                return Some(KeyResult::Continue);
            }
        }
        let count = app
            .path_draft
            .as_ref()
            .map(|d| d.actions.iter().filter(|a| a.as_str() == "move_f").count() as u32)
            .unwrap_or(0);
        if count >= target {
            app.tutorial.as_mut().unwrap().advance();
        }
        return Some(KeyResult::Continue);
    }

    // ── Movement: PathFace — turn the path's nose to a facing ──────────
    if let ExpectedAction::PathFace(target) = step_expected {
        if app.mode != Mode::Movement {
            reopen_tutorial_mode(app, "Movement reopened; keep drawing the path.");
            return Some(KeyResult::Continue);
        }
        let start = app.focused().map(|s| s.facing).unwrap_or(0);
        match key.code {
            KeyCode::Char(c) if c.is_ascii_digit() && c <= '5' => {
                path_turn_to(app, (c as u8 - b'0') as u32);
            }
            // Bind by on-screen direction (see handle_movement's direction note).
            KeyCode::Left => {
                path_append(app, "turn_left");
            }
            KeyCode::Right => {
                path_append(app, "turn_right");
            }
            KeyCode::Backspace => {
                if let Some(d) = app.path_draft.as_mut() {
                    d.pop();
                }
                app.request_path_preview();
            }
            _ => {
                app.tutorial.as_mut().unwrap().set_error(format!(
                    "Press {target} to turn the path's nose to facing {target}."
                ));
                return Some(KeyResult::Continue);
            }
        }
        if projected_current_facing(app, start) == target {
            app.tutorial.as_mut().unwrap().advance();
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
        ExpectedAction::CommitAllocate | ExpectedAction::PathCommit | ExpectedAction::ReadyFire
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
                app.open_movement_for_focus();
            }
            KeyResult::Continue
        }
        KeyCode::Char('f') => {
            if phase == "firing" {
                app.open_fire_for_focus();
            }
            KeyResult::Continue
        }
        KeyCode::Enter => {
            match phase.as_str() {
                "allocate" => app.open_allocate_for_focus(),
                "movement" => app.open_movement_for_focus(),
                "firing" => app.open_fire_for_focus(),
                _ => {}
            }
            KeyResult::Continue
        }
        _ => {
            notice_unknown_key(app);
            KeyResult::Continue
        },
    }
}

fn reopen_phase_form(app: &mut App) {
    let Some(phase) = app.snap.as_ref().map(|snap| snap.phase.as_str()) else {
        return;
    };
    match phase {
        "allocate" => app.open_allocate_for_focus(),
        "movement" => app.open_movement_for_focus(),
        "firing" => app.open_fire_for_focus(),
        _ => {}
    }
}

/// Map-focus mode: WASD/hjkl pans the viewport, +/- zooms, Esc/v returns to Normal.
///
/// Read-only — no orders are sent. The pan is relative to the current origin
/// (auto-centered on the focused ship until the first manual pan). In the
/// unbounded world this is the only way to follow a ship that has drifted to
/// negative coordinates.
fn handle_map(app: &mut App, key: KeyEvent) -> KeyResult {
    match key.code {
        // Pan: WASD and hjkl both work (vi-style + gamer-style).
        // q increases to the east, r increases to the south-east in this
        // axial system, so w/k = north (r-1), s/j = south (r+1),
        // a/h = west (q-1), d/l = east (q+1).
        KeyCode::Char('w') | KeyCode::Char('k') | KeyCode::Up => {
            app.pan_map(0, -1);
            KeyResult::Continue
        }
        KeyCode::Char('s') | KeyCode::Char('j') | KeyCode::Down => {
            app.pan_map(0, 1);
            KeyResult::Continue
        }
        KeyCode::Char('a') | KeyCode::Char('h') | KeyCode::Left => {
            app.pan_map(-1, 0);
            KeyResult::Continue
        }
        KeyCode::Char('d') | KeyCode::Char('l') | KeyCode::Right => {
            app.pan_map(1, 0);
            KeyResult::Continue
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            app.adjust_map_zoom(1);
            KeyResult::Continue
        }
        KeyCode::Char('-') => {
            app.adjust_map_zoom(-1);
            KeyResult::Continue
        }
        KeyCode::Char('[') => {
            cycle_contact_focus(app, -1);
            KeyResult::Continue
        }
        KeyCode::Char(']') => {
            cycle_contact_focus(app, 1);
            KeyResult::Continue
        }
        // Re-center on the focused ship.
        KeyCode::Char('c') | KeyCode::Char('z') => {
            app.reset_map_pan();
            KeyResult::Continue
        }
        // Exit map-focus: restore the phase-appropriate mode (not just Normal,
        // since `v` can be pressed from Allocate/Movement/Fire).
        KeyCode::Esc | KeyCode::Char('v') | KeyCode::Enter => {
            app.exit_map_mode();
            KeyResult::Continue
        }
        _ => {
            notice_unknown_key(app);
            KeyResult::Continue
        },
    }
}

/// `a` / `m` / `f` work from any combat form, not only Normal.
/// During allocate, `m` jumps to the Movement field (engine power → path).
fn phase_hotkey(app: &mut App, key: KeyEvent) -> Option<KeyResult> {
    let phase = app.snap.as_ref()?.phase.as_str();
    match (key.code, phase) {
        (KeyCode::Char('a'), "allocate") => {
            app.open_allocate_for_focus();
            Some(KeyResult::Continue)
        }
        (KeyCode::Char('m'), "movement") => {
            app.open_movement_for_focus();
            Some(KeyResult::Continue)
        }
        (KeyCode::Char('f'), "firing") => {
            app.open_fire_for_focus();
            Some(KeyResult::Continue)
        }
        (KeyCode::Char('m'), "allocate") => {
            if let Some(draft) = app.alloc_draft.as_mut() {
                draft.cursor = 0;
            }
            app.log("allocate: Movement — ←/→ or digits set engine power for this turn's path");
            Some(KeyResult::Continue)
        }
        (KeyCode::Char('f'), "allocate")
        | (KeyCode::Char('f'), "movement")
        | (KeyCode::Char('a'), "firing")
        | (KeyCode::Char('m'), "firing") => {
            app.log(format!(
                "{} is not active; current phase is {}",
                match key.code {
                    KeyCode::Char('a') => "allocate",
                    KeyCode::Char('m') => "movement",
                    _ => "fire",
                },
                phase
            ));
            Some(KeyResult::Continue)
        }
        _ => None,
    }
}

fn handle_allocate(app: &mut App, key: KeyEvent) -> KeyResult {
    if app.snap.is_none() {
        return KeyResult::Continue;
    }
    if let Some(result) = phase_hotkey(app, key) {
        return result;
    }
    let sid = match app.focused_ship {
        Some(id) => id,
        None => return KeyResult::Continue,
    };

    match key.code {
        KeyCode::Char(' ') if app.focused().is_some_and(|ship| ship.power_available == 0) => {
            app.log("allocate: disabled ship passes with zero power");
            app.digit_entry = None;
            emit_order(
                app,
                Order::allocate(
                    sid,
                    0,
                    serde_json::Value::Object(serde_json::Map::new()),
                    vec![0; 6],
                ),
            )
        }
        KeyCode::Enter => {
            let draft = match &app.alloc_draft {
                Some(d) => d.clone(),
                None => return KeyResult::Continue,
            };
            let Some(ship) = app.focused().cloned() else {
                return KeyResult::Continue;
            };

            // Omit offline weapons so the engine never sees dead ids.
            let weapons_json = draft.weapons_json(&ship);
            let shields = draft.shields.to_vec();

            app.log(format!(
                "allocate: mv={} shields={:?} cloak={} repair={} unsquad={}",
                draft.movement, shields, draft.cloak, draft.repair, draft.unsquad
            ));
            app.digit_entry = None;
            emit_order(
                app,
                Order::allocate_full(
                    sid,
                    draft.movement,
                    weapons_json,
                    shields,
                    draft.cloak,
                    draft.repair,
                    draft.unsquad,
                    draft.squad_leader,
                ),
            )
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.digit_entry = None;
            if let Some(ship) = app.focused().cloned() {
                if let Some(draft) = &mut app.alloc_draft {
                    draft.cursor = next_alloc_cursor(draft, &ship, 1);
                }
            }
            KeyResult::Continue
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.digit_entry = None;
            if let Some(ship) = app.focused().cloned() {
                if let Some(draft) = &mut app.alloc_draft {
                    draft.cursor = next_alloc_cursor(draft, &ship, -1);
                }
            }
            KeyResult::Continue
        }
        KeyCode::Char('x') => {
            app.digit_entry = None;
            let msg = {
                let Some(ship) = app.focused().cloned() else {
                    return KeyResult::Continue;
                };
                let Some(draft) = app.alloc_draft.as_mut() else {
                    return KeyResult::Continue;
                };
                if !ship.has_system("cloak") {
                    "no cloak installed".into()
                } else {
                    draft.cloak = !draft.cloak;
                    format!(
                        "cloak {} ({} power)",
                        if draft.cloak { "on" } else { "off" },
                        4 + ship.size
                    )
                }
            };
            app.log(msg);
            KeyResult::Continue
        }
        KeyCode::Char('z') => {
            app.digit_entry = None;
            let msg = {
                let Some(ship) = app.focused().cloned() else {
                    return KeyResult::Continue;
                };
                let Some(draft) = app.alloc_draft.as_mut() else {
                    return KeyResult::Continue;
                };
                if !ship.has_system("repair") {
                    "no repair installed".into()
                } else {
                    match ship.repair_cap {
                        None => "repair cap unavailable from engine".into(),
                        Some(cap) if draft.repair >= cap => {
                            format!("repair already at cap {cap}")
                        }
                        Some(cap) => {
                            draft.repair += 1;
                            format!("repair {}/{} box(es)", draft.repair, cap)
                        }
                    }
                }
            };
            app.log(msg);
            KeyResult::Continue
        }
        KeyCode::Char('u') => {
            app.digit_entry = None;
            let msg = {
                let Some(ship) = app.focused().cloned() else {
                    return KeyResult::Continue;
                };
                let Some(draft) = app.alloc_draft.as_mut() else {
                    return KeyResult::Continue;
                };
                if ship.squad_id.is_none() {
                    "not in a squad".into()
                } else {
                    draft.unsquad = !draft.unsquad;
                    format!(
                        "unsquad {}",
                        if draft.unsquad { "this turn" } else { "off" }
                    )
                }
            };
            app.log(msg);
            KeyResult::Continue
        }
        KeyCode::Char('l') => {
            app.digit_entry = None;
            let msg = {
                let Some(ship) = app.focused().cloned() else {
                    return KeyResult::Continue;
                };
                let Some(draft) = app.alloc_draft.as_mut() else {
                    return KeyResult::Continue;
                };
                if ship.squad_members.is_empty() {
                    "not in a squad".into()
                } else {
                    let members = &ship.squad_members;
                    let current = draft.squad_leader.or(ship.squad_leader).unwrap_or(ship.id);
                    let idx = members.iter().position(|id| *id == current).unwrap_or(0);
                    let next = members[(idx + 1) % members.len()];
                    draft.squad_leader = Some(next);
                    format!("squad leader → #{next}")
                }
            };
            app.log(msg);
            KeyResult::Continue
        }
        KeyCode::Left => {
            app.digit_entry = None;
            if cursor_on_dead_weapon(app) {
                notice_dead_weapon_edit(app);
            } else {
                adjust_field(app, -1);
            }
            KeyResult::Continue
        }
        KeyCode::Right => {
            app.digit_entry = None;
            if cursor_on_dead_weapon(app) {
                notice_dead_weapon_edit(app);
            } else {
                adjust_field(app, 1);
            }
            KeyResult::Continue
        }
        KeyCode::Char(c) if c.is_ascii_digit() => {
            let cursor = app.alloc_draft.as_ref().map(|d| d.cursor);
            if let Some(cursor) = cursor {
                if cursor_on_dead_weapon(app) {
                    notice_dead_weapon_edit(app);
                } else {
                    let d = (c as u8 - b'0') as u32;
                    let new = digit_entry(app, cursor, d);
                    set_allocate_field(app, new);
                }
            }
            KeyResult::Continue
        }
        KeyCode::Backspace => {
            app.digit_entry = None;
            if cursor_on_dead_weapon(app) {
                notice_dead_weapon_edit(app);
            } else {
                set_allocate_field(app, 0);
            }
            KeyResult::Continue
        }
        _ => {
            notice_unknown_key(app);
            KeyResult::Continue
        },
    }
}

fn adjust_field(app: &mut App, delta: i32) {
    let value = app
        .alloc_draft
        .as_ref()
        .map(|draft| (draft.field_value() as i32 + delta).max(0) as u32)
        .unwrap_or(0);
    set_allocate_field(app, value);
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

fn set_allocate_field(app: &mut App, value: u32) {
    let Some(ship) = app.focused().cloned() else {
        return;
    };
    let Some(draft) = app.alloc_draft.as_mut() else {
        return;
    };
    let (minimum, maximum) = allocation_field_bounds(draft, &ship);
    let requested = value;
    let value = value.clamp(minimum, maximum);
    if let Some((field, _)) = app.digit_entry {
        app.digit_entry = Some((field, value));
    }
    let cursor = draft.cursor;
    let on_movement = cursor == 0;
    let field_name = if on_movement {
        "movement".to_string()
    } else if cursor <= draft.weapons.len() {
        draft
            .weapons
            .get(cursor - 1)
            .map(|(id, _)| id.clone())
            .unwrap_or_else(|| "weapon".into())
    } else {
        let face = cursor.saturating_sub(draft.weapons.len() + 1);
        format!("shield {}", shield_label(face as u32))
    };
    draft.set_field_value(value);
    // Say why a raise was refused. Silently clamping reads as a dead key rather
    // than a limit, which is the same "the UI said nothing" failure as the
    // uncapped ceiling above — just in miniature.
    if requested != value {
        if on_movement && requested > value {
            let cap = ship.motion_cap();
            log_once(app, format!(
                "movement capped at {value} power — this hull converts at most {cap} motion points; \
                 extra power would be lost, not banked"
            ));
        } else if requested > value {
            log_once(app, format!("{field_name} capped at {value}"));
        } else if cursor <= draft.weapons.len() && requested < value {
            log_once(app, format!("{field_name} carries {value}; carried charge cannot be removed"));
        }
    }
}

/// True when the allocate cursor sits on a non-operational weapon row.
/// Such rows are display-only: ←/→/digits/backspace must not edit them
/// (the engine rejects allocate for destroyed weapon ids → `not found` spam).
fn cursor_on_dead_weapon(app: &App) -> bool {
    dead_weapon_id_under_cursor(app).is_some()
}

/// Weapon id under the allocate cursor when that weapon is offline.
fn dead_weapon_id_under_cursor(app: &App) -> Option<String> {
    let draft = app.alloc_draft.as_ref()?;
    let ship = app.focused()?;
    let weapon_count = draft.weapons.len();
    if draft.cursor == 0 || draft.cursor > weapon_count {
        return None; // movement or shield — not a weapon row
    }
    let index = draft.cursor - 1;
    let (weapon_id, _) = draft.weapons.get(index)?;
    let offline = ship
        .weapons
        .iter()
        .find(|w| &w.id == weapon_id)
        .map(|w| !w.operational)
        .unwrap_or(false);
    if offline {
        Some(weapon_id.clone())
    } else {
        None
    }
}

/// Soft notice when the player tries to charge an offline gun (mirrors fire path).
fn notice_dead_weapon_edit(app: &mut App) {
    if let Some(id) = dead_weapon_id_under_cursor(app) {
        app.log(format!("allocate: {id} DESTROYED — cannot charge"));
    }
}

/// Move between editable allocation fields without stopping on a destroyed
/// weapon. The cursor remains bounded and the six shield faces stay reachable.
fn next_alloc_cursor(
    draft: &crate::app::AllocDraft,
    ship: &crate::protocol::Ship,
    direction: i32,
) -> usize {
    let last = draft.n_fields().saturating_sub(1);
    let mut cursor = draft.cursor;
    loop {
        let next = if direction > 0 {
            cursor.saturating_add(1).min(last)
        } else {
            cursor.saturating_sub(1)
        };
        if next == cursor {
            return cursor;
        }
        cursor = next;
        let dead = cursor > 0
            && cursor <= draft.weapons.len()
            && draft
                .weapons
                .get(cursor - 1)
                .and_then(|(id, _)| ship.weapons.iter().find(|w| &w.id == id))
                .is_some_and(|weapon| !weapon.operational);
        if !dead {
            return cursor;
        }
    }
}

fn allocation_field_bounds(
    draft: &crate::app::AllocDraft,
    ship: &crate::protocol::Ship,
) -> (u32, u32) {
    let mut base = draft.clone();
    let weapon_count = base.weapons.len();
    if base.cursor == 0 {
        base.movement = 0;
        let residual = ship.power_available.saturating_sub(base.power_cost(ship));
        // Movement was the one field with no real ceiling: it offered the whole
        // power pool while the engine converts at most `motion_cap` motion
        // points and *destroys* the surplus (`usable_motion` truncates;
        // `movement_allocated` still records the full spend). A heavy cruiser
        // could put 20 of 36 power into movement, be told "ok", and get 6.
        // Weapons clamp to `max_charge` and shields to `shield_cap` just below,
        // so this field also trained the player's mental model and then broke it.
        return match ship.movement_power_cap() {
            Some(cap) => (0, residual.min(cap)),
            None => (0, residual),
        };
    }
    if base.cursor <= weapon_count {
        let index = base.cursor - 1;
        let (weapon_id, current) = base.weapons[index].clone();
        let carried = ship
            .weapons
            .iter()
            .find(|weapon| weapon.id == weapon_id)
            .map(|weapon| weapon.charge)
            .unwrap_or(current);
        let weapon = ship.weapons.iter().find(|weapon| weapon.id == weapon_id);
        let max_charge = weapon.map(|weapon| weapon.max_charge).unwrap_or(carried);
        // Dry magazines cannot be charged above the carried total.
        let out_of_ammo = weapon
            .and_then(|w| w.ammo_remaining)
            .is_some_and(|ammo| ammo == 0);
        if out_of_ammo {
            return (carried, carried);
        }
        base.weapons[index].1 = carried;
        let residual = ship.power_available.saturating_sub(base.power_cost(ship));
        return (carried, carried.saturating_add(residual).min(max_charge));
    }

    let shield_index = base.cursor - weapon_count - 1;
    if shield_index < 6 {
        base.shields[shield_index] = 0;
        let residual = ship.power_available.saturating_sub(base.power_cost(ship));
        return (0, residual.min(ship.shield_cap(shield_index)));
    }
    (0, 0)
}

/// Movement stage: an interactive path editor. The player assembles an ordered
/// list of motion actions (each costs one motion point), previews it, then
/// submits one `commit_path`.
///
/// Keys: `w`/↑ = forward, `a` = veer fore-left, `d` = veer fore-right,
/// ←/→ = turn in place, `0`–`5` = turn to that facing, `e` = +1 evasive,
/// Backspace = undo, `x` = clear, Enter = commit, Space = hold (commit empty path).
///
/// Direction note: on the TUI's `r↓` map, **increasing** facing rotates the ship
/// counterclockwise (E→NE→NW→…), i.e. to the on-screen LEFT. The engine names the
/// facing-increasing actions `turn_left` / `move_fl` and the facing-decreasing
/// ones `turn_right` / `move_fr`, matching how they read on screen. The TUI binds
/// keys by **on-screen direction**, so the player's Left key turns the ship left.
/// (See `path::PathAction` for the engine's canonical, rendering-agnostic definitions.)
fn handle_movement(app: &mut App, key: KeyEvent) -> KeyResult {
    if app.focused_ship.is_none() {
        return KeyResult::Continue;
    }
    if let Some(result) = phase_hotkey(app, key) {
        return result;
    }
    match key.code {
        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('f') => path_append(app, "move_f"),
        // Screen-left veer = ship's port = facing-increasing = engine `move_fl`.
        KeyCode::Char('a') => path_append(app, "move_fl"),
        // Screen-right veer = ship's starboard = facing-decreasing = engine `move_fr`.
        KeyCode::Char('d') => path_append(app, "move_fr"),
        // Turn nose left on screen (counterclockwise) = engine `turn_left`.
        KeyCode::Left => path_append(app, "turn_left"),
        // Turn nose right on screen (clockwise) = engine `turn_right`.
        KeyCode::Right => path_append(app, "turn_right"),
        KeyCode::Char(c) if c.is_ascii_digit() && c <= '5' => {
            path_turn_to(app, (c as u8 - b'0') as u32)
        }
        KeyCode::Char('e') => path_add_evasive(app),
        KeyCode::Char('y') => path_follow(app),
        KeyCode::Backspace => {
            if let Some(d) = app.path_draft.as_mut() {
                if d.evasive > 0 {
                    d.evasive -= 1;
                } else {
                    d.pop();
                }
            }
            app.request_path_preview();
            KeyResult::Continue
        }
        KeyCode::Char('x') => {
            if let Some(d) = app.path_draft.as_mut() {
                d.clear();
            }
            app.request_path_preview();
            KeyResult::Continue
        }
        KeyCode::Enter => {
            let empty = app
                .path_draft
                .as_ref()
                .is_none_or(|draft| draft.is_empty());
            if empty {
                app.log("Path empty — Space holds");
                KeyResult::Continue
            } else {
                commit_path(app)
            }
        }
        KeyCode::Char(' ') => {
            if let Some(d) = app.path_draft.as_mut() {
                d.clear();
            }
            commit_path(app)
        }
        _ => {
            notice_unknown_key(app);
            KeyResult::Continue
        },
    }
}

/// Motion points the focused ship can spend on a path this stage.
fn motion_budget(app: &App) -> u32 {
    app.focused().map(|s| s.motion_available).unwrap_or(0)
}

/// Append one action to the draft path, gated by the motion budget.
fn path_append(app: &mut App, action: &str) -> KeyResult {
    let budget = motion_budget(app);
    let over = app
        .path_draft
        .as_ref()
        .map(|d| d.total_motion() >= budget)
        .unwrap_or(true);
    if over {
        app.log(format!("path: no motion left (cap {budget})"));
        return KeyResult::Continue;
    }
    if let Some(d) = app.path_draft.as_mut() {
        d.follow = false;
        d.push(action);
    }
    app.request_path_preview();
    KeyResult::Continue
}

/// Spend one more motion point on declared evasive action.
fn path_add_evasive(app: &mut App) -> KeyResult {
    let budget = motion_budget(app);
    let over = app
        .path_draft
        .as_ref()
        .map(|d| d.total_motion() >= budget)
        .unwrap_or(true);
    if over {
        app.log(format!("evasive: no motion left (cap {budget})"));
        return KeyResult::Continue;
    }
    let next = app
        .path_draft
        .as_ref()
        .map(|d| d.evasive.saturating_add(1))
        .unwrap_or(1);
    if let Some(d) = app.path_draft.as_mut() {
        d.evasive = next;
    }
    app.log(format!("evasive: {next}"));
    KeyResult::Continue
}

fn path_follow(app: &mut App) -> KeyResult {
    let Some(ship) = app.focused() else {
        return KeyResult::Continue;
    };
    if ship.squad_id.is_none() {
        app.log("follow: not in a squad");
        return KeyResult::Continue;
    }
    if ship.squad_leader == Some(ship.id) {
        app.log("follow: you are the leader — plot the path");
        return KeyResult::Continue;
    }
    let next = app
        .path_draft
        .as_ref()
        .map(|d| !d.follow)
        .unwrap_or(true);
    if let Some(d) = app.path_draft.as_mut() {
        d.follow = next;
        if d.follow {
            d.actions.clear();
        }
    }
    app.log(format!(
        "follow {}",
        if next { "on — Enter to commit" } else { "off" }
    ));
    KeyResult::Continue
}

/// Append the minimal turns needed to point the draft path's projected facing
/// at `target` (respecting the motion budget).
fn path_turn_to(app: &mut App, target: u32) -> KeyResult {
    let start = app.focused().map(|s| s.facing).unwrap_or(0);
    let current = app
        .path_draft
        .as_ref()
        .map(|d| projected_facing(start, &d.actions))
        .unwrap_or(start);
    for action in turn_actions(current, target) {
        if let KeyResult::Continue = path_append(app, &action) {
            // Stop early if the budget is exhausted mid-turn.
            let over = app
                .path_draft
                .as_ref()
                .map(|d| d.total_motion() >= motion_budget(app))
                .unwrap_or(true);
            if over && projected_current_facing(app, start) != target {
                break;
            }
        }
    }
    KeyResult::Continue
}

fn projected_current_facing(app: &App, start: u32) -> u32 {
    app.path_draft
        .as_ref()
        .map(|d| projected_facing(start, &d.actions))
        .unwrap_or(start)
}

fn commit_path(app: &mut App) -> KeyResult {
    let sid = match app.focused_ship {
        Some(id) => id,
        None => return KeyResult::Continue,
    };
    let (actions, evasive, follow) = app
        .path_draft
        .as_ref()
        .map(|d| (d.actions.clone(), d.evasive, d.follow))
        .unwrap_or_default();
    app.log(format!(
        "commit_path: {} step(s), evasive {evasive} follow={follow}",
        actions.len()
    ));
    emit_order(
        app,
        Order::commit_path_full(sid, actions, evasive, follow),
    )
}

/// Submit the assembled volley (empty `shots` = hold fire).
fn commit_volley(app: &mut App) -> KeyResult {
    let sid = match app.focused_ship {
        Some(id) => id,
        None => return KeyResult::Continue,
    };
    let shots = app
        .fire_draft
        .as_ref()
        .map(|d| d.shots.clone())
        .unwrap_or_default();
    let Some(snap) = app.snap.as_ref() else {
        return KeyResult::Continue;
    };
    let Some(ship) = snap.ship(sid).cloned() else {
        return KeyResult::Continue;
    };
    for shot in &shots {
        let Some(weapon) = ship.weapons.iter().find(|w| w.id == shot.weapon) else {
            app.log(format!("fire: {} is not available", shot.weapon));
            return KeyResult::Continue;
        };
        let reason = if !weapon.operational {
            Some(format!("{} DESTROYED — cannot fire", weapon.id))
        } else if weapon.is_pd() {
            Some(format!("{} is point defense — it fires automatically", weapon.id))
        } else if weapon.charge == 0 {
            Some(format!("{} UNCHARGED — charge in Allocate", weapon.id))
        } else if weapon.ammo_remaining == Some(0) {
            Some(format!("{} out of ammo", weapon.id))
        } else {
            match app.matching_fire_preview(sid, &shot.weapon, shot.target) {
                None => Some(format!("{} checking shot — select it for a fresh preview", weapon.id)),
                Some(preview) if !preview.legal => Some(
                    preview
                        .reason
                        .clone()
                        .unwrap_or_else(|| "illegal shot".to_string()),
                ),
                Some(preview) if !preview.legal_shield_facings.contains(&shot.shield_facing) => {
                    Some(format!(
                        "{} invalid shield face — choose an indicated legal face",
                        weapon.id
                    ))
                }
                Some(_) => None,
            }
        };
        if let Some(reason) = reason {
            app.log(format!("fire: {reason}"));
            if let Some(draft) = app.fire_draft.as_mut() {
                if let Some(index) = ship.weapons.iter().position(|w| w.id == shot.weapon) {
                    draft.weapon_idx = index;
                    draft.target = Some(shot.target);
                    draft.shield_facing = shot.shield_facing;
                }
            }
            app.request_fire_preview();
            return KeyResult::Continue;
        }
    }
    app.log(format!("commit_volley: {} shot(s)", shots.len()));
    emit_order(app, Order::commit_volley(sid, shots))
}

/// Fold a path's actions over a starting facing to get the projected facing.
/// `move_fl`/`turn_left` add a face; `move_fr`/`turn_right` subtract one.
fn projected_facing(start: u32, actions: &[String]) -> u32 {
    let mut f = start as i32;
    for a in actions {
        match a.as_str() {
            "turn_left" | "move_fl" => f += 1,
            "turn_right" | "move_fr" => f -= 1,
            _ => {}
        }
    }
    f.rem_euclid(6) as u32
}

fn turn_actions(current: u32, target: u32) -> Vec<String> {
    let ccw = (target + 6 - current) % 6;
    let cw = (current + 6 - target) % 6;
    let (action, count) = if ccw <= cw {
        ("turn_left", ccw)
    } else {
        ("turn_right", cw)
    };
    std::iter::repeat_n(action.to_string(), count as usize).collect()
}

fn emit_order(app: &mut App, order: Order) -> KeyResult {
    app.mark_tutorial_order_emitted();
    KeyResult::SendOrder(order)
}

fn handle_fire(app: &mut App, key: KeyEvent) -> KeyResult {
    if let Some(result) = phase_hotkey(app, key) {
        return result;
    }
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
                let n_weapons = ship.map(|s| s.weapons.len()).unwrap_or(1).max(1);
                // Skip non-operational weapons when cycling (M2).
                let mut next = draft.weapon_idx;
                for _ in 0..n_weapons {
                    next = (next + 1) % n_weapons;
                    let op = ship
                        .and_then(|s| s.weapons.get(next))
                        .map(|w| w.operational)
                        .unwrap_or(true);
                    if op {
                        break;
                    }
                }
                draft.weapon_idx = next;
            }
            app.request_fire_preview();
            KeyResult::Continue
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(draft) = &mut app.fire_draft {
                let ship = snap.ship(sid);
                let n_weapons = ship.map(|s| s.weapons.len()).unwrap_or(1).max(1);
                // Skip non-operational weapons when cycling (M2).
                let mut prev = draft.weapon_idx;
                for _ in 0..n_weapons {
                    prev = (prev + n_weapons - 1) % n_weapons;
                    let op = ship
                        .and_then(|s| s.weapons.get(prev))
                        .map(|w| w.operational)
                        .unwrap_or(true);
                    if op {
                        break;
                    }
                }
                draft.weapon_idx = prev;
            }
            app.request_fire_preview();
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
        // Enter queues (or un-queues) the selected weapon into the volley.
        // Nothing is sent to the engine until Space commits the whole volley.
        KeyCode::Enter => {
            let ship = match snap.ship(sid) {
                Some(s) => s,
                None => return KeyResult::Continue,
            };
            let (weapon, shield_facing, target) = {
                let draft = match &app.fire_draft {
                    Some(d) => d,
                    None => return KeyResult::Continue,
                };
                let weapon = match ship.weapons.get(draft.weapon_idx) {
                    Some(w) => w,
                    None => return KeyResult::Continue,
                };
                if !weapon.operational {
                    app.log(format!("fire: {} DESTROYED — cannot queue", weapon.id));
                    return KeyResult::Continue;
                }
                if weapon.is_pd() {
                    app.log(format!(
                        "fire: {} is point defense — it fires automatically at incoming ordnance",
                        weapon.id
                    ));
                    return KeyResult::Continue;
                }
                let target = draft.target.or_else(|| {
                    snap.ships
                        .iter()
                        .find(|s| s.controller != "player" && !s.destroyed)
                        .map(|s| s.id)
                });
                let target = match target {
                    Some(t) => t,
                    None => return KeyResult::Continue,
                };
                if weapon.charge == 0 {
                    app.log(format!(
                        "fire: {} UNCHARGED — charge in Allocate",
                        weapon.id
                    ));
                    return KeyResult::Continue;
                }
                if weapon.ammo_remaining == Some(0) {
                    app.log(format!("fire: {} out of ammo", weapon.id));
                    return KeyResult::Continue;
                }
                let preview = match app.matching_fire_preview(sid, &weapon.id, target) {
                    Some(preview) => preview,
                    None => {
                        app.log(format!("fire: {} checking shot…", weapon.id));
                        app.request_fire_preview();
                        return KeyResult::Continue;
                    }
                };
                if !preview.legal {
                    app.log(format!(
                        "fire: {} {}",
                        weapon.id,
                        preview.reason.as_deref().unwrap_or("illegal shot")
                    ));
                    return KeyResult::Continue;
                }
                if !preview.legal_shield_facings.contains(&draft.shield_facing) {
                    let valid = preview
                        .legal_shield_facings
                        .iter()
                        .map(|face| shield_label(*face))
                        .collect::<Vec<_>>()
                        .join("/");
                    app.log(format!(
                        "fire: {} invalid shield face — use {}",
                        weapon.id, valid
                    ));
                    return KeyResult::Continue;
                }
                (weapon.id.clone(), draft.shield_facing, target)
            };
            if let Some(draft) = app.fire_draft.as_mut() {
                let queued = draft.toggle_shot(VolleyShot {
                    weapon: weapon.clone(),
                    target,
                    shield_facing,
                });
                if queued {
                    app.log(format!(
                        "volley: queued {weapon} → #{target} face {shield_facing}"
                    ));
                } else {
                    app.log(format!("volley: removed {weapon}"));
                    app.last_error = None;
                }
            }
            app.request_fire_preview();
            KeyResult::Continue
        }
        // Backspace un-queues the most recently added shot.
        KeyCode::Backspace => {
            if let Some(draft) = app.fire_draft.as_mut() {
                if draft.shots.pop().is_some() {
                    app.log("volley: removed last shot");
                }
            }
            KeyResult::Continue
        }
        // Space commits the assembled volley (empty = hold fire).
        KeyCode::Char(' ') => commit_volley(app),
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
                app.request_fire_preview();
            }
            KeyResult::Continue
        }
        _ => {
            notice_unknown_key(app);
            KeyResult::Continue
        },
    }
}

/// Map a key press to an ExpectedAction for discrete tutorial steps.
fn map_key_to_action(
    app: &App,
    snap: &crate::protocol::Snapshot,
    key: &KeyEvent,
) -> Option<ExpectedAction> {
    if key.code == KeyCode::Char('v') && app.mode != Mode::Map && app.mode != Mode::GameOver {
        return Some(ExpectedAction::EnterMap);
    }

    match app.mode {
        Mode::Normal => match key.code {
            KeyCode::Char('m') | KeyCode::Enter if snap.phase == "movement" => {
                Some(ExpectedAction::PathCommit)
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
                    let n_fields = draft.n_fields().max(1);
                    let next = (draft.cursor + 1) % n_fields;
                    Some(ExpectedAction::NavField(next))
                }
                KeyCode::Enter => Some(ExpectedAction::CommitAllocate),
                // Arrows/digits for ReachValue are handled in tutorial_gate.
                _ => None,
            }
        }
        // PathForward / PathFace steps are handled inline in `tutorial_gate`.
        // The only discrete movement action is committing the drawn path.
        Mode::Movement => match key.code {
            KeyCode::Enter => Some(ExpectedAction::PathCommit),
            _ => None,
        },
        Mode::Fire => match key.code {
            KeyCode::Enter => Some(ExpectedAction::FireWeapon),
            KeyCode::Down | KeyCode::Char('j') => Some(ExpectedAction::TabWeapon),
            KeyCode::Left => app
                .fire_draft
                .as_ref()
                .map(|draft| ExpectedAction::ShieldFacing((draft.shield_facing + 5) % 6)),
            KeyCode::Right => app
                .fire_draft
                .as_ref()
                .map(|draft| ExpectedAction::ShieldFacing((draft.shield_facing + 1) % 6)),
            KeyCode::Char(' ') => Some(ExpectedAction::ReadyFire),
            _ => None,
        },
        Mode::GameOver => match key.code {
            KeyCode::Enter => Some(ExpectedAction::Dismiss),
            _ => None,
        },
        Mode::Map => match key.code {
            KeyCode::Char('a') | KeyCode::Char('h') | KeyCode::Left => Some(ExpectedAction::PanMap),
            KeyCode::Char('-') => Some(ExpectedAction::ZoomOut),
            KeyCode::Char('+') | KeyCode::Char('=') => Some(ExpectedAction::ZoomIn),
            KeyCode::Char('c') | KeyCode::Char('z') => Some(ExpectedAction::RecenterMap),
            KeyCode::Char('v') | KeyCode::Enter => Some(ExpectedAction::ExitMap),
            _ => None,
        },
    }
}
