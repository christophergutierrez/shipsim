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
    /// Rules content identity. Optional for backwards-compatible reads of
    /// saves created before rules were externalized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules_fingerprint: Option<String>,
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
    Scenario(#[source] Box<LoadError>),
    #[error("saved order {index} is no longer legal: {source}")]
    Replay { index: usize, source: OrderError },
    #[error("save PRNG checkpoint mismatch: replayed {actual}, expected {expected}")]
    PrngMismatch { actual: u64, expected: u64 },
    #[error("save rules fingerprint mismatch: replayed {actual}, expected {expected}")]
    RulesMismatch { actual: String, expected: String },
}

impl From<LoadError> for SaveError {
    fn from(error: LoadError) -> Self {
        SaveError::Scenario(Box::new(error))
    }
}

impl SaveDocument {
    pub fn capture(scenario: PathBuf, orders: Vec<Order>, game: &GameState) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            scenario,
            orders,
            prng_state: game.prng_state(),
            rules_fingerprint: Some(game.rules_fingerprint().to_string()),
        }
    }

    /// Update the checkpoint fields (`prng_state`, `rules_fingerprint`) from a
    /// resumed game. Always sets both together: a save resumed and rewritten
    /// gains (or refreshes) its rules fingerprint, so a legacy save missing one
    /// does not silently keep missing one forever.
    pub fn update_from_checkpoint(&mut self, game: &GameState) {
        self.prng_state = game.prng_state();
        self.rules_fingerprint = Some(game.rules_fingerprint().to_string());
    }

    pub fn read(path: &Path) -> Result<Self, SaveError> {
        let text = std::fs::read_to_string(path).map_err(|source| SaveError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        // Reject unsupported document versions before deserializing the order
        // stream. An older save may carry an order shape that is no longer
        // parseable; checking the version first guarantees it fails with
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
        if let Some(expected) = &self.rules_fingerprint {
            let actual = game.rules_fingerprint().to_string();
            if &actual != expected {
                return Err(SaveError::RulesMismatch {
                    actual,
                    expected: expected.clone(),
                });
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_rejects_a_different_rules_fingerprint() {
        let scenario = PathBuf::from("scenarios/simulation_duel.toml");
        let game = load_scenario(&scenario).expect("scenario");
        let mut save = SaveDocument::capture(scenario, Vec::new(), &game);
        save.rules_fingerprint = Some("fnv1a-not-the-loaded-rules".into());
        assert!(matches!(
            save.replay(),
            Err(SaveError::RulesMismatch { .. })
        ));
    }

    #[test]
    fn old_save_without_a_fingerprint_remains_readable() {
        let scenario = PathBuf::from("scenarios/simulation_duel.toml");
        let game = load_scenario(&scenario).expect("scenario");
        let mut save = SaveDocument::capture(scenario, Vec::new(), &game);
        save.rules_fingerprint = None;
        assert!(save.replay().is_ok());
    }

    #[test]
    fn resuming_and_rewriting_a_legacy_save_gains_a_fingerprint() {
        let scenario = PathBuf::from("scenarios/simulation_duel.toml");
        let game = load_scenario(&scenario).expect("scenario");
        let mut save = SaveDocument::capture(scenario, Vec::new(), &game);
        save.rules_fingerprint = None;

        let resumed = save
            .replay()
            .expect("legacy save without fingerprint replays");
        save.update_from_checkpoint(&resumed);

        assert_eq!(
            save.rules_fingerprint.as_deref(),
            Some(resumed.rules_fingerprint())
        );
    }

    #[test]
    fn update_from_checkpoint_refreshes_prng_state_and_fingerprint_together() {
        let scenario = PathBuf::from("scenarios/simulation_duel.toml");
        let game = load_scenario(&scenario).expect("scenario");
        let mut save = SaveDocument::capture(scenario, Vec::new(), &game);
        save.prng_state = 0;
        save.rules_fingerprint = Some("stale".into());

        save.update_from_checkpoint(&game);

        assert_eq!(save.prng_state, game.prng_state());
        assert_eq!(
            save.rules_fingerprint.as_deref(),
            Some(game.rules_fingerprint())
        );
    }

    #[test]
    fn scenario_error_preserves_its_source_chain() {
        let error = SaveError::from(LoadError::InvalidFacing { facing: 9 });
        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn replay_with_recorded_ai_orders_reaches_same_phase() {
        use crate::movement::{apply_order, Order};
        use crate::path::PathAction;
        use std::collections::BTreeMap;

        let scenario = PathBuf::from("scenarios/ai.toml");
        let mut game = load_scenario(&scenario).expect("scenario");
        let mut orders = Vec::new();

        // Player allocate then AI fill-in (as the harness would record).
        let player_alloc = Order::Allocate {
            ship: 1,
            movement: 2,
            weapons: BTreeMap::from([("beam_1".into(), 2)]),
            shields: [1, 0, 0, 0, 0, 0],
        };
        apply_order(&mut game, player_alloc.clone()).expect("player alloc");
        orders.push(player_alloc);
        orders.extend(game.resolve_v2_npc_actions());

        // Player path (empty hold) + AI path commits recorded.
        let player_path = Order::CommitPath {
            ship: 1,
            actions: vec![PathAction::MoveF],
        };
        apply_order(&mut game, player_path.clone()).expect("player path");
        orders.push(player_path);
        orders.extend(game.resolve_v2_npc_actions());

        let expected_phase = game.phase_name().to_string();
        let expected_turn = game.turn_number();
        let expected_prng = game.prng_state();

        let save = SaveDocument::capture(scenario, orders, &game);
        let replayed = save.replay().expect("replay with AI orders");
        assert_eq!(replayed.phase_name(), expected_phase);
        assert_eq!(replayed.turn_number(), expected_turn);
        assert_eq!(replayed.prng_state(), expected_prng);
    }
}
