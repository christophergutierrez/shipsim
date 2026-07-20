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
                Constraint::Fill(5),
                Constraint::Fill(7),
            ])
            .split(size);
        render_header(f, app, snap, chunks[0]);
        render_middle(f, app, snap, chunks[1]);
        let bottom = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(chunks[2]);
        let event_h = if bottom[1].height >= 16 { 6 } else { 3 };
        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(event_h), Constraint::Min(6)])
            .split(bottom[1]);
        let status_str = snap.status.clone();
        render_input_panel(f, app, &status_str, snap.is_over(), bottom[0]);
        render_events_log(f, app, right[0]);
        render_tutorial_panel(f, app, right[1]);
    } else {
        // Header is Length so ENGINE soft-errors never get crushed when the
        // form/map compete for space. Map and form share the middle via Fill.
        // Combat log grows modestly with terminal height so multi-ship volleys
        // stay readable without crushing map/form at typical 80×24 / 100×30.
        // At the 80×24 floor, six rows plus two-column volley packing keeps
        // a full fleet exchange visible while leaving useful map/form space.
        let combat_h = if app.mode == Mode::Map {
            4
        } else {
            match size.height {
                0..=31 => 6,
                32..=39 => 8,
                _ => 10,
            }
        };
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(header_h),
                Constraint::Fill(5), // map
                Constraint::Fill(6), // phase form (allocate / fire / movement)
                Constraint::Length(combat_h),
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

fn phase_label(phase: &str) -> String {
    match phase {
        "allocate" => "Allocate".into(),
        "movement" => "Movement".into(),
        "firing" => "Firing".into(),
        "turn_end" => "Turn end".into(),
        other => other.to_string(),
    }
}

/// Living player-ship count and how many have completed the current stage,
/// used for the header "path N/M" style progress readout.
fn stage_progress(snap: &Snapshot) -> Option<(&'static str, usize, usize)> {
    let living = snap
        .ships
        .iter()
        .filter(|s| s.controller == "player" && !s.destroyed)
        .count();
    if living == 0 {
        return None;
    }
    let (label, done) = match snap.phase.as_str() {
        "allocate" => ("alloc", snap.ships_allocated_this_turn.len()),
        "movement" => ("path", snap.ships_committed_path.len()),
        "firing" => ("volley", snap.ships_committed_volley.len()),
        _ => return None,
    };
    Some((label, done, living))
}

fn render_header(f: &mut Frame, app: &App, snap: &Snapshot, area: Rect) {
    let status_color = if snap.is_over() {
        Color::Yellow
    } else {
        Color::Green
    };
    // v4: header `queued=N` counts the *focused* ship's drafted volley shots
    // (not yet committed) so the player always knows what their ship will fire
    // on Space. The draft lives entirely client-side now (app.fire_draft).
    let queued = if snap.phase == "firing" {
        app.fire_draft.as_ref().map(|d| d.shots.len()).unwrap_or(0)
    } else {
        0
    };
    let phase = phase_label(&snap.phase);

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
    ];
    // v4 stage progress: how many living player ships have committed this stage.
    if let Some((label, done, total)) = stage_progress(snap) {
        status_spans.push(Span::raw("│"));
        status_spans.push(Span::styled(
            format!(" {label} {done}/{total} "),
            Style::default().fg(Color::DarkGray),
        ));
    }
    if snap.phase == "firing" {
        status_spans.push(Span::raw("│"));
        status_spans.push(Span::styled(
            format!(" queued={queued} "),
            if queued > 0 {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ));
    }
    // Fable Phase 4: concrete next-action guidance (replaces "actions remain").
    let cta = if app.tutorial.is_none() {
        phase_call_to_action(app, snap)
    } else {
        String::new()
    };
    if !cta.is_empty() {
        status_spans.push(Span::styled(
            format!("│ {cta} "),
            Style::default().fg(Color::Yellow),
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
            "Leave this game?\n\ny = quit · n / Esc = cancel".to_string(),
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

const HEX_DIRS: [(i32, i32); 6] = [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)];

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

fn relative_bearing(origin_facing: u32, from_q: i32, from_r: i32, to_q: i32, to_r: i32) -> u8 {
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

struct MapMetrics {
    columns: usize,
    rows: usize,
    cell_width: usize,
    scale: i32,
    zoom: i8,
}

fn map_metrics(app: &App, area: Rect, footer_lines: usize) -> MapMetrics {
    let inner_width = area.width.saturating_sub(2) as usize;
    let inner_height = area.height.saturating_sub(2) as usize;
    let base_columns = ((inner_width.saturating_sub(3)) / 6).max(1);
    // One row for coordinate headers plus `footer_lines` for legend/off-map.
    let reserve = 1 + footer_lines.max(2);
    let rows = inner_height.saturating_sub(reserve).max(1);
    let zoom = app.effective_map_zoom(base_columns as i32, rows as i32);
    let cell_width = 6 * (usize::try_from(zoom.max(0)).unwrap_or(0) + 1);
    let columns = ((inner_width.saturating_sub(3)) / cell_width).max(1);
    let scale = 1_i32 << u32::try_from((-zoom).max(0)).unwrap_or(0);
    MapMetrics {
        columns,
        rows,
        cell_width,
        scale,
        zoom,
    }
}

fn in_cell(value: i32, origin: i32, scale: i32, index: usize) -> bool {
    (value - origin).div_euclid(scale) == index as i32
}

fn pad_cell(text: String, width: usize) -> String {
    // Map labels contain multi-byte arrows. String::truncate uses byte offsets,
    // so truncating a divergent facing/course label such as `A1←→3` can land
    // inside an arrow and panic. These glyphs are all single-column, making a
    // character-counted fit both safe and correct for the map cell.
    let mut fitted: String = text.chars().take(width).collect();
    let padding = width.saturating_sub(fitted.chars().count());
    fitted.extend(std::iter::repeat_n(' ', padding));
    fitted
}

fn render_map(f: &mut Frame, app: &App, snap: &Snapshot, area: Rect) {
    // Two-pass metrics: first assume no off-map strip (legend only), then if
    // any living ship falls outside the viewport reserve one more footer line
    // so the off-map strip is not clipped at small terminal sizes.
    let mut metrics = map_metrics(app, area, 2);
    let (oq, or_) =
        app.map_origin_for_view(metrics.columns as i32, metrics.rows as i32, metrics.scale);
    let off_map = off_map_contacts(
        app,
        snap,
        oq,
        or_,
        metrics.columns as i32,
        metrics.rows as i32,
        metrics.scale,
    );
    if !off_map.is_empty() {
        // Reserve footer space, then re-evaluate — a re-fit may pull contacts
        // back on-map; if so, drop the reserved strip line.
        metrics = map_metrics(app, area, 3);
        let (oq2, or2) =
            app.map_origin_for_view(metrics.columns as i32, metrics.rows as i32, metrics.scale);
        let off_map2 = off_map_contacts(
            app,
            snap,
            oq2,
            or2,
            metrics.columns as i32,
            metrics.rows as i32,
            metrics.scale,
        );
        if off_map2.is_empty() {
            metrics = map_metrics(app, area, 2);
        }
    }
    let (oq, or_) =
        app.map_origin_for_view(metrics.columns as i32, metrics.rows as i32, metrics.scale);
    let off_map = off_map_contacts(
        app,
        snap,
        oq,
        or_,
        metrics.columns as i32,
        metrics.rows as i32,
        metrics.scale,
    );
    let shade = selected_weapon_shade(app);

    // v4: trace the focused ship's drafted path from `path_preview`. Each
    // step's hex is a faint route marker; the final hex is highlighted.
    let preview = app.path_preview_for_focus();
    let preview_endpoints: std::collections::HashSet<(i32, i32)> = preview
        .map(|p| p.steps.iter().map(|s| (s.q, s.r)).collect())
        .unwrap_or_default();
    let preview_coast = preview.map(|p| (p.final_q, p.final_r));

    // Title: always keep `→ callsign d=N` when present; drop optional chrome
    // first so range is not clipped off the Block title.
    let title = build_map_title(
        area.width as usize,
        oq,
        or_,
        metrics.zoom,
        metrics.scale,
        &focused_range_to_nearest_enemy(app, snap),
        shade
            .as_ref()
            .map(|s| (s.mount_label.as_str(), s.max_range)),
        !preview_endpoints.is_empty(),
    );

    let mut lines: Vec<Line> = Vec::new();

    // Fable Phase 5: signed q labels at a non-overlapping cadence (not last-digit only).
    let r_labels: Vec<i32> = (0..metrics.rows)
        .map(|r| or_ + r as i32 * metrics.scale)
        .collect();
    let gutter = r_label_gutter_width(&r_labels);
    let mut hdr = vec![Span::styled(
        format!("{:gutter$}", ""),
        Style::default().fg(Color::DarkGray),
    )];
    let label_every = ((4usize).div_ceil(metrics.cell_width.max(1))).max(1);
    for q in 0..metrics.columns {
        let wq = oq + q as i32 * metrics.scale;
        let cell = if q % label_every == 0 {
            format_signed_coord(wq, metrics.cell_width)
        } else {
            " ".repeat(metrics.cell_width)
        };
        hdr.push(Span::styled(cell, Style::default().fg(Color::DarkGray)));
    }
    lines.push(Line::from(hdr));

    for r in 0..metrics.rows {
        let wr = or_ + r as i32 * metrics.scale;
        let mut spans: Vec<Span> = vec![Span::styled(
            format!("{wr:>gutter$} "),
            Style::default().fg(Color::DarkGray),
        )];
        for q in 0..metrics.columns {
            let wq = oq + q as i32 * metrics.scale;
            let wr = or_ + r as i32 * metrics.scale;
            let in_arc_range = shade.as_ref().map(|s| s.covers(wq, wr)).unwrap_or(false);

            let ships_here: Vec<&Ship> = snap
                .ships
                .iter()
                .filter(|s| {
                    in_cell(s.q, oq, metrics.scale, q) && in_cell(s.r, or_, metrics.scale, r)
                })
                .collect();
            // Prefer focused living ship as the primary glyph when several share a cell.
            let ship = ships_here
                .iter()
                .copied()
                .max_by_key(|s| (app.focused_ship == Some(s.id), !s.destroyed));
            let multipin = ships_here.len() > 1;

            let is_preview_endpoint = preview_endpoints.iter().any(|(q0, r0)| {
                in_cell(*q0, oq, metrics.scale, q) && in_cell(*r0, or_, metrics.scale, r)
            });
            let is_coast = preview_coast.is_some_and(|(q0, r0)| {
                in_cell(q0, oq, metrics.scale, q) && in_cell(r0, or_, metrics.scale, r)
            });

            let (text, fg) = if let Some(s) = ship {
                let cs = callsign(s);
                let focused = app.focused_ship == Some(s.id);
                let short_cs: String = cs.chars().take(2).collect();
                let cell = if s.destroyed {
                    format!("x{}  ", short_cs.chars().next().unwrap_or('?'))
                } else if multipin {
                    // Coarse zoom can pack distinct ships into one cell — mark it.
                    let extra = ships_here.len().saturating_sub(1);
                    pad_cell(format!("{short_cs}+{extra}"), metrics.cell_width)
                } else {
                    // v4 is non-inertial: callsign + facing arrow only.
                    let arrow = facing_arrow(s.facing);
                    pad_cell(format!("{short_cs}{arrow}"), metrics.cell_width)
                };
                (cell, ship_fg(s, focused))
            } else if is_coast {
                (pad_cell("◆".to_string(), metrics.cell_width), Color::Cyan)
            } else if is_preview_endpoint {
                (
                    pad_cell("◇".to_string(), metrics.cell_width),
                    Color::DarkGray,
                )
            } else if in_arc_range {
                (
                    pad_cell("··".to_string(), metrics.cell_width),
                    Color::DarkGray,
                )
            } else {
                (
                    pad_cell(".".to_string(), metrics.cell_width),
                    Color::DarkGray,
                )
            };

            let mut style = Style::default().fg(fg);
            if in_arc_range {
                if let Some(ref s) = shade {
                    style = style.bg(s.bg);
                }
            } else if is_coast {
                style = style.bg(Color::DarkGray);
            }
            if ship.is_some() && app.focused_ship == ship.map(|s| s.id) {
                style = style.add_modifier(Modifier::BOLD);
            }
            spans.push(Span::styled(text, style));
        }
        lines.push(Line::from(spans));
    }

    lines.push(Line::from(""));
    let legend = if !preview_endpoints.is_empty() {
        "A1→ = ship/facing; +N = more ships here. ◆ final ◇ route"
    } else {
        "A1→ = ship/facing; +N = more ships here. Shade = weapon arc"
    };
    lines.push(Line::from(Span::styled(
        legend,
        Style::default().fg(Color::DarkGray),
    )));

    // Off-map contacts strip: living non-focused ships outside the viewport.
    // `off_map` is computed in the two-pass metrics block at the top of
    // render_map so the footer row is reserved before the hex grid is laid out.
    if !off_map.is_empty() {
        lines.push(Line::from(Span::styled(
            off_map,
            Style::default().fg(Color::Yellow),
        )));
    }

    let p = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title));
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
                Span::raw(format!("#{} {} profile={}", ship.id, ship.class, ship.size)),
            ]),
        );
        push(
            f,
            &mut y,
            {
                // v4 non-inertial kinematics: position + facing, and during the
                // movement stage the drafted path cost vs available motion.
                let mut s = format!(
                    "  @({},{}) face={}{}",
                    ship.q,
                    ship.r,
                    ship.facing,
                    facing_arrow(ship.facing),
                );
                if snap.phase == "movement" {
                    let cost = app.path_draft.as_ref().map(|d| d.cost()).unwrap_or(0);
                    s.push_str(&format!("  motion {}/{}", cost, ship.motion_available));
                }
                Line::from(s)
            },
        );
        // v4: short-fall path resolution notice for the focused ship.
        if let Some(line) = path_notice_for_focus(app, snap) {
            push(
                f,
                &mut y,
                Line::from(Span::styled(
                    format!("  {line}"),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )),
            );
        }

        // Show the projected final position from the drafted path preview.
        if let Some(preview) = app.path_preview_for_focus() {
            push(
                f,
                &mut y,
                Line::from(vec![
                    Span::styled("  ▶ projected: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!(
                        "({},{}) face={}{}  motion left {}",
                        preview.final_q,
                        preview.final_r,
                        preview.final_facing,
                        facing_arrow(preview.final_facing),
                        preview.remaining_motion,
                    )),
                ]),
            );
            if let Some(err) = &preview.error {
                push(
                    f,
                    &mut y,
                    Line::from(Span::styled(
                        format!("    illegal: {err}"),
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    )),
                );
            }
        }
        push(
            f,
            &mut y,
            Line::from(format!(
                "  hull {}  (current structure boxes)",
                ship.structure
            )),
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
                Line::from(" a: allocate  m: move  f: fire  v: map"),
                Line::from(" v: map-focus (WASD pan, +/- zoom, [/] inspect contacts)"),
                Line::from(""),
            ],
        ),
        Mode::Map => {
            let (oq, or_) = app.map_origin();
            let auto = if app.map_pan.is_none() {
                " (auto-center)"
            } else {
                ""
            };
            (
                "Map Focus",
                vec![
                    Line::from("          [w]        [+] zoom in"),
                    Line::from("       [a] [c] [d]    [-] zoom out"),
                    Line::from("          [s]        c: auto-fit"),
                    Line::from(" [ / ]: inspect ships   v / Esc / Enter: return"),
                    Line::from(""),
                    Line::from(Span::styled(
                        format!(
                            " origin=({},{}){}  zoom={}",
                            oq,
                            or_,
                            auto,
                            app.map_zoom.map_or("auto".to_string(), |z| z.to_string())
                        ),
                        Style::default().fg(Color::DarkGray),
                    )),
                    Line::from(""),
                ],
            )
        }
        Mode::Allocate => render_allocate_panel(app),
        Mode::Movement => render_movement_panel(app),
        Mode::Fire => render_fire_panel(app),
        Mode::GameOver => ("Game Over", render_game_over_summary(app, status)),
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

    // Fixed headers (top of form) that never scroll out of view.
    // Allocate: budget line + power gauge (Phase 3 / 3.1).
    // Fire: queue summary line (Phase 4 / 4.1) — the `Queued:` count must stay
    // visible even when the weapon list scrolls, and must agree with the
    // header `queued=N`.
    let mut body_area = content_area;
    if matches!(app.mode, Mode::Allocate) {
        // Budget line: fixed header so it never scrolls out of view (3.1).
        if let Some(budget) = allocate_budget_line(app) {
            if body_area.height > 2 {
                f.render_widget(
                    Paragraph::new(budget),
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
        if body_area.height > 1 {
            let footer_y = body_area.y + body_area.height - 1;
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    " Enter commit · ↑/↓ select · ←/→ adjust · digits set",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ))),
                Rect {
                    x: body_area.x,
                    y: footer_y,
                    width: body_area.width,
                    height: 1,
                },
            );
            body_area.height = body_area.height.saturating_sub(1);
        }
    } else if matches!(app.mode, Mode::Fire) {
        // Queue line: fixed header so the pending shot count stays visible
        // when the weapon list scrolls (4.1). Mirrors allocate_budget_line.
        if let Some(queue) = fire_queue_line(app) {
            if body_area.height > 2 {
                f.render_widget(
                    Paragraph::new(queue),
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
        if let Some(preview) = fire_preview_line(app) {
            if body_area.height > 2 {
                f.render_widget(
                    Paragraph::new(preview),
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
    // Scrolled body line layout (budget/gauge are fixed headers, not counted):
    //   hull(0), movement(1), weapons header(2), weapons (n),
    //   shields header (3+n), face map title + 3 diagram rows (4+n .. 7+n),
    //   faces (8+n ..), footer
    let n_weapons = draft.weapons.len();
    let visible = area.height.max(1);
    match draft.cursor {
        0 => 1u16.saturating_sub(visible.saturating_sub(1)),
        n if n <= n_weapons => {
            let line = (3 + (n - 1)) as u16;
            line.saturating_sub(visible.saturating_sub(1))
        }
        n => {
            let face = n - 1 - n_weapons;
            let diagram_top = (4 + n_weapons) as u16;
            let face_line = (8 + n_weapons + face) as u16;
            // Keep the compass in view: pin scroll at the diagram when the
            // selected face still fits below it; otherwise scroll to the face.
            if face_line.saturating_sub(diagram_top) < visible {
                diagram_top
            } else {
                face_line.saturating_sub(visible.saturating_sub(1))
            }
        }
    }
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
    let ship = match app.focused() {
        Some(s) => s,
        None => return 0,
    };
    // Mirror render_fire_panel's line layout so the selected weapon stays in
    // view: 2 legend lines, the volley summary line, an optional "No charge"
    // coach line, blank, Targets header, one line per enemy, blank, Weapons
    // header, then the weapon rows. (The `Queued:` summary is a fixed header
    // rendered by render_input_panel, not part of the scrollable body.)
    let has_charge = ship.weapons.iter().any(|w| w.operational && w.charge > 0);
    let no_charge = if !has_charge && snap.phase == "firing" {
        1
    } else {
        0
    };
    let selected_line = 2 + 1 + no_charge + 1 + 1 + enemy_count + 1 + 1 + draft.weapon_idx;
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

/// The budget line for the allocate panel, rendered as a fixed header so it
/// never scrolls out of view (Phase 3 / criterion 3.1). Returns `None` when
/// there is no focused ship or draft.
fn allocate_budget_line(app: &App) -> Option<Line<'static>> {
    let ship = app.focused()?;
    let draft = app.alloc_draft.as_ref()?;
    let cost = draft.power_cost(ship);
    let pool = ship.power_available;
    let balance = pool as i64 - cost as i64;
    let budget_style = if balance < 0 {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };
    Some(Line::from(vec![
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
    ]))
}

/// The fire-queue summary line, rendered as a fixed header so the pending
/// shot count stays visible even when the weapon list scrolls (Phase 4 /
/// criterion 4.1). Returns `None` when there is no focused ship or snapshot.
fn fire_queue_line(app: &App) -> Option<Line<'static>> {
    let _snap = app.snap.as_ref()?;
    let _ship = app.focused()?;
    let mine_count = app.fire_draft.as_ref().map(|d| d.shots.len()).unwrap_or(0);
    let style = if mine_count == 0 {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    };
    Some(Line::from(Span::styled(
        format!(" Queued: {mine_count} shot(s) pending"),
        style,
    )))
}

fn fire_preview_line(app: &App) -> Option<Line<'static>> {
    let preview = app.fire_preview.as_ref()?;
    let face = app.fire_draft.as_ref()?.shield_facing;
    let snap = app.snap.as_ref()?;
    // Attribute the shot: with several ships carrying identically named
    // weapons ("beam_1"), an unprefixed line doesn't say whose gun this is.
    let attacker = snap
        .ship(preview.ship)
        .map(callsign)
        .unwrap_or_else(|| format!("#{}", preview.ship));
    let target = snap
        .ship(preview.target)
        .map(callsign)
        .unwrap_or_else(|| format!("#{}", preview.target));
    if !preview.legal {
        // A destroyed weapon comes back from the engine as a lookup failure
        // ("weapon X was not found") — technically true, reads like a bug.
        // Say what actually happened to the player's gun.
        let weapon_destroyed = snap.ship(preview.ship).is_some_and(|ship| {
            ship.weapons
                .iter()
                .any(|w| w.id == preview.weapon && !w.operational)
        });
        let reason = if weapon_destroyed {
            format!("{} is destroyed and cannot fire", preview.weapon)
        } else {
            preview
                .reason
                .as_deref()
                .unwrap_or("illegal shot")
                .to_string()
        };
        return Some(Line::from(Span::styled(
            format!(" {} {}→{}: {}", attacker, preview.weapon, target, reason),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
    }
    let face_ok = preview.legal_shield_facings.contains(&face);
    let valid_faces = preview
        .legal_shield_facings
        .iter()
        .map(|f| shield_label(*f))
        .collect::<Vec<_>>()
        .join("/");
    Some(Line::from(Span::styled(
        format!(
            " {} {}→{} d{}: {}% (d{}≤{}) dmg≈{} · face {} {}{}",
            attacker,
            preview.weapon,
            target,
            preview.range.unwrap_or(0),
            preview.hit_percent.unwrap_or(0),
            preview.die_sides.unwrap_or(20),
            preview.threshold.unwrap_or(0),
            preview.projected_damage.unwrap_or(0),
            shield_label(face),
            if face_ok { "ok" } else { "INVALID; use " },
            if face_ok { "" } else { valid_faces.as_str() },
        ),
        if face_ok {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        },
    )))
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

    // Hull line (B2 / criterion 3.3): show current structure only — no fake
    // max. The protocol does not carry max structure, so `N/N` would lie.
    let mut lines = vec![Line::from(Span::styled(
        format!(" hull {}  (structure boxes)", ship.structure),
        Style::default().fg(Color::DarkGray),
    ))];

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
        let operational = ship
            .weapons
            .iter()
            .find(|w| &w.id == id)
            .map(|w| w.operational)
            .unwrap_or(true);
        let row = if operational {
            format!("{mark}{id} ({kind}) charge {chg}/{max}{carried}")
        } else {
            // Non-operational weapon: display-only, no editable charge prompt (M2/2.6).
            format!("{mark}{id} ({kind}) OFFLINE")
        };
        lines.push(Line::from(Span::styled(
            row,
            if selected {
                selected_style()
            } else if !operational {
                Style::default().fg(Color::DarkGray)
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

    // Face diagram *before* the face list so scrolling onto shields shows the
    // compass immediately (play feedback: diagram was easy to miss below the list).
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
    let diagram_style = if sel_face.is_some() {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    lines.push(Line::from(Span::styled(
        " Face map ([] = selected):",
        diagram_style,
    )));
    lines.push(Line::from(Span::styled(
        format!("   {} {} {}", face_cell(5), face_cell(0), face_cell(1)),
        diagram_style,
    )));
    lines.push(Line::from(Span::styled(
        format!("   {}  ·  {}", face_cell(4), face_cell(2)),
        diagram_style,
    )));
    lines.push(Line::from(Span::styled(
        format!("      {}", face_cell(3)),
        diagram_style,
    )));

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

    ("Allocate", lines)
}

/// Short display token for one path action.
fn action_token(action: &str) -> &'static str {
    match action {
        "move_f" => "F",
        "move_fl" => "FL",
        "move_fr" => "FR",
        "turn_left" => "◄",
        "turn_right" => "►",
        _ => "?",
    }
}

fn render_movement_panel(app: &App) -> (&'static str, Vec<Line<'static>>) {
    let ship = match app.focused() {
        Some(s) => s,
        None => return ("Movement", vec![Line::from(" (no ship focused)")]),
    };

    let draft_actions: Vec<String> = app
        .path_draft
        .as_ref()
        .map(|d| d.actions.clone())
        .unwrap_or_default();
    let cost = draft_actions.len() as u32;

    let mut lines = vec![Line::from(format!(
        " {} @({},{}) face={}{}",
        callsign(ship),
        ship.q,
        ship.r,
        ship.facing,
        facing_arrow(ship.facing),
    ))];

    // Drafted path as a row of short tokens.
    let path_tokens = if draft_actions.is_empty() {
        "(empty — will hold position)".to_string()
    } else {
        draft_actions
            .iter()
            .map(|a| action_token(a))
            .collect::<Vec<_>>()
            .join(" ")
    };
    lines.push(Line::from(vec![
        Span::styled(" Path: ", Style::default().fg(Color::Cyan)),
        Span::raw(path_tokens),
    ]));

    // Running motion cost vs available.
    let over = cost > ship.motion_available;
    lines.push(Line::from(Span::styled(
        format!(" motion {}/{}", cost, ship.motion_available),
        if over {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        },
    )));

    // Projected final position/facing and any illegality from the preview.
    if let Some(preview) = app.path_preview_for_focus() {
        lines.push(Line::from(Span::styled(
            format!(
                " ▶ final ({},{}) face={}{}  motion left {}",
                preview.final_q,
                preview.final_r,
                preview.final_facing,
                facing_arrow(preview.final_facing),
                preview.remaining_motion,
            ),
            Style::default().fg(Color::Cyan),
        )));
        if let Some(err) = &preview.error {
            lines.push(Line::from(Span::styled(
                format!(" illegal: {err}"),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " w/↑ forward · a veer-left · d veer-right · ←/→ turn",
        Style::default().fg(Color::Yellow),
    )));
    lines.push(Line::from(Span::styled(
        " 0–5 face · Backspace undo · x clear · Enter commit · Space hold",
        Style::default().fg(Color::Yellow),
    )));

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

    // v4 volley builder: the whole volley is assembled client-side in
    // `app.fire_draft.shots` and submitted as one `commit_volley` on Space.
    let mut lines = vec![
        Line::from(Span::styled(
            " ↑↓ weapon · 1–9 target · ←→ shield face",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(Span::styled(
            " Enter queue/unqueue · Backspace remove · Space fire volley",
            Style::default().fg(Color::Yellow),
        )),
    ];

    // Assembled-volley summary: e.g. "Volley: beam_1→B2(R), torp_1→B2(R)".
    let volley_summary = if draft.shots.is_empty() {
        "Volley: (empty — Space holds fire)".to_string()
    } else {
        let parts: Vec<String> = draft
            .shots
            .iter()
            .map(|shot| {
                let tgt = snap
                    .ship(shot.target)
                    .map(callsign)
                    .unwrap_or_else(|| format!("#{}", shot.target));
                format!(
                    "{}→{}({})",
                    shot.weapon,
                    tgt,
                    shield_label(shot.shield_facing)
                )
            })
            .collect();
        format!("Volley: {}", parts.join(", "))
    };
    lines.push(Line::from(Span::styled(
        format!(" {volley_summary}"),
        if draft.shots.is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        },
    )));

    // "No charge" coach: if every operational weapon is out of charge
    // mid-fire-phase, tell the player Space passes instead of firing.
    let has_charge = ship.weapons.iter().any(|w| w.operational && w.charge > 0);
    if !has_charge && snap.phase == "firing" {
        lines.push(Line::from(Span::styled(
            " No charge left this turn — Space to pass fire",
            Style::default().fg(Color::Yellow),
        )));
    }
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
                " {marker} {} {} d={} face={}{} profile={}",
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
        let queued = draft.is_queued(&w.id);
        let queued_str = if queued { " [QUEUED]" } else { "" };
        let charge_str = if !w.operational {
            "OFFLINE".to_string()
        } else if w.charge > 0 {
            format!("chg={}", w.charge)
        } else {
            "UNCHARGED".to_string()
        };
        let style = if selected {
            selected_style()
        } else if !w.operational {
            Style::default().fg(Color::DarkGray)
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
    ("Fire", lines)
}

fn render_events_log(f: &mut Frame, app: &App, area: Rect) {
    let live_volley = !app.recent_events.is_empty();
    let title = if live_volley {
        format!("Combat Log · volley {}", app.recent_events.len())
    } else {
        "Combat Log".to_string()
    };
    let event_block = Block::default().borders(Borders::ALL).title(title);
    // Prefer the current volley (recent_events) in chronological order so a
    // simultaneous 2v2 exchange (6–8 lines) is readable start-to-finish.
    // Newest-first previously put the AI return fire on top and truncated
    // the player's own shots out of a short panel.
    let events = if live_volley {
        &app.recent_events
    } else {
        &app.combat_history
    };
    let event_entries: Vec<(String, Style)> = if events.is_empty() {
        vec![(
            "(no combat yet)".into(),
            Style::default().fg(Color::DarkGray),
        )]
    } else {
        let ordered: Box<dyn Iterator<Item = &String>> = if live_volley {
            Box::new(events.iter()) // chronological
        } else {
            Box::new(events.iter().rev()) // history: newest first when browsing
        };
        ordered
            .map(|e| {
                let style = if e.contains("HIT") {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                (e.clone(), style)
            })
            .collect()
    };
    let mut entries = Vec::new();
    if let Some(error) = &app.last_error {
        entries.push((
            format!("ENGINE: {error}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    }
    // A collision/edge stop explains why displayed velocity and actual travel
    // differ. Keep the latest notice visible even while combat occupies the log.
    if live_volley {
        if let Some(notice) = app.log.iter().rev().find(|line| line.starts_with("Moved ")) {
            entries.push((notice.clone(), Style::default().fg(Color::Cyan)));
        }
    }
    entries.extend(event_entries);
    // During a live volley, keep the panel for combat only — command-log
    // lines used to crowd out the last shots of a multi-ship exchange.
    if !live_volley {
        entries.extend(
            app.log
                .iter()
                .rev()
                .filter(|line| !line.starts_with("ERROR:"))
                .map(|l| (l.clone(), Style::default().fg(Color::DarkGray))),
        );
    }
    let visible = area.height.saturating_sub(2) as usize;
    let inner_width = area.width.saturating_sub(2) as usize;
    let pair_columns = live_volley && inner_width >= 60 && entries.len() > visible;
    let items: Vec<ListItem> = if pair_columns {
        let column_width = inner_width.saturating_sub(3) / 2;
        entries
            .chunks(2)
            .take(visible)
            .map(|pair| {
                let mut spans = vec![Span::styled(
                    fit_log_cell(&pair[0].0, column_width, true),
                    pair[0].1,
                )];
                if let Some((text, style)) = pair.get(1) {
                    spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
                    spans.push(Span::styled(
                        fit_log_cell(text, column_width, false),
                        *style,
                    ));
                }
                ListItem::new(Line::from(spans))
            })
            .collect()
    } else {
        entries
            .into_iter()
            .take(visible)
            .map(|(text, style)| ListItem::new(Span::styled(text, style)))
            .collect()
    };
    f.render_widget(List::new(items).block(event_block), area);
}

fn fit_log_cell(text: &str, width: usize, pad: bool) -> String {
    let mut value: String = text.chars().take(width).collect();
    if pad {
        value.extend(std::iter::repeat_n(
            ' ',
            width.saturating_sub(value.chars().count()),
        ));
    }
    value
}

fn hex_dist(q1: i32, r1: i32, q2: i32, r2: i32) -> u32 {
    let dq = (q1 - q2).abs();
    let dr = (r1 - r2).abs();
    let ds = (q1 + r1 - q2 - r2).abs();
    ((dq + dr + ds) / 2) as u32
}

/// Range readout for the map title: axial distance from the focused ship to
/// the nearest living enemy, formatted as ` · → B2 d=6`. Pure geometry from
/// snapshot q,r — not a combat rule. Empty string if no focused ship or enemy.
fn focused_range_to_nearest_enemy(app: &App, snap: &Snapshot) -> String {
    let Some(me) = app.focused() else {
        return String::new();
    };
    let nearest = snap
        .ships
        .iter()
        .filter(|s| s.id != me.id && !s.destroyed && s.controller != me.controller)
        .min_by_key(|s| hex_dist(me.q, me.r, s.q, s.r));
    match nearest {
        Some(enemy) => {
            let d = hex_dist(me.q, me.r, enemy.q, enemy.r);
            format!(" · → {} d={}", callsign(enemy), d)
        }
        None => String::new(),
    }
}

/// Build the map Block title, prioritizing the range readout so it is never
/// the first thing clipped when the panel is narrow.
///
/// Order kept when space allows:
///   Map @(q,r) z=N · → B2 d=8 · 2 hex/cell · arc/preview · green=you red=ai
fn r_label_gutter_width(labels: &[i32]) -> usize {
    labels
        .iter()
        .map(|v| format!("{v}").len())
        .max()
        .unwrap_or(2)
        .max(2)
        + 1 // trailing space for separation from the grid
}

fn format_signed_coord(value: i32, width: usize) -> String {
    let s = format!("{value}");
    if s.chars().count() >= width {
        s.chars().take(width).collect()
    } else {
        format!("{s:<width$}")
    }
}

#[allow(clippy::too_many_arguments)]
fn build_map_title(
    max_width: usize,
    oq: i32,
    or_: i32,
    _zoom: i8,
    scale: i32,
    range_readout: &str,
    shade: Option<(&str, u32)>,
    has_preview: bool,
) -> String {
    // Inner title width ≈ panel width minus borders; leave a small margin.
    let budget = max_width.saturating_sub(2).max(12);

    // Fable Phase 5: no internal z=N — only hex/cell (and zoom:auto when manual unset is implied by scale).
    let base = format!("Map @({oq},{or_})");
    let scale_part = if scale > 1 {
        format!(" · {scale} hex/cell")
    } else {
        " · 1 hex/cell".to_string()
    };
    let mode_part = if let Some((mount, max_range)) = shade {
        format!(" · {mount} arc ≤{max_range}")
    } else if has_preview {
        " · ◆/◇ preview".to_string()
    } else {
        String::new()
    };
    let legend = " · you/ai";

    // Assemble in priority order: base + range always first, then scale, mode, legend.
    let mut title = base;
    if !range_readout.is_empty() {
        // If even base+range overflows, keep a compact range-first form.
        let with_range = format!("{title}{range_readout}");
        if with_range.chars().count() > budget {
            // Drop coords if needed: "Map · → B2 d=8"
            let compact = format!("Map{range_readout}");
            return if compact.chars().count() <= budget {
                compact
            } else {
                let bare = format!("Map{range_readout}");
                bare.chars().take(budget).collect()
            };
        }
        title = with_range;
    }
    for part in [&scale_part, &mode_part, &legend.to_string()] {
        if part.is_empty() {
            continue;
        }
        let next = format!("{title}{part}");
        if next.chars().count() <= budget {
            title = next;
        }
        // else drop this and lower-priority parts
    }
    title
}

/// Compass bearing from `from` to `to` as a hex-neighbor direction name.
/// Uses the same HEX_DIRS ordering as the map (0=E, 1=NE, 2=NW, 3=W, 4=SW, 5=SE).
fn hex_bearing_name(from_q: i32, from_r: i32, to_q: i32, to_r: i32) -> &'static str {
    if from_q == to_q && from_r == to_r {
        return "here";
    }
    let bearings = nearest_bearings(from_q, from_r, to_q, to_r);
    let abs = bearings.first().copied().unwrap_or(0);
    match abs {
        0 => "east",
        1 => "northeast",
        2 => "northwest",
        3 => "west",
        4 => "southwest",
        5 => "southeast",
        _ => "?",
    }
}

/// One-line strip listing living non-focused ships that fall outside the map
/// viewport, with bearing and range from the focused ship. Empty if all fit.
fn off_map_contacts(
    app: &App,
    snap: &Snapshot,
    oq: i32,
    or_: i32,
    columns: i32,
    rows: i32,
    scale: i32,
) -> String {
    let Some(me) = app.focused() else {
        return String::new();
    };
    let mut off_map: Vec<(&crate::protocol::Ship, u32)> = snap
        .ships
        .iter()
        .filter(|s| s.id != me.id && !s.destroyed)
        .filter(|s| {
            // A ship is off-map if its q or r falls outside the viewport.
            let q_idx = (s.q - oq).div_euclid(scale);
            let r_idx = (s.r - or_).div_euclid(scale);
            q_idx < 0 || q_idx >= columns || r_idx < 0 || r_idx >= rows
        })
        .map(|s| (s, hex_dist(me.q, me.r, s.q, s.r)))
        .collect();
    if off_map.is_empty() {
        return String::new();
    }
    off_map.sort_by_key(|(_, d)| *d);
    let parts: Vec<String> = off_map
        .iter()
        .map(|(s, d)| {
            format!(
                "{} off-map · {} · d={}",
                callsign(s),
                hex_bearing_name(me.q, me.r, s.q, s.r),
                d
            )
        })
        .collect();
    parts.join("  ")
}

fn render_tutorial_panel(f: &mut Frame, app: &App, area: Rect) {
    let Some(t) = &app.tutorial else {
        return;
    };

    let title = if !t.is_complete() {
        let location = app
            .snap
            .as_ref()
            .map(|s| {
                format!("Turn {} · {}", s.turn, phase_label(&s.phase))
            })
            .unwrap_or_else(|| "Starting".to_string());
        format!("Coach · {location} · {}/{}", t.current + 1, t.steps.len())
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

/// Fable Phase 4: phase-specific call-to-action for the header (replaces "actions remain").
fn phase_call_to_action(app: &App, snap: &Snapshot) -> String {
    // A finished game has no next action; pointing at allocation/maneuvers
    // would be stale advice. The Game Over panel carries the summary.
    if matches!(snap.status.as_str(), "Won" | "Lost") {
        return "Game over — q quits".into();
    }
    let focused_id = app.focused().map(|s| s.id);

    // Keys drive the FOCUSED ship, so the call-to-action must name it. When
    // the focused ship is done but a fleetmate is still pending, say so and
    // point at Tab instead of silently naming the wrong ship.
    let pending_cta = |completed: &[i64], verb: &str| -> String {
        let focused_pending = app
            .focused()
            .filter(|s| s.controller == "player" && !s.destroyed && !completed.contains(&s.id));
        if let Some(ship) = focused_pending {
            return format!("{} {verb}", callsign(ship));
        }
        let other_pending = snap
            .ships
            .iter()
            .find(|s| s.controller == "player" && !s.destroyed && !completed.contains(&s.id));
        match other_pending {
            Some(ship) => format!("{} {verb} — Tab to switch", callsign(ship)),
            None => String::new(),
        }
    };

    match snap.phase.as_str() {
        "allocate" => {
            if app.focused().is_some_and(|ship| {
                ship.controller == "player"
                    && !ship.destroyed
                    && ship.power_available == 0
                    && !snap.ships_allocated_this_turn.contains(&ship.id)
            }) {
                let cs = app.focused().map(callsign).unwrap_or_else(|| "Ship".into());
                format!("{cs} disabled; Space passes")
            } else {
                pending_cta(&snap.ships_allocated_this_turn, "needs power allocation")
            }
        }
        "movement" => {
            if app.focused().is_some_and(|ship| {
                ship.controller == "player"
                    && !ship.destroyed
                    && ship.motion_available == 0
                    && !snap.ships_committed_path.contains(&ship.id)
            }) {
                let cs = app.focused().map(callsign).unwrap_or_else(|| "Ship".into());
                format!("{cs} no motion; Space holds")
            } else {
                pending_cta(&snap.ships_committed_path, "needs a path")
            }
        }
        "firing" => {
            let focused_ready =
                focused_id.is_some_and(|id| snap.ships_committed_volley.contains(&id));
            if focused_ready {
                let cs = app.focused().map(callsign).unwrap_or_else(|| "Ship".into());
                // Opportunity scan already skips committed ships; if another ship
                // still has a shot, point Tab there instead of re-offering this one.
                if let Some(opp) = &snap.fire_opportunity {
                    if focused_id != Some(opp.ship) {
                        let other = snap
                            .ship(opp.ship)
                            .map(callsign)
                            .unwrap_or_else(|| format!("#{}", opp.ship));
                        return format!(
                            "{cs} committed; Tab>{other} {}>{}",
                            opp.weapon,
                            snap.ship(opp.target)
                                .map(callsign)
                                .unwrap_or_else(|| format!("#{}", opp.target))
                        );
                    }
                }
                return format!("{cs} committed");
            }
            let queued = if focused_id.is_some() {
                app.fire_draft.as_ref().map(|d| d.shots.len()).unwrap_or(0)
            } else {
                0
            };
            if queued > 0 {
                format!("{queued} queued; Space fires")
            } else if let Some(opp) = &snap.fire_opportunity {
                let attacker = snap
                    .ship(opp.ship)
                    .map(callsign)
                    .unwrap_or_else(|| format!("#{}", opp.ship));
                let w = &opp.weapon;
                let tgt = snap
                    .ship(opp.target)
                    .map(callsign)
                    .unwrap_or_else(|| format!("#{}", opp.target));
                if focused_id == Some(opp.ship) {
                    format!("{attacker} {w}>{tgt} available")
                } else {
                    let active = app.focused().map(callsign).unwrap_or_else(|| "Ship".into());
                    format!("{active} active; Tab>{attacker} {w}>{tgt}")
                }
            } else {
                "No legal shot; Space passes fire".into()
            }
        }
        "turn_end" => "Turn complete; e".into(),
        _ => String::new(),
    }
}

fn path_notice_for_focus(app: &App, snap: &Snapshot) -> Option<String> {
    let id = app.focused()?.id;
    let pr = snap.path_results.iter().find(|r| r.ship == id)?;
    crate::app::format_path_result(snap, pr)
}

fn render_game_over_summary(app: &App, status: &str) -> Vec<Line<'static>> {
    let banner = match status {
        "Won" => " VICTORY",
        "Lost" => " DEFEAT",
        other => other,
    };
    let turn = app.snap.as_ref().map(|s| s.turn).unwrap_or(0);

    // Stats from structured combat events (player controller attacks/defenses).
    let mut shots = 0u32;
    let mut hits = 0u32;
    let mut int_dealt = 0u32;
    let mut int_taken = 0u32;
    if let Some(snap) = app.snap.as_ref() {
        for e in &app.combat_events {
            let atk_player = snap
                .ship(e.attacker)
                .is_some_and(|s| s.controller == "player");
            let tgt_player = snap
                .ship(e.target)
                .is_some_and(|s| s.controller == "player");
            if atk_player {
                shots += 1;
                if e.kind == "hit" {
                    hits += 1;
                    int_dealt += e.hull_damage;
                }
            }
            if tgt_player && e.kind == "hit" {
                int_taken += e.hull_damage;
            }
        }
    }

    let color = if status == "Won" {
        Color::Green
    } else if status == "Lost" {
        Color::Red
    } else {
        Color::Yellow
    };

    vec![
        Line::from(Span::styled(
            banner.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )),
        Line::from(format!(" Turns: {turn}")),
        Line::from(format!(" Player shots: {shots}  hits: {hits}")),
        Line::from(format!(" Internal damage dealt: {int_dealt}")),
        Line::from(format!(" Internal damage taken: {int_taken}")),
        Line::from(""),
        Line::from(" q: quit  ·  session log written on exit"),
        Line::from(Span::styled(
            " Combat log remains below / in the log panel.",
            Style::default().fg(Color::DarkGray),
        )),
    ]
}
