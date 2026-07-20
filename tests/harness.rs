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
    let orders =
        std::env::temp_dir().join(format!("shipsim-v4-orders-{}.jsonl", std::process::id()));
    // Minimal: allocate player ship only (AI auto-fills).
    std::fs::write(
        &orders,
        r#"{"protocol_version":4,"type":"allocate","ship":1,"movement":4,"weapons":{"beam_1":4},"shields":[2,0,0,0,0,2]}
"#,
    )
    .unwrap();
    let output = shipsim_command()
        .arg("--scenario")
        .arg(manifest_path("scenarios/ai.toml"))
        .arg("--orders")
        .arg(&orders)
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
    assert_eq!(snaps[0]["protocol_version"], 4);
    assert!(snaps[0]["ships"][0]["power"].as_u64().is_some());
    assert!(snaps[0]["ships"][0].get("max_maneuver_actions").is_some());
    assert!(snaps[0].get("velocity").is_none());
}

#[test]
fn test_class_id_emitted_on_every_ship_snapshot() {
    // Phase 1: canonical class_id is an additive snapshot field. Every ship
    // must carry a non-empty class_id equal to the scenario catalog key,
    // distinct from numeric id and display class. Duplicate display names
    // (Escort, Heavy Cruiser) must resolve to distinct class_id values.
    let output = shipsim_command()
        .arg("--scenario")
        .arg(manifest_path("scenarios/class_id_duplicates.toml"))
        .arg("--stdin")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("shipsim runs");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let snaps = parse_stdout(&output.stdout);
    assert!(!snaps.is_empty(), "at least the post-load snapshot");
    let ships = snaps[0]["ships"].as_array().expect("ships array");
    assert_eq!(ships.len(), 4, "four placed ships");

    // class_id -> (numeric id, display class) map for lookup.
    let mut by_class_id = std::collections::HashMap::new();
    for ship in ships {
        let id = ship["id"].as_u64().expect("numeric id");
        let class = ship["class"].as_str().expect("display class");
        let class_id = ship["class_id"]
            .as_str()
            .unwrap_or_else(|| panic!("ship {id} missing class_id"));
        assert!(!class_id.is_empty(), "ship {id} has empty class_id");
        // Numeric id, display class, and canonical class_id are independent.
        assert_ne!(
            class_id, class,
            "class_id must not equal display class for ship {id}"
        );
        by_class_id.insert(class_id.to_string(), (id, class.to_string()));
    }

    // Catalog keys are the expected canonical identities.
    assert_eq!(by_class_id["heavy_cruiser"].1, "Heavy Cruiser");
    assert_eq!(by_class_id["tutorial_heavy_cruiser"].1, "Heavy Cruiser");
    assert_eq!(by_class_id["escort"].1, "Escort");
    assert_eq!(by_class_id["tutorial_escort"].1, "Escort");

    // Duplicate display names resolve to distinct canonical class_id values.
    assert_ne!(
        by_class_id["heavy_cruiser"].0, by_class_id["tutorial_heavy_cruiser"].0,
        "heavy_cruiser and tutorial_heavy_cruiser are distinct ships"
    );
    assert_ne!(
        by_class_id["escort"].0, by_class_id["tutorial_escort"].0,
        "escort and tutorial_escort are distinct ships"
    );

    // Protocol remains v4.
    assert_eq!(snaps[0]["protocol_version"], 4);
}

#[test]
fn test_soft_reject_retired_v3_order() {
    use std::io::Write;
    let mut child = shipsim_command()
        .arg("--scenario")
        .arg(manifest_path("scenarios/ai.toml"))
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
            r#"{{"protocol_version":4,"type":"commit_fire","ship":1,"weapon":"beam_1","target":2,"shield_facing":3}}"#
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
fn test_soft_reject_v3_protocol_version() {
    use std::io::Write;
    let mut child = shipsim_command()
        .arg("--scenario")
        .arg(manifest_path("scenarios/ai.toml"))
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
            r#"{{"protocol_version":3,"type":"allocate","ship":1,"movement":1,"weapons":{{}},"shields":[0,0,0,0,0,0]}}"#
        )
        .unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let lines = parse_stdout(&output.stdout);
    assert!(lines
        .iter()
        .any(|v| { v["type"] == "error" && v["code"] == "unsupported_protocol" }));
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
        .arg(manifest_path("scenarios/ai.toml"))
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
    // post-load snapshot + error
    assert!(lines.len() >= 2);
    assert!(lines.iter().any(|v| v["type"] == "error"));
}

#[test]
fn test_path_preview_rejects_non_array_actions() {
    use std::io::Write;
    let mut child = shipsim_command()
        .arg("--scenario")
        .arg(manifest_path("scenarios/ai.toml"))
        .arg("--stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        // Malformed: actions is a string, not an array.
        writeln!(
            stdin,
            r#"{{"protocol_version":4,"request":"path_preview","ship":1,"actions":"move_f"}}"#
        )
        .unwrap();
        // Follow-up valid request proves the process/state survived.
        writeln!(
            stdin,
            r#"{{"protocol_version":4,"request":"path_preview","ship":1,"actions":["move_f"]}}"#
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
    assert!(
        lines
            .iter()
            .any(|v| v["type"] == "error" && v["code"] == "preview_invalid"),
        "non-array actions must produce preview_invalid; got {lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|v| v["type"] == "path_preview" && v["ok"] == true),
        "a valid follow-up request must still succeed; got {lines:?}"
    );
}

#[test]
fn test_path_preview_absent_or_empty_actions_are_legal() {
    use std::io::Write;
    let mut child = shipsim_command()
        .arg("--scenario")
        .arg(manifest_path("scenarios/ai.toml"))
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
            r#"{{"protocol_version":4,"request":"path_preview","ship":1}}"#
        )
        .unwrap();
        writeln!(
            stdin,
            r#"{{"protocol_version":4,"request":"path_preview","ship":1,"actions":[]}}"#
        )
        .unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let lines = parse_stdout(&output.stdout);
    let previews: Vec<_> = lines
        .iter()
        .filter(|v| v["type"] == "path_preview" && v["ok"] == true)
        .collect();
    assert!(
        previews.len() >= 2,
        "absent and empty actions must both succeed; got {lines:?}"
    );
}
