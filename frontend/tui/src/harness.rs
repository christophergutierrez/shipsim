//! Subprocess wrapper around the `shipsim` binary.
//!
//! Spawns `target/debug/shipsim --scenario <path> --stdin`, writes NDJSON
//! orders to its stdin, and reads NDJSON snapshots/errors from its stdout.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use crate::protocol::{ErrorResponse, FireDecisionPreview, PathPreview, Snapshot};

/// Manages the shipsim engine subprocess.
pub struct Harness {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

/// A line from the engine: either a snapshot, a movement preview, or a soft error.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum EngineLine {
    Snapshot(Snapshot),
    PathPreview(PathPreview),
    FirePreview(FireDecisionPreview),
    Error(ErrorResponse),
    /// A line that didn't parse as either (shouldn't happen in normal play).
    Raw(String),
}

impl Harness {
    /// Spawn the engine with the given scenario path.
    pub fn spawn(engine_path: &str, scenario: &str) -> std::io::Result<Self> {
        let mut child = Command::new(engine_path)
            .arg("--scenario")
            .arg(scenario)
            .arg("--stdin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let stdin = child.stdin.take().expect("stdin pipe");
        let stdout = child.stdout.take().expect("stdout pipe");

        Ok(Harness {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    /// Send an order (one JSON line) to the engine.
    pub fn send(&mut self, json: &str) -> std::io::Result<()> {
        self.stdin.write_all(json.as_bytes())?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()
    }

    /// Read the next line from the engine. Blocks until a line is available
    /// or the engine closes its stdout.
    pub fn read_line(&mut self) -> Option<EngineLine> {
        let mut line = String::new();
        let n = self.stdout.read_line(&mut line).ok()?;
        if n == 0 {
            return None; // EOF
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        // Disambiguate by the "type" field: snapshots have no "type" (they
        // have "phase"), errors have type "error", and movement previews have
        // type "movement_preview". Parse accordingly.
        let type_field: Option<String> = serde_json::from_str::<serde_json::Value>(trimmed)
            .ok()
            .and_then(|v| v.get("type").and_then(|t| t.as_str().map(String::from)));
        match type_field.as_deref() {
            Some("error") => {
                if let Ok(err) = serde_json::from_str::<ErrorResponse>(trimmed) {
                    return Some(EngineLine::Error(err));
                }
            }
            Some("path_preview") => {
                if let Ok(preview) = serde_json::from_str::<PathPreview>(trimmed) {
                    return Some(EngineLine::PathPreview(preview));
                }
            }
            Some("fire_preview") => {
                if let Ok(preview) = serde_json::from_str::<FireDecisionPreview>(trimmed) {
                    return Some(EngineLine::FirePreview(preview));
                }
            }
            _ => {}
        }
        // No "type" field (or unrecognized) → try snapshot.
        if let Ok(snap) = serde_json::from_str::<Snapshot>(trimmed) {
            return Some(EngineLine::Snapshot(snap));
        }
        Some(EngineLine::Raw(trimmed.into()))
    }

    /// Send an order and read the next engine response.
    pub fn send_and_read(&mut self, json: &str) -> Option<EngineLine> {
        self.send(json).ok()?;
        self.read_line()
    }

    /// Kill the engine subprocess.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.kill();
    }
}
