use std::env;
use std::io::{self, BufRead};
use std::path::PathBuf;
use std::process::ExitCode;

use shipsim_core::campaign::Campaign;
use shipsim_core::game_state::{GameState, ScenarioStatus};
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario;
use shipsim_core::snapshot::StateSnapshot;

enum OrderSource {
    File(PathBuf),
    Stdin,
}

enum Mode {
    Scenario(PathBuf),
    Campaign(PathBuf),
}

struct Args {
    mode: Mode,
    orders: OrderSource,
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
            apply_orders(&mut game, &args.orders)?;
        }
        Mode::Campaign(path) => {
            let mut campaign = Campaign::load(&path).map_err(|e| e.to_string())?;
            loop {
                let mut game = campaign.load_current().map_err(|e| e.to_string())?;
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
    }

    Ok(())
}

fn apply_orders(game: &mut GameState, orders: &OrderSource) -> Result<(), String> {
    match orders {
        OrderSource::File(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|error| format!("cannot read orders {path:?}: {error}"))?;
            for line in text.lines() {
                apply_order_line(game, line)?;
            }
        }
        OrderSource::Stdin => {
            for line in io::stdin().lock().lines() {
                let line = line.map_err(|error| format!("cannot read stdin: {error}"))?;
                apply_order_line(game, &line)?;
            }
        }
    }
    Ok(())
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut mode = None;
    let mut orders = None;
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
        mode: mode.ok_or_else(|| "--scenario or --campaign is required".to_string())?,
        orders: orders.ok_or_else(|| "choose exactly one of --orders or --stdin".to_string())?,
    })
}

fn apply_order_line(game: &mut GameState, line: &str) -> Result<(), String> {
    if line.trim().is_empty() {
        return Ok(());
    }

    let order: Order = serde_json::from_str(line)
        .map_err(|error| format!("cannot parse order {line:?}: {error}"))?;
    apply_order(game, order)
        .map_err(|error| format!("cannot apply order {line:?}: {error}"))?;

    let snapshot = StateSnapshot::from_game_state(game);
    println!(
        "{}",
        serde_json::to_string(&snapshot).map_err(|error| error.to_string())?
    );
    Ok(())
}
