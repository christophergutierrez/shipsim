use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::process::{Child, Command, Stdio};
use std::thread::JoinHandle;
use std::time::Duration;

struct SessionProcess {
    child: Child,
    stderr_drain: Option<JoinHandle<()>>,
}

impl Drop for SessionProcess {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(handle) = self.stderr_drain.take() {
            let _ = handle.join();
        }
    }
}

struct Client {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
}

impl Client {
    fn connect(address: SocketAddr) -> Self {
        let stream = TcpStream::connect(address).expect("connect session client");
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let reader = BufReader::new(stream.try_clone().unwrap());
        Self { stream, reader }
    }

    fn send(&mut self, value: Value) {
        writeln!(self.stream, "{value}").unwrap();
        self.stream.flush().unwrap();
    }

    fn receive(&mut self) -> Value {
        let mut line = String::new();
        self.reader.read_line(&mut line).expect("read session line");
        assert!(!line.is_empty(), "session closed before expected response");
        serde_json::from_str(&line).expect("valid session JSON")
    }

    fn receive_type(&mut self, wanted: &str) -> Value {
        for _ in 0..100 {
            let value = self.receive();
            if value.get("type").and_then(Value::as_str) == Some(wanted) {
                return value;
            }
        }
        panic!("did not receive message type {wanted}");
    }

    fn receive_snapshot(&mut self) -> Value {
        for _ in 0..100 {
            let value = self.receive();
            if value.get("protocol_version") == Some(&json!(4)) && value.get("phase").is_some() {
                return value;
            }
        }
        panic!("did not receive a game snapshot");
    }

    fn hello(&mut self, name: &str) {
        self.send(json!({
            "type": "hello",
            "session_protocol_version": 1,
            "client_kind": "test",
            "display_name": name,
            "supported_session_versions": [1],
            "supported_game_protocol_versions": [4]
        }));
        let _ = self.receive_type("welcome");
        let _ = self.receive_type("scenario_catalog");
        let _ = self.receive_type("lobby_state");
    }
}

fn start_session() -> (SessionProcess, SocketAddr) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_shipsim-session"))
        .args(["--listen", "127.0.0.1:0"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn shipsim-session");
    let mut line = String::new();
    let mut stderr = BufReader::new(child.stderr.take().unwrap());
    stderr.read_line(&mut line).expect("read listening address");
    let address = line
        .split_whitespace()
        .find_map(|word| word.parse::<SocketAddr>().ok())
        .unwrap_or_else(|| panic!("server did not report a socket address: {line}"));
    let stderr_drain = std::thread::spawn(move || {
        for line in stderr.lines() {
            if line.is_err() {
                break;
            }
        }
    });
    (
        SessionProcess {
            child,
            stderr_drain: Some(stderr_drain),
        },
        address,
    )
}

fn ship_ids(snapshot: &Value, side: &str) -> Vec<u32> {
    snapshot["ships"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|ship| ship["side"] == side && ship["destroyed"] != true)
        .map(|ship| ship["id"].as_u64().unwrap() as u32)
        .collect()
}

fn submit_broadcast(a: &mut Client, b: &mut Client, actor: &str, order: Value) -> (Value, Value) {
    if actor == "a" {
        a.send(order);
    } else {
        b.send(order);
    }
    (a.receive_snapshot(), b.receive_snapshot())
}

fn next_hold_order(snapshot: &Value, side: &str) -> Option<Value> {
    let phase = snapshot["phase"].as_str()?;
    let completed = match phase {
        "allocate" => &snapshot["ships_allocated_this_turn"],
        "movement" => &snapshot["ships_committed_path"],
        "firing" => &snapshot["ships_committed_volley"],
        _ => return None,
    }
    .as_array()?;
    let ship = ship_ids(snapshot, side).into_iter().find(|id| {
        !completed
            .iter()
            .any(|completed_id| completed_id.as_u64() == Some(u64::from(*id)))
    })?;
    Some(match phase {
        "allocate" => json!({"type":"allocate","protocol_version":4,"ship":ship,"movement":0,"weapons":{},"shields":[0,0,0,0,0,0],"cloak":false,"repair":0,"unsquad":false,"squad_leader":null}),
        "movement" => json!({"type":"commit_path","protocol_version":4,"ship":ship,"actions":[],"evasive":0,"follow":false}),
        _ => json!({"type":"commit_volley","protocol_version":4,"ship":ship,"shots":[]}),
    })
}

#[test]
fn two_real_clients_join_and_advance_three_turns_side_b_first() {
    let (mut process, address) = start_session();
    let mut a = Client::connect(address);
    a.hello("captain-a");
    let mut b = Client::connect(address);
    b.hello("captain-b");

    a.send(json!({
        "type": "create_match",
        "session_protocol_version": 1,
        "scenario_id": "shipyard_assault",
        "controllers": {"a": {"kind": "human"}, "b": {"kind": "human"}}
    }));
    let _ = a.receive_type("seat_assigned");
    let invitation = a.receive_type("seat_invitation");
    let token = invitation["join_token"].as_str().unwrap().to_owned();
    let _ = a.receive_type("lobby_state");

    b.send(json!({
        "type": "join_match",
        "session_protocol_version": 1,
        "join_token": token,
        "display_name": "captain-b"
    }));
    let assigned = b.receive_type("seat_assigned");
    assert_eq!(assigned["side"], "b");
    let _ = b.receive_type("lobby_state");
    let mut snapshot_b = b.receive_snapshot();
    let mut snapshot_a = a.receive_snapshot();

    for expected_turn in 1..=3 {
        assert_eq!(snapshot_a["turn"], expected_turn);
        assert_eq!(snapshot_b["turn"], expected_turn);
        for phase in ["allocate", "movement", "firing"] {
            assert_eq!(snapshot_a["phase"], phase);
            let ids_b = ship_ids(&snapshot_b, "b");
            let ids_a = ship_ids(&snapshot_a, "a");
            for id in ids_b {
                let order = match phase {
                    "allocate" => json!({"type":"allocate","protocol_version":4,"ship":id,"movement":0,"weapons":{},"shields":[0,0,0,0,0,0],"cloak":false,"repair":0,"unsquad":false,"squad_leader":null}),
                    "movement" => json!({"type":"commit_path","protocol_version":4,"ship":id,"actions":[],"evasive":0,"follow":false}),
                    _ => json!({"type":"commit_volley","protocol_version":4,"ship":id,"shots":[]}),
                };
                (snapshot_a, snapshot_b) = submit_broadcast(&mut a, &mut b, "b", order);
            }
            for id in ids_a {
                let order = match phase {
                    "allocate" => json!({"type":"allocate","protocol_version":4,"ship":id,"movement":0,"weapons":{},"shields":[0,0,0,0,0,0],"cloak":false,"repair":0,"unsquad":false,"squad_leader":null}),
                    "movement" => json!({"type":"commit_path","protocol_version":4,"ship":id,"actions":[],"evasive":0,"follow":false}),
                    _ => json!({"type":"commit_volley","protocol_version":4,"ship":id,"shots":[]}),
                };
                (snapshot_a, snapshot_b) = submit_broadcast(&mut a, &mut b, "a", order);
            }
        }
    }
    assert_eq!(snapshot_a["turn"], 4);
    assert_eq!(snapshot_b["turn"], 4);

    let _ = b.stream.shutdown(Shutdown::Both);
    let disconnected = a.receive_type("error");
    assert_eq!(disconnected["code"], "participant_disconnected");
    let _ = a.stream.shutdown(Shutdown::Both);
    let status = process.child.wait().expect("wait for session server");
    assert!(status.success());
}

#[test]
fn external_fake_agent_plays_five_turns_through_real_tcp() {
    let (mut process, address) = start_session();
    let mut host = Client::connect(address);
    host.hello("captain-a");
    host.send(json!({
        "type": "create_match",
        "session_protocol_version": 1,
        "scenario_id": "shipyard_assault",
        "controllers": {"a": {"kind": "human"}, "b": {"kind": "llm_agent"}}
    }));
    let _ = host.receive_type("seat_assigned");
    let invitation = host.receive_type("seat_invitation");
    let token = invitation["join_token"].as_str().unwrap().to_owned();
    let _ = host.receive_type("lobby_state");

    let config_dir = tempfile::tempdir().unwrap();
    let config_path = config_dir.path().join("agents.toml");
    std::fs::write(
        &config_path,
        "[profiles.fake]\nkind='fake'\nmodel='deterministic-hold'\nmax_requests=1000\n",
    )
    .unwrap();
    let agent_script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("frontend/agent/shipsim-agent");
    let mut agent = Command::new("python3")
        .arg(agent_script)
        .args([
            "play",
            "--connect",
            &address.to_string(),
            "--profile",
            "fake",
            "--join-token-stdin",
        ])
        .env("SHIPSIM_AGENT_CONFIG", &config_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn fake agent");
    writeln!(agent.stdin.take().unwrap(), "{token}").unwrap();

    let mut snapshot = host.receive_snapshot();
    while snapshot["turn"].as_u64().unwrap_or(0) <= 5 {
        if let Some(order) = next_hold_order(&snapshot, "a") {
            host.send(order);
        }
        snapshot = host.receive_snapshot();
    }
    assert!(agent.try_wait().unwrap().is_none(), "agent exited before five turns");

    agent.kill().unwrap();
    let _ = agent.wait();
    let disconnected = host.receive_type("error");
    assert_eq!(disconnected["code"], "participant_disconnected");
    let _ = host.stream.shutdown(Shutdown::Both);
    let status = process.child.wait().expect("wait for session server");
    assert!(status.success());
}
