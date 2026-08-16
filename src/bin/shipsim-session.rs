//! Two-client TCP host for the shipsim NDJSON contract.
//!
//! The first connection is side A and the second is side B. Orders remain
//! ordinary protocol-v4 JSON; the host only adds connection ownership checks.

use serde_json::json;
use shipsim_core::game_state::GameState;
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario;
use shipsim_core::schema::SideId;
use shipsim_core::snapshot::StateSnapshot;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

struct Args {
    scenario: String,
    listen: String,
}

fn main() -> Result<(), String> {
    let args = parse_args(env::args().skip(1))?;
    let mut game = load_scenario(args.scenario.as_ref()).map_err(|e| e.to_string())?;
    let listener =
        TcpListener::bind(&args.listen).map_err(|e| format!("bind {}: {e}", args.listen))?;
    eprintln!(
        "shipsim-session listening on {} (first client=A, second client=B)",
        args.listen
    );

    let (stream_a, addr_a) = listener.accept().map_err(|e| e.to_string())?;
    let (stream_b, addr_b) = listener.accept().map_err(|e| e.to_string())?;
    eprintln!("side A connected from {addr_a}; side B connected from {addr_b}");
    let writers = vec![
        Arc::new(Mutex::new(stream_a.try_clone().map_err(|e| e.to_string())?)),
        Arc::new(Mutex::new(stream_b.try_clone().map_err(|e| e.to_string())?)),
    ];
    broadcast_snapshots(&writers, &game)?;

    let (tx, rx) = mpsc::channel::<(SideId, String)>();
    spawn_reader(stream_a, SideId::A, tx.clone());
    spawn_reader(stream_b, SideId::B, tx);

    while game.status() == shipsim_core::game_state::ScenarioStatus::InProgress {
        let (side, line) = rx
            .recv()
            .map_err(|_| "both clients disconnected".to_string())?;
        let value: serde_json::Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) => {
                send_error(&writers, side, "invalid_json", &error.to_string())?;
                continue;
            }
        };
        let order: Order = match serde_json::from_value(value.clone()) {
            Ok(order) => order,
            Err(error) => {
                send_error(&writers, side, "invalid_order", &error.to_string())?;
                continue;
            }
        };
        if let Err((code, message)) = authorize_order(&game, side, &order) {
            send_error(&writers, side, code, &message)?;
            continue;
        }
        apply_or_error(&mut game, side, order, &writers)?;
    }
    broadcast_snapshots(&writers, &game)?;
    Ok(())
}

fn spawn_reader(stream: TcpStream, side: SideId, tx: mpsc::Sender<(SideId, String)>) {
    thread::spawn(move || {
        for line in BufReader::new(stream).lines() {
            match line {
                Ok(line) if !line.trim().is_empty() => {
                    if tx.send((side, line)).is_err() {
                        break;
                    }
                }
                _ => break,
            }
        }
    });
}

fn apply_or_error(
    game: &mut GameState,
    side: SideId,
    order: Order,
    writers: &[Arc<Mutex<TcpStream>>],
) -> Result<(), String> {
    match apply_order(game, order) {
        Ok(()) => broadcast_snapshots(writers, game),
        Err(error) => send_error(writers, side, "order_rejected", &error.to_string()),
    }
}

/// Session ownership is checked before the order reaches the engine. Both
/// purchase variants use the same side check as ship orders.
fn authorize_order(
    game: &GameState,
    side: SideId,
    order: &Order,
) -> Result<(), (&'static str, String)> {
    match order {
        Order::Allocate { ship, .. }
        | Order::CommitPath { ship, .. }
        | Order::CommitVolley { ship, .. } => {
            if game
                .ship(*ship)
                .is_none_or(|candidate| candidate.side != side)
            {
                Err((
                    "ownership",
                    format!("side {side:?} cannot order ship {ship}"),
                ))
            } else {
                Ok(())
            }
        }
        Order::Purchase {
            side: requested, ..
        }
        | Order::PurchaseCustom {
            side: requested, ..
        } => {
            if requested != &side {
                Err((
                    "ownership",
                    "purchase side does not match connection".into(),
                ))
            } else {
                Ok(())
            }
        }
        Order::RetiredUnknown => Ok(()),
    }
}

fn broadcast_snapshots(writers: &[Arc<Mutex<TcpStream>>], game: &GameState) -> Result<(), String> {
    for (index, writer) in writers.iter().enumerate() {
        let side = if index == 0 { SideId::A } else { SideId::B };
        let snapshot = StateSnapshot::from_game_state_for_side(game, side);
        write_line(
            writer,
            &serde_json::to_string(&snapshot).map_err(|e| e.to_string())?,
        )?;
    }
    Ok(())
}

fn send_error(
    writers: &[Arc<Mutex<TcpStream>>],
    side: SideId,
    code: &str,
    message: &str,
) -> Result<(), String> {
    let line = serde_json::to_string(
        &json!({"type":"error","protocol_version":4,"ok":false,"code":code,"message":message}),
    )
    .map_err(|e| e.to_string())?
        + "\n";
    let index = if side == SideId::A { 0 } else { 1 };
    write_line(
        writers
            .get(index)
            .ok_or("missing session writer".to_string())?,
        line.trim_end(),
    )
}

fn write_line(writer: &Arc<Mutex<TcpStream>>, line: &str) -> Result<(), String> {
    writer
        .lock()
        .map_err(|_| "writer lock poisoned".to_string())?
        .write_all(format!("{line}\n").as_bytes())
        .map_err(|e| e.to_string())
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
    Ok(Args {
        scenario: scenario.ok_or("--scenario is required")?,
        listen,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shipsim_core::scenario::load_scenario;
    use std::io::{BufRead, BufReader, Read};
    use std::net::TcpListener;
    use std::path::Path;

    fn socket_pair() -> (Arc<Mutex<TcpStream>>, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let client = TcpStream::connect(address).unwrap();
        let (server, _) = listener.accept().unwrap();
        (Arc::new(Mutex::new(server)), client)
    }

    #[test]
    fn purchase_variants_require_the_bound_side() {
        let game = load_scenario(Path::new("scenarios/shipyard_assault.toml")).unwrap();
        let purchase = Order::Purchase {
            side: SideId::B,
            class: "basic_swarm".into(),
        };
        assert!(authorize_order(&game, SideId::B, &purchase).is_ok());
        assert!(authorize_order(&game, SideId::A, &purchase).is_err());

        let custom = Order::PurchaseCustom {
            side: SideId::A,
            design: shipsim_core::shipyard::new_design("session_custom"),
        };
        assert!(authorize_order(&game, SideId::A, &custom).is_ok());
        assert!(authorize_order(&game, SideId::B, &custom).is_err());
    }

    #[test]
    fn ship_orders_are_side_symmetric() {
        let game = load_scenario(Path::new("scenarios/shipyard_assault.toml")).unwrap();
        let a = Order::Allocate {
            ship: 2,
            movement: 0,
            weapons: Default::default(),
            shields: [0; 6],
            cloak: false,
            repair: 0,
            unsquad: false,
            squad_leader: None,
        };
        let b = Order::Allocate {
            ship: 7,
            movement: 0,
            weapons: Default::default(),
            shields: [0; 6],
            cloak: false,
            repair: 0,
            unsquad: false,
            squad_leader: None,
        };
        assert!(authorize_order(&game, SideId::A, &a).is_ok());
        assert!(authorize_order(&game, SideId::B, &b).is_ok());
        assert!(authorize_order(&game, SideId::A, &b).is_err());
    }

    #[test]
    fn session_errors_are_sent_only_to_the_offending_client() {
        let (writer_a, mut client_a) = socket_pair();
        let (writer_b, mut client_b) = socket_pair();
        client_b
            .set_read_timeout(Some(std::time::Duration::from_millis(100)))
            .unwrap();
        let writers = vec![writer_a, writer_b];
        send_error(&writers, SideId::A, "ownership", "private failure").unwrap();

        let mut line = String::new();
        BufReader::new(&mut client_a).read_line(&mut line).unwrap();
        assert!(line.contains("private failure"));
        let mut leaked = [0_u8; 1];
        assert!(client_b.read(&mut leaked).is_err());
    }

    #[test]
    fn two_client_snapshots_are_viewer_relative_and_draw_is_symmetric() {
        let (writer_a, mut client_a) = socket_pair();
        let (writer_b, mut client_b) = socket_pair();
        let mut game = load_scenario(Path::new("scenarios/shipyard_assault.toml")).unwrap();
        game.set_ship_structure(6, 0).unwrap();
        game.refresh_status();
        broadcast_snapshots(&[writer_a.clone(), writer_b.clone()], &game).unwrap();
        let mut a_line = String::new();
        let mut b_line = String::new();
        BufReader::new(&mut client_a)
            .read_line(&mut a_line)
            .unwrap();
        BufReader::new(&mut client_b)
            .read_line(&mut b_line)
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&a_line).unwrap()["status"],
            "Won"
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&b_line).unwrap()["status"],
            "Lost"
        );

        game.set_ship_structure(1, 0).unwrap();
        game.refresh_status();
        broadcast_snapshots(&[writer_a, writer_b], &game).unwrap();
        a_line.clear();
        b_line.clear();
        BufReader::new(&mut client_a)
            .read_line(&mut a_line)
            .unwrap();
        BufReader::new(&mut client_b)
            .read_line(&mut b_line)
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&a_line).unwrap()["status"],
            "Draw"
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&b_line).unwrap()["status"],
            "Draw"
        );
    }
}
