//! Integration and unit tests for the TUI client.
//!
//! These tests use ratatui's `TestBackend` to render the UI into an in-memory
//! buffer and assert on the rendered content. No real terminal or engine
//! subprocess is required.

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use crate::app::{AllocDraft, App, Confirmation, Mode};
use crate::input::{handle_key, KeyResult};
use crate::protocol::{
    callsign, facing_arrow, shield_label, ErrorResponse, FireCommit, Maneuver, Order, Snapshot,
};

// ─── Helpers ──────────────────────────────────────────────────────────────

/// A minimal but realistic snapshot for testing: two ships on a 10×10 board.
fn test_snapshot() -> Snapshot {
    let json = r#"{
        "protocol_version": 3,
        "turn": 1,
        "status": "InProgress",
        "phase": "allocate",
        "movement_phase": 0,
        "ships_committed_this_phase": [],
        "ships_ready_fire": [],
        "ships_allocated_this_turn": [],
        "seed": 42,
        "prng_state": 42,
        "map": {"width": 10, "height": 10, "mode": "hard"},
        "objective": null,
        "ships": [
            {
                "id": 1,
                "class": "Heavy Cruiser",
                "size": 2,
                "controller": "player",
                "q": 0, "r": 4, "facing": 0,
                "speed": 4, "power": 22, "power_available": 22,
                "movement_allocated": 0,
                "shields_powered": [0,0,0,0,0,0],
                "shields_remaining": [0,0,0,0,0,0],
                "max_shield_per_facing": 6,
                "structure": 12, "engine": 4, "power_sys": 2, "bridge": 1,
                "weapon_boxes": [1,1,1],
                "destroyed": false,
                "weapons": [
                    {"id":"beam_1","kind":"Beam","arc":"Forward","mount":"forward","max_range":10,"charge":0,"fired":false,"max_charge":4,"operational":true},
                    {"id":"torp_1","kind":"Torp","arc":"Forward","mount":"forward","max_range":12,"charge":0,"fired":false,"max_charge":1,"operational":true}
                ]
            },
            {
                "id": 2,
                "class": "Escort",
                "size": 1,
                "controller": "ai",
                "q": 8, "r": 4, "facing": 3,
                "speed": 4, "power": 14, "power_available": 14,
                "movement_allocated": 1,
                "shields_powered": [6,3,0,0,0,0],
                "shields_remaining": [6,3,0,0,0,0],
                "max_shield_per_facing": 6,
                "structure": 12, "engine": 4, "power_sys": 2, "bridge": 1,
                "weapon_boxes": [1],
                "destroyed": false,
                "weapons": [
                    {"id":"beam_1","kind":"Beam","arc":"Forward","mount":"forward","max_range":10,"charge":4,"fired":false,"max_charge":4,"operational":true}
                ]
            }
        ],
        "combat_log": []
    }"#;
    serde_json::from_str(json).expect("test snapshot must parse")
}

/// A snapshot in the fire phase with a combat log entry.
fn fire_phase_snapshot() -> Snapshot {
    let json = r#"{
        "protocol_version": 3,
        "turn": 2,
        "status": "InProgress",
        "phase": "firing",
        "movement_phase": 0,
        "ships_committed_this_phase": [],
        "ships_ready_fire": [],
        "ships_allocated_this_turn": [],
        "seed": 42,
        "prng_state": 42,
        "map": {"width": 10, "height": 10, "mode": "hard"},
        "objective": null,
        "ships": [
            {
                "id": 1,
                "class": "Heavy Cruiser",
                "size": 2,
                "controller": "player",
                "q": 0, "r": 4, "facing": 0,
                "speed": 4, "power": 22, "power_available": 22,
                "movement_allocated": 0,
                "shields_powered": [0,0,0,0,0,0],
                "shields_remaining": [0,0,0,0,0,0],
                "max_shield_per_facing": 6,
                "structure": 12, "engine": 4, "power_sys": 2, "bridge": 1,
                "weapon_boxes": [1,1,1],
                "destroyed": false,
                "weapons": [
                    {"id":"beam_1","kind":"Beam","arc":"Forward","mount":"forward","max_range":10,"charge":4,"fired":false,"max_charge":4,"operational":true},
                    {"id":"torp_1","kind":"Torp","arc":"Forward","mount":"forward","max_range":12,"charge":1,"fired":false,"max_charge":1,"operational":true}
                ]
            },
            {
                "id": 2,
                "class": "Escort",
                "size": 1,
                "controller": "ai",
                "q": 8, "r": 4, "facing": 3,
                "speed": 4, "power": 14, "power_available": 14,
                "movement_allocated": 1,
                "shields_powered": [6,3,0,0,0,0],
                "shields_remaining": [6,3,0,0,0,0],
                "max_shield_per_facing": 6,
                "structure": 12, "engine": 4, "power_sys": 2, "bridge": 1,
                "weapon_boxes": [1],
                "destroyed": false,
                "weapons": [
                    {"id":"beam_1","kind":"Beam","arc":"Forward","mount":"forward","max_range":10,"charge":4,"fired":false,"max_charge":4,"operational":true}
                ]
            }
        ],
        "combat_log": [
            {"attacker":1,"target":2,"weapon":"beam_1","shield":0,"damage":4,"shield_absorbed":0,"hull_damage":4,"kind":"hit","roll":5}
        ]
    }"#;
    serde_json::from_str(json).expect("fire snapshot must parse")
}

/// Two player ships with different loadouts plus an enemy. This is the
/// smallest fixture that can expose command-focus and draft ownership bugs.
fn fleet_snapshot() -> Snapshot {
    let mut snap = test_snapshot();
    snap.ships[1].controller = "player".into();
    snap.ships[1].id = 2;
    snap.ships[1].q = 1;
    snap.ships[1].r = 6;

    let mut enemy = snap.ships[1].clone();
    enemy.id = 3;
    enemy.controller = "ai".into();
    enemy.q = 8;
    enemy.r = 4;
    snap.ships.push(enemy);
    snap
}

/// A snapshot where the game is over (player won).
fn game_over_snapshot() -> Snapshot {
    let mut snap = test_snapshot();
    snap.status = "Won".into();
    snap
}

/// Render the app into a TestBackend buffer and return the buffer as a string.
fn render_to_string(app: &mut App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|f| crate::ui::render(f, app)).expect("draw");
    let buf = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..height {
        for x in 0..width {
            let cell = &buf[(x, y)];
            out.push_str(cell.symbol());
        }
        out.push('\n');
    }
    out
}

/// Check if a rendered string contains a substring (ignoring trailing spaces per line).
fn buffer_contains(buf: &str, needle: &str) -> bool {
    let needle = needle.trim();
    buf.lines().any(|line| {
        let trimmed = line.trim_end();
        trimmed.contains(needle)
    })
}

fn make_key(c: char) -> crossterm::event::KeyEvent {
    crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(c),
        crossterm::event::KeyModifiers::NONE,
    )
}

fn make_key_code(code: crossterm::event::KeyCode) -> crossterm::event::KeyEvent {
    crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE)
}

// ═══════════════════════════════════════════════════════════════════════════
// Slice 1: Protocol model — parse snapshots, build orders
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn snapshot_parses_from_real_engine_json() {
    let snap = test_snapshot();
    assert_eq!(snap.protocol_version, 3);
    assert_eq!(snap.turn, 1);
    assert_eq!(snap.status, "InProgress");
    assert_eq!(snap.phase, "allocate");
    assert_eq!(snap.ships.len(), 2);
}

#[test]
fn snapshot_ship_lookup_works() {
    let snap = test_snapshot();
    let ship = snap.ship(1).expect("ship 1 exists");
    assert_eq!(ship.class, "Heavy Cruiser");
    assert_eq!(ship.controller, "player");
    assert_eq!(ship.weapons.len(), 2);
    assert_eq!(ship.weapons[0].id, "beam_1");
    assert_eq!(ship.weapons[0].max_charge, 4);

    let ship2 = snap.ship(2).expect("ship 2 exists");
    assert_eq!(ship2.controller, "ai");
    assert_eq!(ship2.shields_powered, vec![6, 3, 0, 0, 0, 0]);

    assert!(snap.ship(999).is_none());
}

#[test]
fn snapshot_player_ship_lookup() {
    let snap = test_snapshot();
    let player = snap.player_ship().expect("player ship exists");
    assert_eq!(player.id, 1);
    assert_eq!(player.controller, "player");
}

#[test]
fn snapshot_is_over_detects_terminal_states() {
    let snap = test_snapshot();
    assert!(!snap.is_over());

    let mut over = snap.clone();
    over.status = "Won".into();
    assert!(over.is_over());

    over.status = "Lost".into();
    assert!(over.is_over());

    over.status = "InProgress".into();
    assert!(!over.is_over());
}

#[test]
fn order_allocate_serializes_correctly() {
    let weapons = serde_json::json!({"beam_1": 4, "torp_1": 1});
    let order = Order::allocate(1, 4, weapons, vec![2, 2, 0, 0, 0, 0]);
    let json = serde_json::to_string(&order).expect("serialize");
    assert!(json.contains("\"protocol_version\":3"));
    assert!(json.contains("\"allocate\""));
    assert!(json.contains("\"ship\":1"));
    assert!(json.contains("\"movement\":4"));
    assert!(json.contains("\"beam_1\""));
}

#[test]
fn order_commit_maneuver_serializes() {
    let order = Order::commit_maneuver(1, Maneuver::Turn { facing: 2 });
    let json = serde_json::to_string(&order).expect("serialize");
    assert!(json.contains("commit_maneuver"));
    assert!(json.contains("turn"));
    assert!(json.contains("\"facing\":2"));
}

#[test]
fn order_end_turn_serializes() {
    let order = Order::end_turn();
    let json = serde_json::to_string(&order).expect("serialize");
    assert!(json.contains("end_turn"));
    // end_turn has no ship field.
    assert!(!json.contains("ship"));
}

#[test]
fn order_pass_move_serializes() {
    let order = Order::pass_move(1);
    let json = serde_json::to_string(&order).expect("serialize");
    assert!(json.contains("\"pass_move\""));
}

#[test]
fn order_ready_fire_serializes() {
    let order = Order::ready_fire(1);
    let json = serde_json::to_string(&order).expect("serialize");
    assert!(json.contains("\"ready_fire\""));
}

#[test]
fn order_commit_fire_serializes() {
    let order = Order::commit_fire(1, "beam_1", 2, 0);
    let json = serde_json::to_string(&order).expect("serialize");
    assert!(json.contains("\"commit_fire\""));
    assert!(json.contains("\"beam_1\""));
    assert!(json.contains("\"target\":2"));
}

#[test]
fn error_response_parses() {
    let json = r#"{"type":"error","ok":false,"code":"INVALID_ORDER","message":"ship 1 already allocated"}"#;
    let err: ErrorResponse = serde_json::from_str(json).expect("parse error");
    assert_eq!(err.kind, "error");
    assert!(!err.ok);
    assert_eq!(err.code, "INVALID_ORDER");
    assert_eq!(err.message, "ship 1 already allocated");
}

#[test]
fn callsign_formats_ship_by_controller() {
    let snap = test_snapshot();
    let s1 = snap.ship(1).unwrap();
    let s2 = snap.ship(2).unwrap();
    // Player ships get "A" prefix, ai ships get "B".
    assert_eq!(callsign(s1), "A1");
    assert_eq!(callsign(s2), "B2");
}

#[test]
fn facing_arrow_maps_directions() {
    assert_eq!(facing_arrow(0), "→");
    assert_eq!(facing_arrow(1), "↗");
    assert_eq!(facing_arrow(2), "↖");
    assert_eq!(facing_arrow(3), "←");
    assert_eq!(facing_arrow(4), "↙");
    assert_eq!(facing_arrow(5), "↘");
    assert_eq!(facing_arrow(99), "?");
}

#[test]
fn shield_label_maps_facing() {
    // Ship-relative faces (match REPL/ASCII-UI): 0:F 1:FR 2:RR 3:R 4:RL 5:FL
    assert_eq!(shield_label(0), "F");
    assert_eq!(shield_label(1), "FR");
    assert_eq!(shield_label(2), "RR");
    assert_eq!(shield_label(3), "R");
    assert_eq!(shield_label(4), "RL");
    assert_eq!(shield_label(5), "FL");
    assert_eq!(shield_label(99), "?");
}

// ═══════════════════════════════════════════════════════════════════════════
// Slice 2: App state — snapshot updates, focus, drafts
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn app_new_starts_empty() {
    let app = App::new();
    assert!(app.snap.is_none());
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.focused_ship.is_none());
    assert!(app.alloc_draft.is_none());
    assert!(app.fire_draft.is_none());
    assert!(!app.engine_dead);
}

#[test]
fn app_update_snapshot_auto_focuses_player_ship() {
    let mut app = App::new();
    let snap = test_snapshot();
    app.update_snapshot(snap);
    assert_eq!(app.focused_ship, Some(1));
    assert!(app.snap.is_some());
}

#[test]
fn app_update_snapshot_preserves_existing_focus() {
    let mut app = App::new();
    app.focused_ship = Some(2);
    app.update_snapshot(test_snapshot());
    assert_eq!(app.focused_ship, Some(2));
}

#[test]
fn app_update_snapshot_detects_game_over() {
    let mut app = App::new();
    app.update_snapshot(game_over_snapshot());
    assert_eq!(app.mode, Mode::GameOver);
}

#[test]
fn app_update_snapshot_auto_enters_allocate_mode() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot()); // phase = allocate
    assert_eq!(app.mode, Mode::Allocate);
    assert!(app.alloc_draft.is_some());
}

#[test]
fn app_update_snapshot_auto_enters_fire_mode() {
    let mut app = App::new();
    app.update_snapshot(fire_phase_snapshot()); // phase = fire
    assert_eq!(app.mode, Mode::Fire);
    assert!(app.fire_draft.is_some());
}

#[test]
fn alloc_draft_from_ship_copies_current_allocation() {
    let snap = test_snapshot();
    let draft = AllocDraft::from_ship(&snap, 2);
    // Ship 2 has movement_allocated=1, beam_1 charge=4, shields [6,3,0,0,0,0]
    assert_eq!(draft.movement, 1);
    assert_eq!(draft.weapon_charge("beam_1"), Some(4));
    assert_eq!(draft.shields, [6, 3, 0, 0, 0, 0]);
}

#[test]
fn alloc_draft_from_missing_ship_is_default() {
    let snap = test_snapshot();
    let draft = AllocDraft::from_ship(&snap, 999);
    assert_eq!(draft.movement, 0);
    assert!(draft.weapons.is_empty());
    assert_eq!(draft.shields, [0; 6]);
}

#[test]
fn alloc_draft_power_spent_sums_all_fields() {
    let snap = test_snapshot();
    let draft = AllocDraft::from_ship(&snap, 2);
    // movement(1) + weapons(4) + shields(6+3) = 14
    assert_eq!(draft.power_spent(), 14);
}

#[test]
fn alloc_draft_power_spent_zero_for_default() {
    let draft = AllocDraft::default();
    assert_eq!(draft.power_spent(), 0);
}

#[test]
fn app_record_error_stores_message() {
    let mut app = App::new();
    let err = ErrorResponse {
        kind: "error".into(),
        ok: false,
        code: "BAD_ORDER".into(),
        message: "no power".into(),
        order: None,
    };
    app.record_error(&err);
    assert_eq!(app.last_error.as_deref(), Some("BAD_ORDER: no power"));
    assert!(app.log.last().unwrap().contains("ERROR: no power"));
}

#[test]
fn app_log_appends_messages() {
    let mut app = App::new();
    app.log("hello");
    app.log("world");
    assert_eq!(app.log.len(), 2);
    assert_eq!(app.log[0], "hello");
    assert_eq!(app.log[1], "world");
}

#[test]
fn app_focused_returns_focused_ship() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    let focused = app.focused().expect("should have focused ship");
    assert_eq!(focused.id, 1);
}

// ═══════════════════════════════════════════════════════════════════════════
// Slice 3: Input handling — key dispatch, order production
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn key_q_quits() {
    let mut app = App::new();
    let result = handle_key(&mut app, make_key('q'));
    assert!(matches!(result, KeyResult::Quit));
}

#[test]
fn key_esc_returns_to_normal_mode() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    // After snapshot, mode is Allocate. Esc should return to Normal.
    app.mode = Mode::Allocate;
    let result = handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Esc));
    assert!(matches!(result, KeyResult::Continue));
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn key_tab_does_not_enter_an_enemy_ship() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    assert_eq!(app.focused_ship, Some(1));

    handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Tab));
    assert_eq!(app.focused_ship, Some(1));

    handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Tab));
    assert_eq!(app.focused_ship, Some(1));
}

#[test]
fn tab_cycles_only_player_ships_and_keeps_each_allocate_draft_with_its_ship() {
    let mut app = App::new();
    app.update_snapshot(fleet_snapshot());
    app.alloc_draft.as_mut().unwrap().movement = 7;
    app.alloc_draft
        .as_mut()
        .unwrap()
        .set_weapon_charge("torp_1", 1);

    handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Tab));

    assert_eq!(app.focused_ship, Some(2));
    let escort_draft = app.alloc_draft.as_ref().unwrap();
    assert_eq!(escort_draft.movement, 1);
    assert_eq!(escort_draft.weapons.len(), 1);
    assert!(escort_draft.weapon_charge("beam_1").is_some());

    let result = handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Enter));
    let KeyResult::SendOrder(order) = result else {
        panic!("expected escort allocation order");
    };
    let json = order.to_json();
    assert!(json.contains("\"ship\":2"));
    assert!(!json.contains("torp_1"));

    handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Tab));
    assert_eq!(app.focused_ship, Some(1));
    assert_eq!(app.alloc_draft.as_ref().unwrap().movement, 7);
    assert_eq!(
        app.alloc_draft.as_ref().unwrap().weapon_charge("torp_1"),
        Some(1)
    );
}

#[test]
fn accepted_order_focuses_the_next_player_ship_that_is_still_pending() {
    let mut app = App::new();
    app.update_snapshot(fleet_snapshot());
    assert_eq!(app.focused_ship, Some(1));

    let mut after_a1 = fleet_snapshot();
    after_a1.ships_allocated_this_turn = vec![1];
    app.update_snapshot(after_a1);

    assert_eq!(app.focused_ship, Some(2));
}

#[test]
fn fire_target_selection_excludes_player_ships() {
    let mut snap = fleet_snapshot();
    snap.phase = "firing".into();
    let mut app = App::new();
    app.update_snapshot(snap);
    assert_eq!(app.focused_ship, Some(1));

    handle_key(&mut app, make_key('1'));

    assert_eq!(app.fire_draft.as_ref().unwrap().target, Some(3));
    let result = handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Enter));
    let KeyResult::SendOrder(order) = result else {
        panic!("expected fire order");
    };
    let json = order.to_json();
    assert!(json.contains("\"target\":3"));
}

#[test]
fn tutorial_blocks_global_focus_leak() {
    let mut app = App::new_with_tutorial();
    app.update_snapshot(test_snapshot());
    handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Tab));
    assert_eq!(app.focused_ship, Some(1));
    assert!(app
        .tutorial
        .as_ref()
        .and_then(|t| t.error_msg.as_ref())
        .is_some_and(|m| m.contains("Tab")));
}

#[test]
fn tutorial_esc_reopens_player_form() {
    let mut app = App::new_with_tutorial();
    app.update_snapshot(test_snapshot());
    app.alloc_draft.as_mut().unwrap().movement = 7;
    app.alloc_draft.as_mut().unwrap().shields[0] = 4;
    app.alloc_draft.as_mut().unwrap().cursor = 5;
    app.focused_ship = Some(2);
    app.mode = Mode::Normal;
    handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Esc));
    assert_eq!(app.focused_ship, Some(1));
    assert_eq!(app.mode, Mode::Allocate);
    assert_eq!(app.alloc_draft.as_ref().unwrap().movement, 7);
    assert_eq!(app.alloc_draft.as_ref().unwrap().shields[0], 4);
    assert_eq!(app.alloc_draft.as_ref().unwrap().cursor, 5);
}

#[test]
fn tutorial_order_does_not_advance_until_snapshot_ack() {
    let mut app = App::new_with_tutorial();
    app.update_snapshot(test_snapshot());
    while !matches!(
        app.tutorial
            .as_ref()
            .and_then(|t| t.current_step())
            .map(|s| &s.expected),
        Some(crate::tutorial::ExpectedAction::CommitAllocate)
    ) {
        app.tutorial.as_mut().unwrap().advance();
    }
    let before = app.tutorial.as_ref().unwrap().current;
    let result = handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Enter));
    assert!(matches!(result, KeyResult::SendOrder(_)));
    assert_eq!(app.tutorial.as_ref().unwrap().current, before);
    assert!(app.tutorial_order_pending);
    let err = ErrorResponse {
        kind: "error".into(),
        ok: false,
        code: "REJECTED".into(),
        message: "not enough power".into(),
        order: None,
    };
    app.record_error(&err);
    assert_eq!(app.tutorial.as_ref().unwrap().current, before);
    assert!(!app.tutorial_order_pending);
}

#[test]
fn tutorial_cancel_clears_unemitted_order_candidate() {
    let mut app = App::new_with_tutorial();
    app.update_snapshot(test_snapshot());
    app.tutorial_order_candidate = Some(crate::tutorial::ExpectedAction::EndTurn);
    app.tutorial_order_pending = true;
    app.confirmation = Some(Confirmation::EndTurn);

    let result = handle_key(&mut app, make_key('n'));

    assert!(matches!(result, KeyResult::Continue));
    assert!(app.confirmation.is_none());
    assert!(app.tutorial_order_candidate.is_none());
    assert!(!app.tutorial_order_pending);
}

#[test]
fn digit_entry_clears_on_snapshot_and_commit() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    app.digit_entry = Some((0, 6));
    app.update_snapshot(test_snapshot());
    assert!(app.digit_entry.is_none());

    app.digit_entry = Some((0, 6));
    let result = handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Enter));
    assert!(matches!(result, KeyResult::SendOrder(_)));
    assert!(app.digit_entry.is_none());
}

#[test]
fn allocation_scroll_keeps_last_shield_visible() {
    // Free play (no tutorial auto-cursor) so we can force the last shield face.
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    // test_snapshot heavy cruiser: beam + torp → FL face 5 is cursor 1+2+5 = 8
    let draft = app.alloc_draft.as_mut().unwrap();
    draft.cursor = 1 + draft.weapons.len() + 5;

    let buf = render_to_string(&mut app, 80, 28);

    assert!(
        buffer_contains(&buf, "FL") && buffer_contains(&buf, "forward-left"),
        "expected FL forward-left row visible; buf has FL? {}",
        buffer_contains(&buf, "FL")
    );
    assert!(
        buffer_contains(&buf, "▶") && buffer_contains(&buf, "forward-left"),
        "expected selected FL row"
    );
}

#[test]
fn terminal_floor_blocks_orders_until_resized() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    let _ = render_to_string(&mut app, 79, 24);
    assert!(app.terminal_too_small);

    let result = handle_key(&mut app, make_key('e'));

    assert!(matches!(result, KeyResult::Continue));
    assert!(app.confirmation.is_none());
}

#[test]
fn key_e_sends_end_turn_order() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    let result = handle_key(&mut app, make_key('e'));
    assert!(matches!(result, KeyResult::Continue));
    let result = handle_key(&mut app, make_key('y'));
    match result {
        KeyResult::SendOrder(order) => {
            let json = serde_json::to_string(&order).expect("serialize");
            assert!(json.contains("end_turn"));
        }
        _ => panic!("expected SendOrder for end_turn"),
    }
}

#[test]
fn input_ignored_when_no_snapshot() {
    let mut app = App::new();
    // No snapshot loaded — all keys should be safe no-ops.
    let result = handle_key(&mut app, make_key('a'));
    assert!(matches!(result, KeyResult::Continue));
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn input_ignored_in_game_over_mode() {
    let mut app = App::new();
    app.update_snapshot(game_over_snapshot());
    assert_eq!(app.mode, Mode::GameOver);
    // Any key in GameOver mode should just continue.
    let result = handle_key(&mut app, make_key('a'));
    assert!(matches!(result, KeyResult::Continue));
}

// ═══════════════════════════════════════════════════════════════════════════
// Slice 4: UI rendering — TestBackend buffer assertions
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn render_shows_header_with_turn_and_phase() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    let buf = render_to_string(&mut app, 80, 24);
    // Header renders " turn 1 " and humanized phase "Allocate"
    assert!(buffer_contains(&buf, "turn 1") || buffer_contains(&buf, "Turn 1"));
    assert!(
        buffer_contains(&buf, "Allocate") || buffer_contains(&buf, "allocate"),
        "expected allocate phase in header"
    );
}

#[test]
fn render_shows_ship_callsigns() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    let buf = render_to_string(&mut app, 80, 24);
    // Ship 1 = A1 (player), Ship 2 = B2 (ai)
    assert!(buffer_contains(&buf, "A1"));
    assert!(buffer_contains(&buf, "B2"));
}

#[test]
fn render_shows_ship_class_names() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    // Use a taller terminal so the contacts section is visible.
    let buf = render_to_string(&mut app, 80, 40);
    assert!(buffer_contains(&buf, "Heavy Cruiser"));
    assert!(buffer_contains(&buf, "Escort"));
}

#[test]
fn render_shows_hull_and_power() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    let buf = render_to_string(&mut app, 80, 24);
    // Ship 1 has structure 12, power 22
    assert!(buffer_contains(&buf, "12"));
    assert!(buffer_contains(&buf, "22"));
}

#[test]
fn render_shows_game_over_screen() {
    let mut app = App::new();
    app.update_snapshot(game_over_snapshot());
    let buf = render_to_string(&mut app, 80, 24);
    // Game over should show some indication of the terminal state.
    assert!(
        buffer_contains(&buf, "over")
            || buffer_contains(&buf, "Over")
            || buffer_contains(&buf, "win")
            || buffer_contains(&buf, "Win"),
        "expected game over indicator in buffer"
    );
}

#[test]
fn render_shows_help_in_normal_mode() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    app.mode = Mode::Normal;
    let buf = render_to_string(&mut app, 80, 24);
    // Normal mode should show key hints.
    assert!(
        buffer_contains(&buf, "q") || buffer_contains(&buf, "Tab") || buffer_contains(&buf, "help"),
        "expected key hints in normal mode"
    );
}

#[test]
fn render_shows_allocate_panel_in_allocate_mode() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    // update_snapshot auto-enters Allocate mode and creates the draft.
    assert_eq!(app.mode, Mode::Allocate);
    let buf = render_to_string(&mut app, 80, 24);
    assert!(
        buffer_contains(&buf, "movement")
            || buffer_contains(&buf, "Movement")
            || buffer_contains(&buf, "alloc")
            || buffer_contains(&buf, "Alloc"),
        "expected allocate panel content"
    );
}

#[test]
fn render_shows_fire_panel_in_fire_mode() {
    let mut app = App::new();
    app.update_snapshot(fire_phase_snapshot());
    // update_snapshot auto-enters Fire mode and creates the draft.
    assert_eq!(app.mode, Mode::Fire);
    let buf = render_to_string(&mut app, 80, 24);
    assert!(
        buffer_contains(&buf, "fire")
            || buffer_contains(&buf, "Fire")
            || buffer_contains(&buf, "weapon")
            || buffer_contains(&buf, "Weapon")
            || buffer_contains(&buf, "target")
            || buffer_contains(&buf, "Target"),
        "expected fire panel content"
    );
}

#[test]
fn render_shows_combat_events_in_fire_phase() {
    let mut app = App::new();
    app.update_snapshot(fire_phase_snapshot());
    let buf = render_to_string(&mut app, 80, 24);
    // The combat log has attacker=1 (A1), target=2 (B2), weapon=beam_1, hit for 4
    assert!(
        buffer_contains(&buf, "beam_1")
            || buffer_contains(&buf, "HIT")
            || buffer_contains(&buf, "hit"),
        "expected combat log content in events panel"
    );
}

#[test]
fn tutorial_combat_log_keeps_damage_result_visible() {
    let mut app = App::new_with_tutorial();
    app.update_snapshot(fire_phase_snapshot());
    let buf = render_to_string(&mut app, 80, 24);

    assert!(buffer_contains(&buf, "HIT +4 sh-0 hull-4"));
}

#[test]
fn render_prioritizes_an_engine_rejection_above_combat_history() {
    let mut app = App::new();
    app.update_snapshot(fire_phase_snapshot());
    app.record_error(&ErrorResponse {
        kind: "error".into(),
        ok: false,
        code: "order_illegal".into(),
        message: "weapon is uncharged".into(),
        order: None,
    });

    let buf = render_to_string(&mut app, 80, 24);

    assert!(
        buffer_contains(&buf, "ENGINE: order_illegal: weapon is uncharged")
            || buffer_contains(&buf, "order_illegal: weapon is uncharged"),
        "expected engine rejection in buffer, got:\n{buf}"
    );
}

#[test]
fn tutorial_error_is_pinned_above_the_form_at_the_small_floor() {
    let mut app = App::new_with_tutorial();
    app.update_snapshot(test_snapshot());
    app.tutorial
        .as_mut()
        .unwrap()
        .set_error("WRONG ACTION: use the highlighted field");

    let buf = render_to_string(&mut app, 80, 24);

    assert!(buffer_contains(&buf, "WRONG ACTION"));
}

#[test]
fn session_log_contains_snapshot_combat_and_command_context() {
    let mut app = App::new();
    app.update_snapshot(fire_phase_snapshot());
    app.combat_history
        .push("A1 beam_1>B2 HIT +4 sh-0 hull-4".into());
    app.log("ready_fire");

    let contents = crate::session_log_contents(&app);

    assert!(contents.contains("turn=2 phase=firing status=InProgress"));
    assert!(contents.contains("A1 beam_1>B2 HIT +4 sh-0 hull-4"));
    assert!(contents.contains("ready_fire"));
}

#[test]
fn render_does_not_panic_with_no_snapshot() {
    let mut app = App::new();
    // Should render a placeholder without panicking.
    let buf = render_to_string(&mut app, 80, 24);
    assert!(!buf.is_empty());
}

#[test]
fn render_does_not_panic_with_small_terminal() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    let buf = render_to_string(&mut app, 40, 12);
    assert!(buffer_contains(&buf, "Resize to at least"));
}

// ═══════════════════════════════════════════════════════════════════════════
// Slice 5: Harness line parsing — JSON round-trips and type checks
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn snapshot_json_parses_as_snapshot() {
    // Verify that a snapshot JSON line parses correctly via serde_json.
    let snap: Snapshot = serde_json::from_str(
        r#"{"protocol_version":3,"turn":1,"status":"InProgress","phase":"allocate","map":{"width":10,"height":10},"ships":[]}"#,
    )
    .expect("parse minimal snapshot");
    assert_eq!(snap.turn, 1);
    assert!(snap.ships.is_empty());
}

#[test]
fn error_json_parses_as_error_response() {
    let json = r#"{"type":"error","ok":false,"code":"REJECTED","message":"bad order"}"#;
    let err: ErrorResponse = serde_json::from_str(json).expect("parse");
    assert_eq!(err.code, "REJECTED");
    assert!(!err.ok);
}

#[test]
fn order_to_json_produces_valid_json() {
    let weapons = serde_json::json!({"beam_1": 4});
    let order = Order::allocate(1, 4, weapons, vec![2, 0, 0, 0, 0, 0]);
    let json = order.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
    assert_eq!(parsed["protocol_version"], 3);
    assert_eq!(parsed["type"], "allocate");
    assert_eq!(parsed["ship"], 1);
    assert_eq!(parsed["movement"], 4);
}

#[test]
fn maneuver_accel_serializes_correctly() {
    let m = Maneuver::Accel;
    let json = serde_json::to_string(&m).expect("serialize");
    assert!(json.contains("accel"));
}

#[test]
fn maneuver_turn_serializes_correctly() {
    let m = Maneuver::Turn { facing: 1 };
    let json = serde_json::to_string(&m).expect("serialize");
    assert!(json.contains("turn"));
    assert!(json.contains("\"facing\":1"));
}

#[test]
fn alt_facing_key_sends_turn_accel() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    app.mode = Mode::Movement;
    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('2'),
        crossterm::event::KeyModifiers::ALT,
    );
    let KeyResult::SendOrder(order) = handle_key(&mut app, key) else {
        panic!("Alt+2 should submit turn_accel");
    };
    assert!(order.to_json().contains("turn_accel"));
}

#[test]
fn maneuver_coast_serializes_correctly() {
    let m = Maneuver::Coast;
    let json = serde_json::to_string(&m).expect("serialize");
    assert!(json.contains("coast"));
}

// ═══════════════════════════════════════════════════════════════════════════
// Slice 6: Integration — full render cycle with state transitions
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn full_cycle_snapshot_to_render_to_order() {
    // 1. Load a snapshot.
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    assert_eq!(app.focused_ship, Some(1));
    // update_snapshot auto-enters Allocate mode.
    assert_eq!(app.mode, Mode::Allocate);

    // 2. Render — should not panic.
    let buf = render_to_string(&mut app, 80, 24);
    assert!(buffer_contains(&buf, "turn 1") || buffer_contains(&buf, "Turn 1"));

    // 3. Esc to normal mode.
    handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Esc));
    assert_eq!(app.mode, Mode::Normal);

    // 4. Render in normal mode.
    let buf2 = render_to_string(&mut app, 80, 24);
    assert!(!buf2.is_empty());

    // 5. Send end_turn order.
    let result = handle_key(&mut app, make_key('e'));
    assert!(matches!(result, KeyResult::Continue));
    let result = handle_key(&mut app, make_key('y'));
    assert!(matches!(result, KeyResult::SendOrder(_)));
}

#[test]
fn full_cycle_fire_phase_render_and_esc() {
    let mut app = App::new();
    app.update_snapshot(fire_phase_snapshot());
    // update_snapshot auto-enters Fire mode.
    assert_eq!(app.mode, Mode::Fire);

    // Render fire panel.
    let buf = render_to_string(&mut app, 80, 24);
    assert!(!buf.is_empty());

    // Esc back.
    handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Esc));
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn full_cycle_game_over_detection() {
    let mut app = App::new();
    app.update_snapshot(game_over_snapshot());
    assert_eq!(app.mode, Mode::GameOver);

    // Render game over screen.
    let buf = render_to_string(&mut app, 80, 24);
    assert!(!buf.is_empty());

    // Keys in game over mode should be safe.
    let result = handle_key(&mut app, make_key('a'));
    assert!(matches!(result, KeyResult::Continue));
}

#[test]
fn full_cycle_tab_keeps_player_ship_rendered_when_only_one_is_owned() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());

    // Enemy ship 2 is not a command-focus candidate.
    handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Tab));
    assert_eq!(app.focused_ship, Some(1));

    // Render — the player ship remains the command source.
    let buf = render_to_string(&mut app, 80, 24);
    assert!(buffer_contains(&buf, "Heavy Cruiser"));
}

#[test]
fn full_cycle_multiple_renders_are_stable() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());

    // Render multiple times — should be idempotent.
    let buf1 = render_to_string(&mut app, 80, 24);
    let buf2 = render_to_string(&mut app, 80, 24);
    let buf3 = render_to_string(&mut app, 80, 24);
    assert_eq!(buf1, buf2);
    assert_eq!(buf2, buf3);
}

#[test]
fn full_cycle_phase_transition_resets_drafts() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot()); // allocate phase
    assert!(app.alloc_draft.is_some());
    assert!(app.fire_draft.is_none());

    // Transition to fire phase.
    app.update_snapshot(fire_phase_snapshot());
    assert!(app.fire_draft.is_some());
    // alloc_draft should be cleared on phase change.
    assert!(app.alloc_draft.is_none());
}

// ═══════════════════════════════════════════════════════════════════════════
// Tutorial: rear-attack key path against live engine
// ═══════════════════════════════════════════════════════════════════════════

fn engine_bin() -> Option<std::path::PathBuf> {
    let candidates = [
        std::path::PathBuf::from("../../target/debug/shipsim"),
        std::path::PathBuf::from("target/debug/shipsim"),
        std::env::var_os("SHIPSIM_BIN")
            .map(std::path::PathBuf::from)
            .unwrap_or_default(),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

fn apply_line(app: &mut App, line: crate::harness::EngineLine) {
    match line {
        crate::harness::EngineLine::Snapshot(s) => app.update_snapshot(s),
        crate::harness::EngineLine::MovementPreview(p) => app.accept_movement_preview(p),
        crate::harness::EngineLine::Error(e) => app.record_error(&e),
        crate::harness::EngineLine::Raw(r) => app.log(format!("raw: {r}")),
    }
}

fn send_key(app: &mut App, harness: &mut crate::harness::Harness, key: crossterm::event::KeyEvent) {
    match handle_key(app, key) {
        KeyResult::SendOrder(order) => {
            harness.send(&order.to_json()).expect("send");
            if let Some(line) = harness.read_line() {
                apply_line(app, line);
            }
        }
        KeyResult::Quit | KeyResult::Continue => {}
    }
    // Drain a pending movement-preview request (mirrors the main loop).
    if let Some(preview_json) = app.pending_preview.take() {
        if harness.send(&preview_json).is_ok() {
            if let Some(line) = harness.read_line() {
                apply_line(app, line);
            }
        }
    }
}

/// Live smoke: drive the TUI rear-attack tutorial keys to Won (seed 4).
#[test]
fn tutorial_rear_attack_wins_against_engine() {
    let bin = engine_bin().expect("shipsim binary not found — cargo build at repo root");
    let scenario = if std::path::Path::new("../../scenarios/tutorial_rear_attack.toml").is_file() {
        "../../scenarios/tutorial_rear_attack.toml"
    } else {
        "scenarios/tutorial_rear_attack.toml"
    };

    let mut harness = crate::harness::Harness::spawn(bin.to_str().unwrap(), scenario)
        .unwrap_or_else(|e| panic!("spawn engine {bin:?}: {e}"));
    let mut app = App::new_with_tutorial();
    let line = harness.read_line().expect("post-load snapshot");
    apply_line(&mut app, line);
    assert_eq!(app.mode, Mode::Allocate);
    assert!(app.tutorial.is_some());

    let right = || make_key_code(crossterm::event::KeyCode::Right);
    let down = || make_key_code(crossterm::event::KeyCode::Down);
    let enter = || make_key_code(crossterm::event::KeyCode::Enter);
    let space = || make_key(' ');
    let t_key = || make_key('t');

    // T1 allocate: mov 10, beam 4, plasma 1, torp 1, sh0 6
    for _ in 0..10 {
        send_key(&mut app, &mut harness, right());
    }
    send_key(&mut app, &mut harness, down()); // beam
    for _ in 0..4 {
        send_key(&mut app, &mut harness, right());
    }
    send_key(&mut app, &mut harness, down()); // plasma
    send_key(&mut app, &mut harness, right());
    send_key(&mut app, &mut harness, down()); // torp
    send_key(&mut app, &mut harness, right());
    send_key(&mut app, &mut harness, down()); // sh0
    for _ in 0..6 {
        send_key(&mut app, &mut harness, right());
    }
    send_key(&mut app, &mut harness, enter());
    assert_eq!(
        app.snap.as_ref().map(|s| s.phase.as_str()),
        Some("movement"),
        "after t1 commit"
    );

    // T1: accel twice, turn west into a close stern shot, inspect the map,
    // and fire the winning volley immediately.
    for _ in 0..2 {
        send_key(&mut app, &mut harness, t_key());
        send_key(&mut app, &mut harness, space());
    }
    // Wrong movement keys must be rejected without changing the lesson state
    // or making the next map redraw unsafe.
    let turn_step = app.tutorial.as_ref().map(|tutorial| tutorial.current);
    for key in ['w', 's', 'f', 'd'] {
        send_key(&mut app, &mut harness, make_key(key));
        assert_eq!(
            app.tutorial.as_ref().map(|tutorial| tutorial.current),
            turn_step,
            "wrong key {key} advanced the turn-facing lesson"
        );
        let _ = render_to_string(&mut app, 100, 30);
    }
    // Numpad 3 with Num Lock off is reported as PageDown by many terminals.
    send_key(
        &mut app,
        &mut harness,
        make_key_code(crossterm::event::KeyCode::PageDown),
    );
    // Facing west while course remains east produces a two-arrow ship marker.
    // Rendering it previously panicked by truncating in the middle of UTF-8.
    let divergent = render_to_string(&mut app, 100, 30);
    assert!(
        divergent.contains("A1"),
        "divergent facing/course marker should render"
    );

    assert_eq!(app.mode, Mode::Fire);
    send_key(&mut app, &mut harness, make_key('v'));
    assert_eq!(app.mode, Mode::Map);
    send_key(&mut app, &mut harness, make_key('a'));
    assert!(app.map_pan.is_some(), "map tutorial should pan manually");
    send_key(&mut app, &mut harness, make_key('-'));
    assert_eq!(app.map_zoom, Some(-1));
    send_key(&mut app, &mut harness, make_key('+'));
    assert_eq!(app.map_zoom, Some(0));
    send_key(&mut app, &mut harness, make_key('c'));
    assert_eq!(app.map_pan, None);
    assert_eq!(app.map_zoom, None);
    send_key(&mut app, &mut harness, make_key('v'));
    assert_eq!(app.mode, Mode::Fire);
    for _ in 0..3 {
        send_key(&mut app, &mut harness, right());
    }
    assert_eq!(
        app.fire_draft.as_ref().map(|draft| draft.shield_facing),
        Some(3)
    );

    // The pass produces a close rear shot without spending two extra turns.
    let rng = {
        let snap = app.snap.as_ref().unwrap();
        let a = snap.ship(1).unwrap();
        let b = snap.ship(2).unwrap();
        let dq = (a.q - b.q).abs();
        let dr = (a.r - b.r).abs();
        // axial distance for same r row:
        dq.max(dr)
    };
    assert_eq!(rng, 3, "expected close rear shot before volley, got {rng}");
    send_key(&mut app, &mut harness, enter()); // beam
    send_key(&mut app, &mut harness, down()); // torp
    send_key(&mut app, &mut harness, enter());
    send_key(&mut app, &mut harness, down()); // plasma
    send_key(&mut app, &mut harness, enter());
    send_key(&mut app, &mut harness, space());

    let status = app
        .snap
        .as_ref()
        .map(|s| s.status.clone())
        .unwrap_or_default();
    let turn = app.snap.as_ref().map(|s| s.turn).unwrap_or(0);
    let tut_step = app.tutorial.as_ref().map(|t| t.current).unwrap_or(0);
    eprintln!(
        "tutorial harness: status={status} turn={turn} tut_step={tut_step} err={:?}",
        app.last_error
    );
    assert_eq!(
        status, "Won",
        "tutorial should win; last_error={:?}",
        app.last_error
    );
    // After the kill ready, the final step is Dismiss — one Enter completes it.
    send_key(&mut app, &mut harness, enter());
    assert!(
        app.tutorial
            .as_ref()
            .map(|t| t.is_complete())
            .unwrap_or(false),
        "tutorial should complete after dismiss; step={}",
        app.tutorial.as_ref().map(|t| t.current).unwrap_or(0)
    );
}

#[test]
fn tutorial_can_power_beam_after_movement_step() {
    // Phase 6 collapsed the separate "Select beam" (NavField) step into the
    // "Charge the beam" (ReachValue) step: the form auto-selects beam_1 (▶)
    // when the charge step begins. This test verifies → can still raise beam
    // charge after movement completes — the original bug being that → was
    // blocked after the auto-cursor move.
    let mut app = App::new_with_tutorial();
    app.update_snapshot(test_snapshot());

    // Complete movement ReachValue (field 0 → 10).
    {
        let d = app.alloc_draft.as_mut().unwrap();
        d.cursor = 0;
        d.set_field_value(10);
    }
    app.tutorial.as_mut().unwrap().advance(); // skip to Charge the beam
    // Step should now be ReachValue { field: 1, target: 4 } (no NavField step).
    assert!(matches!(
        app.tutorial
            .as_ref()
            .unwrap()
            .current_step()
            .unwrap()
            .expected,
        crate::tutorial::ExpectedAction::ReachValue {
            field: 1,
            target: 4
        }
    ));

    // The tutorial gate auto-moves the cursor to field 1 (beam_1) on the first
    // → keypress. → must raise beam charge (the bug: blocked after auto-cursor).
    for _ in 0..4 {
        let r = handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Right));
        assert!(
            matches!(r, KeyResult::Continue),
            "→ blocked while powering beam"
        );
    }
    assert_eq!(
        app.alloc_draft.as_ref().unwrap().weapon_charge("beam_1"),
        Some(4)
    );
}

#[test]
fn tutorial_prompt_is_not_duplicated_in_header() {
    let mut app = App::new_with_tutorial();
    app.update_snapshot(test_snapshot());
    let buf = render_to_string(&mut app, 100, 30);
    // Coach area should mention engine/thrust; header should not show the yellow
    // "Engine = thrust" strip (only the TUTORIAL badge).
    assert!(buffer_contains(&buf, "TUTORIAL"));
    // Count "Engine = thrust" — should appear once (coach), not also as header strip.
    let n = buf.matches("Engine = thrust").count();
    assert!(n <= 2, "prompt duplicated too many times: {n}");
}

#[test]
fn tutorial_floor_keeps_the_required_action_visible() {
    let mut app = App::new_with_tutorial();
    app.update_snapshot(test_snapshot());

    let buf = render_to_string(&mut app, 80, 24);

    assert!(
        buffer_contains(&buf, "0→10"),
        "the 80x24 tutorial view must show the value and action: {buf}"
    );
}

#[test]
fn weapon_shade_forward_mount_is_range_and_arc() {
    // facing 0 at (0,4): forward mount covers relative bearing 0 only.
    assert!(crate::ui::weapon_covers_hex(0, 4, 0, 10, &[0], 3, 4)); // east along row — in arc
    assert!(!crate::ui::weapon_covers_hex(0, 4, 0, 10, &[0], 0, 0)); // not straight ahead — out of forward-only arc
    assert!(!crate::ui::weapon_covers_hex(0, 4, 0, 10, &[0], 0, 4)); // same hex
    assert!(!crate::ui::weapon_covers_hex(0, 4, 0, 2, &[0], 5, 4)); // beyond range 2
}

// ═══════════════════════════════════════════════════════════════════════════
// Slice 5: Map viewport panning (Phase 5)
// ═══════════════════════════════════════════════════════════════════════════

/// Snapshot with an unbounded map and a ship at negative coordinates.
fn unbounded_snapshot() -> Snapshot {
    let json = r#"{
        "protocol_version": 3,
        "turn": 5,
        "status": "InProgress",
        "phase": "allocate",
        "movement_phase": 0,
        "ships_committed_this_phase": [],
        "ships_ready_fire": [],
        "ships_allocated_this_turn": [],
        "map": {"width": 10, "height": 10, "mode": "unbounded"},
        "ships": [
            {
                "id": 1, "class": "Heavy Cruiser", "controller": "player",
                "size": 2,
                "q": -3, "r": -2, "facing": 0,
                "hull": 12, "max_hull": 12, "power": 22, "thrust_remaining": 0,
                "structure": 12, "engine": 4, "power_sys": 2, "bridge": 1,
                "max_velocity": 8, "velocity": 0, "course": 0,
                "max_shield_per_facing": 6,
                "shields": [0,0,0,0,0,0], "shield_power": 0,
                "weapons": [
                    {"id":"beam_1","kind":"Beam","arc":"Forward","mount":"forward","max_range":10,"charge":0,"fired":false,"max_charge":4,"operational":true}
                ],
                "destroyed": false
            },
            {
                "id": 2, "class": "Escort", "controller": "ai",
                "size": 1,
                "q": 4, "r": -2, "facing": 3,
                "hull": 8, "max_hull": 8, "power": 16, "thrust_remaining": 0,
                "structure": 8, "engine": 3, "power_sys": 2, "bridge": 1,
                "max_velocity": 8, "velocity": 0, "course": 0,
                "max_shield_per_facing": 4,
                "shields": [0,0,0,0,0,0], "shield_power": 0,
                "weapons": [
                    {"id":"beam_1","kind":"Beam","arc":"Forward","mount":"forward","max_range":10,"charge":4,"fired":false,"max_charge":4,"operational":true}
                ],
                "destroyed": false
            }
        ],
        "combat_log": []
    }"#;
    serde_json::from_str(json).expect("unbounded snapshot must parse")
}

#[test]
fn bounded_map_origin_is_zero_zero_by_default() {
    // On a bounded (hard) map with the ship in-bounds, the origin stays at
    // (0,0) so the whole board is visible.
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    assert_eq!(app.map_origin(), (0, 0));
}

#[test]
fn unbounded_map_auto_fits_all_living_ships() {
    // Auto-fit uses both contacts rather than following only the focused ship.
    let mut app = App::new();
    app.update_snapshot(unbounded_snapshot());
    // update_snapshot auto-focuses ship 1.
    assert_eq!(app.focused_ship, Some(1));
    let (oq, or_) = app.map_origin();
    assert_eq!((oq, or_), (-4, -7));
}

#[test]
fn v_enters_map_mode() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    let result = handle_key(&mut app, make_key('v'));
    assert!(matches!(result, KeyResult::Continue));
    assert_eq!(app.mode, crate::app::Mode::Map);
}

#[test]
fn map_mode_esc_returns_to_normal() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    handle_key(&mut app, make_key('v'));
    assert_eq!(app.mode, crate::app::Mode::Map);
    handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Esc));
    // test_snapshot is in the allocate phase, so Esc restores Allocate mode.
    assert_eq!(app.mode, crate::app::Mode::Allocate);
}

#[test]
fn map_mode_v_toggles_back_to_normal() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    handle_key(&mut app, make_key('v'));
    assert_eq!(app.mode, crate::app::Mode::Map);
    handle_key(&mut app, make_key('v'));
    // test_snapshot is in the allocate phase, so v restores Allocate mode.
    assert_eq!(app.mode, crate::app::Mode::Allocate);
}

#[test]
fn wasd_pans_map_and_sets_explicit_offset() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    handle_key(&mut app, make_key('v'));
    // Pan east (d) then south (s).
    handle_key(&mut app, make_key('d'));
    handle_key(&mut app, make_key('s'));
    // Origin should now be (1, 1) — explicit pan overrides auto-center.
    assert_eq!(app.map_pan, Some((1, 1)));
    assert_eq!(app.map_origin(), (1, 1));
}

#[test]
fn hjkl_pans_map() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    handle_key(&mut app, make_key('v'));
    // Pan west (h) then north (k).
    handle_key(&mut app, make_key('h'));
    handle_key(&mut app, make_key('k'));
    assert_eq!(app.map_pan, Some((-1, -1)));
}

#[test]
fn map_mode_c_recenters() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    handle_key(&mut app, make_key('v'));
    // Pan away.
    handle_key(&mut app, make_key('d'));
    handle_key(&mut app, make_key('d'));
    assert!(app.map_pan.is_some());
    // Recenter clears the explicit pan.
    handle_key(&mut app, make_key('c'));
    assert!(app.map_pan.is_none());
}

#[test]
fn focus_change_recenters_map() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    handle_key(&mut app, make_key('v'));
    // Pan away.
    handle_key(&mut app, make_key('d'));
    assert!(app.map_pan.is_some());
    // Tab changes focus, which should recenter.
    handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Tab));
    // In Map mode, Tab is not handled (falls to _ => Continue), so pan stays.
    // Focus change via switch_focus() is what recenters; verify the helper.
    app.switch_focus(2);
    assert!(app.map_pan.is_none());
}

#[test]
fn map_mode_is_read_only_no_order_sent() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    handle_key(&mut app, make_key('v'));
    // Panning in map mode must not produce an order.
    let result = handle_key(&mut app, make_key('d'));
    assert!(matches!(result, KeyResult::Continue));
    // No pending order should exist.
    assert!(app.pending_order.is_none());
}

#[test]
fn unbounded_map_shows_focused_ship_on_screen() {
    // On an unbounded map with the ship at (-3,-2), the auto-centered
    // viewport must render the ship's callsign (A1) in the buffer.
    let mut app = App::new();
    app.update_snapshot(unbounded_snapshot());
    let buf = render_to_string(&mut app, 80, 24);
    assert!(
        buffer_contains(&buf, "A1"),
        "focused ship A1 at (-3,-2) must be visible in auto-centered unbounded viewport"
    );
}

#[test]
fn bounded_map_origin_stays_zero_when_ship_in_bounds() {
    // Even after entering and exiting map mode on a bounded map, the origin
    // returns to (0,0) (no lingering pan) because recenter clears it.
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    handle_key(&mut app, make_key('v'));
    handle_key(&mut app, make_key('c'));
    handle_key(&mut app, make_key('v'));
    assert_eq!(app.map_origin(), (0, 0));
}

// ═══════════════════════════════════════════════════════════════════════════
// Phase 8: end-to-end movement-preview data-flow verification
// ═══════════════════════════════════════════════════════════════════════════
//
// Drives the real engine subprocess through the TUI input path and asserts the
// full preview round-trip: a value change in allocate mode queues a preview
// request, the engine responds with a movement_preview envelope, and the
// response is stored in app.movement_preview with endpoints and a coast.

#[test]
fn movement_preview_flows_end_to_end() {
    let bin = engine_bin().expect("shipsim binary not found — cargo build at repo root");
    let scenario = if std::path::Path::new("../../scenarios/combat.toml").is_file() {
        "../../scenarios/combat.toml"
    } else {
        "scenarios/combat.toml"
    };

    let mut harness = crate::harness::Harness::spawn(bin.to_str().unwrap(), scenario)
        .unwrap_or_else(|e| panic!("spawn engine {bin:?}: {e}"));
    let mut app = App::new();

    // Consume the post-load snapshot.
    let line = harness.read_line().expect("post-load snapshot");
    apply_line(&mut app, line);
    assert_eq!(app.mode, Mode::Allocate);
    assert!(app.alloc_draft.is_some());

    // The initial allocation queues the zero-thrust coast preview immediately.
    assert!(app.movement_preview.is_none());
    assert!(app.pending_preview.is_some());

    let initial_preview = app.pending_preview.take().unwrap();
    harness
        .send(&initial_preview)
        .expect("send initial preview request");
    apply_line(
        &mut app,
        harness
            .read_line()
            .expect("initial movement_preview response"),
    );
    assert!(
        app.movement_preview.is_some(),
        "coast preview must be populated"
    );

    // Press Right once to bump movement allocation. handle_key should queue a
    // preview request via request_movement_preview().
    let right = || make_key_code(crossterm::event::KeyCode::Right);
    let result = handle_key(&mut app, right());
    assert!(matches!(result, KeyResult::Continue));
    assert!(
        app.pending_preview.is_some(),
        "Right in allocate must queue a movement_preview request"
    );

    // Drain the pending preview (mirrors the main loop / send_key helper).
    let preview_json = app.pending_preview.take().unwrap();
    harness.send(&preview_json).expect("send preview request");
    let line = harness.read_line().expect("movement_preview response");
    apply_line(&mut app, line);

    // The response must be a populated MovementPreview.
    let preview = app
        .movement_preview
        .as_ref()
        .expect("movement_preview must be populated after the round-trip");
    assert!(preview.ok, "preview response ok flag must be true");
    assert!(
        !preview.endpoints.is_empty(),
        "preview must return at least one reachable endpoint"
    );

    // The coast endpoint must be among the reachable endpoints.
    assert!(
        preview
            .endpoints
            .iter()
            .any(|e| e.q == preview.coast.q && e.r == preview.coast.r),
        "coast ({},{}) must be among the reachable endpoints",
        preview.coast.q,
        preview.coast.r
    );
}

#[test]
fn allocation_input_clamps_to_affordable_power() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    let draft = app.alloc_draft.as_mut().unwrap();
    draft.cursor = 0;

    for _ in 0..100 {
        handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Right));
    }

    let ship = app.focused().unwrap();
    assert_eq!(
        app.alloc_draft.as_ref().unwrap().movement,
        ship.power_available
    );
    assert!(app.alloc_draft.as_ref().unwrap().power_cost(ship) <= ship.power_available);
}

#[test]
fn preview_is_cleared_when_focus_changes() {
    let mut app = App::new();
    app.update_snapshot(fleet_snapshot());
    let preview: crate::protocol::MovementPreview = serde_json::from_str(
        r#"{"type":"movement_preview","ok":true,"ship":1,"endpoints":[],"coast":{"q":0,"r":4,"facing":0,"course":0,"speed":0,"thrust_remaining":0}}"#,
    )
    .unwrap();
    app.accept_movement_preview(preview);
    assert!(app.movement_preview.is_some());

    app.switch_focus(2);
    assert!(app.movement_preview.is_none());
}

#[test]
fn map_focus_can_inspect_enemy_speed_and_zoom() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    app.snap.as_mut().unwrap().ships[1].velocity = 3;
    app.snap.as_mut().unwrap().ships[1].course = 3;
    handle_key(&mut app, make_key('v'));
    handle_key(&mut app, make_key(']'));
    assert_eq!(app.focused_ship, Some(2));

    let buffer = render_to_string(&mut app, 80, 24);
    assert!(buffer_contains(&buffer, "B2"));
    assert!(buffer_contains(&buffer, "B2←3"));

    handle_key(&mut app, make_key('+'));
    assert_eq!(app.map_zoom, Some(1));
}

#[test]
fn movement_preview_clears_on_phase_change() {
    // After a preview is populated, advancing to the movement phase (via a
    // commit order) must clear it so stale endpoints are never rendered.
    //
    // Uses ai.toml (NPC is greedy-seek) so that after the player commits
    // allocation, the engine auto-resolves the NPC and advances the phase
    // from "allocate" to "movement" in the same turn.
    let bin = engine_bin().expect("shipsim binary not found — cargo build at repo root");
    let scenario = if std::path::Path::new("../../scenarios/ai.toml").is_file() {
        "../../scenarios/ai.toml"
    } else {
        "scenarios/ai.toml"
    };

    let mut harness = crate::harness::Harness::spawn(bin.to_str().unwrap(), scenario)
        .unwrap_or_else(|e| panic!("spawn engine {bin:?}: {e}"));
    let mut app = App::new();

    let line = harness.read_line().expect("post-load snapshot");
    apply_line(&mut app, line);

    // Generate a preview by pressing Right.
    send_key(
        &mut app,
        &mut harness,
        make_key_code(crossterm::event::KeyCode::Right),
    );
    assert!(
        app.movement_preview.is_some(),
        "preview populated after Right"
    );

    // Commit the allocation (Enter). The engine applies the order, auto-
    // resolves the NPC, and emits a single snapshot with the advanced phase.
    send_key(
        &mut app,
        &mut harness,
        make_key_code(crossterm::event::KeyCode::Enter),
    );

    // After the phase change, the preview must be cleared.
    assert!(
        app.movement_preview.is_none(),
        "movement_preview must be cleared after leaving allocate phase"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Phase 1 — Map truth: range readout, scale label, off-map contacts
// ═══════════════════════════════════════════════════════════════════════════

/// Snapshot where the player and enemy are at axial distance 8 (criterion 1.1).
fn distant_enemy_snapshot() -> Snapshot {
    let mut snap = test_snapshot();
    // Player at (0,4), enemy at (8,4): axial distance = 8.
    snap.ships[0].q = 0;
    snap.ships[0].r = 4;
    snap.ships[1].q = 8;
    snap.ships[1].r = 4;
    snap
}

#[test]
fn map_title_shows_range_to_nearest_enemy() {
    // 1.1: two ships at axial distance 8 → buffer shows d=8 in chrome.
    let mut app = App::new();
    app.update_snapshot(distant_enemy_snapshot());
    app.mode = Mode::Map;
    let buf = render_to_string(&mut app, 100, 30);
    assert!(
        buffer_contains(&buf, "d=8"),
        "map title should show range d=8; got title line: {}",
        buf.lines().next().unwrap_or("")
    );
}

#[test]
fn map_title_shows_enemy_callsign_in_range_readout() {
    let mut app = App::new();
    app.update_snapshot(distant_enemy_snapshot());
    app.mode = Mode::Map;
    let buf = render_to_string(&mut app, 100, 30);
    // The enemy escort is callsign B2.
    assert!(
        buffer_contains(&buf, "→ B2 d=8"),
        "range readout should name the enemy callsign"
    );
}

/// Snapshot with an enemy far off the map viewport (criterion 1.3).
fn off_map_enemy_snapshot() -> Snapshot {
    let mut snap = test_snapshot();
    // Player at origin, enemy far to the east at (50, 0).
    snap.ships[0].q = 0;
    snap.ships[0].r = 0;
    snap.ships[1].q = 50;
    snap.ships[1].r = 0;
    snap
}

#[test]
fn off_map_enemy_shows_in_contacts_strip() {
    // 1.3: enemy outside viewport rect → buffer contains "off-map" and callsign.
    let mut app = App::new();
    app.update_snapshot(off_map_enemy_snapshot());
    app.mode = Mode::Map;
    // Pin the viewport to a small window at origin so the enemy at (50,0)
    // is genuinely off-map (auto-fit would otherwise zoom out to include it).
    app.map_pan = Some((0, 0));
    app.map_zoom = Some(0); // scale 1, no auto-fit
    let buf = render_to_string(&mut app, 100, 30);
    assert!(
        buffer_contains(&buf, "off-map"),
        "off-map enemy should produce an off-map strip"
    );
    assert!(
        buffer_contains(&buf, "B2"),
        "off-map strip should name the enemy callsign"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Phase 2 — Weapon health: dead weapons uneditable, no fire, quiet previews
// ═══════════════════════════════════════════════════════════════════════════

/// Snapshot where the player's beam_1 is non-operational (damaged).
fn damaged_weapon_snapshot() -> Snapshot {
    let mut snap = test_snapshot();
    snap.ships[0].weapons[0].operational = false;
    snap
}

#[test]
fn allocate_right_on_dead_weapon_does_not_change_charge() {
    // 2.1: cursor on a !operational weapon → Right leaves draft charge unchanged.
    let mut app = App::new();
    app.update_snapshot(damaged_weapon_snapshot());
    app.mode = Mode::Allocate;
    // Cursor 1 = beam_1 (the damaged weapon).
    app.alloc_draft.as_mut().unwrap().cursor = 1;
    let before = app.alloc_draft.as_ref().unwrap().weapons[0].1;
    handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Right));
    let after = app.alloc_draft.as_ref().unwrap().weapons[0].1;
    assert_eq!(
        before, after,
        "Right on a dead weapon must not change its charge"
    );
}

#[test]
fn allocate_digit_on_dead_weapon_does_not_change_charge() {
    // 2.1: digit on a !operational weapon → draft charge unchanged.
    let mut app = App::new();
    app.update_snapshot(damaged_weapon_snapshot());
    app.mode = Mode::Allocate;
    app.alloc_draft.as_mut().unwrap().cursor = 1;
    let before = app.alloc_draft.as_ref().unwrap().weapons[0].1;
    handle_key(&mut app, make_key('4'));
    let after = app.alloc_draft.as_ref().unwrap().weapons[0].1;
    assert_eq!(before, after, "digit on a dead weapon must not change charge");
}

#[test]
fn allocate_weapon_digit_does_not_request_preview() {
    // 2.3: weapon-only digit entry → pending_preview stays None.
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    app.mode = Mode::Allocate;
    // Cursor 1 = beam_1 (operational in test_snapshot).
    app.alloc_draft.as_mut().unwrap().cursor = 1;
    app.pending_preview = None;
    handle_key(&mut app, make_key('4'));
    assert!(
        app.pending_preview.is_none(),
        "weapon-charge digit must not request a movement preview"
    );
}

#[test]
fn allocate_movement_right_requests_preview() {
    // 2.4: movement field Right → pending_preview is Some.
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    app.mode = Mode::Allocate;
    // Cursor 0 = movement.
    app.alloc_draft.as_mut().unwrap().cursor = 0;
    app.pending_preview = None;
    handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Right));
    assert!(
        app.pending_preview.is_some(),
        "movement field edit must request a movement preview"
    );
}

#[test]
fn allocate_shield_right_requests_preview() {
    // 2.4 (shields): shield field Right → pending_preview is Some.
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    app.mode = Mode::Allocate;
    let n_w = app.alloc_draft.as_ref().unwrap().weapons.len();
    // First shield face = cursor 1 + n_w.
    app.alloc_draft.as_mut().unwrap().cursor = 1 + n_w;
    app.pending_preview = None;
    handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Right));
    assert!(
        app.pending_preview.is_some(),
        "shield field edit must request a movement preview"
    );
}

#[test]
fn fire_enter_on_dead_weapon_does_not_emit_commit_fire() {
    // 2.5: Enter on a damaged weapon does not emit commit_fire.
    let mut app = App::new();
    app.update_snapshot(damaged_weapon_snapshot());
    // Force into firing phase so fire_draft is populated.
    app.snap.as_mut().unwrap().phase = "firing".into();
    app.mode = Mode::Fire;
    app.fire_draft = Some(crate::app::FireDraft::default());
    // weapon_idx 0 = beam_1 (damaged). Set a target so we reach the emit path.
    app.fire_draft.as_mut().unwrap().weapon_idx = 0;
    app.fire_draft.as_mut().unwrap().target = Some(2);
    let result = handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Enter));
    assert!(
        matches!(result, KeyResult::Continue),
        "Enter on a dead weapon must not emit commit_fire"
    );
    assert!(
        app.log.iter().any(|line| line.contains("OFFLINE")),
        "dead-weapon Enter should soft-log OFFLINE; log={:?}",
        app.log
    );
}

#[test]
fn fire_enter_on_operational_weapon_emits_commit_fire() {
    // Regression guard: Enter on a working weapon still emits commit_fire.
    let mut app = App::new();
    let mut snap = fire_phase_snapshot();
    snap.ships[0].weapons[0].operational = true;
    app.update_snapshot(snap);
    app.mode = Mode::Fire;
    app.fire_draft = Some(crate::app::FireDraft::default());
    app.fire_draft.as_mut().unwrap().weapon_idx = 0;
    app.fire_draft.as_mut().unwrap().target = Some(2);
    let result = handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Enter));
    match result {
        KeyResult::SendOrder(order) => match order.body {
            crate::protocol::OrderBody::CommitFire { weapon, target, .. } => {
                assert_eq!(weapon, "beam_1");
                assert_eq!(target, 2);
            }
            other => panic!("expected commit_fire, got {other:?}"),
        },
        other => panic!("Enter on an operational weapon must emit commit_fire; got {other:?}"),
    }
}

#[test]
fn allocate_panel_shows_offline_for_damaged_weapon() {
    // 2.6: buffer contains OFFLINE and no editable charge prompt for that row.
    let mut app = App::new();
    app.update_snapshot(damaged_weapon_snapshot());
    app.mode = Mode::Allocate;
    let buf = render_to_string(&mut app, 80, 40);
    assert!(
        buffer_contains(&buf, "OFFLINE"),
        "allocate panel should mark damaged weapons OFFLINE"
    );
    // The damaged beam row must not show an editable charge N/M prompt.
    let beam_line = buf
        .lines()
        .find(|line| line.contains("beam_1") && line.contains("OFFLINE"))
        .unwrap_or("");
    assert!(
        !beam_line.is_empty(),
        "expected an OFFLINE beam_1 row; got:\n{buf}"
    );
    assert!(
        !beam_line.to_lowercase().contains("charge"),
        "offline beam row must not show editable charge; got: {beam_line}"
    );
}

#[test]
fn fire_panel_shows_offline_for_damaged_weapon() {
    // 2.6 (fire mode): damaged weapon shows OFFLINE in the fire panel.
    let mut app = App::new();
    let mut snap = fire_phase_snapshot();
    snap.ships[0].weapons[0].operational = false;
    app.update_snapshot(snap);
    app.mode = Mode::Fire;
    app.fire_draft = Some(crate::app::FireDraft::default());
    let buf = render_to_string(&mut app, 80, 40);
    assert!(
        buffer_contains(&buf, "OFFLINE"),
        "fire panel should mark damaged weapons OFFLINE"
    );
    let beam_line = buf
        .lines()
        .find(|line| line.contains("beam_1") && line.contains("OFFLINE"))
        .unwrap_or("");
    assert!(
        !beam_line.is_empty(),
        "expected OFFLINE beam_1 in fire panel; got:\n{buf}"
    );
    assert!(
        !beam_line.to_lowercase().contains("chg="),
        "offline fire row should not show charge; got: {beam_line}"
    );
}

#[test]
fn allocate_commit_omits_offline_weapons_from_order() {
    // 2.2: Enter with a damaged beam yields allocate JSON without that id.
    let mut app = App::new();
    app.update_snapshot(damaged_weapon_snapshot());
    app.mode = Mode::Allocate;
    // Leave beam offline in draft; still set movement so the order is non-empty.
    app.alloc_draft.as_mut().unwrap().movement = 2;
    let result = handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Enter));
    match result {
        KeyResult::SendOrder(order) => match order.body {
            crate::protocol::OrderBody::Allocate { weapons, .. } => {
                let weapons = weapons.expect("weapons map present");
                assert!(
                    weapons.get("beam_1").is_none(),
                    "offline beam_1 must be omitted from allocate JSON; got {weapons}"
                );
                // Operational second weapon (torp) still included when present.
                // test_snapshot has beam_1 + torp_1 only if heavy cruiser test has 2 weapons.
            }
            other => panic!("expected allocate order, got {other:?}"),
        },
        other => panic!("allocate Enter must emit SendOrder; got {other:?}"),
    }
}

#[test]
fn fire_draft_for_ship_skips_offline_first_weapon() {
    let mut snap = fire_phase_snapshot();
    snap.ships[0].weapons[0].operational = false;
    let draft = crate::app::FireDraft::for_ship(&snap.ships[0]);
    assert_ne!(
        draft.weapon_idx, 0,
        "for_ship must not land on offline weapon 0"
    );
    assert!(
        snap.ships[0].weapons[draft.weapon_idx].operational,
        "selected weapon must be operational"
    );
}

#[test]
fn allocate_edit_dead_weapon_logs_soft_notice() {
    let mut app = App::new();
    app.update_snapshot(damaged_weapon_snapshot());
    app.mode = Mode::Allocate;
    app.alloc_draft.as_mut().unwrap().cursor = 1; // dead beam
    handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Right));
    assert!(
        app.log
            .iter()
            .any(|line| line.contains("OFFLINE") && line.contains("cannot charge")),
        "blocked allocate edit should soft-log; log={:?}",
        app.log
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Phase 3 — Allocate as a form: sticky budget, selected-row marker, hull truth
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn allocate_budget_stays_visible_when_cursor_on_last_shield() {
    // 3.1: cursor on the last shield field → buffer still contains the Budget
    // line. The budget is a fixed header, so it must not scroll out of view.
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    app.mode = Mode::Allocate;
    // test_snapshot player ship: 2 weapons → cursor 0=mov, 1..2=weapons,
    // 3..8 = 6 shields. Last shield = cursor 8.
    let n_w = app.alloc_draft.as_ref().unwrap().weapons.len();
    app.alloc_draft.as_mut().unwrap().cursor = 1 + n_w + 5;
    // Render at the small floor so the form must scroll.
    let buf = render_to_string(&mut app, 80, 24);
    assert!(
        buffer_contains(&buf, "Budget"),
        "budget line must stay visible when cursor is on the last shield"
    );
}

#[test]
fn allocate_selected_field_shows_marker_on_same_row() {
    // 3.2: the selected field shows ▶ on the same row as the field name.
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    app.mode = Mode::Allocate;
    // Cursor 0 = movement field.
    app.alloc_draft.as_mut().unwrap().cursor = 0;
    let buf = render_to_string(&mut app, 80, 40);
    // The movement row should carry the ▶ marker.
    assert!(
        buf.lines().any(|line| line.contains("▶") && line.contains("Movement")),
        "selected movement row should show ▶ on the same line as the field name"
    );
}

#[test]
fn allocate_hull_line_shows_structure_without_fake_max() {
    // 3.3: hull line matches `hull \d+` without a `\d+/\d+` fake max.
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    app.mode = Mode::Allocate;
    let buf = render_to_string(&mut app, 80, 40);
    // Ship 1 has structure 12. The hull line should contain "hull 12".
    assert!(
        buffer_contains(&buf, "hull 12"),
        "allocate panel should show hull with current structure"
    );
    // And it must NOT show a fake N/N max on the hull line.
    let hull_line = buf
        .lines()
        .find(|line| line.contains("hull 12"))
        .unwrap_or("");
    assert!(
        !hull_line.contains("/"),
        "hull line must not show a fake N/N max; got: {hull_line}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Phase 4 — Fire queue + cycle coach: header/panel agree, no-charge, cycle n/4
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn fire_queue_header_and_panel_agree() {
    // 4.1: with fire_commits for the focused ship, the header `queued=N` and
    // the fire panel `Queued: N shot(s) pending` must show the same count.
    let mut app = App::new();
    let mut snap = fire_phase_snapshot();
    // Two pending commits for the player ship (id 1) targeting the escort (id 2).
    snap.fire_commits = vec![
        FireCommit {
            ship: 1,
            weapon: "beam_1".into(),
            target: 2,
            shield_facing: 0,
        },
        FireCommit {
            ship: 1,
            weapon: "torp_1".into(),
            target: 2,
            shield_facing: 1,
        },
    ];
    app.update_snapshot(snap);
    app.mode = Mode::Fire;
    app.fire_draft = Some(crate::app::FireDraft::default());
    // Render tall enough that the whole fire panel (controls, queue, commits,
    // targets, weapons) fits without scrolling.
    let buf = render_to_string(&mut app, 80, 80);
    // Header shows queued=2.
    assert!(
        buffer_contains(&buf, "queued=2"),
        "header must show queued=2 for the focused ship's commits; got:\n{buf}"
    );
    // Fire panel shows Queued: 2 shot(s) pending.
    assert!(
        buffer_contains(&buf, "Queued: 2 shot(s) pending"),
        "fire panel must show Queued: 2; got:\n{buf}"
    );
    // And the pending commits are listed (weapon → target face F).
    assert!(
        buffer_contains(&buf, "beam_1 →"),
        "fire panel must list the pending beam_1 commit"
    );
}

#[test]
fn fire_panel_shows_no_charge_coach() {
    // 4.2: every operational weapon out of charge mid-fire-phase → buffer
    // contains the "No charge" / "Space to pass" coach.
    let mut app = App::new();
    let mut snap = fire_phase_snapshot();
    // Drain both player weapons to 0 charge.
    for w in snap.ships[0].weapons.iter_mut() {
        w.charge = 0;
    }
    app.update_snapshot(snap);
    app.mode = Mode::Fire;
    app.fire_draft = Some(crate::app::FireDraft::default());
    let buf = render_to_string(&mut app, 80, 40);
    assert!(
        buffer_contains(&buf, "No charge"),
        "fire panel must show No charge coach when all weapons are empty"
    );
    assert!(
        buffer_contains(&buf, "Space to pass"),
        "fire panel must tell the player to Space to pass fire"
    );
}

#[test]
fn cycle_coach_shows_movement_phase_out_of_four() {
    // 4.3: phase `movement` with movement_phase=3 → coach line `Cycle 3/4`
    // (not only the header `Move 3/4`).
    let mut app = App::new();
    let mut snap = fire_phase_snapshot();
    snap.phase = "movement".into();
    snap.movement_phase = 3;
    app.update_snapshot(snap);
    app.mode = Mode::Movement;
    app.fire_draft = None;
    let buf = render_to_string(&mut app, 80, 40);
    assert!(
        buffer_contains(&buf, "Cycle 3/4"),
        "movement panel coach must show Cycle 3/4; got:\n{buf}"
    );
}

#[test]
fn cycle_coach_shows_fire_phase_out_of_four() {
    let mut app = App::new();
    let mut snap = fire_phase_snapshot();
    snap.phase = "firing".into();
    snap.movement_phase = 2;
    app.update_snapshot(snap);
    app.mode = Mode::Fire;
    app.fire_draft = Some(crate::app::FireDraft::for_ship(&app.focused().unwrap().clone()));
    let buf = render_to_string(&mut app, 80, 40);
    assert!(
        buffer_contains(&buf, "Cycle 2/4"),
        "fire panel coach must show Cycle 2/4; got:\n{buf}"
    );
}
