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
    let mut command = Command::new("cargo");
    command
        .arg("run")
        .arg("--quiet")
        .arg("--bin")
        .arg("shipsim")
        .arg("--")
        .current_dir(env!("CARGO_MANIFEST_DIR"));
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
    assert!(snaps[0]["ships"][0]["power_remaining"].as_u64().is_some());
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
        // v2: fire before allocating — CommitFire is illegal in the Allocate phase.
        writeln!(
            stdin,
            r#"{{"type":"commit_fire","ship":1,"weapon":"phaser_1","target":2,"shield_facing":3}}"#
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
fn test_d8_fixture_regenerate_lock() {
    let output = shipsim_command()
        .arg("--scenario")
        .arg(manifest_path("scenarios/combat.toml"))
        .arg("--orders")
        .arg(manifest_path("scenarios/d8_frontend_orders.jsonl"))
        .output()
        .unwrap();
    assert!(output.status.success());
    std::fs::write(
        manifest_path("tests/fixtures/d8/snapshots.jsonl"),
        &output.stdout,
    )
    .unwrap();
    let expected = std::fs::read(manifest_path("tests/fixtures/d8/snapshots.jsonl")).unwrap();
    assert_eq!(output.stdout, expected);
}
