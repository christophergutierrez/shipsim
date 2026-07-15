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
    callsign, facing_arrow, shield_label, ErrorResponse, Maneuver, Order, Snapshot,
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
    assert_eq!(draft.weapons.get("beam_1"), Some(&4));
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
fn key_tab_cycles_focus() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());
    assert_eq!(app.focused_ship, Some(1));

    handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Tab));
    assert_eq!(app.focused_ship, Some(2));

    handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Tab));
    assert_eq!(app.focused_ship, Some(1));
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
fn tutorial_allocation_scroll_keeps_last_shield_visible() {
    let mut app = App::new_with_tutorial();
    app.update_snapshot(test_snapshot());
    let draft = app.alloc_draft.as_mut().unwrap();
    draft.cursor = 8;

    let buf = render_to_string(&mut app, 80, 24);

    assert!(buffer_contains(&buf, "FL:0"));
    assert!(buffer_contains(&buf, "▶ FL:0"));
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
    // Header renders " turn 1 " and " phase=allocate "
    assert!(buffer_contains(&buf, "turn 1") || buffer_contains(&buf, "Turn 1"));
    assert!(buffer_contains(&buf, "allocate"));
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
fn full_cycle_tab_focus_then_render() {
    let mut app = App::new();
    app.update_snapshot(test_snapshot());

    // Focus ship 2.
    handle_key(&mut app, make_key_code(crossterm::event::KeyCode::Tab));
    assert_eq!(app.focused_ship, Some(2));

    // Render — should show ship 2's details.
    let buf = render_to_string(&mut app, 80, 24);
    assert!(buffer_contains(&buf, "Escort"));
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
    let e_key = || make_key('e');

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

    // T1: accel×3, turn 3, ready each fire, end
    for _ in 0..3 {
        send_key(&mut app, &mut harness, t_key());
        send_key(&mut app, &mut harness, space());
    }
    send_key(&mut app, &mut harness, make_key('3'));
    send_key(&mut app, &mut harness, space());
    send_key(&mut app, &mut harness, e_key());
    send_key(&mut app, &mut harness, make_key('y'));
    assert_eq!(app.snap.as_ref().map(|s| s.turn), Some(2), "enter turn 2");

    // T2 allocate: mov 10, F6, FR3, FL3
    for _ in 0..10 {
        send_key(&mut app, &mut harness, right());
    }
    for _ in 0..4 {
        send_key(&mut app, &mut harness, down());
    } // to F
    for _ in 0..6 {
        send_key(&mut app, &mut harness, right());
    }
    send_key(&mut app, &mut harness, down()); // FR
    for _ in 0..3 {
        send_key(&mut app, &mut harness, right());
    }
    for _ in 0..4 {
        send_key(&mut app, &mut harness, down());
    } // to FL
    for _ in 0..3 {
        send_key(&mut app, &mut harness, right());
    }
    send_key(&mut app, &mut harness, enter());

    // T2: brake×3 + push west
    for _ in 0..4 {
        send_key(&mut app, &mut harness, t_key());
        send_key(&mut app, &mut harness, space());
    }
    send_key(&mut app, &mut harness, e_key());
    send_key(&mut app, &mut harness, make_key('y'));
    assert_eq!(app.snap.as_ref().map(|s| s.turn), Some(3), "enter turn 3");

    // T3 allocate (same shields pattern)
    for _ in 0..10 {
        send_key(&mut app, &mut harness, right());
    }
    for _ in 0..4 {
        send_key(&mut app, &mut harness, down());
    }
    for _ in 0..6 {
        send_key(&mut app, &mut harness, right());
    }
    send_key(&mut app, &mut harness, down());
    for _ in 0..3 {
        send_key(&mut app, &mut harness, right());
    }
    for _ in 0..4 {
        send_key(&mut app, &mut harness, down());
    }
    for _ in 0..3 {
        send_key(&mut app, &mut harness, right());
    }
    send_key(&mut app, &mut harness, enter());

    // T3 close: accel, ready, accel, ready, accel, fire×3, ready
    send_key(&mut app, &mut harness, t_key());
    send_key(&mut app, &mut harness, space());
    send_key(&mut app, &mut harness, t_key());
    send_key(&mut app, &mut harness, space());
    send_key(&mut app, &mut harness, t_key());
    // fire window at PB
    let rng = {
        let snap = app.snap.as_ref().unwrap();
        let a = snap.ship(1).unwrap();
        let b = snap.ship(2).unwrap();
        let dq = (a.q - b.q).abs();
        let dr = (a.r - b.r).abs();
        // axial distance for same r row:
        dq.max(dr)
    };
    assert_eq!(rng, 1, "expected point-blank before volley, got {rng}");
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
