use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::game_state::GameState;
use crate::movement::{apply_order, Order, OrderError};
use crate::protocol::PROTOCOL_VERSION;
use crate::scenario::{load_scenario, LoadError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveDocument {
    pub protocol_version: u32,
    pub scenario: PathBuf,
    pub orders: Vec<Order>,
    pub prng_state: u64,
}

/// Minimal view used to check a save document's protocol version *before*
/// deserializing the (potentially incompatible) order stream (ADR-0022 M6).
///
/// Unknown fields are ignored so a v1 save carrying a retired order shape still
/// probes successfully and is rejected by version, not by a parse error.
#[derive(Debug, Deserialize)]
struct SaveVersionProbe {
    protocol_version: u32,
}

#[derive(Debug, Error)]
pub enum SaveError {
    #[error("cannot read save {path:?}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot write save {path:?}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot parse save {path:?}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("unsupported protocol version {actual}; expected {expected}")]
    UnsupportedVersion { actual: u32, expected: u32 },
    #[error("cannot load saved scenario: {0}")]
    Scenario(#[from] LoadError),
    #[error("saved order {index} is no longer legal: {source}")]
    Replay { index: usize, source: OrderError },
    #[error("save PRNG checkpoint mismatch: replayed {actual}, expected {expected}")]
    PrngMismatch { actual: u64, expected: u64 },
}

impl SaveDocument {
    pub fn capture(scenario: PathBuf, orders: Vec<Order>, game: &GameState) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            scenario,
            orders,
            prng_state: game.prng_state(),
        }
    }

    pub fn read(path: &Path) -> Result<Self, SaveError> {
        let text = std::fs::read_to_string(path).map_err(|source| SaveError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        // M6 (ADR-0022): reject unsupported document versions *before* deserializing
        // the order stream. A v1 save may carry an order shape that is no longer
        // parseable; checking the version first guarantees such a save fails with
        // `UnsupportedVersion` rather than `Parse`.
        let version_probe: SaveVersionProbe =
            serde_json::from_str(&text).map_err(|source| SaveError::Parse {
                path: path.to_path_buf(),
                source,
            })?;
        if version_probe.protocol_version != PROTOCOL_VERSION {
            return Err(SaveError::UnsupportedVersion {
                actual: version_probe.protocol_version,
                expected: PROTOCOL_VERSION,
            });
        }
        let document = serde_json::from_str::<Self>(&text).map_err(|source| SaveError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(document)
    }

    pub fn write(&self, path: &Path) -> Result<(), SaveError> {
        let bytes = serde_json::to_vec_pretty(self).expect("SaveDocument serialization");
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("shipsim-save");
        let temporary = path.with_file_name(format!(".{file_name}.tmp"));
        std::fs::write(&temporary, bytes).map_err(|source| SaveError::Write {
            path: path.to_path_buf(),
            source,
        })?;
        std::fs::rename(&temporary, path).map_err(|source| SaveError::Write {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn replay(&self) -> Result<GameState, SaveError> {
        let mut game = load_scenario(&self.scenario)?;
        for (index, order) in self.orders.iter().cloned().enumerate() {
            apply_order(&mut game, order).map_err(|source| SaveError::Replay { index, source })?;
        }
        let actual = game.prng_state();
        if actual != self.prng_state {
            return Err(SaveError::PrngMismatch {
                actual,
                expected: self.prng_state,
            });
        }
        Ok(game)
    }
}
