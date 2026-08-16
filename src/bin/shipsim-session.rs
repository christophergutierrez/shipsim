//! Two-client TCP host for the shipsim NDJSON contract.
//!
//! The first connection is side A and the second is side B. Orders remain
//! ordinary protocol-v4 JSON; the host only adds connection ownership checks.

use shipsim_core::game_state::GameState;
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::schema::SideId;
use shipsim_core::scenario::load_scenario;
use shipsim_core::snapshot::StateSnapshot;
use serde_json::json;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

struct Args { scenario: String, listen: String }

fn main() -> Result<(), String> {
    let args = parse_args(env::args().skip(1))?;
    let mut game = load_scenario(args.scenario.as_ref()).map_err(|e| e.to_string())?;
    let listener = TcpListener::bind(&args.listen).map_err(|e| format!("bind {}: {e}", args.listen))?;
    eprintln!("shipsim-session listening on {} (first client=A, second client=B)", args.listen);

    let (stream_a, addr_a) = listener.accept().map_err(|e| e.to_string())?;
    let (stream_b, addr_b) = listener.accept().map_err(|e| e.to_string())?;
    eprintln!("side A connected from {addr_a}; side B connected from {addr_b}");
    let writers = vec![
        Arc::new(Mutex::new(stream_a.try_clone().map_err(|e| e.to_string())?)),
        Arc::new(Mutex::new(stream_b.try_clone().map_err(|e| e.to_string())?)),
    ];
    broadcast(&writers, &StateSnapshot::from_game_state(&game))?;

    let (tx, rx) = mpsc::channel::<(SideId, String)>();
    spawn_reader(stream_a, SideId::A, tx.clone());
    spawn_reader(stream_b, SideId::B, tx);

    while game.status() == shipsim_core::game_state::ScenarioStatus::InProgress {
        let (side, line) = rx.recv().map_err(|_| "both clients disconnected".to_string())?;
        let value: serde_json::Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) => { broadcast_error(&writers, "invalid_json", &error.to_string())?; continue; }
        };
        let order: Order = match serde_json::from_value(value.clone()) {
            Ok(order) => order,
            Err(error) => { broadcast_error(&writers, "invalid_order", &error.to_string())?; continue; }
        };
        if let Some(ship) = order_ship(&order) {
            if game.ship(ship).is_none_or(|candidate| candidate.side != side) {
                broadcast_error(&writers, "ownership", &format!("side {:?} cannot order ship {ship}", side))?;
                continue;
            }
        }
        if let Order::Purchase { side: requested, .. } = order {
            if requested != side {
                broadcast_error(&writers, "ownership", "purchase side does not match connection")?;
                continue;
            }
            // Re-create the owned value after the validation match.
            let order: Order = serde_json::from_value(value).map_err(|e| e.to_string())?;
            apply_or_error(&mut game, order, &writers)?;
        } else {
            apply_or_error(&mut game, order, &writers)?;
        }
    }
    broadcast(&writers, &StateSnapshot::from_game_state(&game))?;
    Ok(())
}

fn spawn_reader(stream: TcpStream, side: SideId, tx: mpsc::Sender<(SideId, String)>) {
    thread::spawn(move || {
        for line in BufReader::new(stream).lines() {
            match line { Ok(line) if !line.trim().is_empty() => { if tx.send((side, line)).is_err() { break; } }, _ => break }
        }
    });
}

fn apply_or_error(game: &mut GameState, order: Order, writers: &[Arc<Mutex<TcpStream>>]) -> Result<(), String> {
    match apply_order(game, order) {
        Ok(()) => broadcast(writers, &StateSnapshot::from_game_state(game)),
        Err(error) => broadcast_error(writers, "order_rejected", &error.to_string()),
    }
}

fn order_ship(order: &Order) -> Option<u32> {
    match order {
        Order::Allocate { ship, .. } | Order::CommitPath { ship, .. } | Order::CommitVolley { ship, .. } => Some(*ship),
        Order::Purchase { .. } | Order::PurchaseCustom { .. } | Order::RetiredUnknown => None,
    }
}

fn broadcast(writers: &[Arc<Mutex<TcpStream>>], snapshot: &StateSnapshot) -> Result<(), String> {
    let line = serde_json::to_string(snapshot).map_err(|e| e.to_string())? + "\n";
    for writer in writers { writer.lock().map_err(|_| "writer lock poisoned".to_string())?.write_all(line.as_bytes()).map_err(|e| e.to_string())?; }
    Ok(())
}

fn broadcast_error(writers: &[Arc<Mutex<TcpStream>>], code: &str, message: &str) -> Result<(), String> {
    let line = serde_json::to_string(&json!({"type":"error","protocol_version":4,"ok":false,"code":code,"message":message})).map_err(|e| e.to_string())? + "\n";
    for writer in writers { writer.lock().map_err(|_| "writer lock poisoned".to_string())?.write_all(line.as_bytes()).map_err(|e| e.to_string())?; }
    Ok(())
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut scenario = None;
    let mut listen = "127.0.0.1:4100".to_string();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--scenario" => scenario = args.next(),
            "--listen" => listen = args.next().ok_or("--listen requires address")?,
            _ => return Err(format!("unknown argument {arg}; usage: shipsim-session --scenario PATH [--listen HOST:PORT]")),
        }
    }
    Ok(Args { scenario: scenario.ok_or("--scenario is required")?, listen })
}
