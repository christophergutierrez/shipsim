//! Keyboard input handling.
//!
//! Translates key events into app state changes and pending orders.

use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, Mode};
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
    // Global keys
    match key.code {
        KeyCode::Char('q') => return KeyResult::Quit,
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            app.last_error = None;
            return KeyResult::Continue;
        }
        KeyCode::Tab => {
            // Ship focus cycle is always available (field nav uses ↓/j).
            cycle_ship_focus(app);
            return KeyResult::Continue;
        }
        _ => {}
    }

    // Tutorial gating for allocate ReachValue needs special pre-check.
    if app.tutorial.is_some() {
        if let Some(result) = tutorial_gate(app, &key) {
            return result;
        }
    }

    // 'e' (end turn) works in any non-GameOver mode.
    if key.code == KeyCode::Char('e') && app.mode != Mode::GameOver && app.snap.is_some() {
        app.log("end_turn");
        return KeyResult::SendOrder(Order::end_turn());
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

fn cycle_ship_focus(app: &mut App) {
    if let Some(snap) = &app.snap {
        let living: Vec<i64> = snap
            .ships
            .iter()
            .filter(|s| !s.destroyed)
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
        app.focused_ship = Some(next);
    }
}

/// Tutorial gate. Returns `Some(Continue)` if the key is blocked.
/// Returns `None` if the key is allowed (and tutorial may have advanced).
fn tutorial_gate(app: &mut App, key: &KeyEvent) -> Option<KeyResult> {
    if app.tutorial.as_ref().map(|t| t.is_complete()).unwrap_or(true) {
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
            app.tutorial
                .as_mut()
                .unwrap()
                .set_error("Allocate form should be open — press Enter if it is not.");
            return Some(KeyResult::Continue);
        }
        let draft = app.alloc_draft.as_ref()?;
        let field = draft.cursor;

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

        let old = draft.field_value();
        let new = match key.code {
            KeyCode::Right => old.saturating_add(1),
            KeyCode::Left => old.saturating_sub(1),
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let d = (c as u8 - b'0') as u32;
                if old > 0 && old < 10 && old * 10 + d <= 30 {
                    old * 10 + d
                } else {
                    d
                }
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Up | KeyCode::Char('k') => {
                app.tutorial.as_mut().unwrap().set_error(
                    "Stay on this field — use → / ← to set the value (↓ moves away).",
                );
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

    let ok = app.tutorial.as_mut().unwrap().check_action(&action);
    if !ok {
        return Some(KeyResult::Continue);
    }
    // Allowed — fall through to normal handlers.
    None
}

fn handle_normal(app: &mut App, key: KeyEvent) -> KeyResult {
    let snap = match &app.snap {
        Some(s) => s,
        None => return KeyResult::Continue,
    };

    match key.code {
        KeyCode::Char('a') => {
            if snap.phase == "allocate" {
                if let Some(sid) = app.focused_ship {
                    app.alloc_draft = Some(crate::app::AllocDraft::from_ship(snap, sid));
                    app.mode = Mode::Allocate;
                }
            }
            KeyResult::Continue
        }
        KeyCode::Char('m') => {
            if snap.phase == "movement" {
                app.mode = Mode::Movement;
            }
            KeyResult::Continue
        }
        KeyCode::Char('f') => {
            if snap.phase == "firing" {
                app.fire_draft = Some(crate::app::FireDraft::default());
                app.mode = Mode::Fire;
            }
            KeyResult::Continue
        }
        // Maneuvers also work from Normal during movement (quick keys).
        KeyCode::Char('c') if snap.phase == "movement" => send_coast(app),
        KeyCode::Char('t') if snap.phase == "movement" => send_accel(app),
        KeyCode::Char(c) if snap.phase == "movement" && c.is_ascii_digit() && c <= '5' => {
            let facing = (c as u8 - b'0') as u32;
            send_turn(app, facing)
        }
        KeyCode::Char(' ') if snap.phase == "firing" => send_ready(app),
        KeyCode::Enter => {
            match snap.phase.as_str() {
                "allocate" => {
                    if let Some(sid) = app.focused_ship {
                        app.alloc_draft = Some(crate::app::AllocDraft::from_ship(snap, sid));
                        app.mode = Mode::Allocate;
                    }
                }
                "movement" => app.mode = Mode::Movement,
                "firing" => {
                    app.fire_draft = Some(crate::app::FireDraft::default());
                    app.mode = Mode::Fire;
                }
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
            KeyResult::SendOrder(Order::allocate(
                sid,
                draft.movement,
                weapons_json,
                shields,
            ))
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(draft) = &mut app.alloc_draft {
                let n_fields = 1 + draft.weapons.len() + 6;
                draft.cursor = (draft.cursor + 1) % n_fields.max(1);
            }
            KeyResult::Continue
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(draft) = &mut app.alloc_draft {
                let n_fields = 1 + draft.weapons.len() + 6;
                draft.cursor = (draft.cursor + n_fields - 1) % n_fields.max(1);
            }
            KeyResult::Continue
        }
        KeyCode::Left => {
            if let Some(draft) = &mut app.alloc_draft {
                adjust_field(draft, -1);
            }
            KeyResult::Continue
        }
        KeyCode::Right => {
            if let Some(draft) = &mut app.alloc_draft {
                adjust_field(draft, 1);
            }
            KeyResult::Continue
        }
        KeyCode::Char(c) if c.is_ascii_digit() => {
            if let Some(draft) = &mut app.alloc_draft {
                let d = (c as u8 - b'0') as u32;
                let old = draft.field_value();
                let new = if old > 0 && old < 10 && old * 10 + d <= 30 {
                    old * 10 + d
                } else {
                    d
                };
                draft.set_field_value(new);
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
            let facing = app
                .focused()
                .map(|s| (s.facing + 1) % 6)
                .unwrap_or(0);
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
    KeyResult::SendOrder(Order::commit_maneuver(sid, Maneuver::Coast))
}

fn send_accel(app: &mut App) -> KeyResult {
    let sid = match app.focused_ship {
        Some(id) => id,
        None => return KeyResult::Continue,
    };
    app.log("maneuver: accel");
    KeyResult::SendOrder(Order::commit_maneuver(sid, Maneuver::Accel))
}

fn send_turn(app: &mut App, facing: u32) -> KeyResult {
    let sid = match app.focused_ship {
        Some(id) => id,
        None => return KeyResult::Continue,
    };
    app.log(format!("maneuver: turn facing {facing}"));
    KeyResult::SendOrder(Order::commit_maneuver(
        sid,
        Maneuver::Turn { facing },
    ))
}

fn send_ready(app: &mut App) -> KeyResult {
    let sid = match app.focused_ship {
        Some(id) => id,
        None => return KeyResult::Continue,
    };
    app.log("ready_fire");
    app.mode = Mode::Normal;
    KeyResult::SendOrder(Order::ready_fire(sid))
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
                    let enemy = snap.ships.iter().find(|s| s.id != sid && !s.destroyed);
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
            KeyResult::SendOrder(Order::commit_fire(
                sid,
                &weapon,
                target,
                draft.shield_facing,
            ))
        }
        KeyCode::Char(' ') => send_ready(app),
        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
            let idx = (c as u8 - b'1') as usize;
            let enemies: Vec<i64> = snap
                .ships
                .iter()
                .filter(|s| s.id != sid && !s.destroyed)
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
            KeyCode::Char(c)
                if snap.phase == "movement" && c.is_ascii_digit() && c <= '5' =>
            {
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
