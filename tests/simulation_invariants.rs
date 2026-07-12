use std::path::{Path, PathBuf};

use shipsim_core::game_state::ScenarioStatus;
use shipsim_core::simulation::{run_match, MatchConfig};

fn scenario() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios/simulation_duel.toml")
}

#[test]
#[ignore = "blocked on M5 fire interleaving (ADR-0022): Phase::Firing is unreachable from \
            Phase::Movement in M4, so no simulated match can ever produce damage or a \
            destruction/objective terminus yet"]
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
