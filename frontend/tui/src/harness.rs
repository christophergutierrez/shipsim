//! Local subprocess and asynchronous TCP transports for the TUI.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use crate::protocol::{ErrorResponse, FireDecisionPreview, PathPreview, Snapshot};
use shipsim_core::session_protocol::SessionMessage;

pub struct Harness {
    kind: HarnessKind,
}

enum HarnessKind {
    Local {
        child: Child,
        stdin: ChildStdin,
        stdout: BufReader<ChildStdout>,
    },
    Network {
        stream: TcpStream,
        lines: Receiver<Option<String>>,
    },
    #[cfg(test)]
    Fake {
        tx: mpsc::Sender<String>,
        lines: Receiver<Option<String>>,
    },
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum EngineLine {
    Snapshot(Snapshot),
    PathPreview(PathPreview),
    FirePreview(FireDecisionPreview),
    Error(ErrorResponse),
    Session(SessionMessage),
    Eof,
    Raw(String),
}

impl Harness {
    pub fn spawn(engine_path: &str, scenario: &str) -> std::io::Result<Self> {
        let mut child = Command::new(engine_path)
            .arg("--scenario")
            .arg(scenario)
            .arg("--stdin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        Ok(Self {
            kind: HarnessKind::Local {
                stdin: child.stdin.take().expect("stdin pipe"),
                stdout: BufReader::new(child.stdout.take().expect("stdout pipe")),
                child,
            },
        })
    }

    pub fn connect(address: &str) -> std::io::Result<Self> {
        let stream = TcpStream::connect(address)?;
        stream.set_nodelay(true)?;
        let reader = stream.try_clone()?;
        let (tx, lines) = mpsc::channel();
        thread::Builder::new()
            .name("shipsim-tui-network-reader".into())
            .spawn(move || {
                let mut reader = BufReader::new(reader);
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) => {
                            let _ = tx.send(None);
                            break;
                        }
                        Ok(_) => {
                            let _ = tx.send(Some(line));
                        }
                        Err(_) => {
                            let _ = tx.send(None);
                            break;
                        }
                    }
                }
            })?;
        Ok(Self {
            kind: HarnessKind::Network { stream, lines },
        })
    }

    #[cfg(test)]
    pub fn fake(lines: Vec<String>) -> Self {
        let (input_tx, _input_rx) = mpsc::channel();
        let (output_tx, output_rx) = mpsc::channel();
        thread::spawn(move || {
            for line in lines {
                let _ = output_tx.send(Some(line));
            }
            let _ = output_tx.send(None);
        });
        Self {
            kind: HarnessKind::Fake {
                tx: input_tx,
                lines: output_rx,
            },
        }
    }

    pub fn is_network(&self) -> bool {
        matches!(self.kind, HarnessKind::Network { .. })
    }

    pub fn send_session(&mut self, message: &SessionMessage) -> std::io::Result<()> {
        let json = serde_json::to_string(message).map_err(std::io::Error::other)?;
        self.send(&json)
    }

    pub fn send(&mut self, json: &str) -> std::io::Result<()> {
        match &mut self.kind {
            HarnessKind::Local { stdin, .. } => {
                stdin.write_all(json.as_bytes())?;
                stdin.write_all(b"\n")?;
                stdin.flush()
            }
            HarnessKind::Network { stream, .. } => {
                stream.write_all(json.as_bytes())?;
                stream.write_all(b"\n")?;
                stream.flush()
            }
            #[cfg(test)]
            HarnessKind::Fake { tx, .. } => tx.send(json.into()).map_err(std::io::Error::other),
        }
    }

    pub fn read_line(&mut self) -> Option<EngineLine> {
        let line = match &mut self.kind {
            HarnessKind::Local { stdout, .. } => {
                let mut line = String::new();
                stdout.read_line(&mut line).ok()?;
                if line.is_empty() {
                    return None;
                }
                line
            }
            HarnessKind::Network { lines, .. } => lines.recv().ok()??,
            #[cfg(test)]
            HarnessKind::Fake { lines, .. } => lines.recv().ok()??,
        };
        Some(parse_line(&line))
    }

    #[allow(clippy::while_let_loop)]
    pub fn try_read_lines(&mut self) -> Vec<EngineLine> {
        let mut result = Vec::new();
        loop {
            let next = match &mut self.kind {
                HarnessKind::Network { lines, .. } => lines.try_recv(),
                #[cfg(test)]
                HarnessKind::Fake { lines, .. } => lines.try_recv(),
                HarnessKind::Local { .. } => break,
            };
            match next {
                Ok(Some(line)) => result.push(parse_line(&line)),
                Ok(None) => {
                    result.push(EngineLine::Eof);
                    break;
                }
                Err(mpsc::TryRecvError::Empty) | Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }
        result
    }

    pub fn send_and_read(&mut self, json: &str) -> Option<EngineLine> {
        self.send(json).ok()?;
        self.read_line()
    }

    pub fn kill(&mut self) {
        if let HarnessKind::Local { child, .. } = &mut self.kind {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn parse_line(line: &str) -> EngineLine {
    let trimmed = line.trim();
    let kind = serde_json::from_str::<serde_json::Value>(trimmed)
        .ok()
        .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(str::to_owned));
    match kind.as_deref() {
        Some("error") => serde_json::from_str(trimmed)
            .map(EngineLine::Error)
            .unwrap_or_else(|_| EngineLine::Raw(trimmed.into())),
        Some("path_preview") => serde_json::from_str(trimmed)
            .map(EngineLine::PathPreview)
            .unwrap_or_else(|_| EngineLine::Raw(trimmed.into())),
        Some("fire_preview") => serde_json::from_str(trimmed)
            .map(EngineLine::FirePreview)
            .unwrap_or_else(|_| EngineLine::Raw(trimmed.into())),
        _ => serde_json::from_str(trimmed)
            .map(EngineLine::Snapshot)
            .or_else(|_| serde_json::from_str(trimmed).map(EngineLine::Session))
            .unwrap_or_else(|_| EngineLine::Raw(trimmed.into())),
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.kill();
    }
}

impl crate::transport::Transport for Harness {
    fn send_line(&mut self, line: &str) -> std::io::Result<()> {
        self.send(line)
    }
    fn drain_lines(&mut self) -> Vec<String> {
        self.try_read_lines()
            .into_iter()
            .map(|line| match line {
                EngineLine::Raw(raw) => raw,
                EngineLine::Eof => String::new(),
                other => format!("{other:?}"),
            })
            .collect()
    }
    fn is_closed(&self) -> bool {
        false
    }
}

pub fn redact_for_log(line: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(line) else {
        return line.into();
    };
    if let Some(object) = value.as_object_mut() {
        for key in ["join_token", "authorization", "api_key", "token"] {
            if object.contains_key(key) {
                object.insert(key.into(), serde_json::Value::String("[REDACTED]".into()));
            }
        }
    }
    serde_json::to_string(&value).unwrap_or_else(|_| "[REDACTED]".into())
}

#[cfg(test)]
mod tests {
    use super::redact_for_log;
    #[test]
    fn redaction_removes_secret_values() {
        let output = redact_for_log(r#"{"join_token":"secret","authorization":"Bearer secret"}"#);
        assert!(!output.contains("secret"));
        assert!(output.contains("REDACTED"));
    }
}
