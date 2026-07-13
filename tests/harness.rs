use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::Value;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn parse_stdout(stdout: &[u8]) -> Vec<Value> {
    let text = std::str::from_utf8(stdout).expect("stdout is utf-8");
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("stdout line is JSON"))
        .collect()
}

fn shipsim_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_shipsim"));
    command.current_dir(env!("CARGO_MANIFEST_DIR"));
    command
}

#[test]
fn test_orders_file_emits_snapshots() {
    let output = shipsim_command()
        .arg("--scenario")
        .arg(manifest_path("scenarios/combat.toml"))
        .arg("--orders")
        .arg(manifest_path("scenarios/d8_frontend_orders.jsonl"))
        .output()
        .expect("shipsim runs");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let snaps = parse_stdout(&output.stdout);
    assert!(snaps.len() >= 2, "post-load + orders");
    assert_eq!(snaps[0]["turn"], 1);
    assert_eq!(snaps[0]["phase"], "allocate");
    assert_eq!(snaps[0]["protocol_version"], 3);
    assert!(snaps[0]["ships"][0]["power"].as_u64().is_some());
}

#[test]
fn test_soft_reject_illegal_fire() {
    use std::io::Write;
    let mut child = shipsim_command()
        .arg("--scenario")
        .arg(manifest_path("scenarios/combat.toml"))
        .arg("--stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            r#"{{"protocol_version":3,"type":"commit_fire","ship":1,"weapon":"missing_weapon","target":2,"shield_facing":3}}"#
        )
        .unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let lines = parse_stdout(&output.stdout);
    assert!(lines.iter().any(|v| v["type"] == "error"));
}

#[test]
fn test_soft_reject_missing_protocol_version_without_mutation() {
    let orders_path = std::env::temp_dir().join(format!(
        "shipsim-unversioned-orders-{}.jsonl",
        std::process::id()
    ));
    std::fs::write(
        &orders_path,
        r#"{"type":"allocate","ship":1,"movement":4,"weapons":{},"shields":[0,0,0,0,0,0]}"#,
    )
    .unwrap();
    let output = shipsim_command()
        .arg("--scenario")
        .arg(manifest_path("scenarios/combat.toml"))
        .arg("--orders")
        .arg(&orders_path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let lines: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1]["code"], "unsupported_protocol");
    assert_eq!(lines[1]["protocol_version"], 3);
    assert_eq!(lines[0]["phase"], "allocate");
    let _ = std::fs::remove_file(orders_path);
}

#[test]
fn test_soft_reject_protocol_v2_orders() {
    let orders_path = std::env::temp_dir().join(format!(
        "shipsim-v2-orders-{}.jsonl",
        std::process::id()
    ));
    std::fs::write(
        &orders_path,
        r#"{"protocol_version":2,"type":"allocate","ship":1,"movement":4,"weapons":{},"shields":[0,0,0,0,0,0]}"#,
    )
    .unwrap();
    let output = shipsim_command()
        .arg("--scenario")
        .arg(manifest_path("scenarios/combat.toml"))
        .arg("--orders")
        .arg(&orders_path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let lines: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(lines.iter().any(|v| v["code"] == "unsupported_protocol"));
    let _ = std::fs::remove_file(orders_path);
}
