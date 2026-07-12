use std::path::{Path, PathBuf};

use shipsim_core::game_state::ScenarioStatus;
use shipsim_core::simulation::{run_match, MatchConfig};

fn scenario() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios/simulation_duel.toml")
}

#[test]
fn baseline_match_terminates_without_illegal_orders() {
    let result = run_match(&MatchConfig {
        scenario: scenario(),
        seed: 7,
        player_policy: "aggressive".into(),
        opponent_policy: "mobility".into(),
        max_turns: 50,
        max_orders: 20_000,
    })
    .expect("simulation");

    assert_ne!(result.status, ScenarioStatus::InProgress);
    assert!(result.metrics.terminated);
    assert_eq!(result.metrics.rejected_orders, 0);
    assert!(result.metrics.orders <= 20_000);
    assert!(result.metrics.damage > 0);
}

#[test]
fn every_baseline_policy_completes_legal_inertial_turns() {
    for policy in ["random", "greedy", "aggressive", "defensive", "mobility"] {
        let result = run_match(&MatchConfig {
            scenario: scenario(),
            seed: 17,
            player_policy: policy.into(),
            opponent_policy: "greedy".into(),
            max_turns: 20,
            max_orders: 20_000,
        })
        .unwrap_or_else(|error| panic!("{policy} failed: {error}"));
        assert_eq!(result.metrics.rejected_orders, 0, "{policy}");
        assert!(result.metrics.movement_orders > 0, "{policy} moved");
        assert!(
            result.metrics.velocity_observations > 0,
            "{policy} observed motion"
        );
    }
}
