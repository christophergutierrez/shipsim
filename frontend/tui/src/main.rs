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
mod transport;
mod tutorial;
mod ui;
mod yard;

use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
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
use shipsim_core::session_protocol::{
    ClientKind, SessionMessage, SESSION_PROTOCOL_VERSION, SUPPORTED_GAME_PROTOCOL_VERSIONS,
    SUPPORTED_SESSION_PROTOCOL_VERSIONS,
};

fn main() -> std::io::Result<()> {
    // Parse args: --tutorial / --yard, then optional scenario path.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let tutorial_mode = args.iter().any(|a| a == "--tutorial");
    let yard_mode = args.iter().any(|a| a == "--yard" || a == "yard");
    if yard_mode {
        return run_yard();
    }
    let connect = args
        .iter()
        .find_map(|a| a.strip_prefix("--connect=").map(str::to_owned))
        .or_else(|| {
            args.iter()
                .position(|a| a == "--connect")
                .and_then(|i| args.get(i + 1).cloned())
        });
    let network = connect.is_some();
    let join_token_stdin = args.iter().any(|a| a == "--join-token-stdin");
    let mut join_token = String::new();
    if join_token_stdin {
        std::io::stdin().read_to_string(&mut join_token)?;
        join_token = join_token.trim().into();
        reopen_controlling_tty()?;
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        eprintln!("{}", no_tty_message());
        std::process::exit(1);
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

    if !network && !std::path::Path::new(&scenario).is_file() {
        eprintln!("error: scenario not found: {scenario}");
        eprintln!("       run from the repo root; ship *classes* live in data/ships/,");
        eprintln!("       playable fights live in scenarios/ (e.g. scenarios/battle.toml).");
        std::process::exit(1);
    }

    // Spawn the local engine or connect to a session server.
    let harness_result = match connect.as_deref() {
        Some(address) => Harness::connect(address),
        None => Harness::spawn(&engine_path, &scenario),
    };
    let mut harness = match harness_result {
        Ok(h) => h,
        Err(e) => {
            eprintln!("error: cannot spawn engine '{engine_path}': {e}");
            eprintln!("       build it first:  cargo build");
            eprintln!("       or set SHIPSIM_BIN=/path/to/shipsim");
            std::process::exit(1);
        }
    };

    let mut app = if network {
        let mut app = App::new_network(connect.clone().unwrap_or_default());
        if let Some(lobby) = app.lobby.as_mut() {
            lobby.join_token = join_token;
        }
        app
    } else if tutorial_mode {
        App::new_with_tutorial()
    } else {
        App::new()
    };
    if network {
        let profiles = discover_agent_profiles();
        if let Some(lobby) = app.lobby.as_mut() {
            lobby.agent_profiles = profiles;
        }
    }
    if network {
        let hello = SessionMessage::Hello {
            session_protocol_version: SESSION_PROTOCOL_VERSION,
            client_kind: ClientKind::Tui,
            display_name: "TUI player".into(),
            supported_session_versions: SUPPORTED_SESSION_PROTOCOL_VERSIONS.to_vec(),
            supported_game_protocol_versions: SUPPORTED_GAME_PROTOCOL_VERSIONS.to_vec(),
        };
        harness.send_session(&hello)?;
    } else {
        match harness.read_line() {
            Some(line) => apply_engine_line(&mut app, line),
            None => {
                eprintln!("error: engine produced no snapshot for '{scenario}'.");
                eprintln!("       it exited before the first line — see any message above.");
                std::process::exit(1);
            }
        }
    }
    if !network && app.snap.is_none() {
        eprintln!(
            "error: first engine line was not a snapshot (scenario may have failed to load)."
        );
        if let Some(err) = &app.last_error {
            eprintln!("       {err}");
        }
        std::process::exit(1);
    }
    if !network {
        pump_scripted(&mut app, &mut harness);
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

    if result.is_ok() {
        match write_session_log(&app) {
            Ok(path) => println!("Session log: {}", path.display()),
            Err(error) => eprintln!("warning: could not write session log: {error}"),
        }
    }

    result
}

/// `--join-token-stdin` consumes a pipe for the secret, then restores the
/// controlling terminal as stdin so crossterm can continue receiving keys.
#[cfg(unix)]
fn reopen_controlling_tty() -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    let tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")?;
    let result = unsafe { libc::dup2(tty.as_raw_fd(), libc::STDIN_FILENO) };
    if result < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(unix))]
fn reopen_controlling_tty() -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "--join-token-stdin requires a controlling terminal on this platform",
    ))
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
                KeyResult::Continue | KeyResult::SendOrder(_) | KeyResult::SendSession(_) => {}
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
        out.push_str(&format!("last_error={}\n", harness::redact_for_log(error)));
    }
    out.push_str("\nCombat history:\n");
    for event in &app.combat_history {
        out.push_str(event);
        out.push('\n');
    }
    out.push_str("\nCommand log:\n");
    for line in &app.log {
        out.push_str(&harness::redact_for_log(line));
        out.push('\n');
    }
    out
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    harness: &mut Harness,
) -> std::io::Result<()> {
    let mut agent = AgentProcess::default();
    loop {
        drain_transport(app, harness);
        agent.maybe_spawn(app);
        agent.refresh_status(app);
        drain_pending_previews(app, harness);
        if !harness.is_network() {
            pump_scripted(app, harness);
        }
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
                if harness.is_network() {
                    drain_transport(app, harness);
                } else {
                    match harness.read_line() {
                        Some(line) => apply_engine_line(app, line),
                        None => {
                            app.engine_dead = true;
                            app.log("engine exited");
                        }
                    }
                }
                if !harness.is_network() {
                    pump_disabled_autopass(app, harness);
                    pump_scripted(app, harness);
                }
            }
            KeyResult::SendSession(message) => {
                if let Err(error) = harness.send_session(&message) {
                    if let Some(lobby) = app.lobby.as_mut() {
                        lobby.error = Some(format!("send failed: {error}"));
                        lobby.screen = app::LobbyScreen::Error;
                    }
                } else if let Some(lobby) = app.lobby.as_mut() {
                    lobby.screen = app::LobbyScreen::Waiting;
                }
            }
            KeyResult::Continue => {}
        }
    }
}

#[derive(Default)]
struct AgentProcess {
    child: Option<Child>,
}

impl AgentProcess {
    fn maybe_spawn(&mut self, app: &mut App) {
        let Some(lobby) = app.lobby.as_mut() else { return };
        if !matches!(
            lobby.opponent,
            shipsim_core::session_protocol::ControllerSpec::LlmAgent {}
        ) || self.child.is_some()
        {
            return;
        }
        let Some(token) = lobby.invitation_token.clone() else { return };
        let Some(profile) = lobby.selected_agent_profile().cloned() else {
            lobby.agent_status = Some("error: no agent profile available".into());
            return;
        };
        let Some(address) = (!lobby.address.is_empty()).then(|| lobby.address.clone()) else {
            return;
        };
        let executable = resolve_agent_binary();
        let mut command = Command::new(&executable);
        command.args(agent_argv(&address, &profile.name))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        match command.spawn() {
            Ok(mut child) => {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = writeln!(stdin, "{token}");
                }
                lobby.agent_status = Some(format!("starting {}", profile.name));
                self.child = Some(child);
                // Do not retain the token after handing it to the child.
                lobby.invitation_token = None;
                lobby.invitation = None;
            }
            Err(error) => {
                lobby.agent_status = Some(format!("error: unable to start agent ({error})"));
            }
        }
    }

    fn refresh_status(&mut self, app: &mut App) {
        let Some(child) = self.child.as_mut() else { return };
        match child.try_wait() {
            Ok(Some(status)) => {
                let lobby = if let Some(lobby) = app.lobby.as_mut() {
                    Some(lobby)
                } else {
                    app.lobby_history.as_mut()
                };
                if let Some(lobby) = lobby {
                    lobby.agent_status = Some(if status.success() {
                        "agent exited".into()
                    } else {
                        format!("agent exited ({status})")
                    });
                }
                self.child = None;
            }
            Ok(None) => {}
            Err(error) => {
                let lobby = if let Some(lobby) = app.lobby.as_mut() {
                    Some(lobby)
                } else {
                    app.lobby_history.as_mut()
                };
                if let Some(lobby) = lobby {
                    lobby.agent_status = Some(format!("agent status error: {error}"));
                }
            }
        }
    }
}

impl Drop for AgentProcess {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn resolve_agent_binary() -> PathBuf {
    if let Ok(path) = std::env::var("SHIPSIM_AGENT_BIN") {
        return PathBuf::from(path);
    }
    let repo_default = crate::yard::find_repo_root().join("frontend/agent/shipsim-agent");
    if repo_default.is_file() {
        return repo_default;
    }
    PathBuf::from("shipsim-agent")
}

pub(crate) fn agent_argv(address: &str, profile: &str) -> Vec<String> {
    vec![
        "play".into(),
        "--connect".into(),
        address.into(),
        "--profile".into(),
        profile.into(),
        "--join-token-stdin".into(),
    ]
}

fn discover_agent_profiles() -> Vec<app::AgentProfile> {
    let executable = resolve_agent_binary();
    let output = Command::new(executable)
        .arg("profiles")
        .arg("--json")
        .stderr(Stdio::null())
        .output();
    let Ok(output) = output else { return Vec::new() };
    if !output.status.success() {
        return Vec::new();
    }
    let Ok(values) = serde_json::from_slice::<Vec<serde_json::Value>>(&output.stdout) else {
        return Vec::new();
    };
    values
        .into_iter()
        .filter_map(|value| {
            Some(app::AgentProfile {
                name: value.get("name")?.as_str()?.to_owned(),
                kind: value.get("kind").and_then(|v| v.as_str()).unwrap_or("unknown").into(),
                model: value.get("model").and_then(|v| v.as_str()).unwrap_or("").into(),
            })
        })
        .collect()
}

fn drain_transport(app: &mut App, harness: &mut Harness) {
    for line in harness.try_read_lines() {
        match line {
            EngineLine::Session(message) => app.apply_session_message(message),
            EngineLine::Eof => {
                app.engine_dead = true;
                if let Some(lobby) = app.lobby.as_mut() {
                    lobby.error = Some("Server disconnected; press q to quit".into());
                    lobby.screen = app::LobbyScreen::Error;
                } else {
                    app.last_error = Some(
                        "Opponent disconnected; match ended. Press q to quit.".into(),
                    );
                }
            }
            other => apply_engine_line(app, other),
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
                "Disabled turn {start_turn} passed; no recovery. Space passes turn {}.",
                snap.turn
            ));
            app.clear_disabled_autopass();
            return;
        }
        if app.focused_ship != Some(ship_id) || !crate::ui::is_disabled_ship(app) || snap.is_over()
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
            if harness.is_network() {
                drain_transport(app, harness);
            } else if let Some(line) = harness.read_line() {
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
        EngineLine::Session(message) => app.apply_session_message(message),
        EngineLine::Eof => app.engine_dead = true,
        EngineLine::Raw(r) => app.log(format!("engine: {r}")),
    }
}

#[cfg(test)]
mod tests;
