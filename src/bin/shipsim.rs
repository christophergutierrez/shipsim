use std::env;
use std::io::{self, BufRead};
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
            emit_snapshot(&game)?;
            document
                .orders
                .extend(apply_orders(&mut game, &args.orders)?);
            document.prng_state = game.prng_state();
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
    println!(
        "{}",
        serde_json::to_string(&snapshot).map_err(|error| error.to_string())?
    );
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
    println!(
        "{}",
        serde_json::to_string(&body).map_err(|error| error.to_string())?
    );
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
