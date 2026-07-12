use std::path::{Path, PathBuf};

use shipsim_core::game_state::ScenarioStatus;
use shipsim_core::simulation::{run_match, MatchConfig};

fn scenario() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios/simulation_duel.toml")
}

#[test]
#[ignore = "blocked on M7 (ADR-0022), not M5: Phase::Firing is reachable now, but simulation \
            policies still coast every movement phase (the ai::v2_move_decision/CommitManeuver \
            stub is M3-M6 scope, replaced by real maneuver selection in M7). scenarios/simulation_duel.toml \
            places the two ships 15 hexes apart — farther than any weapon's max range — so with \
            no policy ever closing that distance, no damage is ever dealt and the match never \
            reaches a terminus within max_turns. Revisit once M7 lands real maneuver-selection logic"]
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
