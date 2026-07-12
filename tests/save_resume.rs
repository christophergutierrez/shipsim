use std::path::{Path, PathBuf};
use std::process::Command;

fn manifest_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn last_line(bytes: &[u8]) -> &[u8] {
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .next_back()
        .expect("at least one NDJSON line")
}

fn snapshots(bytes: &[u8]) -> Vec<serde_json::Value> {
    std::str::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|value: &serde_json::Value| value["phase"].is_string())
        .collect()
}

/// Run shipsim with a scenario + orders file, optionally saving to `save_path`.
fn run_scenario_orders(orders: &Path, save: Option<&Path>) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_shipsim"));
    cmd.arg("--scenario")
        .arg(manifest_path("scenarios/m5_turn_loop.toml"))
        .arg("--orders")
        .arg(orders);
    if let Some(save) = save {
        cmd.arg("--save").arg(save);
    }
    cmd.output().expect("shipsim run")
}

/// Resume from a save file with additional orders.
fn run_resume(save: &Path, orders: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_shipsim"))
        .arg("--resume")
        .arg(save)
        .arg("--orders")
        .arg(orders)
        .output()
        .expect("resumed run")
}

/// Assert that splitting the order stream at `split_index` and resuming from a
/// save produces the same final snapshot bytes as an uninterrupted run.
fn assert_split_matches_uninterrupted(split_index: usize) {
    let orders =
        std::fs::read_to_string(manifest_path("tests/fixtures/v2/save_resume_orders.jsonl"))
            .expect("duel orders");
    let lines: Vec<_> = orders.lines().collect();
    assert!(
        split_index > 0 && split_index < lines.len(),
        "split_index must be interior"
    );

    let prefix_path = std::env::temp_dir().join(format!(
        "shipsim-prefix-{}-{}.jsonl",
        std::process::id(),
        split_index
    ));
    let suffix_path = std::env::temp_dir().join(format!(
        "shipsim-suffix-{}-{}.jsonl",
        std::process::id(),
        split_index
    ));
    let save_path = std::env::temp_dir().join(format!(
        "shipsim-save-{}-{}.json",
        std::process::id(),
        split_index
    ));
    std::fs::write(
        &prefix_path,
        format!("{}\n", lines[..split_index].join("\n")),
    )
    .unwrap();
    std::fs::write(
        &suffix_path,
        format!("{}\n", lines[split_index..].join("\n")),
    )
    .unwrap();

    let initial = run_scenario_orders(&prefix_path, Some(&save_path));
    assert!(
        initial.status.success(),
        "initial run (split {}): {}",
        split_index,
        String::from_utf8_lossy(&initial.stderr)
    );

    let resumed = run_resume(&save_path, &suffix_path);
    assert!(
        resumed.status.success(),
        "resumed run (split {}): {}",
        split_index,
        String::from_utf8_lossy(&resumed.stderr)
    );

    let uninterrupted = run_scenario_orders(
        &manifest_path("tests/fixtures/v2/save_resume_orders.jsonl"),
        None,
    );
    assert!(uninterrupted.status.success());
    assert_eq!(
        last_line(&resumed.stdout),
        last_line(&uninterrupted.stdout),
        "final snapshot mismatch at split {}",
        split_index
    );

    let _ = std::fs::remove_file(prefix_path);
    let _ = std::fs::remove_file(suffix_path);
    let _ = std::fs::remove_file(save_path);
}

#[test]
fn save_resume_matches_uninterrupted_replay() {
    let orders =
        std::fs::read_to_string(manifest_path("tests/fixtures/v2/save_resume_orders.jsonl"))
            .expect("save/resume orders");
    let lines: Vec<_> = orders.lines().collect();

    let uninterrupted = run_scenario_orders(
        &manifest_path("tests/fixtures/v2/save_resume_orders.jsonl"),
        None,
    );
    assert!(uninterrupted.status.success());
    let replay_snapshots = snapshots(&uninterrupted.stdout);
    let movement_phases: std::collections::BTreeSet<_> = replay_snapshots
        .iter()
        .filter(|snap| snap["phase"] == "movement")
        .map(|snap| snap["movement_phase"].as_u64().unwrap())
        .collect();
    assert_eq!(movement_phases, [1, 2, 3, 4].into_iter().collect());
    let firing_phases: std::collections::BTreeSet<_> = replay_snapshots
        .iter()
        .filter(|snap| snap["phase"] == "firing")
        .map(|snap| snap["movement_phase"].as_u64().unwrap())
        .collect();
    assert_eq!(firing_phases, [1, 2, 3, 4].into_iter().collect());
    assert!(replay_snapshots.iter().any(|snap| {
        snap["phase"] == "movement" && snap["ships_committed_this_phase"] == serde_json::json!([1])
    }));
    assert!(replay_snapshots.iter().any(|snap| {
        snap["phase"] == "firing" && snap["ships_ready_fire"] == serde_json::json!([1])
    }));

    // Every interior boundary covers a meaningful save state across all four
    // movement phases and all four firing windows. The stream deliberately
    // omits EndTurn until phase 4 is complete.
    for split_index in 1..lines.len() {
        assert_split_matches_uninterrupted(split_index);
    }

    // Verify the saved document itself is v2 and carries all orders.
    let save_path =
        std::env::temp_dir().join(format!("shipsim-save-full-{}.json", std::process::id()));
    let _ = run_scenario_orders(
        &manifest_path("tests/fixtures/v2/save_resume_orders.jsonl"),
        Some(&save_path),
    );
    let save: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&save_path).unwrap()).unwrap();
    assert_eq!(save["protocol_version"], 2);
    assert_eq!(save["orders"].as_array().unwrap().len(), lines.len());
    let _ = std::fs::remove_file(save_path);
}
