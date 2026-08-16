//! Transport contracts used by the TUI.

#![allow(dead_code)]
//!
//! `Harness` is the production adapter. This small interface is intentionally
//! independent of ratatui so deterministic UI tests can exercise the same
//! send/drain contract without a child process or a live server.

use std::io;

pub trait Transport {
    fn send_line(&mut self, line: &str) -> io::Result<()>;
    fn drain_lines(&mut self) -> Vec<String>;
    fn is_closed(&self) -> bool;
}

#[derive(Debug, Default)]
pub struct FakeTransport {
    pub sent: Vec<String>,
    pub incoming: std::collections::VecDeque<String>,
    pub closed: bool,
}

impl FakeTransport {
    pub fn with_incoming(lines: impl IntoIterator<Item = String>) -> Self {
        Self {
            incoming: lines.into_iter().collect(),
            ..Self::default()
        }
    }
}

impl Transport for FakeTransport {
    fn send_line(&mut self, line: &str) -> io::Result<()> {
        self.sent.push(line.to_owned());
        Ok(())
    }
    fn drain_lines(&mut self) -> Vec<String> {
        self.incoming.drain(..).collect()
    }
    fn is_closed(&self) -> bool {
        self.closed
    }
}
