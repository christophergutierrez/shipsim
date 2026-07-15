use std::path::{Path, PathBuf};

use shipsim_core::game_state::ScenarioStatus;
use shipsim_core::simulation::{run_match, MatchConfig};

fn scenario() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios/simulation_duel.toml")
}

#[test]
fn baseline_match_runs_without_illegal_orders() {
    // Hull-only destruction can leave mobile duels unfinished within a short cap;
    // this gate is legal orders + combat activity, not forced termination.
    let result = run_match(&MatchConfig::from_scenario(
        scenario(),
        7,
        "aggressive".into(),
        "mobility".into(),
        50,
        20_000,
    ))
    .expect("simulation");

    assert_eq!(result.metrics.rejected_orders, 0);
    assert!(result.metrics.orders <= 20_000);
    assert!(result.metrics.damage > 0);
    assert!(result.metrics.turns >= 1);
    let _ = result.status; // Won / Lost / InProgress all acceptable for this seed
}

#[test]
fn every_baseline_policy_completes_legal_inertial_turns() {
    for policy in ["random", "greedy", "aggressive", "defensive", "mobility"] {
        let result = run_match(&MatchConfig::from_scenario(
            scenario(),
            17,
            policy.into(),
            "greedy".into(),
            20,
            20_000,
        ))
        .unwrap_or_else(|error| panic!("{policy} failed: {error}"));
        assert_eq!(result.metrics.rejected_orders, 0, "{policy}");
        assert!(result.metrics.movement_orders > 0, "{policy} moved");
        assert!(
            result.metrics.velocity_observations > 0,
            "{policy} observed motion"
        );
    }
}
