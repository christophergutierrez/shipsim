use std::env;
use std::io::{self, BufRead};
use std::path::PathBuf;
use std::process::ExitCode;

use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario;
use shipsim_core::snapshot::StateSnapshot;

enum OrderSource {
    File(PathBuf),
    Stdin,
}

struct Args {
    scenario: PathBuf,
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
    let mut game = load_scenario(&args.scenario).map_err(|error| error.to_string())?;

    match args.orders {
        OrderSource::File(path) => {
            let text = std::fs::read_to_string(&path)
                .map_err(|error| format!("cannot read orders {path:?}: {error}"))?;
            for line in text.lines() {
                apply_order_line(&mut game, line)?;
            }
        }
        OrderSource::Stdin => {
            for line in io::stdin().lock().lines() {
                let line = line.map_err(|error| format!("cannot read stdin: {error}"))?;
                apply_order_line(&mut game, &line)?;
            }
        }
    }

    Ok(())
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut scenario = None;
    let mut orders = None;
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--scenario" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--scenario requires a path".to_string())?;
                scenario = Some(PathBuf::from(value));
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
        scenario: scenario.ok_or_else(|| "--scenario is required".to_string())?,
        orders: orders.ok_or_else(|| "choose exactly one of --orders or --stdin".to_string())?,
    })
}

fn apply_order_line(
    game: &mut shipsim_core::game_state::GameState,
    line: &str,
) -> Result<(), String> {
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
