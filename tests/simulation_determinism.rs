use std::path::{Path, PathBuf};

use shipsim_core::simulation::{run_match, MatchConfig};

fn config() -> MatchConfig {
    MatchConfig::from_scenario(
        PathBuf::from(Path::new(env!("CARGO_MANIFEST_DIR")))
            .join("scenarios/simulation_duel.toml"),
        4242,
        "random".into(),
        "greedy".into(),
        50,
        20_000,
    )
}

#[test]
fn same_seed_and_policies_produce_identical_trace() {
    let first = run_match(&config()).expect("first simulation");
    let second = run_match(&config()).expect("second simulation");

    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
}
