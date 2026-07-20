use std::env;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use shipsim_core::campaign::Campaign;
use shipsim_core::game_state::{GameState, ScenarioStatus};
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::protocol::PROTOCOL_VERSION;
use shipsim_core::save::SaveDocument;
use shipsim_core::scenario::load_scenario;
use shipsim_core::snapshot::StateSnapshot;

enum OrderSource {
    File(PathBuf),
    Stdin,
}

enum Mode {
    Scenario(PathBuf),
    Campaign(PathBuf),
    Resume(PathBuf),
}

struct Args {
    mode: Mode,
    orders: OrderSource,
    save: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let args = parse_args(env::args().skip(1))?;

    match args.mode {
        Mode::Scenario(path) => {
            let mut game = load_scenario(&path).map_err(|e| e.to_string())?;
            // A scenario with no player-controlled ship is headless self-play
            // (e.g. AI-vs-AI duels). Driving it through the interactive harness
            // would fast-forward through every turn and confuse clients, so
            // refuse it up front with a clear error.
            if !game.has_player_ship() {
                return Err(format!(
                    "scenario {} has no player-controlled ship; it is headless self-play",
                    path.display()
                ));
            }
            // Let AI ships act before the first human order when the opening
            // phase is entirely NPC-driven (e.g. AI-only allocate). Persist
            // those orders so save/resume can replay barrier stages exactly.
            let mut orders = game.resolve_v2_npc_actions();
            // Post-load snapshot so a thin client can paint before any order (D8).
            emit_snapshot(&game)?;
            orders.extend(apply_orders(&mut game, &args.orders)?);
            if let Some(save_path) = args.save {
                SaveDocument::capture(path, orders, &game)
                    .write(&save_path)
                    .map_err(|error| error.to_string())?;
            }
        }
        Mode::Campaign(path) => {
            if args.save.is_some() {
                return Err("--save is not supported with --campaign".to_string());
            }
            let mut campaign = Campaign::load(&path).map_err(|e| e.to_string())?;
            loop {
                let mut game = campaign.load_current().map_err(|e| e.to_string())?;
                game.resolve_v2_npc_actions();
                emit_snapshot(&game)?;
                apply_orders(&mut game, &args.orders)?;
                if game.status() == ScenarioStatus::Won {
                    match campaign.advance_on_win(&game) {
                        Ok(true) => continue,
                        Ok(false) => break,
                        Err(shipsim_core::campaign::CampaignError::Complete) => break,
                        Err(e) => return Err(e.to_string()),
                    }
                } else {
                    break;
                }
            }
        }
        Mode::Resume(path) => {
            let mut document = SaveDocument::read(&path).map_err(|error| error.to_string())?;
            // Replay includes every barrier-stage AI order from the save; do not
            // invent additional NPC actions before continuing, or the stream
            // diverges from the checkpoint.
            let mut game = document.replay().map_err(|error| error.to_string())?;
            // Only auto-act for AI when the resumed state is still waiting on
            // NPC collection and the save did not already include those orders
            // (legacy safety). Prefer recorded orders on the happy path.
            let mut pending_npc = game.resolve_v2_npc_actions();
            emit_snapshot(&game)?;
            document.orders.append(&mut pending_npc);
            document
                .orders
                .extend(apply_orders(&mut game, &args.orders)?);
            document.update_from_checkpoint(&game);
            let save_path = args.save.unwrap_or(path);
            document
                .write(&save_path)
                .map_err(|error| error.to_string())?;
        }
    }

    Ok(())
}

fn emit_snapshot(game: &GameState) -> Result<(), String> {
    let snapshot = StateSnapshot::from_game_state(game);
    let mut out = io::stdout().lock();
    writeln!(
        out,
        "{}",
        serde_json::to_string(&snapshot).map_err(|error| error.to_string())?
    )
    .map_err(|error| error.to_string())?;
    out.flush().map_err(|error| error.to_string())?;
    Ok(())
}

/// Soft-reject line (ADR-0018). Process stays alive; state unchanged.
fn emit_error(
    code: &str,
    message: &str,
    order: Option<serde_json::Value>,
    source: &str,
) -> Result<(), String> {
    let mut body = serde_json::json!({
        "type": "error",
        "protocol_version": PROTOCOL_VERSION,
        "ok": false,
        "code": code,
        "message": message,
        "source": source,
    });
    if let Some(order) = order {
        body["order"] = order;
    }
    let mut out = io::stdout().lock();
    writeln!(
        out,
        "{}",
        serde_json::to_string(&body).map_err(|error| error.to_string())?
    )
    .map_err(|error| error.to_string())?;
    out.flush().map_err(|error| error.to_string())?;
    Ok(())
}

fn apply_orders(game: &mut GameState, orders: &OrderSource) -> Result<Vec<Order>, String> {
    let mut accepted = Vec::new();
    match orders {
        OrderSource::File(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|error| format!("cannot read orders {path:?}: {error}"))?;
            for line in text.lines() {
                if let Some(batch) = apply_order_line(game, line)? {
                    accepted.extend(batch);
                }
            }
        }
        OrderSource::Stdin => {
            for line in io::stdin().lock().lines() {
                let line = line.map_err(|error| format!("cannot read stdin: {error}"))?;
                if let Some(batch) = apply_order_line(game, &line)? {
                    accepted.extend(batch);
                }
            }
        }
    }
    Ok(accepted)
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut mode = None;
    let mut orders = None;
    let mut save = None;
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--scenario" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--scenario requires a path".to_string())?;
                if mode.replace(Mode::Scenario(PathBuf::from(value))).is_some() {
                    return Err("choose exactly one of --scenario or --campaign".to_string());
                }
            }
            "--campaign" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--campaign requires a path".to_string())?;
                if mode.replace(Mode::Campaign(PathBuf::from(value))).is_some() {
                    return Err("choose exactly one of --scenario or --campaign".to_string());
                }
            }
            "--resume" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--resume requires a path".to_string())?;
                if mode.replace(Mode::Resume(PathBuf::from(value))).is_some() {
                    return Err(
                        "choose exactly one of --scenario, --campaign, or --resume".to_string()
                    );
                }
            }
            "--save" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--save requires a path".to_string())?;
                if save.replace(PathBuf::from(value)).is_some() {
                    return Err("--save may only be provided once".to_string());
                }
            }
            "--orders" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--orders requires a path".to_string())?;
                if orders
                    .replace(OrderSource::File(PathBuf::from(value)))
                    .is_some()
                {
                    return Err("choose exactly one of --orders or --stdin".to_string());
                }
            }
            "--stdin" => {
                if orders.replace(OrderSource::Stdin).is_some() {
                    return Err("choose exactly one of --orders or --stdin".to_string());
                }
            }
            _ => return Err(format!("unknown argument {arg:?}")),
        }
    }

    Ok(Args {
        mode: mode.ok_or_else(|| "--scenario, --campaign, or --resume is required".to_string())?,
        orders: orders.ok_or_else(|| "choose exactly one of --orders or --stdin".to_string())?,
        save,
    })
}

fn apply_order_line(game: &mut GameState, line: &str) -> Result<Option<Vec<Order>>, String> {
    if line.trim().is_empty() {
        return Ok(None);
    }

    let order_val: serde_json::Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => {
            emit_error(
                "parse_error",
                &format!("cannot parse order: {error}"),
                None,
                "harness",
            )?;
            return Ok(None);
        }
    };
    let version = order_val
        .get("protocol_version")
        .and_then(serde_json::Value::as_u64);
    if version != Some(u64::from(PROTOCOL_VERSION)) {
        emit_error(
            "unsupported_protocol",
            &format!(
                "order protocol_version must be {PROTOCOL_VERSION}, got {}",
                version.map_or_else(|| "missing".to_string(), |value| value.to_string())
            ),
            Some(order_val),
            "harness",
        )?;
        return Ok(None);
    }

    // Read-only request dispatch (ADR-0022). A line carrying `request:
    // "movement_preview"` is *not* an order: it never mutates game state, is
    // excluded from save/replay, and returns a preview envelope instead of a
    // snapshot. Intercept it before order parsing so an unknown `request`
    // value is reported clearly rather than mis-parsed as an order.
    if let Some(request) = order_val.get("request").and_then(serde_json::Value::as_str) {
        return handle_request(game, request, &order_val);
    }

    let parsed: Result<Order, _> = serde_json::from_str(line);
    let order = match parsed {
        Ok(order) => order,
        Err(error) => {
            emit_error(
                "parse_error",
                &format!("cannot parse order: {error}"),
                Some(order_val),
                "harness",
            )?;
            return Ok(None);
        }
    };

    match apply_order(game, order.clone()) {
        Ok(()) => {
            // Drive greedy AI until a human must act or the scenario ends.
            // Record AI orders in the save stream so resume/replay is exact.
            let mut batch = vec![order];
            batch.extend(game.resolve_v2_npc_actions());
            emit_snapshot(game)?;
            Ok(Some(batch))
        }
        Err(error) => {
            emit_error(
                "order_illegal",
                &error.to_string(),
                Some(order_val),
                "harness",
            )?;
            Ok(None)
        }
    }
}

/// Read-only request dispatch (ADR-0022).
///
/// A `request` line is never an order: it must not mutate game state, is
/// excluded from save/replay, and returns a request-specific envelope instead
/// of a snapshot. Supported requests are projections only; none mutate state.
fn handle_request(
    game: &GameState,
    request: &str,
    order_val: &serde_json::Value,
) -> Result<Option<Vec<Order>>, String> {
    match request {
        "path_preview" => handle_path_preview(game, order_val),
        "reach_preview" => handle_reach_preview(game, order_val),
        "fire_preview" => handle_fire_preview(game, order_val),
        "movement_preview" | "maneuver_options" => {
            emit_error(
                "retired_request",
                &format!("{request} was removed in protocol v4; use path_preview / reach_preview"),
                Some(order_val.clone()),
                "harness",
            )?;
            Ok(None)
        }
        other => {
            emit_error(
                "unknown_request",
                &format!("unknown request: {other}"),
                Some(order_val.clone()),
                "harness",
            )?;
            Ok(None)
        }
    }
}

fn request_ship(order_val: &serde_json::Value, request: &str) -> Result<Option<u32>, String> {
    let Some(value) = order_val.get("ship").and_then(serde_json::Value::as_u64) else {
        emit_error(
            "preview_invalid",
            &format!("{request} requires integer `ship`"),
            Some(order_val.clone()),
            "harness",
        )?;
        return Ok(None);
    };
    let Ok(ship) = u32::try_from(value) else {
        emit_error(
            "preview_invalid",
            &format!("{request} `ship` is out of range"),
            Some(order_val.clone()),
            "harness",
        )?;
        return Ok(None);
    };
    Ok(Some(ship))
}

fn emit_request_body(body: &serde_json::Value) -> Result<Option<Vec<Order>>, String> {
    let mut out = io::stdout().lock();
    writeln!(
        out,
        "{}",
        serde_json::to_string(body).map_err(|error| error.to_string())?
    )
    .map_err(|error| error.to_string())?;
    out.flush().map_err(|error| error.to_string())?;
    Ok(None)
}

/// Validate a drafted path without committing it.
fn handle_path_preview(
    game: &GameState,
    order_val: &serde_json::Value,
) -> Result<Option<Vec<Order>>, String> {
    let Some(ship) = request_ship(order_val, "path_preview")? else {
        return Ok(None);
    };
    let mut actions = Vec::new();
    match order_val.get("actions") {
        None => {} // absent field == empty path (unchanged)
        Some(v) if v.is_array() => {
            for item in v.as_array().expect("checked is_array") {
                let Some(s) = item.as_str() else {
                    emit_error(
                        "preview_invalid",
                        "path_preview actions must be strings",
                        Some(order_val.clone()),
                        "harness",
                    )?;
                    return Ok(None);
                };
                let Some(action) = shipsim_core::path::PathAction::parse(s) else {
                    emit_error(
                        "preview_invalid",
                        &format!("unknown path action: {s}"),
                        Some(order_val.clone()),
                        "harness",
                    )?;
                    return Ok(None);
                };
                actions.push(action);
            }
        }
        Some(_) => {
            emit_error(
                "preview_invalid",
                "path_preview actions must be an array",
                Some(order_val.clone()),
                "harness",
            )?;
            return Ok(None);
        }
    }
    match game.path_preview(ship, &actions) {
        Ok(preview) => emit_request_body(&serde_json::json!({
            "type": "path_preview",
            "protocol_version": PROTOCOL_VERSION,
            "ok": true,
            "ship": preview.ship,
            "cost": preview.cost,
            "remaining_motion": preview.remaining_motion,
            "final_q": preview.final_q,
            "final_r": preview.final_r,
            "final_facing": preview.final_facing,
            "steps": preview.steps,
            "error_index": preview.error_index,
            "error": preview.error,
        })),
        Err(error) => {
            emit_error(
                "preview_invalid",
                &error.to_string(),
                Some(order_val.clone()),
                "harness",
            )?;
            Ok(None)
        }
    }
}

/// Reachable endpoints for shading (optional budget override).
fn handle_reach_preview(
    game: &GameState,
    order_val: &serde_json::Value,
) -> Result<Option<Vec<Order>>, String> {
    let Some(ship) = request_ship(order_val, "reach_preview")? else {
        return Ok(None);
    };
    let budget = order_val
        .get("budget")
        .and_then(|v| v.as_u64())
        .map(|v| u32::try_from(v).unwrap_or(u32::MAX));
    match game.reach_preview(ship, budget) {
        Ok(endpoints) => emit_request_body(&serde_json::json!({
            "type": "reach_preview",
            "protocol_version": PROTOCOL_VERSION,
            "ok": true,
            "ship": ship,
            "endpoints": endpoints,
        })),
        Err(error) => {
            emit_error(
                "preview_invalid",
                &error.to_string(),
                Some(order_val.clone()),
                "harness",
            )?;
            Ok(None)
        }
    }
}

/// Authoritative hit chance, projected damage, and legal target shield faces
/// for a weapon/target pairing. Illegal pairings remain soft preview results.
fn handle_fire_preview(
    game: &GameState,
    order_val: &serde_json::Value,
) -> Result<Option<Vec<Order>>, String> {
    let Some(ship) = request_ship(order_val, "fire_preview")? else {
        return Ok(None);
    };
    let Some(weapon) = order_val.get("weapon").and_then(serde_json::Value::as_str) else {
        emit_error(
            "preview_invalid",
            "fire_preview requires string `weapon`",
            Some(order_val.clone()),
            "harness",
        )?;
        return Ok(None);
    };
    let Some(target_value) = order_val.get("target").and_then(serde_json::Value::as_u64) else {
        emit_error(
            "preview_invalid",
            "fire_preview requires integer `target`",
            Some(order_val.clone()),
            "harness",
        )?;
        return Ok(None);
    };
    let Ok(target) = u32::try_from(target_value) else {
        emit_error(
            "preview_invalid",
            "fire_preview `target` is out of range",
            Some(order_val.clone()),
            "harness",
        )?;
        return Ok(None);
    };

    let body = match game.fire_decision_preview(ship, weapon, target) {
        Ok(preview) => serde_json::json!({
            "type": "fire_preview",
            "protocol_version": PROTOCOL_VERSION,
            "ok": true,
            "legal": true,
            "ship": preview.ship,
            "weapon": preview.weapon,
            "target": preview.target,
            "range": preview.range,
            "threshold": preview.threshold,
            "die_sides": preview.die_sides,
            "hit_percent": preview.hit_percent,
            "projected_damage": preview.projected_damage,
            "legal_shield_facings": preview.legal_shield_facings,
        }),
        Err(error) => serde_json::json!({
            "type": "fire_preview",
            "protocol_version": PROTOCOL_VERSION,
            "ok": true,
            "legal": false,
            "ship": ship,
            "weapon": weapon,
            "target": target,
            "reason": error.to_string(),
        }),
    };
    emit_request_body(&body)
}
