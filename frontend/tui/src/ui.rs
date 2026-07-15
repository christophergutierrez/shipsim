//! UI rendering — all ratatui widget construction.
//!
//! The layout is a fixed Small-tier grid:
//!   ┌─────────────────────────────────┐
//!   │ Status header (1 line)          │
//!   ├──────────────┬──────────────────┤
//!   │ Map          │ Ship status      │
//!   │              │ (focused ship)   │
//!   ├──────────────┴──────────────────┤
//!   │ Input panel (phase-specific)    │
//!   ├─────────────────────────────────┤
//!   │ Events / log (recent)           │
//!   └─────────────────────────────────┘

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Mode};
use crate::protocol::{callsign, facing_arrow, shield_label, Snapshot};

/// Render the full frame.
pub fn render(f: &mut Frame, app: &mut App) {
    let size = f.area();

    // This layout has a measured Small-tier floor. Rendering a partial map and
    // hiding the instruction bar is worse than pausing with an actionable
    // resize message; the snapshot and engine keep running underneath.
    const MIN_WIDTH: u16 = 80;
    const MIN_HEIGHT: u16 = 24;
    if size.width < MIN_WIDTH || size.height < MIN_HEIGHT {
        app.terminal_too_small = true;
        let msg = format!(
            "Terminal too small to play\n\nResize to at least {MIN_WIDTH}×{MIN_HEIGHT}\n\nYour game is paused here and will resume when the window grows."
        );
        let p = Paragraph::new(msg)
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("shipsim TUI"));
        f.render_widget(p, size);
        return;
    }
    app.terminal_too_small = false;

    // If no snapshot yet, show a loading message.
    if app.snap.is_none() {
        let msg = if app.engine_dead {
            "Engine exited. Press q to quit."
        } else {
            "Loading…"
        };
        let p = Paragraph::new(msg).alignment(ratatui::layout::Alignment::Center);
        f.render_widget(p, size);
        return;
    }

    let snap = app.snap.as_ref().unwrap();

    if app.tutorial.is_some() {
        // Tutorial gets one coherent lower workspace: the active form is
        // beside resolution history and coach text instead of three slivers.
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Min(10),
                Constraint::Min(8),
            ])
            .split(size);
        render_header(f, app, snap, chunks[0]);
        render_middle(f, app, snap, chunks[1]);
        let bottom = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(chunks[2]);
        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(4)])
            .split(bottom[1]);
        let status_str = snap.status.clone();
        render_input_panel(f, app, &status_str, snap.is_over(), bottom[0]);
        render_events_log(f, app, right[0]);
        render_tutorial_panel(f, app, right[1]);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(8),
            Constraint::Length(6),
        ])
        .split(size);
    render_header(f, app, snap, chunks[0]);
    render_middle(f, app, snap, chunks[1]);
    let status_str = snap.status.clone();
    render_input_panel(f, app, &status_str, snap.is_over(), chunks[2]);
    render_events_log(f, app, chunks[3]);
}

fn tutorial_do_now(app: &App) -> Option<String> {
    let t = app.tutorial.as_ref()?;
    if t.is_complete() {
        return Some(t.do_now_line(None, None));
    }
    let (cursor, value) = app
        .alloc_draft
        .as_ref()
        .map(|d| (Some(d.cursor), Some(d.field_value())))
        .unwrap_or((None, None));
    Some(t.do_now_line(cursor, value))
}

fn render_header(f: &mut Frame, app: &App, snap: &Snapshot, area: Rect) {
    let status_color = if snap.is_over() {
        Color::Yellow
    } else {
        Color::Green
    };
    let queued = snap.fire_commits.len();

    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!(" turn {} ", snap.turn),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("│"),
        Span::styled(
            format!(" phase={} ", snap.phase),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw("│"),
        Span::styled(
            format!(
                " cycle={}/4 ",
                if snap.phase == "allocate" {
                    0
                } else {
                    snap.movement_phase
                }
            ),
            Style::default().fg(Color::Magenta),
        ),
        Span::raw("│"),
        Span::styled(
            format!(" {} ", snap.status),
            Style::default().fg(status_color),
        ),
        Span::raw("│"),
        Span::styled(
            if queued > 0 {
                format!(" queued={queued} ")
            } else {
                " queued=0 ".to_string()
            },
            if queued > 0 {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ),
        if snap.end_turn_warning {
            Span::styled("│ actions remain ", Style::default().fg(Color::Red))
        } else {
            Span::raw("")
        },
        Span::raw(if app.tutorial.is_some() {
            "│ TUTORIAL"
        } else {
            ""
        }),
    ])];

    if let Some(do_now) = tutorial_do_now(app) {
        lines.push(Line::from(Span::styled(
            format!(" {do_now}"),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
    }

    if let Some(confirmation) = app.confirmation {
        let text = match confirmation {
            crate::app::Confirmation::Quit => " Confirm quit? y = yes · n/Esc = cancel",
            crate::app::Confirmation::EndTurn => {
                " Confirm end turn? y = yes · n/Esc = cancel (queued fire may be discarded)"
            }
        };
        lines.insert(
            0,
            Line::from(Span::styled(
                text,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
        );
    }

    let block = Block::default().borders(Borders::ALL).title("shipsim TUI");
    let p = Paragraph::new(lines).block(block);
    f.render_widget(p, area);
}

fn render_middle(f: &mut Frame, app: &App, snap: &Snapshot, area: Rect) {
    let mid = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    render_map(f, app, snap, mid[0]);
    render_ship_status(f, app, snap, mid[1]);
}

fn render_map(f: &mut Frame, app: &App, snap: &Snapshot, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title("Map");

    // Build a simple grid of cells.
    let w = snap.map.width as usize;
    let h = snap.map.height as usize;

    // Build occupancy map: (q,r) → ship
    let mut lines: Vec<Line> = Vec::new();

    // Column header
    let mut hdr = String::from("   ");
    for q in 0..w {
        hdr.push_str(&format!("{:<4}", q % 10));
    }
    lines.push(Line::from(Span::styled(
        hdr,
        Style::default().fg(Color::DarkGray),
    )));

    for r in 0..h {
        let mut row = format!("{:2} ", r);
        for q in 0..w {
            let ship = snap
                .ships
                .iter()
                .find(|s| s.q as usize == q && s.r as usize == r);
            if let Some(s) = ship {
                let cs = callsign(s);
                let arrow = facing_arrow(s.facing);
                let is_focused = app.focused_ship == Some(s.id);
                let is_destroyed = s.destroyed;
                let cell = if is_destroyed {
                    format!("x{}  ", cs.chars().next().unwrap_or('?'))
                } else {
                    format!("{}{}  ", cs, arrow)
                };
                let span = if is_focused {
                    Span::styled(
                        cell,
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )
                } else if s.controller == "player" {
                    Span::styled(cell, Style::default().fg(Color::Green))
                } else {
                    Span::styled(cell, Style::default().fg(Color::Red))
                };
                row.push_str(&span.content);
            } else {
                row.push_str(".   ");
            }
        }
        lines.push(Line::from(row));
    }

    // Legend
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "A#=player B#=ai C#=scripted  →=facing  x=destroyed",
        Style::default().fg(Color::DarkGray),
    )));

    let p = Paragraph::new(lines).block(block);
    f.render_widget(p, area);
}

fn render_ship_status(f: &mut Frame, app: &App, snap: &Snapshot, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title("Ship Status");

    let mut lines: Vec<Line> = Vec::new();

    // Focused ship details
    if let Some(ship) = app.focused() {
        let cs = callsign(ship);
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", cs),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("#{} {} size={}", ship.id, ship.class, ship.size)),
        ]));
        lines.push(Line::from(format!(
            "  @({},{}) face={}{}  hull={}/{}",
            ship.q,
            ship.r,
            ship.facing,
            facing_arrow(ship.facing),
            ship.structure,
            ship.structure, // structure is current; max not in snapshot directly
        )));

        // Power
        lines.push(Line::from(format!(
            "  power: {} avail, {} engine, {} sys",
            ship.power_available, ship.engine, ship.power_sys
        )));

        // Shields
        let shield_str: Vec<String> = (0..6)
            .map(|i| {
                let powered = ship.shields_powered.get(i).copied().unwrap_or(0);
                let remaining = ship.shields_remaining.get(i).copied().unwrap_or(0);
                format!("{}:{}/{}", shield_label(i as u32), remaining, powered)
            })
            .collect();
        lines.push(Line::from(format!(
            "  shields: {}",
            shield_str[..3].join(" ")
        )));
        lines.push(Line::from(format!(
            "           {}",
            shield_str[3..].join(" ")
        )));

        // Weapons
        lines.push(Line::from("  weapons:"));
        for w in &ship.weapons {
            let op = if w.operational { "" } else { " [DAMAGED]" };
            let fired = if w.fired { " [fired]" } else { "" };
            lines.push(Line::from(format!(
                "    {} {} rng≤{} chg={}/{}{}{}",
                w.id, w.kind, w.max_range, w.charge, w.max_charge, fired, op
            )));
        }
    } else {
        lines.push(Line::from(" (no ship focused)"));
    }

    // Enemy contacts
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " Contacts:",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    for s in &snap.ships {
        if s.id == app.focused_ship.unwrap_or(-1) || s.destroyed {
            continue;
        }
        let cs = callsign(s);
        let dist = if let Some(me) = app.focused() {
            hex_dist(me.q, me.r, s.q, s.r)
        } else {
            0
        };
        lines.push(Line::from(format!(
            "  {} #{} {} @({},{}) rng={} {}",
            cs,
            s.id,
            s.class,
            s.q,
            s.r,
            dist,
            facing_arrow(s.facing)
        )));
    }

    let p = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

fn render_input_panel(f: &mut Frame, app: &mut App, status: &str, _is_over: bool, area: Rect) {
    let (title, lines) = match &app.mode {
        Mode::Normal => (
            "Help",
            vec![
                Line::from(" q: quit  Tab: cycle focus  Enter: act in phase"),
                Line::from(" a: allocate  m: move  f: fire  e: end turn"),
                Line::from(""),
            ],
        ),
        Mode::Allocate => render_allocate_panel(app),
        Mode::Movement => render_movement_panel(app),
        Mode::Fire => render_fire_panel(app),
        Mode::GameOver => (
            "Game Over",
            vec![
                Line::from(Span::styled(
                    format!(" Game status: {}", status),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(" q: quit"),
            ],
        ),
    };

    let title = title.to_string();

    let block = Block::default().borders(Borders::ALL).title(title);
    let scroll = if matches!(app.mode, Mode::Allocate) {
        allocate_scroll(app, area)
    } else {
        0
    };
    let wrap = if matches!(app.mode, Mode::Allocate) {
        Wrap { trim: false }
    } else {
        Wrap { trim: true }
    };
    let p = Paragraph::new(lines)
        .block(block)
        .wrap(wrap)
        .scroll((scroll, 0));
    f.render_widget(p, area);
}

fn allocate_scroll(app: &App, area: Rect) -> u16 {
    let Some(draft) = &app.alloc_draft else {
        return 0;
    };
    let n_weapons = draft.weapons.len();
    let line: u16 = match draft.cursor {
        0 => 1,
        n if n <= n_weapons => (2 + n) as u16,
        n => (3 + n) as u16,
    };
    let visible = area.height.saturating_sub(2).max(1);
    line.saturating_sub(visible.saturating_sub(1))
}

fn render_allocate_panel(app: &App) -> (&'static str, Vec<Line<'static>>) {
    let ship = match app.focused() {
        Some(s) => s,
        None => return ("Allocate", vec![Line::from(" (no ship focused)")]),
    };

    let draft = match &app.alloc_draft {
        Some(d) => d,
        None => return ("Allocate", vec![Line::from(" (no draft)")]),
    };

    let cost = draft.power_cost(ship);
    let pool = ship.power_available;
    let balance = pool as i64 - cost as i64;
    let budget_style = if balance < 0 {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };

    let mut lines = vec![
        Line::from(vec![
            Span::raw(format!(" Pwr {cost}/{pool} (")),
            Span::styled(
                if balance < 0 {
                    format!("{} over", -balance)
                } else {
                    format!("{balance} free")
                },
                budget_style,
            ),
            Span::raw(") · ▶"),
        ]),
        Line::from(format!(
            "{}Movement: {} · digits=set · Backspace=clear",
            if draft.cursor == 0 { "▶ " } else { "  " },
            draft.movement
        )),
    ];

    lines.push(Line::from(" Weapons:"));
    for (i, (id, chg)) in draft.weapons.iter().enumerate() {
        let max = ship
            .weapons
            .iter()
            .find(|w| &w.id == id)
            .map(|w| w.max_charge)
            .unwrap_or(0);
        let cur = ship
            .weapons
            .iter()
            .find(|w| &w.id == id)
            .map(|w| w.charge)
            .unwrap_or(0);
        let mark = if draft.cursor == i + 1 { "▶ " } else { "  " };
        let carried = if cur > 0 && *chg >= cur {
            format!(" (carried {cur})")
        } else {
            String::new()
        };
        lines.push(Line::from(format!(
            "{mark}{id} charge={chg}/{max}{carried}"
        )));
    }

    lines.push(Line::from(" Shields:"));
    let n_w = draft.weapons.len();
    for i in 0..6 {
        let mark = if draft.cursor == 1 + n_w + i {
            "▶ "
        } else {
            "  "
        };
        lines.push(Line::from(format!(
            "{mark}{}:{}",
            shield_label(i as u32),
            draft.shields.get(i).copied().unwrap_or(0)
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(" Enter commit · ↓/j next · ←/→ adjust"));

    ("Allocate", lines)
}

fn render_movement_panel(app: &App) -> (&'static str, Vec<Line<'static>>) {
    let ship = match app.focused() {
        Some(s) => s,
        None => return ("Movement", vec![Line::from(" (no ship focused)")]),
    };

    let lines = vec![
        Line::from(format!(
            " {} @({},{}) face={}{} vel={} course={}{} thrust={} (next accel: {}→{})",
            callsign(ship),
            ship.q,
            ship.r,
            ship.facing,
            facing_arrow(ship.facing),
            ship.velocity,
            ship.course,
            facing_arrow(ship.course),
            ship.thrust_remaining,
            ship.thrust_remaining,
            ship.thrust_remaining.saturating_sub(1)
        )),
        Line::from(""),
        Line::from(" c: coast (slide only)"),
        Line::from(" t: accel (along facing — thrust +1/−1/revector)"),
        Line::from(" 0-5: turn to absolute facing"),
        Line::from(" digits: set absolute facing (not speed)"),
        Line::from(" r: turn +1 facing"),
        Line::from(""),
        Line::from(" Space (in fire): ready   e: end turn"),
    ];

    ("Movement", lines)
}

fn render_fire_panel(app: &App) -> (&'static str, Vec<Line<'static>>) {
    let snap = match &app.snap {
        Some(s) => s,
        None => return ("Fire", vec![Line::from(" (no snapshot)")]),
    };
    let ship = match app.focused() {
        Some(s) => s,
        None => return ("Fire", vec![Line::from(" (no ship focused)")]),
    };

    let draft = match &app.fire_draft {
        Some(d) => d,
        None => return ("Fire", vec![Line::from(" (no draft)")]),
    };

    let mut lines = Vec::new();

    // List weapons
    lines.push(Line::from(" Weapons:"));
    for (i, w) in ship.weapons.iter().enumerate() {
        let marker = if i == draft.weapon_idx { "▶" } else { " " };
        let queued = snap
            .fire_commits
            .iter()
            .any(|c| c.ship == ship.id && c.weapon == w.id);
        let queued_str = if queued { " [QUEUED]" } else { "" };
        let charge_str = if w.charge > 0 {
            format!("chg={}", w.charge)
        } else {
            "UNCHARGED".to_string()
        };
        lines.push(Line::from(format!(
            " {} {} {} rng≤{} {}{}",
            marker, w.id, w.kind, w.max_range, charge_str, queued_str
        )));
    }

    // Targets
    lines.push(Line::from(""));
    lines.push(Line::from(" Targets:"));
    for s in &snap.ships {
        if s.id == ship.id || s.destroyed {
            continue;
        }
        let marker = if draft.target == Some(s.id) {
            "▶"
        } else {
            " "
        };
        let dist = hex_dist(ship.q, ship.r, s.q, s.r);
        lines.push(Line::from(format!(
            " {} {} #{} {} rng={} face={}{} size={}",
            marker,
            callsign(s),
            s.id,
            s.class,
            dist,
            s.facing,
            facing_arrow(s.facing),
            s.size
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(format!(
        " Shield facing: {}  (←/→ cycle)",
        shield_label(draft.shield_facing)
    )));
    lines.push(Line::from(
        " digits: pick target  · ←/→: choose shield facing",
    ));
    lines.push(Line::from(Span::styled(
        " Hit chance uses range, arc, and target size; smaller ships are harder to hit.",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(
        " Enter: fire  Tab: next weapon/target  Esc: cancel",
    ));

    ("Fire", lines)
}

fn render_events_log(f: &mut Frame, app: &App, area: Rect) {
    let event_block = Block::default().borders(Borders::ALL).title("Combat Log");
    let events = if app.recent_events.is_empty() {
        &app.combat_history
    } else {
        &app.recent_events
    };
    let event_items: Vec<ListItem> = if events.is_empty() {
        vec![ListItem::new("(no combat yet)")]
    } else {
        events
            .iter()
            .rev()
            .map(|e| {
                let style = if e.contains("HIT") {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                ListItem::new(Span::styled(e.as_str(), style))
            })
            .collect()
    };
    let mut items = event_items;
    items.extend(app.log.iter().rev().map(|l| {
        ListItem::new(Span::styled(
            l.as_str(),
            Style::default().fg(Color::DarkGray),
        ))
    }));
    let visible = area.height.saturating_sub(2) as usize;
    if items.len() > visible {
        items.truncate(visible);
    }
    f.render_widget(List::new(items).block(event_block), area);
}

/// Hex distance (axial coordinates).
fn hex_dist(q1: i32, r1: i32, q2: i32, r2: i32) -> u32 {
    let dq = (q1 - q2).abs();
    let dr = (r1 - r2).abs();
    let ds = (q1 + r1 - q2 - r2).abs();
    ((dq + dr + ds) / 2) as u32
}

/// Render the tutorial narration panel (why this step exists).
///
/// Longer coach text (systems + intent). Yellow bars in header/input hold
/// the short why + key line.
fn render_tutorial_panel(f: &mut Frame, app: &App, area: Rect) {
    let narration = match &app.tutorial {
        Some(t) => t.narration(),
        None => return,
    };

    let title = match &app.tutorial {
        Some(t) if !t.is_complete() => {
            let location = app
                .snap
                .as_ref()
                .map(|s| format!("Turn {} · {}", s.turn, s.phase))
                .unwrap_or_else(|| "Starting".to_string());
            format!(
                "Coach · {location} · step {}/{}",
                t.current + 1,
                t.steps.len()
            )
        }
        Some(_) => "Tutorial complete".to_string(),
        None => "Tutorial".to_string(),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Yellow));

    let p = Paragraph::new(narration)
        .block(block)
        .wrap(Wrap { trim: true });
    f.render_widget(p, area);
}
