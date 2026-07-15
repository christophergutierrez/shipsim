//! shipsim TUI — ratatui terminal client (Small tier).
//!
//! Spawns the `shipsim` engine as a subprocess, drives it over NDJSON, and
//! renders game state with ratatui. This is a thin client: it never
//! recomputes rules, only reads snapshots and builds orders.

mod app;
mod harness;
mod input;
mod protocol;
mod tutorial;
mod ui;

use std::time::Duration;

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
    // Parse args: --tutorial flag, then optional scenario path.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let tutorial_mode = args.iter().any(|a| a == "--tutorial");
    let scenario = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| {
            if tutorial_mode {
                "scenarios/tutorial_rear_attack.toml".to_string()
            } else {
                "scenarios/ai.toml".to_string()
            }
        });

    let engine_path =
        std::env::var("SHIPSIM_BIN").unwrap_or_else(|_| "target/debug/shipsim".to_string());

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
    if let Some(line) = harness.read_line() {
        apply_engine_line(&mut app, line);
    }

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

    result
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    harness: &mut Harness,
) -> std::io::Result<()> {
    loop {
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
                let json = order.to_json();
                if let Err(e) = harness.send(&json) {
                    app.log(format!("send error: {e}"));
                }
                // Read the engine's response (may be a snapshot or a soft error).
                if let Some(line) = harness.read_line() {
                    apply_engine_line(app, line);
                }
            }
            KeyResult::Continue => {}
        }
    }
}

/// Apply one engine line (snapshot or error) to app state.
fn apply_engine_line(app: &mut App, line: EngineLine) {
    match line {
        EngineLine::Snapshot(s) => app.update_snapshot(s),
        EngineLine::Error(e) => app.record_error(&e),
        EngineLine::Raw(r) => app.log(format!("engine: {r}")),
    }
}

#[cfg(test)]
mod tests;
