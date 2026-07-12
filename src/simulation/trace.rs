use serde::Serialize;

use crate::game_state::ScenarioStatus;
use crate::movement::Order;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceOutcome {
    Accepted,
    Rejected { error: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct TraceEvent {
    pub sequence: usize,
    pub turn: u32,
    pub phase: String,
    pub actor: Option<u32>,
    pub policy: String,
    pub order: Order,
    pub outcome: TraceOutcome,
    pub status_after: ScenarioStatus,
    pub prng_state_after: u64,
}
