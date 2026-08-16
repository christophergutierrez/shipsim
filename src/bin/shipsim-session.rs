//! One-match authoritative session/lobby server.
//!
//! Session messages use protocol v1. Once a seat is assigned, ordinary game
//! protocol-v4 orders and read-only requests use the same connection. The
//! server is intentionally single-match and blocking: a slow participant is
//! visible in lobby state rather than silently timed out.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use shipsim_core::game_state::{GameState, Phase, ScenarioStatus};
use shipsim_core::movement::Order;
use shipsim_core::request_dispatch::{dispatch, parse_order};
use shipsim_core::scenario::load_scenario;
use shipsim_core::schema::SideId;
use shipsim_core::session_protocol::{
    negotiate_versions, BotPolicySummary, ControllerAssignments, ControllerSpec, LobbyPhase,
    ParticipantStatus, ScenarioCatalog, ScenarioSummary, SeatLobbyState, SeatOccupancy,
    ServerCapability, SessionErrorCode, SessionMessage, SESSION_PROTOCOL_VERSION,
};
use shipsim_core::simulation::{build_policy_for_side, policy_catalog, Policy, PurchaseContext};
use shipsim_core::snapshot::StateSnapshot;
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

struct Args {
    scenario: Option<String>,
    listen: String,
}

struct Connection {
    writer: Arc<Mutex<TcpStream>>,
    connected_at: Instant,
    participant_id: String,
    host: bool,
    negotiated: bool,
    side: Option<SideId>,
    ready: bool,
    status: ParticipantStatus,
}

struct TokenSeat {
    side: SideId,
    used: bool,
}

enum WireEvent {
    Line(usize, String),
    Disconnected(usize),
}

struct Server {
    catalog: ScenarioCatalog,
    configured_scenario: Option<String>,
    connections: BTreeMap<usize, Connection>,
    readers: Vec<JoinHandle<()>>,
    tokens: HashMap<[u8; 32], TokenSeat>,
    next_connection: usize,
    host: Option<usize>,
    controllers: Option<ControllerAssignments>,
    scenario: Option<ScenarioSummary>,
    game: Option<GameState>,
    bot_side: bool,
    bot_policy: Option<Box<dyn Policy>>,
    bot_purchases_turn: Option<u32>,
    phase: LobbyPhase,
    match_id: String,
    events: Receiver<WireEvent>,
    event_tx: Sender<WireEvent>,
}

fn main() -> Result<(), String> {
    let args = parse_args(env::args().skip(1))?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let catalog = ScenarioCatalog::load(&root).map_err(|e| e.to_string())?;
    let listener =
        TcpListener::bind(&args.listen).map_err(|e| format!("bind {}: {e}", args.listen))?;
    let address = listener.local_addr().map_err(|e| e.to_string())?;
    eprintln!("shipsim-session listening on {address} (session protocol v1, game protocol v4)");
    run_listener(listener, catalog, args.scenario)
}

fn run_listener(
    listener: TcpListener,
    catalog: ScenarioCatalog,
    configured_scenario: Option<String>,
) -> Result<(), String> {
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("set listener nonblocking: {e}"))?;
    let (event_tx, event_rx) = mpsc::channel();
    let mut server = Server {
        catalog,
        configured_scenario,
        connections: BTreeMap::new(),
        readers: Vec::new(),
        tokens: HashMap::new(),
        next_connection: 1,
        host: None,
        controllers: None,
        scenario: None,
        game: None,
        bot_side: false,
        bot_policy: None,
        bot_purchases_turn: None,
        phase: LobbyPhase::Unconfigured,
        match_id: "match-1".into(),
        events: event_rx,
        event_tx: event_tx.clone(),
    };

    let result = server.run(&listener);
    for connection in server.connections.values() {
        if let Ok(stream) = connection.writer.lock() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
    for reader in server.readers {
        let _ = reader.join();
    }
    result
}

impl Server {
    fn run(&mut self, listener: &TcpListener) -> Result<(), String> {
        loop {
            loop {
                match listener.accept() {
                    Ok((stream, address)) => {
                        self.accept_connection(stream, address)?;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(error) => return Err(format!("accept: {error}")),
                }
            }

            match self.events.recv_timeout(POLL_INTERVAL) {
                Ok(WireEvent::Line(id, line)) => self.handle_line(id, &line)?,
                Ok(WireEvent::Disconnected(id)) => {
                    if self.handle_disconnect(id)? {
                        return Ok(());
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }

            self.expire_handshakes();

            if self.phase == LobbyPhase::Finished {
                return Ok(());
            }
        }
    }

    fn accept_connection(
        &mut self,
        stream: TcpStream,
        address: std::net::SocketAddr,
    ) -> Result<(), String> {
        if self.phase == LobbyPhase::Finished {
            let writer = Arc::new(Mutex::new(stream));
            self.send_game_error_to_writer(
                &writer,
                "seat_already_occupied",
                "this one-match server has no available seat",
                None,
            )?;
            if let Ok(stream) = writer.lock() {
                let _ = stream.shutdown(Shutdown::Both);
            }
            return Ok(());
        }
        if self.connections.len() >= 2 {
            if let Some(pending_id) = self
                .connections
                .iter()
                .find(|(_, connection)| !connection.negotiated)
                .map(|(id, _)| *id)
            {
                if let Some(pending) = self.connections.remove(&pending_id) {
                    if let Ok(stream) = pending.writer.lock() {
                        let _ = stream.shutdown(Shutdown::Both);
                    }
                }
            } else {
                let writer = Arc::new(Mutex::new(stream));
                self.send_game_error_to_writer(
                    &writer,
                    "seat_already_occupied",
                    "this one-match server has no available seat",
                    None,
                )?;
                if let Ok(stream) = writer.lock() {
                    let _ = stream.shutdown(Shutdown::Both);
                }
                return Ok(());
            }
        }
        let id = self.next_connection;
        self.next_connection += 1;
        let writer = Arc::new(Mutex::new(stream.try_clone().map_err(|e| e.to_string())?));
        let participant_id = format!("participant-{id}");
        let host = false;
        self.connections.insert(
            id,
            Connection {
                writer: writer.clone(),
                connected_at: Instant::now(),
                participant_id: participant_id.clone(),
                host,
                negotiated: false,
                side: None,
                ready: false,
                status: ParticipantStatus::Ready,
            },
        );
        eprintln!(
            "connection {participant_id} from {address}{}",
            if host { " (host)" } else { "" }
        );
        let tx = self.event_tx.clone();
        let reader = thread::spawn(move || {
            for result in BufReader::new(stream).lines() {
                match result {
                    Ok(line) if !line.trim().is_empty() => {
                        if tx.send(WireEvent::Line(id, line)).is_err() {
                            return;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            let _ = tx.send(WireEvent::Disconnected(id));
        });
        self.readers.push(reader);
        Ok(())
    }

    fn expire_handshakes(&mut self) {
        let now = Instant::now();
        let expired = self
            .connections
            .iter()
            .filter(|(_, connection)| {
                !connection.negotiated
                    && now.saturating_duration_since(connection.connected_at) >= HANDSHAKE_TIMEOUT
            })
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();

        for id in expired {
            let _ = self.send_session_error(
                id,
                SessionErrorCode::InvalidState,
                "hello handshake timed out",
            );
            if let Some(connection) = self.connections.remove(&id) {
                eprintln!(
                    "connection {} handshake timed out",
                    connection.participant_id
                );
                if let Ok(stream) = connection.writer.lock() {
                    let _ = stream.shutdown(Shutdown::Both);
                }
            }
        }
    }

    fn handle_line(&mut self, id: usize, line: &str) -> Result<(), String> {
        if !self.connections.contains_key(&id) {
            return Ok(());
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(error) => {
                return self.send_session_error(
                    id,
                    SessionErrorCode::InvalidMessage,
                    &format!("invalid JSON: {error}"),
                )
            }
        };
        let type_name = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if self
            .connections
            .get(&id)
            .is_some_and(|connection| connection.side.is_none())
        {
            return self.handle_lobby_message(id, &type_name, value);
        }
        if matches!(
            type_name.as_str(),
            "participant_status" | "join_match" | "create_match" | "hello"
        ) {
            return self.handle_lobby_message(id, &type_name, value);
        }
        self.handle_game_message(id, value)
    }

    fn handle_lobby_message(
        &mut self,
        id: usize,
        type_name: &str,
        value: Value,
    ) -> Result<(), String> {
        if type_name == "hello" {
            return self.handle_hello(id, value);
        }
        if !self.is_negotiated(id) {
            return self.send_session_error(
                id,
                SessionErrorCode::InvalidState,
                "send hello before lobby messages",
            );
        }
        let message: SessionMessage = match serde_json::from_value(value) {
            Ok(message) => message,
            Err(error) => {
                return self.send_session_error(
                    id,
                    SessionErrorCode::InvalidMessage,
                    &error.to_string(),
                )
            }
        };
        if let Err(error) = message.validate_version() {
            return self.send_session_error(id, error.code, &error.message);
        }
        match message {
            SessionMessage::CreateMatch {
                scenario_id,
                controllers,
                ..
            } => self.create_match(id, scenario_id, controllers),
            SessionMessage::JoinMatch {
                join_token,
                display_name,
                ..
            } => self.join_match(id, join_token, display_name),
            SessionMessage::ParticipantStatus { status, .. } => self.participant_status(id, status),
            _ => self.send_session_error(
                id,
                SessionErrorCode::InvalidMessage,
                "message is not valid in the lobby",
            ),
        }
    }

    fn handle_hello(&mut self, id: usize, value: Value) -> Result<(), String> {
        if self.is_negotiated(id) {
            return self.send_session_error(
                id,
                SessionErrorCode::InvalidState,
                "hello was already negotiated for this connection",
            );
        }
        let message: SessionMessage = match serde_json::from_value(value) {
            Ok(message @ SessionMessage::Hello { .. }) => message,
            Ok(_) => {
                return self.send_session_error(
                    id,
                    SessionErrorCode::InvalidMessage,
                    "expected hello",
                )
            }
            Err(error) => {
                return self.send_session_error(
                    id,
                    SessionErrorCode::InvalidMessage,
                    &error.to_string(),
                )
            }
        };
        let SessionMessage::Hello {
            supported_session_versions,
            supported_game_protocol_versions,
            ..
        } = message
        else {
            unreachable!()
        };
        let negotiated = match negotiate_versions(
            &supported_session_versions,
            &supported_game_protocol_versions,
        ) {
            Ok(versions) => versions,
            Err(error) => {
                self.send_session_error(id, error.code, &error.message)?;
                if let Some(connection) = self.connections.remove(&id) {
                    if let Ok(stream) = connection.writer.lock() {
                        let _ = stream.shutdown(Shutdown::Both);
                    }
                }
                return Ok(());
            }
        };
        if self.host.is_none() {
            self.host = Some(id);
            if let Some(connection) = self.connections.get_mut(&id) {
                connection.host = true;
            }
        }
        let can_configure = self
            .connections
            .get(&id)
            .is_some_and(|connection| connection.host);
        if let Some(connection) = self.connections.get_mut(&id) {
            connection.negotiated = true;
        }
        self.send(
            id,
            SessionMessage::Welcome {
                session_protocol_version: negotiated.session_protocol_version,
                game_protocol_version: negotiated.game_protocol_version,
                connection_id: format!("connection-{id}"),
                can_configure,
                capabilities: vec![
                    ServerCapability::ScenarioCatalog,
                    ServerCapability::PreviewRequests,
                    ServerCapability::ParticipantStatus,
                ],
            },
        )?;
        self.send(
            id,
            SessionMessage::ScenarioCatalog {
                session_protocol_version: SESSION_PROTOCOL_VERSION,
                scenarios: self.catalog.projection(),
            },
        )?;
        self.broadcast_lobby()
    }

    fn create_match(
        &mut self,
        id: usize,
        scenario_id: String,
        controllers: ControllerAssignments,
    ) -> Result<(), String> {
        if self.host != Some(id) {
            return self.send_session_error(
                id,
                SessionErrorCode::HostRequired,
                "only the first negotiated client may configure the match",
            );
        }
        if self.controllers.is_some() {
            return self.send_session_error(
                id,
                SessionErrorCode::LobbyAlreadyConfigured,
                "this server already has a match",
            );
        }
        if !matches!(controllers.a, ControllerSpec::Human {}) {
            return self.send_session_error(
                id,
                SessionErrorCode::UnknownController,
                "side A must be human in session v1",
            );
        }
        if let ControllerSpec::Bot { policy } = &controllers.b {
            if !policy_catalog()
                .iter()
                .any(|metadata| metadata.id == policy)
            {
                return self.send_session_error(
                    id,
                    SessionErrorCode::UnknownBotPolicy,
                    "unknown bot policy",
                );
            }
        }
        let scenario_id = if scenario_id.is_empty() {
            self.configured_scenario
                .clone()
                .ok_or_else(|| "scenario_id is required".to_string())?
        } else {
            scenario_id
        };
        let path = match self.catalog.resolve(&scenario_id) {
            Ok(path) => path.to_path_buf(),
            Err(error) => {
                return self.send_session_error(
                    id,
                    SessionErrorCode::UnknownScenario,
                    &error.to_string(),
                )
            }
        };
        let game = load_scenario(&path).map_err(|e| e.to_string())?;
        let bot_policy = match &controllers.b {
            ControllerSpec::Bot { policy } => build_policy_for_side(policy, game.seed(), SideId::B),
            _ => None,
        };
        if matches!(controllers.b, ControllerSpec::Bot { .. }) && bot_policy.is_none() {
            return self.send_session_error(
                id,
                SessionErrorCode::UnknownBotPolicy,
                "unable to construct bot policy",
            );
        }
        let invitation_token = if matches!(
            controllers.b,
            ControllerSpec::Human {} | ControllerSpec::LlmAgent {}
        ) {
            match self.make_token(SideId::B) {
                Ok(token) => Some(token),
                Err(error) => {
                    return self.send_session_error(id, SessionErrorCode::RequestRejected, &error)
                }
            }
        } else {
            None
        };
        let summary = self
            .catalog
            .projection()
            .into_iter()
            .find(|s| s.id == scenario_id)
            .ok_or_else(|| "scenario not in catalog".to_string())?;
        self.game = Some(game);
        self.scenario = Some(summary);
        self.bot_side = matches!(controllers.b, ControllerSpec::Bot { .. });
        self.bot_policy = bot_policy;
        self.bot_purchases_turn = None;
        self.controllers = Some(controllers);
        if let Some(connection) = self.connections.get_mut(&id) {
            connection.side = Some(SideId::A);
            connection.ready = true;
        }
        self.send(
            id,
            SessionMessage::SeatAssigned {
                session_protocol_version: SESSION_PROTOCOL_VERSION,
                side: SideId::A,
                match_id: self.match_id.clone(),
                participant_id: format!("participant-{id}"),
            },
        )?;
        if let Some(token) = invitation_token {
            self.send(
                id,
                SessionMessage::SeatInvitation {
                    session_protocol_version: SESSION_PROTOCOL_VERSION,
                    side: SideId::B,
                    display_code: "B".into(),
                    join_token: token,
                },
            )?;
        }
        self.phase = LobbyPhase::WaitingForSeats;
        self.maybe_start()
    }

    fn join_match(&mut self, id: usize, token: String, display_name: String) -> Result<(), String> {
        if self.controllers.is_none() {
            return self.send_session_error(
                id,
                SessionErrorCode::InvalidState,
                "no match is configured",
            );
        }
        if self
            .connections
            .get(&id)
            .is_some_and(|connection| connection.side.is_some())
        {
            return self.send_session_error(
                id,
                SessionErrorCode::SeatAlreadyOccupied,
                "connection already has an assigned seat",
            );
        }
        let digest = digest_token(&token);
        let Some(seat) = self.tokens.get_mut(&digest) else {
            return self.send_session_error(
                id,
                SessionErrorCode::InvalidJoinToken,
                "join token is invalid",
            );
        };
        if seat.used {
            return self.send_session_error(
                id,
                SessionErrorCode::SeatAlreadyOccupied,
                "seat token was already used",
            );
        }
        let side = seat.side;
        if self
            .connections
            .values()
            .any(|connection| connection.side == Some(side))
        {
            return self.send_session_error(
                id,
                SessionErrorCode::SeatAlreadyOccupied,
                "seat is already occupied",
            );
        }
        seat.used = true;
        if let Some(connection) = self.connections.get_mut(&id) {
            connection.side = Some(side);
            connection.ready = true;
        }
        self.send(
            id,
            SessionMessage::SeatAssigned {
                session_protocol_version: SESSION_PROTOCOL_VERSION,
                side,
                match_id: self.match_id.clone(),
                participant_id: format!("participant-{id}"),
            },
        )?;
        let _ = display_name;
        self.maybe_start()
    }

    fn participant_status(&mut self, id: usize, status: ParticipantStatus) -> Result<(), String> {
        let Some(connection) = self.connections.get_mut(&id) else {
            return Ok(());
        };
        if connection.side.is_none() {
            return self.send_session_error(
                id,
                SessionErrorCode::SeatNotReady,
                "join a seat first",
            );
        }
        connection.ready = matches!(status, ParticipantStatus::Ready);
        connection.status = status;
        if connection.ready && self.phase == LobbyPhase::WaitingForSeats {
            self.maybe_start()
        } else {
            self.broadcast_lobby()
        }
    }

    fn maybe_start(&mut self) -> Result<(), String> {
        let Some(controllers) = &self.controllers else {
            return Ok(());
        };
        let a_ready = self
            .connections
            .values()
            .any(|c| c.side == Some(SideId::A) && c.ready);
        let b_ready = self.bot_side
            || self
                .connections
                .values()
                .any(|c| c.side == Some(SideId::B) && c.ready);
        if a_ready && b_ready {
            self.phase = LobbyPhase::Running;
            if self.bot_side {
                if let Err(error) = self.schedule_bot() {
                    self.finish_bot_error(&error)?;
                    return Ok(());
                }
            }
            self.broadcast_lobby()?;
            self.broadcast_snapshots()
        } else {
            let _ = controllers;
            self.broadcast_lobby()
        }
    }

    fn handle_game_message(&mut self, id: usize, value: Value) -> Result<(), String> {
        if self.phase != LobbyPhase::Running {
            return self.send_session_error(
                id,
                SessionErrorCode::SeatNotReady,
                "match has not started",
            );
        }
        let Some(side) = self.connections.get(&id).and_then(|c| c.side) else {
            return self.send_session_error(
                id,
                SessionErrorCode::Ownership,
                "connection has no assigned seat",
            );
        };
        let version = value.get("protocol_version").and_then(Value::as_u64);
        if version != Some(4) {
            return self.send_game_error(
                id,
                "unsupported_protocol",
                "game protocol_version must be 4",
                value.get("request_id").cloned(),
            );
        }
        if value.get("request").is_some() {
            let ship = value
                .get("ship")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok());
            if ship.is_none_or(|ship| {
                self.game
                    .as_ref()
                    .is_none_or(|game| game.ship(ship).is_none_or(|s| s.side != side))
            }) {
                return self.send_game_error(
                    id,
                    "ownership",
                    "preview ship belongs to the other side",
                    value.get("request_id").cloned(),
                );
            }
            if let Some(response) = self.game.as_ref().and_then(|game| dispatch(game, &value)) {
                return self.send_value(id, response);
            }
            return self.send_game_error(
                id,
                "request_rejected",
                "unknown request",
                value.get("request_id").cloned(),
            );
        }
        let order = match parse_order(&value) {
            Ok(order) => order,
            Err(error) => return self.send_game_error(id, "invalid_order", &error, None),
        };
        if let Err((code, message)) =
            authorize_order(self.game.as_ref().expect("running game"), side, &order)
        {
            return self.send_game_error(id, code, &message, None);
        }
        let game = self.game.as_mut().expect("running game");
        if let Err(error) = shipsim_core::apply_order(game, order) {
            return self.send_game_error(id, "order_rejected", &error.to_string(), None);
        }
        if self.bot_side {
            if let Err(error) = self.schedule_bot() {
                self.finish_bot_error(&error)?;
                return Ok(());
            }
        }
        self.broadcast_lobby()?;
        self.broadcast_snapshots()?;
        if self
            .game
            .as_ref()
            .is_some_and(|game| game.status() != ScenarioStatus::InProgress)
        {
            self.phase = LobbyPhase::Finished;
        }
        Ok(())
    }

    fn schedule_bot(&mut self) -> Result<(), String> {
        const STEP_CAP: usize = 8192;
        for _ in 0..STEP_CAP {
            let Some(game) = self.game.as_ref() else {
                return Ok(());
            };
            if game.status() != ScenarioStatus::InProgress {
                return Ok(());
            }
            let phase = game.phase();
            let turn = game.turn_number();
            if phase == Phase::Allocate && self.bot_purchases_turn != Some(turn) {
                let snapshot = StateSnapshot::from_game_state(game);
                let orders = {
                    let policy = self.bot_policy.as_mut().ok_or("bot policy is missing")?;
                    policy.purchase_orders(&PurchaseContext {
                        snapshot: &snapshot,
                        side: SideId::B,
                        turn,
                    })
                };
                for order in orders.into_iter().take(1) {
                    self.apply_bot_order(order)?;
                }
                self.bot_purchases_turn = Some(turn);
                continue;
            }
            let snapshot = StateSnapshot::from_game_state(game);
            let pending = match phase {
                Phase::Allocate => snapshot.ships.iter().find(|ship| {
                    ship.side == SideId::B
                        && !ship.destroyed
                        && !snapshot.ships_allocated_this_turn.contains(&ship.id)
                }),
                Phase::Movement => snapshot.ships.iter().find(|ship| {
                    ship.side == SideId::B
                        && !ship.destroyed
                        && !snapshot.ships_committed_path.contains(&ship.id)
                }),
                Phase::Firing => snapshot.ships.iter().find(|ship| {
                    ship.side == SideId::B
                        && !ship.destroyed
                        && !snapshot.ships_committed_volley.contains(&ship.id)
                }),
            };
            let Some(ship) = pending else {
                return Ok(());
            };
            let order = {
                let policy = self.bot_policy.as_mut().ok_or("bot policy is missing")?;
                if phase == Phase::Allocate {
                    policy.allocate_with_context(ship, &snapshot)
                } else {
                    policy.choose_order(&shipsim_core::simulation::DecisionContext {
                        snapshot: &snapshot,
                        ship,
                        rules: game.rules(),
                        legal_orders: &[],
                    })
                }
            };
            self.apply_bot_order(order)?;
        }
        Err("bot scheduling exceeded the bounded 8192-order limit".into())
    }

    fn apply_bot_order(&mut self, order: Order) -> Result<(), String> {
        let game = self.game.as_ref().ok_or("bot order has no game")?;
        if let Err((code, message)) = authorize_order(game, SideId::B, &order) {
            return Err(format!("bot order {code}: {message}"));
        }
        self.game
            .as_mut()
            .ok_or_else(|| "bot order has no game".to_string())
            .and_then(|game| {
                shipsim_core::apply_order(game, order)
                    .map_err(|error| format!("bot order rejected: {error}"))
            })
    }

    fn finish_bot_error(&mut self, message: &str) -> Result<(), String> {
        self.phase = LobbyPhase::Finished;
        for id in self.connections.keys().copied().collect::<Vec<_>>() {
            self.send_session_error(
                id,
                SessionErrorCode::RequestRejected,
                &format!("bot scheduling failed: {message}"),
            )?;
        }
        Ok(())
    }

    fn handle_disconnect(&mut self, id: usize) -> Result<bool, String> {
        let Some(connection) = self.connections.remove(&id) else {
            return Ok(false);
        };
        if self.phase == LobbyPhase::Running {
            self.phase = LobbyPhase::Finished;
            for other in self.connections.keys().copied().collect::<Vec<_>>() {
                self.send_session_error(
                    other,
                    SessionErrorCode::ParticipantDisconnected,
                    &format!("{} disconnected", connection.participant_id),
                )?;
            }
            return Ok(true);
        }
        if connection.host {
            return Ok(true);
        }
        if connection.side == Some(SideId::B)
            && self.controllers.as_ref().is_some_and(|controllers| {
                matches!(
                    controllers.b,
                    ControllerSpec::Human {} | ControllerSpec::LlmAgent {}
                )
            })
        {
            let token = self.make_token(SideId::B)?;
            if let Some(host) = self.host {
                self.send(
                    host,
                    SessionMessage::SeatInvitation {
                        session_protocol_version: SESSION_PROTOCOL_VERSION,
                        side: SideId::B,
                        display_code: "B".into(),
                        join_token: token,
                    },
                )?;
            }
        }
        self.broadcast_lobby()?;
        Ok(false)
    }

    fn broadcast_lobby(&self) -> Result<(), String> {
        for id in self.connections.keys().copied().collect::<Vec<_>>() {
            if self.is_negotiated(id) {
                self.send(id, self.lobby_message())?;
            }
        }
        Ok(())
    }

    fn lobby_message(&self) -> SessionMessage {
        let seats = [SideId::A, SideId::B]
            .into_iter()
            .map(|side| {
                let controller = self
                    .controllers
                    .as_ref()
                    .map(|c| {
                        if side == SideId::A {
                            c.a.clone()
                        } else {
                            c.b.clone()
                        }
                    })
                    .unwrap_or(ControllerSpec::Human {});
                let connection = self.connections.values().find(|c| c.side == Some(side));
                SeatLobbyState {
                    side,
                    controller: controller.clone(),
                    occupancy: if self.bot_side && side == SideId::B {
                        SeatOccupancy::Internal
                    } else if connection.is_some() {
                        SeatOccupancy::Occupied
                    } else if self.controllers.is_some() && side == SideId::B {
                        SeatOccupancy::Reserved
                    } else {
                        SeatOccupancy::Vacant
                    },
                    display_name: connection.map(|c| c.participant_id.clone()),
                    ready: self.bot_side && side == SideId::B
                        || connection.is_some_and(|c| c.ready),
                    participant_status: connection.map(|c| c.status),
                }
            })
            .collect();
        SessionMessage::LobbyState {
            session_protocol_version: SESSION_PROTOCOL_VERSION,
            state: self.phase,
            scenario: self.scenario.clone(),
            controllers: self.controllers.clone(),
            seats,
            bot_policies: bot_policies(),
            waiting_reason: self.waiting_reason(),
        }
    }

    fn waiting_reason(&self) -> Option<String> {
        if self.phase == LobbyPhase::WaitingForSeats {
            return Some(if self.bot_side {
                "waiting for side A".into()
            } else {
                "waiting for side B".into()
            });
        }
        if self.phase != LobbyPhase::Running {
            return None;
        }
        let game = self.game.as_ref()?;
        let committed = match game.phase() {
            Phase::Allocate => game.allocated_this_turn(),
            Phase::Movement => game.ships_committed_path(),
            Phase::Firing => game.ships_committed_volley(),
        };
        let mut pending = [SideId::A, SideId::B]
            .into_iter()
            .filter(|side| {
                game.ships().iter().any(|ship| {
                    ship.side == *side && !ship.destroyed && !committed.contains(&ship.id)
                })
            })
            .map(|side| match side {
                SideId::A => "side A",
                SideId::B => "side B",
            });
        let first = pending.next()?;
        let sides = pending.next().map_or_else(
            || first.to_string(),
            |second| format!("{first} and {second}"),
        );
        Some(format!(
            "waiting for {sides} to finish {}",
            game.phase_name()
        ))
    }

    fn broadcast_snapshots(&self) -> Result<(), String> {
        let Some(game) = &self.game else {
            return Ok(());
        };
        for connection in self.connections.values() {
            if let Some(side) = connection.side {
                self.send_value_to_writer(
                    &connection.writer,
                    serde_json::to_value(StateSnapshot::from_game_state_for_side(game, side))
                        .map_err(|e| e.to_string())?,
                )?;
            }
        }
        Ok(())
    }

    fn make_token(&mut self, side: SideId) -> Result<String, String> {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes).map_err(|error| format!("OS entropy unavailable: {error}"))?;
        let token = bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        self.tokens
            .insert(digest_token(&token), TokenSeat { side, used: false });
        Ok(token)
    }

    fn is_negotiated(&self, id: usize) -> bool {
        self.connections.get(&id).is_some_and(|c| c.negotiated)
    }

    fn send(&self, id: usize, message: SessionMessage) -> Result<(), String> {
        self.send_value_to_writer(
            &self
                .connections
                .get(&id)
                .ok_or_else(|| "connection missing".to_string())?
                .writer,
            serde_json::to_value(message).map_err(|e| e.to_string())?,
        )
    }

    fn send_session_error(
        &self,
        id: usize,
        code: SessionErrorCode,
        message: &str,
    ) -> Result<(), String> {
        self.send(
            id,
            SessionMessage::Error {
                session_protocol_version: SESSION_PROTOCOL_VERSION,
                code,
                message: message.into(),
            },
        )
    }

    fn send_game_error(
        &self,
        id: usize,
        code: &str,
        message: &str,
        request_id: Option<Value>,
    ) -> Result<(), String> {
        let mut body =
            json!({"type":"error","protocol_version":4,"ok":false,"code":code,"message":message});
        if let Some(request_id) = request_id {
            body["request_id"] = request_id;
        }
        self.send_value(id, body)
    }

    fn send_value(&self, id: usize, value: Value) -> Result<(), String> {
        self.send_value_to_writer(
            &self
                .connections
                .get(&id)
                .ok_or_else(|| "connection missing".to_string())?
                .writer,
            value,
        )
    }

    fn send_value_to_writer(
        &self,
        writer: &Arc<Mutex<TcpStream>>,
        value: Value,
    ) -> Result<(), String> {
        let line = serde_json::to_string(&value).map_err(|e| e.to_string())?;
        let mut stream = writer
            .lock()
            .map_err(|_| "writer lock poisoned".to_string())?;
        writeln!(stream, "{line}").map_err(|e| e.to_string())?;
        stream.flush().map_err(|e| e.to_string())
    }

    fn send_game_error_to_writer(
        &self,
        writer: &Arc<Mutex<TcpStream>>,
        code: &str,
        message: &str,
        request_id: Option<Value>,
    ) -> Result<(), String> {
        let mut body =
            json!({"type":"error","protocol_version":4,"ok":false,"code":code,"message":message});
        if let Some(request_id) = request_id {
            body["request_id"] = request_id;
        }
        self.send_value_to_writer(writer, body)
    }
}

fn bot_policies() -> Vec<BotPolicySummary> {
    policy_catalog()
        .iter()
        .map(|metadata| BotPolicySummary {
            id: metadata.id.into(),
            display_name: metadata.label.into(),
        })
        .collect()
}

fn digest_token(token: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"shipsim-session-v1-token\0");
    hasher.update(token.as_bytes());
    hasher.finalize().into()
}

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

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut scenario = None;
    let mut listen = "127.0.0.1:4100".to_string();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--scenario" => scenario = Some(args.next().ok_or("--scenario requires ID")?),
            "--listen" => listen = args.next().ok_or("--listen requires address")?,
            _ => return Err("usage: shipsim-session [--scenario ID] [--listen HOST:PORT]".into()),
        }
    }
    Ok(Args { scenario, listen })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shipsim_core::session_protocol::ClientKind;
    use std::io::{BufRead, BufReader};
    use std::path::Path;

    fn hello() -> String {
        serde_json::to_string(&SessionMessage::Hello {
            session_protocol_version: 1,
            client_kind: ClientKind::Test,
            display_name: "test".into(),
            supported_session_versions: vec![1],
            supported_game_protocol_versions: vec![4],
        })
        .unwrap()
    }

    #[test]
    fn token_digest_is_stable_and_not_the_token() {
        assert_eq!(digest_token("abc"), digest_token("abc"));
        assert_ne!(digest_token("abc"), [0; 32]);
    }

    #[test]
    fn args_default_to_empty_lobby() {
        let args = parse_args(["--listen".into(), "127.0.0.1:0".into()].into_iter()).unwrap();
        assert_eq!(args.listen, "127.0.0.1:0");
        assert!(args.scenario.is_none());
    }

    #[test]
    fn fragmented_hello_is_valid_session_message() {
        let message: SessionMessage = serde_json::from_str(&hello()).unwrap();
        assert!(matches!(message, SessionMessage::Hello { .. }));
    }

    #[test]
    fn private_preview_response_has_request_id() {
        let game = load_scenario(Path::new("scenarios/shipyard_assault.toml")).unwrap();
        let response = dispatch(
            &game,
            &json!({"request":"reach_preview","request_id":"p1","protocol_version":4,"ship":2}),
        )
        .unwrap();
        assert_eq!(response["request_id"], "p1");
    }

    #[test]
    fn invalid_order_does_not_panic() {
        assert!(parse_order(&json!({"protocol_version":4,"type":"not_an_order"})).is_ok());
    }

    #[test]
    fn snapshot_status_is_side_relative() {
        let mut game = load_scenario(Path::new("scenarios/shipyard_assault.toml")).unwrap();
        game.set_ship_structure(6, 0).unwrap();
        game.refresh_status();
        assert_eq!(
            StateSnapshot::from_game_state_for_side(&game, SideId::A).status,
            ScenarioStatus::Won
        );
        assert_eq!(
            StateSnapshot::from_game_state_for_side(&game, SideId::B).status,
            ScenarioStatus::Lost
        );
    }

    struct TestClient {
        stream: TcpStream,
        reader: BufReader<TcpStream>,
    }

    fn test_client(address: std::net::SocketAddr) -> TestClient {
        let stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        TestClient {
            reader: BufReader::new(stream.try_clone().unwrap()),
            stream,
        }
    }

    fn send_json(client: &mut TestClient, value: Value) {
        let text = serde_json::to_string(&value).unwrap();
        client.stream.write_all(text.as_bytes()).unwrap();
        client.stream.write_all(b"\n").unwrap();
        client.stream.flush().unwrap();
    }

    fn send_fragmented(client: &mut TestClient, value: Value) {
        let text = serde_json::to_string(&value).unwrap();
        let split = text.len() / 2;
        client.stream.write_all(&text.as_bytes()[..split]).unwrap();
        client
            .stream
            .write_all(text.as_bytes()[split..].as_ref())
            .unwrap();
        client.stream.write_all(b"\n").unwrap();
        client.stream.flush().unwrap();
    }

    fn read_type(client: &mut TestClient, wanted: &str) -> Value {
        loop {
            let mut line = String::new();
            client.reader.read_line(&mut line).unwrap();
            let value: Value = serde_json::from_str(&line).unwrap();
            if value.get("type").and_then(Value::as_str) == Some(wanted) {
                return value;
            }
        }
    }

    fn read_snapshot(client: &mut TestClient) -> Value {
        loop {
            let mut line = String::new();
            client.reader.read_line(&mut line).unwrap();
            let value: Value = serde_json::from_str(&line).unwrap();
            if value.get("protocol_version").and_then(Value::as_u64) == Some(4)
                && value.get("phase").is_some()
            {
                return value;
            }
        }
    }

    fn hello_value() -> Value {
        serde_json::to_value(SessionMessage::Hello {
            session_protocol_version: 1,
            client_kind: ClientKind::Test,
            display_name: "test".into(),
            supported_session_versions: vec![1],
            supported_game_protocol_versions: vec![4],
        })
        .unwrap()
    }

    fn start_test_server() -> (std::net::SocketAddr, JoinHandle<Result<(), String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let catalog = ScenarioCatalog::load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        let handle = thread::spawn(move || run_listener(listener, catalog, None));
        (address, handle)
    }

    #[test]
    fn incompatible_client_does_not_claim_host() {
        let (address, handle) = start_test_server();
        let mut incompatible = test_client(address);
        let mut bad = hello_value();
        bad["supported_session_versions"] = json!([99]);
        send_json(&mut incompatible, bad);
        assert_eq!(
            read_type(&mut incompatible, "error")["code"],
            "unsupported_session_version"
        );

        let mut host = test_client(address);
        send_json(&mut host, hello_value());
        assert_eq!(read_type(&mut host, "welcome")["can_configure"], true);
        let _ = read_type(&mut host, "scenario_catalog");
        let _ = read_type(&mut host, "lobby_state");
        send_json(&mut host, hello_value());
        assert_eq!(read_type(&mut host, "error")["code"], "invalid_state");
        let _ = host.stream.shutdown(Shutdown::Both);
        let _ = incompatible.stream.shutdown(Shutdown::Both);
        handle.join().unwrap().unwrap();
    }

    #[test]
    fn localhost_human_lobby_has_token_ownership_and_private_previews() {
        let (address, handle) = start_test_server();
        let mut host = test_client(address);
        send_fragmented(&mut host, hello_value());
        let _ = read_type(&mut host, "welcome");
        let _ = read_type(&mut host, "scenario_catalog");
        let _ = read_type(&mut host, "lobby_state");

        let mut guest = test_client(address);
        send_json(&mut guest, hello_value());
        let _ = read_type(&mut guest, "welcome");
        let _ = read_type(&mut guest, "scenario_catalog");
        let _ = read_type(&mut guest, "lobby_state");

        send_json(
            &mut host,
            serde_json::to_value(SessionMessage::CreateMatch {
                session_protocol_version: 1,
                scenario_id: "shipyard_assault".into(),
                controllers: ControllerAssignments {
                    a: ControllerSpec::Human {},
                    b: ControllerSpec::Human {},
                },
            })
            .unwrap(),
        );
        let _ = read_type(&mut host, "seat_assigned");
        let invitation = read_type(&mut host, "seat_invitation");
        let token = invitation["join_token"].as_str().unwrap().to_string();
        let _ = read_type(&mut host, "lobby_state");

        send_json(
            &mut host,
            serde_json::to_value(SessionMessage::JoinMatch {
                session_protocol_version: 1,
                join_token: token.clone(),
                display_name: "host-cannot-steal-b".into(),
            })
            .unwrap(),
        );
        assert_eq!(
            read_type(&mut host, "error")["code"],
            "seat_already_occupied"
        );

        send_json(
            &mut guest,
            serde_json::to_value(SessionMessage::JoinMatch {
                session_protocol_version: 1,
                join_token: token,
                display_name: "guest".into(),
            })
            .unwrap(),
        );
        let _ = read_type(&mut guest, "seat_assigned");
        let _ = read_type(&mut guest, "lobby_state");
        let _ = read_snapshot(&mut guest);
        let _ = read_snapshot(&mut host);

        let custom = shipsim_core::shipyard::new_design("wire_custom");
        send_json(
            &mut guest,
            json!({"type":"purchase_custom","protocol_version":4,"side":"a","design":custom}),
        );
        assert_eq!(read_type(&mut guest, "error")["code"], "ownership");

        send_json(
            &mut guest,
            json!({"request":"reach_preview","request_id":"guest-preview","protocol_version":4,"ship":7}),
        );
        let preview = read_type(&mut guest, "reach_preview");
        assert_eq!(preview["request_id"], "guest-preview");

        send_json(
            &mut guest,
            json!({"request":"reach_preview","request_id":"private","protocol_version":4,"ship":2}),
        );
        assert_eq!(read_type(&mut guest, "error")["code"], "ownership");
        guest
            .stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let mut leaked = String::new();
        assert!(guest.reader.read_line(&mut leaked).is_err());

        let _ = host.stream.shutdown(Shutdown::Both);
        handle.join().unwrap().unwrap();
        let _ = guest.stream.shutdown(Shutdown::Both);
    }

    #[test]
    fn prestart_guest_disconnect_gets_a_fresh_one_time_invitation() {
        let (address, handle) = start_test_server();
        let mut host = test_client(address);
        send_json(&mut host, hello_value());
        let _ = read_type(&mut host, "welcome");
        let _ = read_type(&mut host, "scenario_catalog");
        let _ = read_type(&mut host, "lobby_state");

        let mut guest = test_client(address);
        send_json(&mut guest, hello_value());
        let _ = read_type(&mut guest, "welcome");
        let _ = read_type(&mut guest, "scenario_catalog");
        let _ = read_type(&mut guest, "lobby_state");

        send_json(
            &mut host,
            serde_json::to_value(SessionMessage::CreateMatch {
                session_protocol_version: 1,
                scenario_id: "shipyard_assault".into(),
                controllers: ControllerAssignments {
                    a: ControllerSpec::Human {},
                    b: ControllerSpec::Human {},
                },
            })
            .unwrap(),
        );
        let _ = read_type(&mut host, "seat_assigned");
        let first = read_type(&mut host, "seat_invitation")["join_token"]
            .as_str()
            .unwrap()
            .to_string();
        let _ = read_type(&mut host, "lobby_state");
        let _ = read_type(&mut guest, "lobby_state");

        send_json(
            &mut host,
            serde_json::to_value(SessionMessage::ParticipantStatus {
                session_protocol_version: 1,
                status: ParticipantStatus::Thinking,
            })
            .unwrap(),
        );
        let _ = read_type(&mut host, "lobby_state");
        let _ = read_type(&mut guest, "lobby_state");
        send_json(
            &mut guest,
            serde_json::to_value(SessionMessage::JoinMatch {
                session_protocol_version: 1,
                join_token: first.clone(),
                display_name: "guest".into(),
            })
            .unwrap(),
        );
        let _ = read_type(&mut guest, "seat_assigned");
        let _ = read_type(&mut guest, "lobby_state");
        let _ = read_type(&mut host, "lobby_state");
        guest.stream.shutdown(Shutdown::Both).unwrap();

        let second = read_type(&mut host, "seat_invitation")["join_token"]
            .as_str()
            .unwrap()
            .to_string();
        assert_ne!(first, second, "a disconnected seat must get a new secret");
        let _ = read_type(&mut host, "lobby_state");

        let mut replacement = test_client(address);
        send_json(&mut replacement, hello_value());
        let _ = read_type(&mut replacement, "welcome");
        let _ = read_type(&mut replacement, "scenario_catalog");
        let _ = read_type(&mut replacement, "lobby_state");
        send_json(
            &mut replacement,
            serde_json::to_value(SessionMessage::JoinMatch {
                session_protocol_version: 1,
                join_token: second,
                display_name: "replacement".into(),
            })
            .unwrap(),
        );
        let _ = read_type(&mut replacement, "seat_assigned");
        let _ = read_type(&mut replacement, "lobby_state");
        let _ = read_type(&mut host, "lobby_state");

        send_json(
            &mut host,
            serde_json::to_value(SessionMessage::ParticipantStatus {
                session_protocol_version: 1,
                status: ParticipantStatus::Ready,
            })
            .unwrap(),
        );
        let _ = read_type(&mut host, "lobby_state");
        let _ = read_snapshot(&mut host);
        let _ = read_type(&mut replacement, "lobby_state");
        let _ = read_snapshot(&mut replacement);

        host.stream.shutdown(Shutdown::Both).unwrap();
        handle.join().unwrap().unwrap();
        let _ = replacement.stream.shutdown(Shutdown::Both);
    }

    #[test]
    fn localhost_bot_lobby_uses_shared_policy_and_reaches_running() {
        let (address, handle) = start_test_server();
        let mut host = test_client(address);
        send_json(&mut host, hello_value());
        let _ = read_type(&mut host, "welcome");
        let _ = read_type(&mut host, "scenario_catalog");
        let _ = read_type(&mut host, "lobby_state");
        send_json(
            &mut host,
            serde_json::to_value(SessionMessage::CreateMatch {
                session_protocol_version: 1,
                scenario_id: "shipyard_assault".into(),
                controllers: ControllerAssignments {
                    a: ControllerSpec::Human {},
                    b: ControllerSpec::Bot {
                        policy: "greedy".into(),
                    },
                },
            })
            .unwrap(),
        );
        let _ = read_type(&mut host, "seat_assigned");
        let lobby = read_type(&mut host, "lobby_state");
        assert_eq!(lobby["state"], "running");
        let snapshot = read_snapshot(&mut host);
        assert_eq!(snapshot["protocol_version"], 4);
        assert_eq!(
            snapshot["ships_committed_path"].as_array().unwrap().len(),
            0
        );
        let _ = host.stream.shutdown(Shutdown::Both);
        handle.join().unwrap().unwrap();
    }

    #[test]
    fn negotiated_client_survives_past_handshake_deadline() {
        let (address, handle) = start_test_server();
        let mut host = test_client(address);
        send_json(&mut host, hello_value());
        let _ = read_type(&mut host, "welcome");
        let _ = read_type(&mut host, "scenario_catalog");
        let _ = read_type(&mut host, "lobby_state");

        thread::sleep(HANDSHAKE_TIMEOUT + Duration::from_millis(250));

        send_json(
            &mut host,
            serde_json::to_value(SessionMessage::CreateMatch {
                session_protocol_version: 1,
                scenario_id: "shipyard_assault".into(),
                controllers: ControllerAssignments {
                    a: ControllerSpec::Human {},
                    b: ControllerSpec::Bot {
                        policy: "greedy".into(),
                    },
                },
            })
            .unwrap(),
        );
        let _ = read_type(&mut host, "seat_assigned");
        assert_eq!(read_type(&mut host, "lobby_state")["state"], "running");
        let _ = read_snapshot(&mut host);

        let _ = host.stream.shutdown(Shutdown::Both);
        handle.join().unwrap().unwrap();
    }

    #[allow(dead_code)]
    fn read_line(stream: &mut TcpStream) -> Value {
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }
}
