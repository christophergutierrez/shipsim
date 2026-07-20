use std::path::{Path, PathBuf};

use shipsim_core::simulation::{run_match, MatchConfig};

fn scenario() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios/simulation_duel.toml")
}

#[test]
fn baseline_match_runs_without_illegal_orders() {
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
    assert!(result.metrics.turns >= 1);
    // Whole-match combat activity: hits+misses, not only terminal-turn events.
    assert!(
        result.metrics.hits + result.metrics.misses > 0,
        "expected fire events across the match, got hits={} misses={} damage={}",
        result.metrics.hits,
        result.metrics.misses,
        result.metrics.damage
    );
    assert!(
        result.metrics.damage > 0 || result.metrics.misses > 0,
        "expected either damage or misses from combat"
    );
}

#[test]
fn every_baseline_policy_completes_legal_path_turns() {
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
        assert!(
            result.metrics.path_orders > 0,
            "{policy} must submit at least one path (path_orders={})",
            result.metrics.path_orders
        );
        assert!(
            result.metrics.volley_orders > 0,
            "{policy} must submit at least one volley (volley_orders={})",
            result.metrics.volley_orders
        );
    }
}

#[test]
fn allocation_metrics_never_exceed_available_power() {
    let result = run_match(&MatchConfig::from_scenario(
        scenario(),
        3,
        "greedy".into(),
        "greedy".into(),
        30,
        20_000,
    ))
    .expect("simulation");
    for (class, hull) in &result.metrics.hull_efficiency {
        if hull.allocation_observations == 0 {
            continue;
        }
        assert!(
            hull.power_spent_total <= hull.power_available_total,
            "{class}: power_spent_total {} > power_available_total {}",
            hull.power_spent_total,
            hull.power_available_total
        );
    }
}

#[test]
fn combat_metrics_include_the_terminal_volley() {
    let result = run_match(&MatchConfig::from_scenario(
        scenario(),
        1,
        "aggressive".into(),
        "mobility".into(),
        20,
        20_000,
    ))
    .expect("simulation");
    let terminal_log_damage: u64 = result
        .final_snapshot
        .combat_log
        .iter()
        .map(|event| u64::from(event.damage))
        .sum();
    assert!(
        result.metrics.damage >= terminal_log_damage,
        "whole-match damage {} omitted terminal-volley damage {}",
        result.metrics.damage,
        terminal_log_damage
    );
}
