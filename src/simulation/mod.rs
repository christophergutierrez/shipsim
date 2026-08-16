pub mod fleet;
pub mod metrics;
pub mod policies;
pub mod policy;
pub mod rubric;
pub mod runner;
pub mod trace;

pub use fleet::{
    build_budget_fleet, BudgetError, BudgetPolicy, EngagementSpec, FleetLine, FleetMapSpec,
    PowerSweepSpec,
};
pub use metrics::{AggregateMetrics, MatchMetrics};
pub use policies::{build_policy_for_side, policy_catalog, policy_seed, POLICY_METADATA};
pub use policy::{DecisionContext, Policy};
pub use policy::{PolicyMetadata, PurchaseContext};
pub use rubric::{EngagementBreakdown, RubricResult, RubricSpec};
pub use runner::{
    run_match, run_suite, FailedMatch, MatchConfig, MatchResult, SimulationError, StalemateScoring,
    SuiteReport, SuiteSpec,
};
pub use trace::{TraceEvent, TraceOutcome};
