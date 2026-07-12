pub mod metrics;
pub mod policies;
pub mod policy;
pub mod rubric;
pub mod runner;
pub mod trace;

pub use metrics::{AggregateMetrics, MatchMetrics};
pub use policy::{DecisionContext, Policy};
pub use rubric::{RubricResult, RubricSpec};
pub use runner::{
    run_match, run_suite, MatchConfig, MatchResult, SimulationError, SuiteReport, SuiteSpec,
};
pub use trace::{TraceEvent, TraceOutcome};
