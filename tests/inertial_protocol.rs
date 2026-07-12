//! M6 inertial protocol contract tests (ADR-0022).
//!
//! Covers the external v2 contract:
//! - v1 movement orders (move, pass_move) soft-reject without mutation
//! - missing maneuver commitments cannot advance the movement phase
//! - v2 snapshots expose velocity/course/facing/thrust/movement_phase/commitments
//! - v1 saves fail by version before order deserialization
//! - malformed and illegal maneuver orders are soft errors without mutation
//! - only protocol v2 is emitted externally (snapshots and errors)
//!
//! Wire-level tests drive the shipsim binary as a subprocess; snapshot-field
//! tests drive the core in-process for precise field assertions.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::Value;

use shipsim_core::motion::Maneuver;
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario;
use shipsim_core::snapshot::StateSnapshot;

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

fn empty_shields() -> [u32; 6] {
    [0; 6]
}

fn charged_beam() -> BTreeMap<String, u32> {
    let mut w = BTreeMap::new();
    w.insert("beam_1".to_string(), 1);
    w
}

fn snapshot_bytes(game: &shipsim_core::game_state::GameState) -> Vec<u8> {
    serde_json::to_vec(&StateSnapshot::from_game_state(game)).expect("snapshot serializes")
}

/// Allocate both ships and return the game in the movement phase.
fn game_in_movement_phase() -> shipsim_core::game_state::GameState {
    let mut game = load_scenario(&manifest_path("scenarios/combat.toml")).expect("scenario");
    apply_order(
        &mut game,
        Order::Allocate {
            ship: 1,
            movement: 4,
            weapons: charged_beam(),
            shields: empty_shields(),
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::Allocate {
            ship: 2,
            movement: 2,
            weapons: charged_beam(),
            shields: empty_shields(),
        },
    )
    .unwrap();
    game
}

// ---------------------------------------------------------------------------
// 1. Legacy movement orders soft-reject without mutation
// ---------------------------------------------------------------------------

#[test]
fn test_legacy_move_order_soft_rejects_without_mutation() {
    let mut game = game_in_movement_phase();
    let before = snapshot_bytes(&game);

    let err = apply_order(
        &mut game,
        Order::Move {
            ship: 1,
            mode: "forward".to_string(),
        },
    )
    .unwrap_err();
    assert!(
        matches!(err, shipsim_core::movement::OrderError::MoveOrderRetired),
        "Move should be rejected as retired: {err:?}"
    );

    assert_eq!(snapshot_bytes(&game), before, "retired Move mutated state");
}

#[test]
fn test_legacy_pass_move_order_soft_rejects_without_mutation() {
    let mut game = game_in_movement_phase();
    let before = snapshot_bytes(&game);

    let err = apply_order(&mut game, Order::PassMove { ship: 1 }).unwrap_err();
    assert!(
        matches!(
            err,
            shipsim_core::movement::OrderError::PassMoveOrderRetired
        ),
        "PassMove should be rejected as retired: {err:?}"
    );

    assert_eq!(
        snapshot_bytes(&game),
        before,
        "retired PassMove mutated state"
    );
}

// ---------------------------------------------------------------------------
// 2. Missing maneuver commitments cannot advance the movement phase
// ---------------------------------------------------------------------------

#[test]
fn test_missing_commitments_block_phase_advance() {
    let mut game = game_in_movement_phase();
    // Only ship 1 commits; ship 2 has not.
    apply_order(
        &mut game,
        Order::CommitManeuver {
            ship: 1,
            maneuver: Maneuver::Coast,
        },
    )
    .unwrap();

    // The phase must still be movement — ship 2 owes a commitment.
    assert_eq!(game.phase_name(), "movement");
    assert!(game.ships_committed_this_phase().contains(&1));
    assert!(!game.ships_committed_this_phase().contains(&2));

    // Now ship 2 commits; the phase should resolve.
    apply_order(
        &mut game,
        Order::CommitManeuver {
            ship: 2,
            maneuver: Maneuver::Coast,
        },
    )
    .unwrap();
    // After all living ships commit, the movement phase resolves (to firing or
    // the next movement phase). Either way, we are no longer waiting on commits.
    assert!(
        game.ships_committed_this_phase().is_empty()
            || game.phase_name() != "movement"
            || game.movement_phase() > 1,
        "phase advanced after all ships committed"
    );
}

// ---------------------------------------------------------------------------
// 3. v2 snapshot fields and commitments
// ---------------------------------------------------------------------------

#[test]
fn test_v2_snapshot_exposes_inertial_fields_and_commitments() {
    let mut game = game_in_movement_phase();
    // Ship 1 commits a coast; ship 2 has not.
    apply_order(
        &mut game,
        Order::CommitManeuver {
            ship: 1,
            maneuver: Maneuver::Coast,
        },
    )
    .unwrap();

    let snap = StateSnapshot::from_game_state(&game);

    // Protocol version is v2.
    assert_eq!(snap.protocol_version, 2);

    // Game-level movement fields.
    assert_eq!(snap.phase, "movement");
    assert!(snap.movement_phase >= 1, "movement_phase is 1-indexed");
    assert!(
        snap.ships_committed_this_phase.contains(&1),
        "ship 1 is committed"
    );
    assert!(
        !snap.ships_committed_this_phase.contains(&2),
        "ship 2 is not committed"
    );

    // Per-ship inertial fields exist and have sane values.
    for ship in &snap.ships {
        assert!(ship.velocity <= ship.max_velocity, "velocity within max");
        assert!(ship.course <= 5, "course is a hex direction 0..=5");
        assert!(ship.facing <= 5, "facing is a hex direction 0..=5");
    }
}

// ---------------------------------------------------------------------------
// 4. v1 saves fail by version before order deserialization
// ---------------------------------------------------------------------------

#[test]
fn test_v1_save_fails_by_version_before_parse() {
    // A v1 save whose "orders" array contains a shape that would fail to
    // deserialize as a v2 Order. The version check must fire first.
    let v1_save = serde_json::json!({
        "protocol_version": 1,
        "scenario": "scenarios/combat.toml",
        "orders": [
            {"type": "bogus_retired_shape", "ship": 1, "nonsense": true}
        ],
        "prng_state": 42
    });
    let save_path =
        std::env::temp_dir().join(format!("shipsim-v1-save-{}.json", std::process::id()));
    std::fs::write(&save_path, v1_save.to_string()).unwrap();

    let err = shipsim_core::save::SaveDocument::read(&save_path).unwrap_err();
    assert!(
        matches!(
            err,
            shipsim_core::save::SaveError::UnsupportedVersion {
                actual: 1,
                expected: 2
            }
        ),
        "v1 save must fail with UnsupportedVersion, not Parse: {err:?}"
    );

    let _ = std::fs::remove_file(save_path);
}

// ---------------------------------------------------------------------------
// 5. Malformed and illegal maneuver orders are soft errors without mutation
// ---------------------------------------------------------------------------

#[test]
fn test_malformed_maneuver_order_is_soft_error_via_binary() {
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
        // Allocate both ships to reach the movement phase.
        writeln!(
            stdin,
            r#"{{"protocol_version":2,"type":"allocate","ship":1,"movement":4,"weapons":{{"beam_1":1}},"shields":[0,0,0,0,0,0]}}"#
        )
        .unwrap();
        writeln!(
            stdin,
            r#"{{"protocol_version":2,"type":"allocate","ship":2,"movement":2,"weapons":{{"beam_1":1}},"shields":[0,0,0,0,0,0]}}"#
        )
        .unwrap();
        // Malformed maneuver: missing the "maneuver" field entirely.
        writeln!(
            stdin,
            r#"{{"protocol_version":2,"type":"commit_maneuver","ship":1}}"#
        )
        .unwrap();
        // A valid order after the rejection exposes the continuing aggregate state.
        writeln!(
            stdin,
            r#"{{"protocol_version":2,"type":"commit_maneuver","ship":1,"maneuver":{{"type":"coast"}}}}"#
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
    // post-load + 2 allocates + error + valid continuation snapshot
    assert!(lines.len() >= 5, "expected error and continuation snapshot");
    let error_line = &lines[3];
    assert_eq!(error_line["type"], "error", "malformed order is an error");
    assert_eq!(
        error_line["code"], "parse_error",
        "malformed order is parse_error"
    );
    assert_eq!(error_line["protocol_version"], 2, "error carries v2");

    let mut valid = shipsim_command()
        .arg("--scenario")
        .arg(manifest_path("scenarios/combat.toml"))
        .arg("--stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = valid.stdin.as_mut().unwrap();
        writeln!(stdin, r#"{{"protocol_version":2,"type":"allocate","ship":1,"movement":4,"weapons":{{"beam_1":1}},"shields":[0,0,0,0,0,0]}}"#).unwrap();
        writeln!(stdin, r#"{{"protocol_version":2,"type":"allocate","ship":2,"movement":2,"weapons":{{"beam_1":1}},"shields":[0,0,0,0,0,0]}}"#).unwrap();
        writeln!(stdin, r#"{{"protocol_version":2,"type":"commit_maneuver","ship":1,"maneuver":{{"type":"coast"}}}}"#).unwrap();
    }
    let valid_lines = parse_stdout(&valid.wait_with_output().unwrap().stdout);
    assert_eq!(
        lines.last().unwrap(),
        valid_lines.last().unwrap(),
        "malformed wire order changed the subsequent aggregate snapshot"
    );
}

#[test]
fn test_illegal_maneuver_order_is_soft_error_without_mutation() {
    let mut game = game_in_movement_phase();

    // Commit the same ship twice in one phase — the second is illegal.
    apply_order(
        &mut game,
        Order::CommitManeuver {
            ship: 1,
            maneuver: Maneuver::Coast,
        },
    )
    .unwrap();
    let before_duplicate = snapshot_bytes(&game);
    let err = apply_order(
        &mut game,
        Order::CommitManeuver {
            ship: 1,
            maneuver: Maneuver::Coast,
        },
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            shipsim_core::movement::OrderError::AlreadyCommittedThisPhase(1)
        ),
        "double-commit should be AlreadyCommittedThisPhase: {err:?}"
    );

    assert_eq!(
        snapshot_bytes(&game),
        before_duplicate,
        "duplicate commit mutated state"
    );
}

#[test]
fn test_insufficient_thrust_rejection_preserves_complete_snapshot() {
    let mut game = load_scenario(&manifest_path("scenarios/combat.toml")).unwrap();
    for ship in [1, 2] {
        apply_order(
            &mut game,
            Order::Allocate {
                ship,
                movement: 0,
                weapons: BTreeMap::new(),
                shields: empty_shields(),
            },
        )
        .unwrap();
    }
    let before = snapshot_bytes(&game);
    let err = apply_order(
        &mut game,
        Order::CommitManeuver {
            ship: 1,
            maneuver: Maneuver::TurnCoursePort,
        },
    )
    .unwrap_err();
    assert!(matches!(
        err,
        shipsim_core::movement::OrderError::InsufficientThrust { .. }
    ));
    assert_eq!(
        snapshot_bytes(&game),
        before,
        "insufficient thrust mutated state"
    );
}

#[test]
fn test_illegal_maneuver_rejection_preserves_complete_snapshot() {
    let mut game = game_in_movement_phase();
    let before = snapshot_bytes(&game);
    let err = apply_order(
        &mut game,
        Order::CommitManeuver {
            ship: 1,
            maneuver: Maneuver::Decelerate,
        },
    )
    .unwrap_err();
    assert!(matches!(
        err,
        shipsim_core::movement::OrderError::IllegalManeuver { .. }
    ));
    assert_eq!(
        snapshot_bytes(&game),
        before,
        "illegal maneuver mutated state"
    );
}

// ---------------------------------------------------------------------------
// 6. Only protocol v2 is emitted externally
// ---------------------------------------------------------------------------

#[test]
fn test_only_v2_emitted_externally() {
    let output = Command::new(env!("CARGO_BIN_EXE_shipsim"))
        .arg("--scenario")
        .arg(manifest_path("scenarios/combat.toml"))
        .arg("--orders")
        .arg(manifest_path("tests/fixtures/v2/duel_orders.jsonl"))
        .output()
        .expect("shipsim binary");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let lines = parse_stdout(&output.stdout);
    assert!(lines.len() >= 2, "at least post-load + one snapshot");
    for (i, line) in lines.iter().enumerate() {
        assert_eq!(
            line["protocol_version"], 2,
            "line {i} emits protocol_version 2"
        );
    }
}

#[test]
fn test_v1_order_soft_rejects_with_v2_error_via_binary() {
    let orders_path = std::env::temp_dir().join(format!(
        "shipsim-v1-move-orders-{}.jsonl",
        std::process::id()
    ));
    std::fs::write(
        &orders_path,
        r#"{"protocol_version":1,"type":"pass_move","ship":1}"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_shipsim"))
        .arg("--scenario")
        .arg(manifest_path("scenarios/combat.toml"))
        .arg("--orders")
        .arg(&orders_path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let lines = parse_stdout(&output.stdout);
    assert_eq!(lines.len(), 2, "post-load + one error");
    assert_eq!(lines[1]["type"], "error");
    assert_eq!(lines[1]["code"], "unsupported_protocol");
    assert_eq!(lines[1]["protocol_version"], 2, "error carries v2");
    // The post-load snapshot is still v2.
    assert_eq!(lines[0]["protocol_version"], 2);
    assert_eq!(
        lines[0]["phase"], "allocate",
        "no mutation from rejected order"
    );
    let _ = std::fs::remove_file(orders_path);
}
