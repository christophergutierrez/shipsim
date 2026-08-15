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
use crate::yard::YardScreen;

/// Render the full frame.
pub fn render(f: &mut Frame, app: &mut App) {
    let size = f.area();

    let (min_width, min_height) = if app.yard.is_some() { (60, 16) } else { (80, 24) };
    if size.width < min_width || size.height < min_height {
        app.terminal_too_small = true;
        let message = if app.yard.is_some() {
            format!("Shipyard needs at least {min_width}x{min_height}\n\nq quits")
        } else {
            format!("Terminal too small to play\n\nResize to at least {min_width}x{min_height}\n\nYour game is paused here and will resume when the window grows.")
        };
        let p = Paragraph::new(message)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("shipsim TUI"));
        f.render_widget(p, size);
        return;
    }
    app.terminal_too_small = false;

    if app.yard.is_some() {
        render_yard(f, app, size);
        return;
    }

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
    // Ghost marker: a dim shadow of the focused ship at the planned end hex,
    // showing where it *will* be and which way it *will* face after simultaneous
    // movement resolves. Not yet committed — rendered dim to signal "pending".
    let ghost: Option<(i32, i32, String, u32)> = preview.and_then(|p| {
        let ship = app.focused()?;
        let short: String = callsign(ship).chars().take(2).collect();
        Some((p.final_q, p.final_r, short, p.final_facing))
    });

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
                    // Coarse zoom can pack distinct ships into one cell — name
                    // the occupants so a count is never unexplained.
                    let occupants = ships_here
                        .iter()
                        .map(|other| callsign(other))
                        .collect::<Vec<_>>()
                        .join("+");
                    pad_cell(occupants, metrics.cell_width)
                } else {
                    // v4 is non-inertial: callsign + facing arrow only.
                    let arrow = facing_arrow(s.facing);
                    pad_cell(format!("{short_cs}{arrow}"), metrics.cell_width)
                };
                (cell, ship_fg(s, focused))
            } else if is_coast {
                // Dim shadow of the ship at its planned end hex. The diamond
                // prefix keeps the destination visually tied to route cells,
                // while the callsign and facing keep it distinct from a ship.
                let label = ghost
                    .as_ref()
                    .filter(|(gq, gr, _, _)| {
                        in_cell(*gq, oq, metrics.scale, q) && in_cell(*gr, or_, metrics.scale, r)
                    })
                    .map(|(_, _, cs, facing)| format!("◇{cs}{}", facing_arrow(*facing)))
                    .unwrap_or_else(|| "◇".to_string());
                (pad_cell(label, metrics.cell_width), Color::Cyan)
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
                // Shadow, not a solid marker: dim + italic so it reads as a
                // projection of the ship rather than the ship itself.
                style = style
                    .add_modifier(Modifier::DIM)
                    .add_modifier(Modifier::ITALIC);
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
        "A1→ ship · ◇A1→ end · ◇ route"
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

pub(crate) fn render_ship_status(f: &mut Frame, app: &App, snap: &Snapshot, area: Rect) {
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
                Span::raw(format!("#{} {} size {}", ship.id, ship.class, ship.size)),
            ]),
        );
        push(f, &mut y, {
            // v4 non-inertial kinematics: position + facing, and during the
            // movement stage the drafted path cost vs available motion.
            let mut s = format!(
                "  @({},{}) face={}{} · hull {}",
                ship.q,
                ship.r,
                ship.facing,
                facing_arrow(ship.facing),
                ship.structure,
            );
            if snap.phase == "movement" {
                let cost = app.path_draft.as_ref().map(|d| d.cost()).unwrap_or(0);
                s.push_str(&format!("  motion {}/{}", cost, ship.motion_available));
            }
            Line::from(s)
        });
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
        if ship.cloaked {
            push(f, &mut y, Line::from("  CLOAKED"));
        }
        if !ship.systems.is_empty() {
            let systems: Vec<String> = ship
                .systems
                .iter()
                .map(|system| match system.mk {
                    Some(mk) => format!("{} mk{mk}", system.kind),
                    None => system.kind.clone(),
                })
                .collect();
            push(f, &mut y, Line::from(format!("  systems {}", systems.join(" · "))));
        }
        if let Some(squad) = ship.squad_id {
            push(
                f,
                &mut y,
                Line::from(format!(
                    "  squad {squad} leader={} members={:?}",
                    ship.squad_leader.unwrap_or(ship.id),
                    ship.squad_members
                )),
            );
        }

        push(
            f,
            &mut y,
            Line::from(format!(
                "  power avail {} · engine boxes {} · sys {}",
                ship.power_available, ship.engine, ship.power_sys
            )),
        );

        let pending_alloc = if snap.phase == "allocate" {
            app.alloc_draft.as_ref()
        } else {
            None
        };
        let target_face = app
            .fire_draft
            .as_ref()
            .and_then(|draft| draft.target)
            .and_then(|target| snap.ship(target))
            .map(|target| relative_bearing(ship.facing, ship.q, ship.r, target.q, target.r));
        let shield_str: Vec<String> = (0..6)
            .map(|i| {
                let cap = ship.shield_cap(i);
                let committed = ship.shields_powered.get(i).copied().unwrap_or(0);
                let (value, pending) = match pending_alloc {
                    Some(draft) => {
                        let value = draft.shields.get(i).copied().unwrap_or(0);
                        (value, value != committed)
                    }
                    None => (ship.shields_remaining.get(i).copied().unwrap_or(0), false),
                };
                format!(
                    "{}:{}/{}{}{}",
                    shield_label(i as u32),
                    value,
                    cap,
                    if pending { "*" } else { "" }
                    , if target_face == Some(i as u8) { "^" } else { "" }
                )
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
        if pending_alloc.is_some() {
            push(f, &mut y, Line::from("  * = not yet committed"));
        }

        push(f, &mut y, Line::from("  weapons:"));
        for w in &ship.weapons {
            let (charge, pending) = pending_alloc
                .and_then(|draft| {
                    draft
                        .weapons
                        .iter()
                        .find(|(id, _)| id == &w.id)
                        .map(|(_, charge)| (*charge, *charge != w.charge))
                })
                .unwrap_or((w.charge, false));
            let fired = if w.fired { " [fired]" } else { "" };
            let ammo = match (w.ammo_remaining, w.max_ammo) {
                (Some(left), Some(max)) => format!(" ammo={left}/{max}"),
                _ => String::new(),
            };
            let quality = match (w.accuracy_bonus, w.damage_bonus) {
                (0, 0) => String::new(),
                (acc, 0) => format!(" acc+{acc}"),
                (0, dmg) => format!(" dmg+{dmg}"),
                (acc, dmg) => format!(" acc+{acc} dmg+{dmg}"),
            };
            let tags = w.tags();
            let tag = if tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", tags.join(","))
            };
            let state = if w.operational {
                format!(
                    "chg={}/{}{}",
                    charge,
                    w.max_charge,
                    if pending { "*" } else { "" }
                )
            } else {
                "DESTROYED".to_string()
            };
            push(
                f,
                &mut y,
                Line::from(format!(
                    "    {} {} rng≤{} {}{}{}{}{}",
                    w.id, w.kind, w.max_range, state, ammo, quality, tag, fired
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
                "  {} #{} {} @({},{}) rng={} face={}{}{}",
                cs,
                s.id,
                s.class,
                s.q,
                s.r,
                dist,
                s.facing,
                facing_arrow(s.facing),
                if s.cloaked { " CLOAKED" } else { "" }
            )),
        );
    }
}

fn render_input_panel(f: &mut Frame, app: &mut App, status: &str, _is_over: bool, area: Rect) {
    let (title, lines) = match &app.mode {
        Mode::Normal => (
            "Overview & Help",
            vec![
                Line::from(" Esc/Enter return · q quit · a allocate · m move · f fire"),
                Line::from(" Tab: cycle focus  v: map-focus"),
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
        Mode::Allocate if is_disabled_ship(app) => (
            "Allocate",
            vec![Line::from(Span::styled(
                format!(
                    " DISABLED — no power; cannot move or fire{}",
                    app.input_notice
                        .as_ref()
                        .map(|notice| format!(" · {notice}"))
                        .unwrap_or_default()
                ),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ))],
        ),
        Mode::Movement if is_disabled_ship(app) => (
            "Movement",
            vec![Line::from(Span::styled(
                format!(
                    " DISABLED — no power; cannot move or fire{}",
                    app.input_notice
                        .as_ref()
                        .map(|notice| format!(" · {notice}"))
                        .unwrap_or_default()
                ),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ))],
        ),
        Mode::Fire if is_disabled_ship(app) => (
            "Fire",
            vec![Line::from(Span::styled(
                format!(
                    " DISABLED — no power; cannot move or fire{}",
                    app.input_notice
                        .as_ref()
                        .map(|notice| format!(" · {notice}"))
                        .unwrap_or_default()
                ),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ))],
        ),
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
                    .label(if over {
                        format!("power {cost}/{} OVER", ship.power_available)
                    } else if cost == 0 {
                        format!(
                            "power 0/{} · no motion · no charge · no shields",
                            ship.power_available
                        )
                    } else {
                        format!("power {cost}/{} ok", ship.power_available)
                    });
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

    // Keep a compact, mode-specific exit row fixed at the bottom of every
    // combat form. Detailed controls remain in the scrollable panel body.
    let combat_footer = app.disabled_pass_notice.clone().or_else(|| match app.mode {
        Mode::Allocate if is_disabled_ship(app) => Some("Enter unavailable · Space pass disabled ship".to_string()),
        Mode::Movement if is_disabled_ship(app) => Some("Enter unavailable · Space pass disabled ship".to_string()),
        Mode::Allocate => Some("Esc help · Enter commit power → Movement · ↑/↓ field · PgDn shields".to_string()),
        Mode::Movement => Some(movement_footer(app)),
        Mode::Fire if is_disabled_ship(app) => Some("Enter unavailable · Space pass disabled ship".to_string()),
        Mode::Fire => Some(fire_footer(app)),
        Mode::GameOver => Some("Enter quit · q quit".to_string()),
        _ => None,
    });
    if let Some(footer) = combat_footer {
        if body_area.height > 1 {
            let footer_y = body_area.y + body_area.height - 1;
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    footer,
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
    // view: 2 legend lines, the volley summary line, an optional pass coach
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
    let field = if draft.cursor == 0 {
        Some(format!(
            "Movement {}/{} pwr",
            draft.movement,
            ship.movement_power_cap().unwrap_or(ship.power_available)
        ))
    } else if draft.cursor > draft.weapons.len() {
        let face = draft.cursor - draft.weapons.len() - 1;
        draft.shields.get(face).map(|value| {
            format!(
                "Shield {} {}/{} pwr",
                shield_label(face as u32),
                value,
                ship.shield_cap(face)
            )
        })
    } else {
        None
    };
    let budget_style = if balance < 0 {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };
    let mut spans = vec![Span::raw(format!(" Budget {cost}/{pool} (")),
        Span::styled(
            if balance < 0 {
                format!("{} over", -balance)
            } else {
                format!("{balance} free")
            },
            budget_style,
        ),
        Span::raw(") · pwr allocation")];
    if let Some(field) = field {
        spans.push(Span::raw(" · "));
        spans.push(Span::styled(field, Style::default().fg(Color::Cyan)));
    }
    Some(Line::from(spans))
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

fn fire_footer(app: &App) -> String {
    let Some(draft) = app.fire_draft.as_ref() else {
        return "Esc help · ↑/↓ gun · ←/→ face · Enter unavailable · Space pass".into();
    };
    let selected = app
        .focused()
        .and_then(|ship| ship.weapons.get(draft.weapon_idx));
    let target = draft.target.or_else(|| {
        app.snap.as_ref()?.ships.iter().find(|ship| {
            ship.controller != "player" && !ship.destroyed
        }).map(|ship| ship.id)
    });
    let preview = selected.and_then(|weapon| {
        target.and_then(|target| {
            app.focused_ship
                .and_then(|ship| app.matching_fire_preview(ship, &weapon.id, target))
        })
    });
    let can_queue = selected.is_some_and(|weapon| {
        weapon.operational
            && !weapon.is_pd()
            && weapon.charge > 0
            && weapon.ammo_remaining != Some(0)
            && preview.is_some_and(|preview| {
                preview.legal && preview.legal_shield_facings.contains(&draft.shield_facing)
            })
    });
    let queued = selected.is_some_and(|weapon| draft.is_queued(&weapon.id));
    let action = if queued {
        "Enter remove"
    } else if can_queue {
        "Enter queue"
    } else {
        "Enter unavailable"
    };
    let final_action = if draft.shots.is_empty() {
        "Space pass"
    } else {
        "Space fire"
    };
    format!("Esc help · ↑/↓ gun · ←/→ face · {action} · {final_action}")
}

fn movement_footer(app: &App) -> String {
    let has_path = app.path_draft.as_ref().is_some_and(|draft| !draft.is_empty());
    if has_path {
        "Esc help · Enter commit path → Fire · Space hold → Fire".into()
    } else {
        "Esc help · Enter unavailable · Space hold → Fire".into()
    }
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
        format!(
            " hull {}{}",
            ship.structure,
            if ship.cloaked { "  CLOAKED" } else { "" }
        ),
        Style::default().fg(Color::DarkGray),
    ))];
    if let Some(summary) = &app.resolution_summary {
        lines.push(Line::from(Span::styled(
            format!(" Result: {summary}"),
            Style::default().fg(Color::Yellow),
        )));
    }
    if ship.has_system("cloak") || ship.has_system("repair") || ship.squad_id.is_some() {
        let mut system_bits = Vec::new();
        if ship.has_system("cloak") {
            system_bits.push(format!(
                "x cloak={} ({}pwr)",
                if draft.cloak { "on" } else { "off" },
                if draft.cloak { 4 + ship.size } else { 0 }
            ));
        }
        if ship.has_system("repair") {
            system_bits.push(format!(
                "z repair={}/{}",
                draft.repair,
                ship.repair_cap.map_or_else(|| "?".into(), |cap| cap.to_string())
            ));
        }
        if ship.squad_id.is_some() {
            system_bits.push(format!(
                "u unsquad={} l leader={}",
                if draft.unsquad { "yes" } else { "no" },
                draft
                    .squad_leader
                    .or(ship.squad_leader)
                    .map(|id| format!("#{id}"))
                    .unwrap_or_else(|| "—".into())
            ));
        }
        lines.push(Line::from(format!(" {}", system_bits.join("  "))));
    }

    // Movement row
    let mov_selected = draft.cursor == 0;
    lines.push(Line::from(Span::styled(
        format!(
            "{}{}Movement: {:>2}/{} pwr (max path {})   ←/→ or m",
            if mov_selected { "▶ " } else { "  " },
            if mov_selected {
                app.input_notice
                    .as_ref()
                    .map(|notice| format!("[{notice}] "))
                    .unwrap_or_default()
            } else {
                String::new()
            },
            draft.movement,
            ship.movement_power_cap().unwrap_or(ship.power_available),
            ship.motion_cap()
        ),
        if mov_selected {
            selected_style()
        } else {
            Style::default()
        },
    )));

    let first_weapon = draft.weapons.first().and_then(|(id, charge)| {
        ship.weapons.iter().find(|weapon| &weapon.id == id).map(|weapon| {
            if weapon.operational {
                format!("{} charge {charge}/{}", weapon.id, weapon.max_charge)
            } else {
                format!("{} DESTROYED", weapon.id)
            }
        })
    });
    lines.push(Line::from(Span::styled(
        format!(
            " Spend preview: {} · Faces: F{}/{} FR{}/{} RR{}/{} R{}/{} RL{}/{} FL{}/{}",
            first_weapon.unwrap_or_else(|| "no weapon".to_string()),
            draft.shields[0],
            ship.shield_cap(0),
            draft.shields[1], ship.shield_cap(1),
            draft.shields[2], ship.shield_cap(2),
            draft.shields[3], ship.shield_cap(3),
            draft.shields[4], ship.shield_cap(4),
            draft.shields[5], ship.shield_cap(5)
        ),
        Style::default().fg(Color::DarkGray),
    )));
    if let Some(notice) = &app.input_notice {
        lines.push(Line::from(Span::styled(
            format!(" Input: {notice}"),
            Style::default().fg(Color::Yellow),
        )));
    }
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
            format!(" · carried {cur}; add {} for ready {}/{}", chg.saturating_sub(cur), chg, max)
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
            let state = if *chg == 0 { " UNCHARGED" } else { "" };
            format!("{mark}{id} ({kind}) charge {chg}/{max}{state}{carried}")
        } else {
            // Non-operational weapon: display-only, no editable charge prompt.
            format!("{mark}{id} ({kind}) DESTROYED")
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
        let cap = ship.shield_cap(i);
        let lab = shield_label(i as u32);
        if sel_face == Some(i) {
            format!("[{lab}{v}/{cap}]")
        } else {
            format!(" {lab}{v}/{cap} ")
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
                "{mark}{name}: {}/{}",
                draft.shields.get(i).copied().unwrap_or(0),
                ship.shield_cap(i)
            ),
            if selected {
                selected_style()
            } else {
                Style::default()
            },
        )));
    }

    if draft.cursor > 0 && draft.cursor <= draft.weapons.len() {
        let index = draft.cursor - 1;
        let (id, _) = &draft.weapons[index];
        let kind = ship
            .weapons
            .iter()
            .find(|weapon| &weapon.id == id)
            .map(|weapon| weapon.kind.as_str())
            .unwrap_or("?");
        lines.push(Line::from(Span::styled(
            format!(" Selected weapon {}/{}: {id} ({kind})", index + 1, draft.weapons.len()),
            Style::default().fg(Color::Cyan),
        )));
    }

    ("Allocate", lines)
}

/// Short display token for one path action.
/// Draft-path token, labelled by **on-screen direction**. The engine's
/// facing-increasing actions (`move_fl`/`turn_left`) rotate counterclockwise —
/// to the on-screen LEFT on the `r↓` map — so they render as the left tokens.
/// (See `input::handle_movement`'s direction note.)
fn action_token(action: &str) -> &'static str {
    match action {
        "move_f" => "F",
        "move_fl" => "FL", // veers to the on-screen left (port)
        "move_fr" => "FR", // veers to the on-screen right (starboard)
        "turn_left" => "◄",  // rotates nose left on screen
        "turn_right" => "►", // rotates nose right on screen
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
    let path_cost = draft_actions.len() as u32;
    let evasive = app.path_draft.as_ref().map(|d| d.evasive).unwrap_or(0);
    let cost = path_cost.saturating_add(evasive);

    let mut lines = vec![Line::from(format!(
        " {} @({},{}) face={}{}",
        callsign(ship),
        ship.q,
        ship.r,
        ship.facing,
        facing_arrow(ship.facing),
    ))];

    // Drafted path as a row of short tokens.
    let following = app.path_draft.as_ref().is_some_and(|d| d.follow);
    let path_tokens = if following {
        "(follow leader)".to_string()
    } else if draft_actions.is_empty() {
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

    // Running motion cost vs available (path + evasive share the budget).
    let over = cost > ship.motion_available;
    if ship.motion_available == 0 {
        lines.push(Line::from(Span::styled(
            " No motion this turn — put power on Movement in allocate, then Enter.",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )));
    }
    lines.push(Line::from(Span::styled(
        format!(
            " motion {cost}/{} ({path_cost} path, {evasive} evasive)",
            ship.motion_available
        ),
        if over {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        },
    )));
    if ship.motion_cap() < ship.max_maneuver_actions {
        lines.push(Line::from(Span::styled(
            format!(
                " engine loss: {}/{} maneuver capacity",
                ship.motion_cap(),
                ship.max_maneuver_actions
            ),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )));
    }

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
        " w/↑ forward · a veer-left · d veer-right · ←/→ turn · e evasive · y follow",
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
    let has_usable_weapon = ship.weapons.iter().any(|w| {
        w.operational && !w.is_pd() && w.charge > 0 && w.ammo_remaining != Some(0)
    });
    let mut lines = vec![
        Line::from(Span::styled(
            " ↑↓ weapon · 1–9 target · ←→ shield face",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(Span::styled(
            if has_usable_weapon {
                " Enter queue/unqueue · Backspace remove · Space fire volley"
            } else {
                " Enter unavailable · Backspace remove · Space pass"
            },
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

    // Coach: if no weapon can make a legal firing attempt,
    // mid-fire-phase, tell the player Space passes instead of firing.
    if !has_usable_weapon && snap.phase == "firing" {
        lines.push(Line::from(Span::styled(
            " No usable weapons — Space passes",
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
                " {marker} {} {} d={} face={}{} size={}",
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
    lines.push(Line::from(Span::styled(
        format!(
            " Weapons: selected {}/{} {} ({})",
            draft.weapon_idx + 1,
            ship.weapons.len(),
            ship.weapons
                .get(draft.weapon_idx)
                .map(|w| w.id.as_str())
                .unwrap_or("—"),
            ship.weapons
                .get(draft.weapon_idx)
                .map(|w| w.kind.as_str())
                .unwrap_or("?")
        ),
        Style::default().fg(Color::Cyan),
    )));
    for (i, w) in ship.weapons.iter().enumerate() {
        let selected = i == draft.weapon_idx;
        let marker = if selected { "▶" } else { " " };
        let queued = draft.is_queued(&w.id);
        let queued_str = if queued { " [QUEUED]" } else { "" };
        let charge_str = format!(
            "chg={}/{}{}",
            w.charge,
            w.max_charge,
            if !w.operational {
                " DESTROYED"
            } else if w.charge == 0 {
                " UNCHARGED"
            } else {
                ""
            }
        );
        let style = if selected {
            selected_style()
        } else if !w.operational {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
        };
        let quality = match (w.accuracy_bonus, w.damage_bonus) {
            (0, 0) => String::new(),
            (acc, 0) => format!(" acc+{acc}"),
            (0, dmg) => format!(" dmg+{dmg}"),
            (acc, dmg) => format!(" acc+{acc} dmg+{dmg}"),
        };
        let tags = w.tags();
        let tag = if tags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", tags.join(","))
        };
        let pd = if w.is_pd() { " auto" } else { "" };
        let preview_issue = if selected {
            let target = draft.target.or_else(|| {
                snap.ships
                    .iter()
                    .find(|s| s.controller != "player" && !s.destroyed)
                    .map(|s| s.id)
            });
            target
                .and_then(|target| app.matching_fire_preview(ship.id, &w.id, target))
                .and_then(|preview| {
                    if !preview.legal
                        && preview
                            .reason
                            .as_deref()
                            .is_some_and(|reason| reason.to_ascii_lowercase().contains("bear"))
                    {
                        Some(" NO ARC")
                    } else {
                        None
                    }
                })
                .unwrap_or("")
        } else {
            ""
        };
        lines.push(Line::from(Span::styled(
            format!(
                " {marker} {} {} rng≤{} {}{}{}{}{}{}",
                w.id, w.kind, w.max_range, charge_str, preview_issue, quality, tag, pd, queued_str
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
        format!("Combat Log · volley {} · sh=shields", app.recent_events.len())
    } else {
        "Combat Log · sh=shields · int=hull".to_string()
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
                let style = if e.contains("HIT") || e.contains("INTERCEPT") || e.contains("GRAV") {
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
    let base = format!("View origin ({oq},{or_})");
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
            .map(|s| format!("Turn {} · {}", s.turn, phase_label(&s.phase)))
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

/// Whether the focused player ship has no action available and no repair
/// choice worth presenting. This is presentation state derived from the
/// authoritative snapshot; orders still use the engine's normal empty forms.
pub(crate) fn is_disabled_ship(app: &App) -> bool {
    let Some(ship) = app.focused() else {
        return false;
    };
    let repairable = ship.repair_cap.is_some_and(|cap| cap > 0) && ship.has_system("repair");
    let usable_weapon = ship.weapons.iter().any(|weapon| {
        weapon.operational
            && !weapon.is_pd()
            && weapon.charge > 0
            && weapon.ammo_remaining != Some(0)
    });
    ship.controller == "player"
        && !ship.destroyed
        && ship.power_available == 0
        && ship.motion_cap() == 0
        && !usable_weapon
        && !repairable
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
                "No legal shot · Space pass".into()
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

fn render_yard(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(yard) = app.yard.as_mut() else {
        return;
    };
    match yard.screen {
        YardScreen::Browse => render_yard_browse(f, yard, area),
        YardScreen::Edit => render_yard_edit(f, yard, area),
    }
    if yard.picker.is_some() {
        render_yard_picker(f, yard, area);
    }
    if yard.shield_editor.is_some() {
        render_yard_shields(f, yard, area);
    }
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn render_yard_picker(f: &mut Frame, yard: &crate::yard::YardState, area: Rect) {
    let Some(picker) = yard.picker.as_ref() else { return };
    let visible = usize::from(area.height.saturating_sub(8)).max(1);
    let offset = crate::yard::clamp_scroll(picker.scroll, picker.cursor, picker.rows.len(), visible);
    let height = (visible as u16 + 8).min(area.height.saturating_sub(2));
    let rect = centered_rect(area, area.width.saturating_sub(4).min(78), height);
    // Size the table to the actual modal, not the normal 80-column layout.
    // This keeps the required decision columns on one header row at the yard
    // floor while allowing optional ammo/tags columns to compress first.
    let widths = crate::yard::column_widths(
        &picker.headers,
        &picker.rows,
        usize::from(rect.width.saturating_sub(2)),
    );
    f.render_widget(Clear, rect);
    let mut lines = Vec::new();
    let header = picker.headers.iter().enumerate().map(|(i, header)| format!("{:<width$}", header, width = widths.get(i).copied().unwrap_or(header.len()))).collect::<Vec<_>>().join("  ");
    lines.push(Line::from(header));
    for (row_index, row) in picker.rows.iter().enumerate().skip(offset).take(visible) {
        let cells = row.cells.iter().enumerate().map(|(i, cell)| {
            let width = widths.get(i).copied().unwrap_or(8);
            let clipped: String = cell.chars().take(width).collect();
            format!("{:<width$}", clipped, width = width)
        }).collect::<Vec<_>>().join("  ");
        let marker = if row_index == picker.cursor { "▶ " } else { "  " };
        let suffix = if row.fits { "" } else { " (no space)" };
        let mut line = Line::from(format!("{marker}{cells}{suffix}"));
        if !row.fits { line = line.style(Style::default().fg(Color::DarkGray)); }
        lines.push(line);
    }
    lines.push(Line::from(""));
    let delta_width = usize::from(rect.width.saturating_sub(4));
    let delta: String = yard
        .picker_delta_line()
        .chars()
        .take(delta_width)
        .collect();
    lines.push(Line::from(delta));
    lines.push(Line::from("↑↓ browse  Enter take  Esc cancel  / filter  m mount"));
    let block = Block::default().borders(Borders::ALL).title(format!(" {} ", picker.title));
    // Table rows are already width-clipped cell by cell. Wrapping them would
    // consume the modal's scarce 60x16 height and push `/ filter` off-screen.
    f.render_widget(Paragraph::new(lines).block(block), rect);
}

fn render_yard_shields(f: &mut Frame, yard: &crate::yard::YardState, area: Rect) {
    let selected = yard.shield_editor.unwrap_or(0);
    let total: u64 = yard.draft.shields.iter().sum();
    let labels = crate::protocol::SHIELD_LABELS;
    let lines = vec![Line::from(""), Line::from(format!("                 {}  {}", labels[0], shield_value(yard, 0, selected))), Line::from(format!("          {}  {}       {}  {}", labels[5], shield_value(yard, 5, selected), labels[1], shield_value(yard, 1, selected))), Line::from("                ┌────┐"), Line::from(format!("          {}  {} │hull│ {}  {}", labels[4], shield_value(yard, 4, selected), labels[2], shield_value(yard, 2, selected))), Line::from("                └────┘"), Line::from(format!("                 {}  {}", labels[3], shield_value(yard, 3, selected))), Line::from(""), Line::from("←→ face   ↑↓ ±1   PgUp/PgDn ±5   = set all"), Line::from(format!("Esc done                           {total} banks"))];
    let rect = centered_rect(area, 54, 14);
    f.render_widget(Clear, rect);
    f.render_widget(Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(format!(" shields · {total} banks · {total}sp {total}c "))), rect);
}

fn shield_value(yard: &crate::yard::YardState, index: usize, selected: usize) -> String {
    if index == selected { format!("▶{}◀", yard.draft.shields[index]) } else { yard.draft.shields[index].to_string() }
}

fn render_yard_browse(f: &mut Frame, yard: &crate::yard::YardState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(4)])
        .split(area);
    let mut items: Vec<ListItem> = yard
        .listings
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let cost = item
                .preview
                .as_ref()
                .map(|p| format!("{:>5}", p.cost))
                .unwrap_or_else(|_| "  err".into());
            let marker = if i == yard.browse_cursor { "▶" } else { " " };
            let standard = if shipsim_core::shipyard::STANDARD_CLASS_IDS.contains(&item.design.id.as_str()) { "[std]" } else { "     " };
            let hull = yard
                .sizes
                .get(item.design.size)
                .map(|h| h.name.as_str())
                .unwrap_or("?");
            ListItem::new(format!(
                "{marker} {standard} {:<20} {:<10} w{:>2} sp{:>3}/{:<3} cost {cost}",
                item.design.name, hull
                , item.design.weapons.len(), item.preview.as_ref().map(|p| p.space_used).unwrap_or(0), item.preview.as_ref().map(|p| p.space_cap).unwrap_or(0)
            ))
        })
        .collect();
    let new_mark = if yard.is_new_row() { "▶" } else { " " };
    items.push(ListItem::new(format!(
        "{new_mark} + new ship                                  create a hull"
    )));
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(
                " Shipyard — standards by size · user sort: {} (o) ",
                yard.sort_mode.label()
            )),
    );
    f.render_widget(list, chunks[0]);
    let help = Paragraph::new(format!(
        "↑↓ select  Enter edit  n new  y clone  d delete  o sort  q quit\n{}",
        yard.status
    ))
    .wrap(Wrap { trim: true })
    .block(Block::default().borders(Borders::ALL).title(" keys "));
    f.render_widget(help, chunks[1]);
}

fn render_yard_edit(f: &mut Frame, yard: &mut crate::yard::YardState, area: Rect) {
    let preview = yard.preview();
    let cost_line = match &preview {
        Ok(p) => format!(
            "cost {}   space {}/{}   power {}   structure {}   shields {}",
            p.cost,
            p.space_used,
            p.space_cap,
            p.power,
            p.structure,
            p.shield_faces
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join("-")
        ),
        Err(err) => format!("invalid: {err}"),
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(8), Constraint::Length(4)])
        .split(area);
    let mode = if yard.viewing_readonly { "  (read-only — standard class)" } else { "" };
    let header = Paragraph::new(format!("{}{}\n{cost_line}", yard.draft.name, mode))
        .wrap(Wrap { trim: true })
        .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" class "),
        );
    f.render_widget(header, chunks[0]);

    let rows = yard.edit_rows();
    let inner_height = usize::from(chunks[1].height.saturating_sub(2));
    let cursor_row = rows
        .iter()
        .position(|row| matches!(&row.kind, crate::yard::YardRowKind::Field(field) if *field == yard.edit_cursor))
        .unwrap_or(0);
    yard.edit_scroll = crate::yard::clamp_scroll(yard.edit_scroll, cursor_row, rows.len(), inner_height);
    let lines: Vec<Line> = rows.iter().map(|row| match &row.kind {
        crate::yard::YardRowKind::Field(field) => yard_field(*field == yard.edit_cursor, row.text.clone()),
        _ => Line::from(row.text.clone()),
    }).collect();
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(52), Constraint::Length(28)])
        .split(chunks[1]);
    f.render_widget(
        Paragraph::new(lines)
            .scroll((yard.edit_scroll as u16, 0))
            .block(Block::default().borders(Borders::ALL).title(format!(
                " components {}-{}/{} ",
                yard.edit_scroll + 1,
                (yard.edit_scroll + inner_height).min(rows.len()),
                rows.len()
            ))),
        columns[0],
    );
    f.render_widget(
        Paragraph::new(yard.field_description())
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title(" inspector ")),
        columns[1],
    );
    let help = Paragraph::new(format!(
        "↑↓ move  Enter change  a weapon  i system  d delete  s save  c compile  Esc back\n{}",
        yard.status
    ))
    .wrap(Wrap { trim: true })
    .block(Block::default().borders(Borders::ALL).title(" keys "));
    f.render_widget(help, chunks[2]);
}

fn yard_field(selected: bool, text: String) -> Line<'static> {
    let marker = if selected { "▶ " } else { "  " };
    let style = if selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    Line::from(Span::styled(format!("{marker}{text}"), style))
}
