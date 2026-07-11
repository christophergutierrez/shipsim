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

#[test]
fn save_resume_matches_uninterrupted_replay() {
    let orders = std::fs::read_to_string(manifest_path("tests/fixtures/v2/duel_orders.jsonl"))
        .expect("duel orders");
    let lines: Vec<_> = orders.lines().collect();
    let prefix_path =
        std::env::temp_dir().join(format!("shipsim-prefix-{}.jsonl", std::process::id()));
    let suffix_path =
        std::env::temp_dir().join(format!("shipsim-suffix-{}.jsonl", std::process::id()));
    let save_path = std::env::temp_dir().join(format!("shipsim-save-{}.json", std::process::id()));
    std::fs::write(&prefix_path, format!("{}\n", lines[..2].join("\n"))).unwrap();
    std::fs::write(&suffix_path, format!("{}\n", lines[2..].join("\n"))).unwrap();

    let initial = Command::new(env!("CARGO_BIN_EXE_shipsim"))
        .arg("--scenario")
        .arg(manifest_path("scenarios/combat.toml"))
        .arg("--orders")
        .arg(&prefix_path)
        .arg("--save")
        .arg(&save_path)
        .output()
        .expect("initial run");
    assert!(
        initial.status.success(),
        "{}",
        String::from_utf8_lossy(&initial.stderr)
    );

    let resumed = Command::new(env!("CARGO_BIN_EXE_shipsim"))
        .arg("--resume")
        .arg(&save_path)
        .arg("--orders")
        .arg(&suffix_path)
        .output()
        .expect("resumed run");
    assert!(
        resumed.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed.stderr)
    );

    let uninterrupted = Command::new(env!("CARGO_BIN_EXE_shipsim"))
        .arg("--scenario")
        .arg(manifest_path("scenarios/combat.toml"))
        .arg("--orders")
        .arg(manifest_path("tests/fixtures/v2/duel_orders.jsonl"))
        .output()
        .expect("uninterrupted run");
    assert!(uninterrupted.status.success());
    assert_eq!(last_line(&resumed.stdout), last_line(&uninterrupted.stdout));

    let save: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&save_path).unwrap()).unwrap();
    assert_eq!(save["protocol_version"], 1);
    assert_eq!(save["orders"].as_array().unwrap().len(), lines.len());

    let _ = std::fs::remove_file(prefix_path);
    let _ = std::fs::remove_file(suffix_path);
    let _ = std::fs::remove_file(save_path);
}
