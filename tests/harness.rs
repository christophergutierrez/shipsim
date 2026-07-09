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
fn test_orders_file_mode_prints_winning_snapshots() {
    let output = shipsim_command()
        .arg("--scenario")
        .arg(manifest_path("scenarios/slice1.toml"))
        .arg("--orders")
        .arg(manifest_path("scenarios/slice1_orders.jsonl"))
        .output()
        .expect("shipsim binary runs");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let snapshots = parse_stdout(&output.stdout);
    assert_eq!(snapshots.len(), 4);

    let final_snapshot = snapshots.last().expect("at least one snapshot");
    assert_eq!(final_snapshot["status"], "Won");
    assert_eq!(final_snapshot["turn"], 3);
    assert_eq!(final_snapshot["ships"][0]["q"], 4);
    assert_eq!(final_snapshot["ships"][0]["r"], 1);
    assert_eq!(final_snapshot["ships"][0]["turn_mode"], 2);
}

#[test]
fn test_stdin_mode_prints_snapshot_per_order() {
    let orders = std::fs::read_to_string(manifest_path("scenarios/slice1_orders.jsonl")).unwrap();
    let mut child = shipsim_command()
        .arg("--scenario")
        .arg(manifest_path("scenarios/slice1.toml"))
        .arg("--stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shipsim binary starts");

    {
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .expect("stdin is piped")
            .write_all(orders.as_bytes())
            .expect("orders written to stdin");
    }

    let output = child.wait_with_output().expect("shipsim exits");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let snapshots = parse_stdout(&output.stdout);
    assert_eq!(snapshots.len(), 4);
    assert_eq!(snapshots.last().unwrap()["status"], "Won");
}

#[test]
fn test_orders_output_is_reproducible() {
    let first = shipsim_command()
        .arg("--scenario")
        .arg(manifest_path("scenarios/slice1.toml"))
        .arg("--orders")
        .arg(manifest_path("scenarios/slice1_orders.jsonl"))
        .output()
        .expect("first run");
    let second = shipsim_command()
        .arg("--scenario")
        .arg(manifest_path("scenarios/slice1.toml"))
        .arg("--orders")
        .arg(manifest_path("scenarios/slice1_orders.jsonl"))
        .output()
        .expect("second run");

    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(
        second.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(first.stdout, second.stdout);
}
