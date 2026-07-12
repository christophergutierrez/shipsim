use std::path::Path;

use shipsim_core::simulation::{run_suite, SuiteSpec};

#[test]
fn smoke_suite_satisfies_blocking_rubrics() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(root.join("simulation/suites/smoke.toml")).unwrap();
    let mut suite: SuiteSpec = toml::from_str(&text).unwrap();
    suite.scenario = root.join(&suite.scenario);
    suite.rubrics = suite
        .rubrics
        .into_iter()
        .map(|path| root.join(path))
        .collect();

    let report = run_suite(&suite).expect("smoke suite");
    assert_eq!(report.matches.len(), 8);
    assert!(report.rubrics.iter().all(|rubric| rubric.passed));
}
