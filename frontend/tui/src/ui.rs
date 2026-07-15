//! UI rendering — all ratatui widget construction.
//!
//! Small-tier grid (tutorial adds a coach column):
//!   header (status + yellow why·keys + soft errors)
//!   map | ship status
//!   phase form | events / coach
//! Confirmations paint a Clear + centered modal on top.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Mode};
use crate::protocol::{callsign, facing_arrow, shield_label, Ship, Snapshot};

/// Render the full frame.
pub fn render(f: &mut Frame, app: &mut App) {
    let size = f.area();

    const MIN_WIDTH: u16 = 80;
    const MIN_HEIGHT: u16 = 24;
    if size.width < MIN_WIDTH || size.height < MIN_HEIGHT {
        app.terminal_too_small = true;
        let msg = format!(
            "Terminal too small to play\n\nResize to at least {MIN_WIDTH}×{MIN_HEIGHT}\n\nYour game is paused here and will resume when the window grows."
        );
        let p = Paragraph::new(msg)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("shipsim TUI"));
        f.render_widget(p, size);
        return;
    }
    app.terminal_too_small = false;

    if app.snap.is_none() {
        let msg = if app.engine_dead {
            "Engine exited. Press q to quit."
        } else {
            "Loading…"
        };
        let p = Paragraph::new(msg).alignment(Alignment::Center);
        f.render_widget(p, size);
        return;
    }

    // Keep tutorial allocate cursor on the step's field every frame.
    app.sync_tutorial_allocate_cursor();

    let snap = app.snap.as_ref().unwrap();
    let header_lines = header_line_count(app);
    let header_h = (header_lines + 2).min(6); // +2 for block borders

    if app.tutorial.is_some() {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(header_h),
                Constraint::Min(10),
                Constraint::Min(8),
            ])
            .split(size);
        render_header(f, app, snap, chunks[0]);
        render_middle(f, app, snap, chunks[1]);
        let bottom = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(chunks[2]);
        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(bottom[1]);
        let status_str = snap.status.clone();
        render_input_panel(f, app, &status_str, snap.is_over(), bottom[0]);
        render_events_log(f, app, right[0]);
        render_tutorial_panel(f, app, right[1]);
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(header_h),
                Constraint::Min(10),
                Constraint::Length(10),
                Constraint::Length(6),
            ])
            .split(size);
        render_header(f, app, snap, chunks[0]);
        render_middle(f, app, snap, chunks[1]);
        let status_str = snap.status.clone();
        render_input_panel(f, app, &status_str, snap.is_over(), chunks[2]);
        render_events_log(f, app, chunks[3]);
    }

    if app.confirmation.is_some() {
        render_confirm_modal(f, app, size);
    }
}

fn header_line_count(app: &App) -> u16 {
    let mut n = 1u16; // status
    if app.last_error.is_some() {
        n += 1;
    }
    n
}

fn tutorial_prompt(app: &App) -> Option<String> {
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

fn phase_label(phase: &str, movement_phase: u32) -> String {
    match phase {
        "allocate" => "Allocate".into(),
        "movement" => format!("Move {movement_phase}/4"),
        "firing" => format!("Fire {movement_phase}/4"),
        "turn_end" => "Turn end".into(),
        other => other.to_string(),
    }
}

fn render_header(f: &mut Frame, app: &App, snap: &Snapshot, area: Rect) {
    let status_color = if snap.is_over() {
        Color::Yellow
    } else {
        Color::Green
    };
    let queued = snap.fire_commits.len();
    let phase = phase_label(&snap.phase, snap.movement_phase);

    let mut status_spans = vec![
        Span::styled(
            format!(" turn {} ", snap.turn),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("│"),
        Span::styled(format!(" {phase} "), Style::default().fg(Color::Cyan)),
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
    ];
    if snap.end_turn_warning {
        status_spans.push(Span::styled(
            "│ actions remain ",
            Style::default().fg(Color::Red),
        ));
    }
    if let Some(t) = &app.tutorial {
        if !t.is_complete() {
            status_spans.push(Span::styled(
                format!("│ TUTORIAL {}/{} ", t.current + 1, t.steps.len()),
                Style::default().fg(Color::Yellow),
            ));
        } else {
            status_spans.push(Span::raw("│ TUTORIAL "));
        }
    }

    let mut lines = vec![Line::from(status_spans)];

    if let Some(error) = &app.last_error {
        lines.push(Line::from(Span::styled(
            format!(" ENGINE: {error}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
    }

    // Tutorial prompts live in the coach panel (bottom-right), not here —
    // a second yellow strip in the header duplicated the coach.

    let block = Block::default().borders(Borders::ALL).title("shipsim TUI");
    let p = Paragraph::new(lines).block(block);
    f.render_widget(p, area);
}

fn render_confirm_modal(f: &mut Frame, app: &App, area: Rect) {
    let Some(confirmation) = app.confirmation else {
        return;
    };
    let (title, body) = match confirmation {
        crate::app::Confirmation::Quit => (
            "Confirm quit",
            "Leave this game?\n\ny = quit · n / Esc = cancel",
        ),
        crate::app::Confirmation::EndTurn => (
            "Confirm end turn",
            "End the whole turn?\nQueued fire may be discarded.\n\ny = end turn · n / Esc = cancel",
        ),
    };
    let width = 48u16.min(area.width.saturating_sub(4));
    let height = 8u16.min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let rect = Rect {
        x,
        y,
        width,
        height,
    };
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    let p = Paragraph::new(body)
        .alignment(Alignment::Center)
        .block(block)
        .wrap(Wrap { trim: true });
    f.render_widget(p, rect);
}

fn render_middle(f: &mut Frame, app: &App, snap: &Snapshot, area: Rect) {
    let mid = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    render_map(f, app, snap, mid[0]);
    render_ship_status(f, app, snap, mid[1]);
}

// ── Hex map (character grid) ─────────────────────────────────────────────
// Axial (q →, r ↓). Engine range = hex distance; arc = mount relative facings
// (display mirror of src/arc.rs / REPL hexutil — not rules authority).

const HEX_DIRS: [(i32, i32); 6] = [
    (1, 0),
    (1, -1),
    (0, -1),
    (-1, 0),
    (-1, 1),
    (0, 1),
];

fn ship_fg(ship: &Ship, focused: bool) -> Color {
    if ship.destroyed {
        return Color::DarkGray;
    }
    if focused {
        return Color::Yellow;
    }
    match ship.controller.as_str() {
        "player" => Color::Green,
        "ai" => Color::Red,
        _ => Color::Cyan,
    }
}

/// Absolute facings of neighbors of `from` that step closer to `to` (engine nearest_bearings).
fn nearest_bearings(from_q: i32, from_r: i32, to_q: i32, to_r: i32) -> Vec<u8> {
    if from_q == to_q && from_r == to_r {
        return vec![0];
    }
    let mut best = u32::MAX;
    let mut facings = Vec::new();
    for (facing, (dq, dr)) in HEX_DIRS.iter().enumerate() {
        let d = hex_dist(from_q + dq, from_r + dr, to_q, to_r);
        if d < best {
            best = d;
            facings.clear();
            facings.push(facing as u8);
        } else if d == best {
            facings.push(facing as u8);
        }
    }
    facings
}

fn relative_bearing(
    origin_facing: u32,
    from_q: i32,
    from_r: i32,
    to_q: i32,
    to_r: i32,
) -> u8 {
    let abs = nearest_bearings(from_q, from_r, to_q, to_r)
        .into_iter()
        .next()
        .unwrap_or(0);
    (abs + 6 - (origin_facing % 6) as u8) % 6
}

/// Mount-relative faces that can bear (mirrors `src/arc.rs` Mount).
fn mount_relative_facings(mount: &str, arc: &str) -> &'static [u8] {
    match mount.to_ascii_lowercase().as_str() {
        "forward" => &[0],
        "forward_starboard" => &[5, 0],
        "aft_starboard" => &[3, 4],
        "aft" => &[3],
        "aft_port" => &[2, 3],
        "forward_port" => &[0, 1],
        _ => match arc.to_ascii_lowercase().as_str() {
            "forward" => &[0, 5, 1],
            "rear" | "aft" => &[2, 3, 4],
            "left" | "port" => &[4, 5],
            "right" | "starboard" => &[1, 2],
            "all" => &[0, 1, 2, 3, 4, 5],
            _ => &[0], // conservative default: nose only
        },
    }
}

/// Display geometry only — mirrors engine arc+range for map shading.
///
/// INVARIANT GUARD: this function and its helpers (`mount_relative_facings`,
/// `relative_bearing`, `nearest_bearings`, `hex_dist`) re-derive engine
/// geometry purely to color map hexes. They MUST NOT be called from any
/// order-building path (input.rs / app.rs) — the engine is the sole authority
/// for hit/miss, damage, range, and arc legality. If you need to gate a fire
/// order, ask the engine; do not call this. The `#[cfg(...)]`-free `pub(crate)`
/// visibility is retained only because `tests.rs` exercises the shader; keep
/// all call sites inside `ui.rs` (rendering) or `tests.rs`.
pub(crate) fn weapon_covers_hex(
    oq: i32,
    or_: i32,
    facing: u32,
    max_range: u32,
    rel_facings: &[u8],
    tq: i32,
    tr: i32,
) -> bool {
    if oq == tq && or_ == tr {
        return false;
    }
    let d = hex_dist(oq, or_, tq, tr);
    if d == 0 || d > max_range {
        return false;
    }
    let rel = relative_bearing(facing, oq, or_, tq, tr);
    rel_facings.contains(&rel)
}

/// Shade for selected weapon: hexes in **range and arc**.
struct WeaponShade {
    oq: i32,
    or_: i32,
    facing: u32,
    max_range: u32,
    rel_facings: &'static [u8],
    bg: Color,
    mount_label: String,
}

impl WeaponShade {
    fn covers(&self, q: i32, r: i32) -> bool {
        weapon_covers_hex(
            self.oq,
            self.or_,
            self.facing,
            self.max_range,
            self.rel_facings,
            q,
            r,
        )
    }
}

fn selected_weapon_shade(app: &App) -> Option<WeaponShade> {
    if app.mode != Mode::Fire {
        return None;
    }
    let ship = app.focused()?;
    let draft = app.fire_draft.as_ref()?;
    let w = ship.weapons.get(draft.weapon_idx)?;
    if !w.operational || w.max_range == 0 {
        return None;
    }
    let bg = if w.charge == 0 || w.fired {
        Color::Rgb(40, 40, 40)
    } else if ship.controller == "player" {
        Color::Rgb(0, 60, 0)
    } else {
        Color::Rgb(60, 0, 0)
    };
    let mount = if w.mount.is_empty() {
        w.arc.as_str()
    } else {
        w.mount.as_str()
    };
    Some(WeaponShade {
        oq: ship.q,
        or_: ship.r,
        facing: ship.facing,
        max_range: w.max_range,
        rel_facings: mount_relative_facings(&w.mount, &w.arc),
        bg,
        mount_label: mount.to_string(),
    })
}

fn render_map(f: &mut Frame, app: &App, snap: &Snapshot, area: Rect) {
    let w = snap.map.width as usize;
    let h = snap.map.height as usize;
    let shade = selected_weapon_shade(app);

    // Viewport origin (top-left visible hex). Auto-centers on the focused ship
    // unless the player has panned manually. In the unbounded world this keeps
    // ships that drift to negative coordinates on-screen.
    let (oq, or_) = app.map_origin();

    let title = if let Some(ref s) = shade {
        format!(
            "Map @({},{}) · shade = {} arc + range ≤{} · green=you red=ai",
            oq, or_, s.mount_label, s.max_range
        )
    } else {
        format!("Map @({},{}) · green=you red=ai · arrow=facing", oq, or_)
    };

    let mut lines: Vec<Line> = Vec::new();

    // Column header (q)
    let mut hdr = vec![Span::styled("   ", Style::default().fg(Color::DarkGray))];
    for q in 0..w {
        let wq = oq + q as i32;
        hdr.push(Span::styled(
            format!("{:<4}", wq.rem_euclid(10)),
            Style::default().fg(Color::DarkGray),
        ));
    }
    lines.push(Line::from(hdr));

    for r in 0..h {
        let wr = or_ + r as i32;
        let mut spans: Vec<Span> = vec![Span::styled(
            format!("{:2} ", wr),
            Style::default().fg(Color::DarkGray),
        )];
        for q in 0..w {
            let wq = oq + q as i32;
            let wr = or_ + r as i32;
            let in_arc_range = shade
                .as_ref()
                .map(|s| s.covers(wq, wr))
                .unwrap_or(false);

            let ship = snap
                .ships
                .iter()
                .find(|s| s.q == wq && s.r == wr);

            let (text, fg) = if let Some(s) = ship {
                let cs = callsign(s);
                let focused = app.focused_ship == Some(s.id);
                let short_cs: String = cs.chars().take(2).collect();
                let cell = if s.destroyed {
                    format!("x{}  ", short_cs.chars().next().unwrap_or('?'))
                } else {
                    let arrow = facing_arrow(s.facing);
                    if s.velocity > 0 && s.course != s.facing {
                        format!(
                            "{}{}{}",
                            short_cs,
                            arrow,
                            facing_arrow(s.course)
                        )
                    } else {
                        format!("{}{} ", short_cs, arrow)
                    }
                };
                (cell, ship_fg(s, focused))
            } else if in_arc_range {
                ("··  ".to_string(), Color::DarkGray)
            } else {
                (".   ".to_string(), Color::DarkGray)
            };

            let mut style = Style::default().fg(fg);
            if in_arc_range {
                if let Some(ref s) = shade {
                    style = style.bg(s.bg);
                }
            }
            if ship.is_some() && app.focused_ship == ship.map(|s| s.id) {
                style = style.add_modifier(Modifier::BOLD);
            }
            spans.push(Span::styled(text, style));
        }
        lines.push(Line::from(spans));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "A# you  B# ai  arrow=facing  shade=selected weapon (range ∩ arc)",
        Style::default().fg(Color::DarkGray),
    )));

    let p = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(p, area);
}

fn render_ship_status(f: &mut Frame, app: &App, snap: &Snapshot, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title("Ship Status");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut y = inner.y;
    let max_y = inner.y.saturating_add(inner.height);

    let push = |f: &mut Frame, y: &mut u16, line: Line<'static>| {
        if *y >= max_y {
            return;
        }
        f.render_widget(
            Paragraph::new(line),
            Rect {
                x: inner.x,
                y: *y,
                width: inner.width,
                height: 1,
            },
        );
        *y = y.saturating_add(1);
    };

    if let Some(ship) = app.focused() {
        let cs = callsign(ship);
        push(
            f,
            &mut y,
            Line::from(vec![
                Span::styled(
                    format!(" {cs} "),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("#{} {} size={}", ship.id, ship.class, ship.size)),
            ]),
        );
        push(
            f,
            &mut y,
            Line::from(format!(
                "  @({},{}) face={}{} course={}{} vel={} thrust={}",
                ship.q,
                ship.r,
                ship.facing,
                facing_arrow(ship.facing),
                ship.course,
                facing_arrow(ship.course),
                ship.velocity,
                ship.thrust_remaining
            )),
        );
        push(
            f,
            &mut y,
            Line::from(format!("  hull {}  (current structure boxes)", ship.structure)),
        );

        push(
            f,
            &mut y,
            Line::from(format!(
                "  power avail {} · engine boxes {} · sys {}",
                ship.power_available, ship.engine, ship.power_sys
            )),
        );

        let shield_str: Vec<String> = (0..6)
            .map(|i| {
                let powered = ship.shields_powered.get(i).copied().unwrap_or(0);
                let remaining = ship.shields_remaining.get(i).copied().unwrap_or(0);
                format!("{}:{}/{}", shield_label(i as u32), remaining, powered)
            })
            .collect();
        push(
            f,
            &mut y,
            Line::from(format!("  shields {}", shield_str[..3].join(" "))),
        );
        push(
            f,
            &mut y,
            Line::from(format!("          {}", shield_str[3..].join(" "))),
        );

        push(f, &mut y, Line::from("  weapons:"));
        for w in &ship.weapons {
            let op = if w.operational { "" } else { " [DAMAGED]" };
            let fired = if w.fired { " [fired]" } else { "" };
            push(
                f,
                &mut y,
                Line::from(format!(
                    "    {} {} rng≤{} chg={}/{}{}{}",
                    w.id, w.kind, w.max_range, w.charge, w.max_charge, fired, op
                )),
            );
        }
    } else {
        push(f, &mut y, Line::from(" (no ship focused)"));
    }

    push(f, &mut y, Line::from(""));
    push(
        f,
        &mut y,
        Line::from(Span::styled(
            " Contacts:",
            Style::default().add_modifier(Modifier::BOLD),
        )),
    );
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
        push(
            f,
            &mut y,
            Line::from(format!(
                "  {} #{} {} @({},{}) rng={} face={}{}",
                cs,
                s.id,
                s.class,
                s.q,
                s.r,
                dist,
                s.facing,
                facing_arrow(s.facing)
            )),
        );
    }
}

fn render_input_panel(f: &mut Frame, app: &mut App, status: &str, _is_over: bool, area: Rect) {
    let (title, lines) = match &app.mode {
        Mode::Normal => (
            "Help",
            vec![
                Line::from(" q: quit  Tab: cycle focus  Enter: act in phase"),
                Line::from(" a: allocate  m: move  f: fire  e: end turn"),
                Line::from(" v: map-focus (WASD/hjkl pan, c recenter)"),
                Line::from(""),
            ],
        ),
        Mode::Map => {
            let (oq, or_) = app.map_origin();
            let auto = if app.map_pan.is_none() { " (auto-center)" } else { "" };
            (
                "Map Focus",
                vec![
                    Line::from(" WASD / hjkl / arrows: pan  c: recenter"),
                    Line::from(" v / Esc / Enter: back to Normal"),
                    Line::from(""),
                    Line::from(Span::styled(
                        format!(" origin=({},{}){}", oq, or_, auto),
                        Style::default().fg(Color::DarkGray),
                    )),
                    Line::from(""),
                ],
            )
        }
        Mode::Allocate => render_allocate_panel(app),
        Mode::Movement => render_movement_panel(app),
        Mode::Fire => render_fire_panel(app),
        Mode::GameOver => (
            "Game Over",
            vec![
                Line::from(Span::styled(
                    format!(" Game status: {status}"),
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
    let inner = block.inner(area);
    f.render_widget(block, area);

    let tutorial_error = app
        .tutorial
        .as_ref()
        .and_then(|tutorial| tutorial.error_msg.as_deref());
    let content_area = if let Some(error) = tutorial_error {
        let alert_height = inner.height.min(1);
        let alert = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: alert_height,
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" TUTORIAL: {error}"),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ))),
            alert,
        );
        Rect {
            x: inner.x,
            y: inner.y.saturating_add(alert_height),
            width: inner.width,
            height: inner.height.saturating_sub(alert_height),
        }
    } else {
        inner
    };

    // Power gauge on allocate (top of form).
    let mut body_area = content_area;
    if matches!(app.mode, Mode::Allocate) {
        if let (Some(draft), Some(ship)) = (&app.alloc_draft, app.focused()) {
            if body_area.height > 2 {
                let cost = draft.power_cost(ship);
                let pool = ship.power_available.max(1);
                let ratio = (cost as f64 / pool as f64).clamp(0.0, 1.0);
                let over = cost > ship.power_available;
                let g = Gauge::default()
                    .gauge_style(if over {
                        Style::default().fg(Color::Red).bg(Color::DarkGray)
                    } else {
                        Style::default().fg(Color::Cyan).bg(Color::DarkGray)
                    })
                    .ratio(ratio)
                    .label(format!(
                        "power {cost}/{} {}",
                        ship.power_available,
                        if over { "OVER" } else { "ok" }
                    ));
                f.render_widget(
                    g,
                    Rect {
                        x: body_area.x,
                        y: body_area.y,
                        width: body_area.width,
                        height: 1,
                    },
                );
                body_area.y = body_area.y.saturating_add(1);
                body_area.height = body_area.height.saturating_sub(1);
            }
        }
    }

    let scroll = match app.mode {
        Mode::Allocate => allocate_scroll(app, body_area),
        Mode::Fire => fire_scroll(app, body_area),
        _ => 0,
    };
    let wrap = if matches!(app.mode, Mode::Allocate) {
        Wrap { trim: false }
    } else {
        Wrap { trim: true }
    };
    let p = Paragraph::new(lines).wrap(wrap).scroll((scroll, 0));
    f.render_widget(p, body_area);
}

fn allocate_scroll(app: &App, area: Rect) -> u16 {
    let Some(draft) = &app.alloc_draft else {
        return 0;
    };
    // Lines: budget(0), movement(1), weapons header(2), weapons…, shields header, faces, diagram, footer
    let n_weapons = draft.weapons.len();
    let line: u16 = match draft.cursor {
        0 => 1,
        n if n <= n_weapons => (3 + (n - 1)) as u16,
        n => {
            let face = n - 1 - n_weapons;
            (4 + n_weapons + face) as u16
        }
    };
    let visible = area.height.saturating_sub(2).max(1);
    line.saturating_sub(visible.saturating_sub(1))
}

fn fire_scroll(app: &App, area: Rect) -> u16 {
    let Some(snap) = &app.snap else {
        return 0;
    };
    let Some(draft) = &app.fire_draft else {
        return 0;
    };
    let enemy_count = snap
        .ships
        .iter()
        .filter(|ship| ship.controller != "player" && !ship.destroyed)
        .count();
    // Controls, queue, targets header/list, then weapons header/list.
    let selected_line = 7 + enemy_count + draft.weapon_idx;
    let visible = area.height.max(1) as usize;
    selected_line
        .saturating_sub(visible.saturating_sub(1))
        .min(u16::MAX as usize) as u16
}

fn wrapped_line_count(text: &str, width: u16) -> u16 {
    let width = width.max(1) as usize;
    let mut total = 0usize;
    for source_line in text.lines() {
        let mut used = 0usize;
        let mut lines = 1usize;
        for word in source_line.split_whitespace() {
            let word_width = Line::from(word).width().max(1);
            if used == 0 {
                lines += word_width.saturating_sub(1) / width;
                used = word_width % width;
                if used == 0 {
                    used = width;
                }
            } else if used + 1 + word_width <= width {
                used += 1 + word_width;
            } else {
                lines += 1 + word_width.saturating_sub(1) / width;
                used = word_width % width;
                if used == 0 {
                    used = width;
                }
            }
        }
        total += lines;
    }
    total.max(1).min(u16::MAX as usize) as u16
}

fn selected_style() -> Style {
    Style::default()
        .bg(Color::Yellow)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD)
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

    let mut lines = vec![Line::from(vec![
        Span::raw(format!(" Budget {cost}/{pool} (")),
        Span::styled(
            if balance < 0 {
                format!("{} over", -balance)
            } else {
                format!("{balance} free")
            },
            budget_style,
        ),
        Span::raw(") · engine power → thrust this turn only"),
    ])];

    // Movement row
    let mov_selected = draft.cursor == 0;
    lines.push(Line::from(Span::styled(
        format!(
            "{}Engine (Movement): {:2}   ←/→ or digits · Backspace clear",
            if mov_selected { "▶ " } else { "  " },
            draft.movement
        ),
        if mov_selected {
            selected_style()
        } else {
            Style::default()
        },
    )));

    lines.push(Line::from(Span::styled(
        " Weapons (ship order — same as fire list):",
        Style::default().fg(Color::DarkGray),
    )));
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
        let selected = draft.cursor == i + 1;
        let mark = if selected { "▶ " } else { "  " };
        let carried = if cur > 0 && *chg >= cur {
            format!(" · carried {cur}")
        } else {
            String::new()
        };
        let kind = ship
            .weapons
            .iter()
            .find(|w| &w.id == id)
            .map(|w| w.kind.as_str())
            .unwrap_or("?");
        lines.push(Line::from(Span::styled(
            format!("{mark}{id} ({kind}) charge {chg}/{max}{carried}"),
            if selected {
                selected_style()
            } else {
                Style::default()
            },
        )));
    }

    lines.push(Line::from(Span::styled(
        " Shields (ship-relative faces — rebuild every allocate):",
        Style::default().fg(Color::DarkGray),
    )));
    let n_w = draft.weapons.len();
    for i in 0..6 {
        let selected = draft.cursor == 1 + n_w + i;
        let mark = if selected { "▶ " } else { "  " };
        let name = match i {
            0 => "F  forward (nose)",
            1 => "FR forward-right",
            2 => "RR rear-right",
            3 => "R  rear (stern)",
            4 => "RL rear-left",
            5 => "FL forward-left",
            _ => "?",
        };
        lines.push(Line::from(Span::styled(
            format!(
                "{mark}{name}: {:2}",
                draft.shields.get(i).copied().unwrap_or(0)
            ),
            if selected {
                selected_style()
            } else {
                Style::default()
            },
        )));
    }

    // Mini face diagram
    let sel_face = if draft.cursor > n_w {
        Some(draft.cursor - 1 - n_w)
    } else {
        None
    };
    let face_cell = |i: usize| -> String {
        let v = draft.shields.get(i).copied().unwrap_or(0);
        let lab = shield_label(i as u32);
        if sel_face == Some(i) {
            format!("[{lab}{v}]")
        } else {
            format!(" {lab}{v} ")
        }
    };
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " Face map (▶ / [] = selected):",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(format!(
        "      {} {} {}",
        face_cell(5),
        face_cell(0),
        face_cell(1)
    )));
    lines.push(Line::from(format!(
        "      {}  ·  {}",
        face_cell(4),
        face_cell(2)
    )));
    lines.push(Line::from(format!("         {}", face_cell(3))));

    lines.push(Line::from(""));
    lines.push(Line::from(
        " Enter commit · ↓/j next · ↑/k prev · ←/→ adjust · digits set",
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
            " {} @({},{}) face={}{}  course={}{}  vel={}  thrust={}",
            callsign(ship),
            ship.q,
            ship.r,
            ship.facing,
            facing_arrow(ship.facing),
            ship.course,
            facing_arrow(ship.course),
            ship.velocity,
            ship.thrust_remaining
        )),
        Line::from(Span::styled(
            " Accel spends thrust along facing (cost often 1; reverse/revector vary).",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(" c: coast (slide only, free)"),
        Line::from(" t: accel along facing"),
        Line::from(" 0–5: turn to that facing (facing only — course stays)"),
        Line::from(" r: turn +1 facing"),
        Line::from(""),
        Line::from(" Space (fire phase): ready · e: end turn"),
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

    let mut lines = vec![
        Line::from(Span::styled(
            " ↑↓ weapon · digits target · Enter queue",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(Span::styled(
            " Space ready · ←→ shield · Esc cancel",
            Style::default().fg(Color::Yellow),
        )),
    ];

    let mine_count = snap
        .fire_commits
        .iter()
        .filter(|c| c.ship == ship.id)
        .count();
    lines.push(Line::from(Span::styled(
        format!(" Queued: {mine_count} shot(s) pending"),
        if mine_count == 0 {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        },
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(" Targets (digit = target):"));
    for (i, s) in snap
        .ships
        .iter()
        .filter(|s| s.controller != "player" && !s.destroyed)
        .enumerate()
    {
        let selected = draft.target == Some(s.id);
        let marker = if selected { "▶" } else { " " };
        let dist = hex_dist(ship.q, ship.r, s.q, s.r);
        let style = if selected {
            selected_style()
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(
            format!(
                " {marker} {} {} d={} face={}{} sz={}",
                i + 1,
                callsign(s),
                dist,
                s.facing,
                facing_arrow(s.facing),
                s.size
            ),
            style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(" Weapons:"));
    for (i, w) in ship.weapons.iter().enumerate() {
        let selected = i == draft.weapon_idx;
        let marker = if selected { "▶" } else { " " };
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
        let style = if selected {
            selected_style()
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(
            format!(
                " {marker} {} {} rng≤{} {}{}",
                w.id, w.kind, w.max_range, charge_str, queued_str
            ),
            style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(format!(
        " Hit face: {} (←/→ F/FR/RR/R/RL/FL)",
        shield_label(draft.shield_facing)
    )));
    lines.push(Line::from(Span::styled(
        " Accuracy: range + target size (no preview).",
        Style::default().fg(Color::DarkGray),
    )));

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
    let mut items = Vec::new();
    if let Some(error) = &app.last_error {
        items.push(ListItem::new(Span::styled(
            format!("ENGINE: {error}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
    }
    items.extend(event_items);
    items.extend(
        app.log
            .iter()
            .rev()
            .filter(|line| !line.starts_with("ERROR:"))
            .map(|l| {
                ListItem::new(Span::styled(
                    l.as_str(),
                    Style::default().fg(Color::DarkGray),
                ))
            }),
    );
    let visible = area.height.saturating_sub(2) as usize;
    if items.len() > visible {
        items.truncate(visible);
    }
    f.render_widget(List::new(items).block(event_block), area);
}

fn hex_dist(q1: i32, r1: i32, q2: i32, r2: i32) -> u32 {
    let dq = (q1 - q2).abs();
    let dr = (r1 - r2).abs();
    let ds = (q1 + r1 - q2 - r2).abs();
    ((dq + dr + ds) / 2) as u32
}

fn render_tutorial_panel(f: &mut Frame, app: &App, area: Rect) {
    let Some(t) = &app.tutorial else {
        return;
    };

    let title = if !t.is_complete() {
        let location = app
            .snap
            .as_ref()
            .map(|s| format!("Turn {} · {}", s.turn, phase_label(&s.phase, s.movement_phase)))
            .unwrap_or_else(|| "Starting".to_string());
        format!(
            "Coach · {location} · {}/{}",
            t.current + 1,
            t.steps.len()
        )
    } else {
        "Tutorial complete".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Prompt first (single yellow instruction strip), then longer coach text.
    let prompt = tutorial_prompt(app).unwrap_or_default();
    let mut y = inner.y;
    if !prompt.is_empty() && inner.height > 0 {
        let prompt_widget = Paragraph::new(Line::from(Span::styled(
            format!(" {prompt}"),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )))
        .wrap(Wrap { trim: true });
        let prompt_h = wrapped_line_count(&prompt, inner.width).min(inner.height);
        f.render_widget(
            prompt_widget,
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: prompt_h,
            },
        );
        y = y.saturating_add(prompt_h);
    }

    let body_h = inner.height.saturating_sub(y.saturating_sub(inner.y));
    if body_h == 0 {
        return;
    }
    let narration = t.narration();
    f.render_widget(
        Paragraph::new(narration).wrap(Wrap { trim: true }),
        Rect {
            x: inner.x,
            y,
            width: inner.width,
            height: body_h,
        },
    );
}
