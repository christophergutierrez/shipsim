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

    // Vertical layout: header, middle (map+status), input, events/log,
    // and optionally tutorial coach text. Header grows a line for DO NOW.
    let chunks = if app.tutorial.is_some() {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),  // header + DO NOW line
                Constraint::Min(6),     // map + ship status
                Constraint::Length(10), // input panel (allocate + DO NOW bar)
                Constraint::Length(3),  // events/log
                Constraint::Min(6),     // tutorial coach
            ])
            .split(size)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // header
                Constraint::Min(10),   // map + ship status
                Constraint::Length(8),  // input panel
                Constraint::Length(6),  // events/log
            ])
            .split(size)
    };

    render_header(f, app, snap, chunks[0]);
    render_middle(f, app, snap, chunks[1]);
    // Clone the status string so we don't hold an immutable borrow of app
    // while render_input_panel needs &mut app.
    let status_str = snap.status.clone();
    let is_over = snap.is_over();
    render_input_panel(f, app, &status_str, is_over, chunks[2]);
    render_events_log(f, app, chunks[3]);
    if app.tutorial.is_some() {
        render_tutorial_panel(f, app, chunks[4]);
    }
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
        Span::styled(format!(" {} ", snap.status), Style::default().fg(status_color)),
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
            let ship = snap.ships.iter().find(|s| s.q as usize == q && s.r as usize == r);
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
                    Span::styled(cell, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
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
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
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
        lines.push(Line::from(format!("  shields: {}", shield_str.join(" "))));

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

    let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

fn render_input_panel(f: &mut Frame, app: &mut App, status: &str, _is_over: bool, area: Rect) {
    let (title, mut lines) = match &app.mode {
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
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                )),
                Line::from(" q: quit"),
            ],
        ),
    };

    // Tutorial: put DO NOW at the top of the panel you're actually using.
    if let Some(do_now) = tutorial_do_now(app) {
        lines.insert(
            0,
            Line::from(Span::styled(
                format!(" {do_now}"),
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
        );
        if let Some(err) = app.tutorial.as_ref().and_then(|t| t.error_msg.as_ref()) {
            lines.insert(
                1,
                Line::from(Span::styled(
                    format!(" ⚠ {err}"),
                    Style::default().fg(Color::Red),
                )),
            );
        }
    }

    let title = if app.tutorial.is_some() {
        format!("{title} · yellow = why + keys")
    } else {
        title.to_string()
    };

    let block = Block::default().borders(Borders::ALL).title(title);
    let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    f.render_widget(p, area);
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
    let free = pool.saturating_sub(cost);

    let mut lines = vec![
        Line::from(format!(
            " Power: {cost} cost / {pool} pool  ({free} free)  · ▶ = selected field"
        )),
        Line::from(format!(
            "{}Movement: {}   ← lower  → raise  (digits set value)",
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
    lines.push(Line::from(
        " Enter: commit  Esc: cancel  ↓/j: next field  ←/→: adjust",
    ));

    ("Allocate", lines)
}

fn render_movement_panel(app: &App) -> (&'static str, Vec<Line<'static>>) {
    let ship = match app.focused() {
        Some(s) => s,
        None => return ("Movement", vec![Line::from(" (no ship focused)")]),
    };

    let lines = vec![
        Line::from(format!(
            " {} @({},{}) face={}{} vel={} course={}{} thrust={}",
            callsign(ship),
            ship.q,
            ship.r,
            ship.facing,
            facing_arrow(ship.facing),
            ship.velocity,
            ship.course,
            facing_arrow(ship.course),
            ship.thrust_remaining
        )),
        Line::from(""),
        Line::from(" c: coast (slide only)"),
        Line::from(" t: accel (along facing — thrust +1/−1/revector)"),
        Line::from(" 0-5: turn to absolute facing"),
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
        let charge_str = if w.charge > 0 {
            format!("chg={}", w.charge)
        } else {
            "UNCHARGED".to_string()
        };
        lines.push(Line::from(format!(
            " {} {} {} rng≤{} {}",
            marker, w.id, w.kind, w.max_range, charge_str
        )));
    }

    // Targets
    lines.push(Line::from(""));
    lines.push(Line::from(" Targets:"));
    for s in &snap.ships {
        if s.id == ship.id || s.destroyed {
            continue;
        }
        let marker = if draft.target == Some(s.id) { "▶" } else { " " };
        let dist = hex_dist(ship.q, ship.r, s.q, s.r);
        lines.push(Line::from(format!(
            " {} {} #{} {} rng={}",
            marker,
            callsign(s),
            s.id,
            s.class,
            dist
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(format!(
        " Shield facing: {}  (←/→ cycle)",
        shield_label(draft.shield_facing)
    )));
    // Placeholder for to-hit odds (engine change lands after Small tier).
    lines.push(Line::from(Span::styled(
        " To-hit odds: (available after engine update)",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(" Enter: fire  Tab: next weapon/target  Esc: cancel"));

    ("Fire", lines)
}

fn render_events_log(f: &mut Frame, app: &App, area: Rect) {
    let mid = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Events panel
    let event_block = Block::default().borders(Borders::ALL).title("Recent Events");
    let event_items: Vec<ListItem> = if app.recent_events.is_empty() {
        vec![ListItem::new("(no combat this turn)")]
    } else {
        app.recent_events
            .iter()
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
    let event_list = List::new(event_items).block(event_block);
    f.render_widget(event_list, mid[0]);

    // Log panel
    let log_block = Block::default().borders(Borders::ALL).title("Log");
    let log_items: Vec<ListItem> = app
        .log
        .iter()
        .rev()
        .take(area.height.saturating_sub(2) as usize)
        .map(|l| ListItem::new(l.as_str()))
        .collect();
    let log_list = List::new(log_items).block(log_block);
    f.render_widget(log_list, mid[1]);
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
            format!(
                "Coach ({}/{}) — yellow = why · keys",
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
