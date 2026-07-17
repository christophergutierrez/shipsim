use std::collections::BTreeMap;
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
            // phase is entirely NPC-driven (e.g. AI-only allocate).
            game.resolve_v2_npc_actions();
            // Post-load snapshot so a thin client can paint before any order (D8).
            emit_snapshot(&game)?;
            let orders = apply_orders(&mut game, &args.orders)?;
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
            let mut game = document.replay().map_err(|error| error.to_string())?;
            game.resolve_v2_npc_actions();
            emit_snapshot(&game)?;
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
                if let Some(order) = apply_order_line(game, line)? {
                    accepted.push(order);
                }
            }
        }
        OrderSource::Stdin => {
            for line in io::stdin().lock().lines() {
                let line = line.map_err(|error| format!("cannot read stdin: {error}"))?;
                if let Some(order) = apply_order_line(game, &line)? {
                    accepted.push(order);
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

fn apply_order_line(game: &mut GameState, line: &str) -> Result<Option<Order>, String> {
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
            // Clients (REPL, Love) never reimplement AI; they only send player orders.
            game.resolve_v2_npc_actions();
            emit_snapshot(game)?;
            Ok(Some(order))
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
) -> Result<Option<Order>, String> {
    match request {
        "movement_preview" => handle_movement_preview(game, order_val),
        "maneuver_options" => handle_maneuver_options(game, order_val),
        "fire_preview" => handle_fire_preview(game, order_val),
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

fn emit_request_body(body: &serde_json::Value) -> Result<Option<Order>, String> {
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

/// Immediate maneuver costs and affordability for one ship.
fn handle_maneuver_options(
    game: &GameState,
    order_val: &serde_json::Value,
) -> Result<Option<Order>, String> {
    let Some(ship) = request_ship(order_val, "maneuver_options")? else {
        return Ok(None);
    };
    match game.maneuver_options(ship) {
        Ok(options) => emit_request_body(&serde_json::json!({
            "type": "maneuver_options",
            "protocol_version": PROTOCOL_VERSION,
            "ok": true,
            "ship": ship,
            "options": options,
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
) -> Result<Option<Order>, String> {
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

/// `movement_preview` request (ADR-0022 preview contract).
///
/// Request shape (one JSON object per line, `protocol_version: 3`):
/// ```json
/// {"protocol_version":3,"request":"movement_preview","ship":1,
///  "movement":4,"weapons":{"beam_1":2},"shields":[2,0,0,0,0,2]}
/// ```
///
/// The fields mirror the `allocate` order exactly (engine owns the
/// power→thrust conversion). The response is a `movement_preview` envelope:
/// `ok: true`, the queried `ship`, the sorted reachable `endpoints`, the
/// single `coast` endpoint, and the `occupied` endpoint list. State is
/// unchanged; nothing is appended to the save/replay stream.
fn handle_movement_preview(
    game: &GameState,
    order_val: &serde_json::Value,
) -> Result<Option<Order>, String> {
    let ship = match order_val.get("ship").and_then(serde_json::Value::as_u64) {
        Some(value) => value as u32,
        None => {
            emit_error(
                "preview_invalid",
                "movement_preview requires integer `ship`",
                Some(order_val.clone()),
                "harness",
            )?;
            return Ok(None);
        }
    };
    let movement = order_val
        .get("movement")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as u32;

    // `clamp: true` relaxes the total power budget: movement is clamped down to
    // the affordable residual (after weapons + shields) instead of hard-rejecting.
    // Used by the TUI for live slider-drag previews. Defaults to false (strict).
    let clamp = order_val
        .get("clamp")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    // `weapons` is a map of weapon_id → desired total charge, same as `allocate`.
    let mut weapons: BTreeMap<String, u32> = BTreeMap::new();
    if let Some(map) = order_val
        .get("weapons")
        .and_then(serde_json::Value::as_object)
    {
        for (key, value) in map {
            let charge = value.as_u64().unwrap_or(0) as u32;
            weapons.insert(key.clone(), charge);
        }
    }

    // `shields` is six face powers, same as `allocate`. Missing or short
    // arrays are zero-padded; over-long arrays are truncated to six.
    let mut shields: [u32; 6] = [0; 6];
    if let Some(array) = order_val
        .get("shields")
        .and_then(serde_json::Value::as_array)
    {
        for (slot, value) in shields.iter_mut().zip(array.iter()) {
            *slot = value.as_u64().unwrap_or(0) as u32;
        }
    }

    // Report the clamped movement power so the client can show the effective
    // thrust even when the draft exceeds the budget.
    let clamped_movement = if clamp {
        match game.clamp_movement_power(ship, movement, &weapons, &shields) {
            Ok(value) => Some(value),
            Err(error) => {
                emit_error(
                    "preview_invalid",
                    &error.to_string(),
                    Some(order_val.clone()),
                    "harness",
                )?;
                return Ok(None);
            }
        }
    } else {
        None
    };

    let result = if clamp {
        game.movement_preview_clamped(ship, movement, weapons, shields)
    } else {
        game.movement_preview(ship, movement, weapons, shields)
    };
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            emit_error(
                "preview_invalid",
                &error.to_string(),
                Some(order_val.clone()),
                "harness",
            )?;
            return Ok(None);
        }
    };

    // `PreviewResult`/`PreviewEndpoint` are not Serialize by design (the
    // preview is a read-only projection, not a persisted snapshot), so build
    // the response envelope by hand.
    let endpoints: Vec<serde_json::Value> = result
        .endpoints
        .iter()
        .map(|endpoint| {
            serde_json::json!({
                "q": endpoint.q,
                "r": endpoint.r,
                "facing": endpoint.facing,
                "course": endpoint.course,
                "speed": endpoint.speed,
                "thrust_remaining": endpoint.thrust_remaining,
            })
        })
        .collect();
    let coast = serde_json::json!({
        "q": result.coast.q,
        "r": result.coast.r,
        "facing": result.coast.facing,
        "course": result.coast.course,
        "speed": result.coast.speed,
        "thrust_remaining": result.coast.thrust_remaining,
    });
    let occupied: Vec<serde_json::Value> = result
        .occupied
        .iter()
        .map(|(q, r)| serde_json::json!({"q": q, "r": r}))
        .collect();

    let mut body = serde_json::json!({
        "type": "movement_preview",
        "protocol_version": PROTOCOL_VERSION,
        "ok": true,
        "ship": ship,
        "endpoints": endpoints,
        "coast": coast,
        "occupied": occupied,
    });
    if let Some(cm) = clamped_movement {
        body["clamped_movement"] = serde_json::json!(cm);
    }

    let mut out = io::stdout().lock();
    writeln!(
        out,
        "{}",
        serde_json::to_string(&body).map_err(|error| error.to_string())?
    )
    .map_err(|error| error.to_string())?;
    out.flush().map_err(|error| error.to_string())?;
    Ok(None)
}
