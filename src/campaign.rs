//! Multi-scenario campaigns (D10).

use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

use crate::game_state::{GameState, ScenarioStatus};
use crate::scenario::{self, LoadError};

#[derive(Debug, Error)]
pub enum CampaignError {
    #[error("campaign io: {0}")]
    Io(#[from] std::io::Error),
    #[error("campaign parse: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("campaign has no scenarios")]
    Empty,
    #[error("scenario load: {0}")]
    Scenario(#[from] LoadError),
    #[error("campaign complete")]
    Complete,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CampaignDef {
    pub name: String,
    /// Scenario paths relative to campaign file parent or repo-relative.
    pub scenarios: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Campaign {
    pub name: String,
    pub scenario_paths: Vec<PathBuf>,
    pub index: usize,
}

impl Campaign {
    pub fn load(path: &Path) -> Result<Self, CampaignError> {
        let text = std::fs::read_to_string(path)?;
        let def: CampaignDef = toml::from_str(&text)?;
        if def.scenarios.is_empty() {
            return Err(CampaignError::Empty);
        }
        let root = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let scenario_paths = def
            .scenarios
            .into_iter()
            .map(|s| {
                let p = PathBuf::from(&s);
                if p.is_absolute() {
                    p
                } else {
                    // Prefer paths relative to campaign dir, else CWD/repo-style.
                    let rel = root.join(&p);
                    if rel.exists() {
                        rel
                    } else {
                        p
                    }
                }
            })
            .collect();
        let _ = root; // used for path resolution above
        Ok(Self {
            name: def.name,
            scenario_paths,
            index: 0,
        })
    }

    pub fn current_path(&self) -> Result<&Path, CampaignError> {
        self.scenario_paths
            .get(self.index)
            .map(|p| p.as_path())
            .ok_or(CampaignError::Complete)
    }

    pub fn load_current(&self) -> Result<GameState, CampaignError> {
        let path = self.current_path()?;
        Ok(scenario::load_scenario(path)?)
    }

    pub fn is_complete(&self) -> bool {
        self.index >= self.scenario_paths.len()
    }

    /// If `game` is Won, advance to the next scenario. Returns true if advanced.
    pub fn advance_on_win(&mut self, game: &GameState) -> Result<bool, CampaignError> {
        if game.status() != ScenarioStatus::Won {
            return Ok(false);
        }
        self.index += 1;
        if self.is_complete() {
            return Err(CampaignError::Complete);
        }
        Ok(true)
    }
}
