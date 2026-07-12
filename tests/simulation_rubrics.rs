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

#[test]
fn missing_hull_class_fails_through_toml_rubric_path() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let rubric_file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        rubric_file.path(),
        "id='missing_hull'\ndescription='must observe configured hull'\nhull_class='TypoHuge'\nmax_hull_zero_velocity_rate=0.5\n",
    )
    .unwrap();
    let report = run_suite(&SuiteSpec {
        name: "missing hull test".into(),
        scenario: root.join("scenarios/m7_hull_efficiency.toml"),
        seeds: vec![1],
        max_turns: 2,
        max_orders: 1000,
        rubrics: vec![rubric_file.path().to_path_buf()],
        matchups: vec![shipsim_core::simulation::runner::MatchupSpec {
            player: "greedy".into(),
            opponent: "random".into(),
        }],
    })
    .unwrap();
    let rubric = &report.rubrics[0];
    assert!(!rubric.passed);
    assert!(rubric
        .checks
        .iter()
        .any(|check| check.metric == "hull_class_present[TypoHuge]" && !check.passed));
}
