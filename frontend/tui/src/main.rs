//! shipsim TUI — ratatui terminal client (Small tier).
//!
//! Spawns the `shipsim` engine as a subprocess, drives it over NDJSON, and
//! renders game state with ratatui. This is a thin client: it never
//! recomputes rules, only reads snapshots and builds orders.

mod app;
mod harness;
mod input;
mod protocol;
mod scripted_pump;
mod tutorial;
mod ui;
mod yard;

use std::path::PathBuf;
use std::io::IsTerminal;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::App;
use harness::{EngineLine, Harness};
use input::{handle_key, KeyResult};

fn main() -> std::io::Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        eprintln!("{}", no_tty_message());
        std::process::exit(1);
    }
    // Parse args: --tutorial / --yard, then optional scenario path.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let tutorial_mode = args.iter().any(|a| a == "--tutorial");
    let yard_mode = args.iter().any(|a| a == "--yard" || a == "yard");
    if yard_mode {
        return run_yard();
    }
    let scenario = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| {
            if tutorial_mode {
                "scenarios/tutorial_rear_attack.toml".to_string()
            } else {
                "scenarios/battle.toml".to_string()
            }
        });

    let engine_path =
        std::env::var("SHIPSIM_BIN").unwrap_or_else(|_| "target/debug/shipsim".to_string());

    if !std::path::Path::new(&scenario).is_file() {
        eprintln!("error: scenario not found: {scenario}");
        eprintln!("       run from the repo root; ship *classes* live in data/ships/,");
        eprintln!("       playable fights live in scenarios/ (e.g. scenarios/battle.toml).");
        std::process::exit(1);
    }

    // Spawn the engine and read the initial snapshot.
    let mut harness = match Harness::spawn(&engine_path, &scenario) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("error: cannot spawn engine '{engine_path}': {e}");
            eprintln!("       build it first:  cargo build");
            eprintln!("       or set SHIPSIM_BIN=/path/to/shipsim");
            std::process::exit(1);
        }
    };

    let mut app = if tutorial_mode {
        App::new_with_tutorial()
    } else {
        App::new()
    };
    match harness.read_line() {
        Some(line) => apply_engine_line(&mut app, line),
        None => {
            eprintln!("error: engine produced no snapshot for '{scenario}'.");
            eprintln!("       it exited before the first line — see any message above.");
            std::process::exit(1);
        }
    }
    if app.snap.is_none() {
        eprintln!("error: first engine line was not a snapshot (scenario may have failed to load).");
        if let Some(err) = &app.last_error {
            eprintln!("       {err}");
        }
        std::process::exit(1);
    }
    pump_scripted(&mut app, &mut harness);

    // Set up the terminal.
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Event loop.
    let result = run(&mut terminal, &mut app, &mut harness);

    // Restore the terminal regardless of outcome.
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if result.is_ok() {
        match write_session_log(&app) {
            Ok(path) => println!("Session log: {}", path.display()),
            Err(error) => eprintln!("warning: could not write session log: {error}"),
        }
    }

    result
}

pub(crate) fn no_tty_message() -> &'static str {
    "shipsim TUI needs an interactive terminal (a PTY)."
}

fn run_yard() -> std::io::Result<()> {
    let root = crate::yard::find_repo_root();
    if !crate::yard::repo_has_yard(&root) {
        eprintln!("error: no data/designs under {}", root.display());
        eprintln!("       run from the repo root, or set SHIPSIM_ROOT");
        std::process::exit(1);
    }
    let mut app = App::new();
    match crate::yard::YardState::load(root) {
        Ok(yard) => app.yard = Some(yard),
        Err(err) => {
            eprintln!("error: cannot open shipyard: {err}");
            std::process::exit(1);
        }
    }

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = (|| -> std::io::Result<()> {
        loop {
            terminal.draw(|f| ui::render(f, &mut app))?;
            if !event::poll(Duration::from_millis(100))? {
                continue;
            }
            let Event::Key(key) = event::read()? else {
                continue;
            };
            match handle_key(&mut app, key) {
                KeyResult::Quit => return Ok(()),
                KeyResult::Continue | KeyResult::SendOrder(_) => {}
            }
        }
    })();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn write_session_log(app: &App) -> std::io::Result<PathBuf> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local");
    std::fs::create_dir_all(&directory)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = directory.join(format!("session-{stamp}-{}.log", std::process::id()));
    std::fs::write(&path, session_log_contents(app))?;
    Ok(path)
}

fn session_log_contents(app: &App) -> String {
    let mut out = String::from("shipsim TUI session\n");
    if let Some(snapshot) = &app.snap {
        out.push_str(&format!(
            "turn={} phase={} status={}\n",
            snapshot.turn, snapshot.phase, snapshot.status
        ));
    }
    if let Some(error) = &app.last_error {
        out.push_str(&format!("last_error={error}\n"));
    }
    out.push_str("\nCombat history:\n");
    for event in &app.combat_history {
        out.push_str(event);
        out.push('\n');
    }
    out.push_str("\nCommand log:\n");
    for line in &app.log {
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    harness: &mut Harness,
) -> std::io::Result<()> {
    loop {
        drain_pending_previews(app, harness);
        pump_scripted(app, harness);
        terminal.draw(|f| ui::render(f, app))?;

        // Poll for input with a short timeout so we can also drain engine
        // output if it becomes available.
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };

        match handle_key(app, key) {
            KeyResult::Quit => return Ok(()),
            KeyResult::SendOrder(order) => {
                app.clear_resolution_summary();
                let json = order.to_json();
                if let Err(e) = harness.send(&json) {
                    app.log(format!("send error: {e}"));
                }
                // Read the engine's response (may be a snapshot or a soft error).
                match harness.read_line() {
                    Some(line) => apply_engine_line(app, line),
                    None => {
                        app.engine_dead = true;
                        app.log("engine exited");
                    }
                }
                pump_disabled_autopass(app, harness);
                pump_scripted(app, harness);
            }
            KeyResult::Continue => {}
        }
    }
}

/// Continue a disabled ship through the engine's ordinary empty orders after
/// the player explicitly chose Space. This never fabricates a special order
/// or advances another player ship.
fn pump_disabled_autopass(app: &mut App, harness: &mut Harness) {
    for _ in 0..8 {
        let Some(ship_id) = app.disabled_autopass else {
            return;
        };
        let Some(start_turn) = app.disabled_autopass_turn else {
            app.clear_disabled_autopass();
            return;
        };
        let Some(snap) = app.snap.as_ref() else {
            app.clear_disabled_autopass();
            return;
        };
        if snap.turn != start_turn {
            app.disabled_pass_notice = Some(format!(
                "Disabled turn {start_turn} passed. Space passes turn {}.",
                snap.turn
            ));
            app.clear_disabled_autopass();
            return;
        }
        if app.focused_ship != Some(ship_id)
            || !crate::ui::is_disabled_ship(app)
            || snap.is_over()
        {
            app.clear_disabled_autopass();
            return;
        }
        let already_committed = match snap.phase.as_str() {
            "allocate" => snap.ships_allocated_this_turn.contains(&ship_id),
            "movement" => snap.ships_committed_path.contains(&ship_id),
            "firing" => snap.ships_committed_volley.contains(&ship_id),
            _ => {
                app.clear_disabled_autopass();
                return;
            }
        };
        if already_committed {
            return;
        }
        let order = match snap.phase.as_str() {
            "allocate" => crate::protocol::Order::allocate_full(
                ship_id,
                0,
                serde_json::Value::Object(serde_json::Map::new()),
                vec![0; 6],
                false,
                0,
                false,
                None,
            ),
            "movement" => crate::protocol::Order::commit_path(ship_id, Vec::new()),
            "firing" => crate::protocol::Order::commit_volley(ship_id, Vec::new()),
            _ => unreachable!(),
        };
        if harness.send(&order.to_json()).is_err() {
            app.engine_dead = true;
            app.clear_disabled_autopass();
            return;
        }
        match harness.read_line() {
            Some(line) => apply_engine_line(app, line),
            None => {
                app.engine_dead = true;
                app.clear_disabled_autopass();
                return;
            }
        }
        if app.last_error.is_some() {
            app.clear_disabled_autopass();
            return;
        }
    }
}

/// Drive scripted ships until the current stage needs a player action or makes
/// no further progress. Bounded to avoid an infinite retry loop on rejection.
fn pump_scripted(app: &mut App, harness: &mut Harness) {
    for _ in 0..64 {
        let Some(snap) = app.snap.as_ref() else {
            return;
        };
        let before = (snap.turn, snap.phase.clone(), pending_scripted_count(snap));
        let orders = crate::scripted_pump::plan_scripted_orders(snap);
        if orders.is_empty() {
            return;
        }
        for order in orders {
            if harness.send(&order.to_json()).is_err() {
                return;
            }
            match harness.read_line() {
                Some(line) => apply_engine_line(app, line),
                None => {
                    app.engine_dead = true;
                    return;
                }
            }
            // Stop immediately on a rejected order (avoids retry loop).
            if app.last_error.is_some() {
                return;
            }
        }
        // No-progress guard: if the batch didn't advance turn/phase and didn't
        // shrink the pending-scripted set, stop.
        let after = app
            .snap
            .as_ref()
            .map(|s| (s.turn, s.phase.clone(), pending_scripted_count(s)));
        if after.as_ref() == Some(&before) {
            return;
        }
    }
}

fn pending_scripted_count(snap: &crate::protocol::Snapshot) -> usize {
    let done: &[i64] = match snap.phase.as_str() {
        "allocate" => &snap.ships_allocated_this_turn,
        "movement" => &snap.ships_committed_path,
        "firing" => &snap.ships_committed_volley,
        _ => return 0,
    };
    snap.ships
        .iter()
        .filter(|s| !s.destroyed && s.controller == "scripted" && !done.contains(&s.id))
        .count()
}

/// Send queued previews before drawing so allocation opens with the coast
/// endpoint already visible, rather than waiting for an unrelated keypress.
fn drain_pending_previews(app: &mut App, harness: &mut Harness) {
    let requests = [
        app.pending_path_preview.take(),
        app.pending_fire_preview.take(),
    ];
    for request in requests.into_iter().flatten() {
        if harness.send(&request).is_ok() {
            if let Some(line) = harness.read_line() {
                apply_engine_line(app, line);
            }
        }
    }
}

/// Apply one engine line (snapshot, movement preview, or error) to app state.
fn apply_engine_line(app: &mut App, line: EngineLine) {
    match line {
        EngineLine::Snapshot(s) => app.update_snapshot(s),
        EngineLine::PathPreview(p) => app.accept_path_preview(p),
        EngineLine::FirePreview(p) => app.accept_fire_preview(p),
        EngineLine::Error(e) => app.record_error(&e),
        EngineLine::Raw(r) => app.log(format!("engine: {r}")),
    }
}

#[cfg(test)]
mod tests;
