use std::path::Path;

use shipsim_core::simulation::{
    fleet::{engagement_costs, validate_engagement_costs},
    run_match, run_suite, EngagementSpec, FleetLine, FleetMapSpec, MatchConfig, SuiteSpec,
};
use shipsim_core::scenario::load_scenario_def;
use shipsim_core::simulation::fleet::build_engagement_scenario;

#[test]
fn smoke_suite_satisfies_blocking_rubrics() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(root.join("simulation/suites/smoke.toml")).unwrap();
    let mut suite: SuiteSpec = toml::from_str(&text).unwrap();
    if let Some(scenario) = suite.scenario.as_ref() {
        suite.scenario = Some(root.join(scenario));
    }
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
        scenario: Some(root.join("scenarios/m7_hull_efficiency.toml")),
        seeds: vec![1],
        max_turns: 2,
        max_orders: 1000,
        rubrics: vec![rubric_file.path().to_path_buf()],
        matchups: vec![shipsim_core::simulation::runner::MatchupSpec {
            player: "greedy".into(),
            opponent: "random".into(),
        }],
        engagements: vec![],
        power_sweeps: vec![],
        map: None,
        budget: None,
        cost_tolerance: 60,
        skip_cost_validation: false,
        stalemate_scoring: shipsim_core::simulation::StalemateScoring::None,
        data_root: Some(root.to_path_buf()),
    })
    .unwrap();
    let rubric = &report.rubrics[0];
    assert!(!rubric.passed);
    assert!(rubric
        .checks
        .iter()
        .any(|check| check.metric == "hull_class_present[TypoHuge]" && !check.passed));
}

#[test]
fn cost_matched_suite_engagements_validate_and_load() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(root.join("simulation/suites/cost_matched.toml")).unwrap();
    let suite: SuiteSpec = toml::from_str(&text).unwrap();
    assert!(!suite.engagements.is_empty());
    for engagement in &suite.engagements {
        let costs = engagement_costs(root, engagement).expect(engagement.name.as_str());
        validate_engagement_costs(&costs, &engagement.name, suite.budget, suite.cost_tolerance)
            .expect(engagement.name.as_str());
        let def = build_engagement_scenario(engagement, &FleetMapSpec::default(), 1)
            .expect(engagement.name.as_str());
        load_scenario_def(&def, root).expect(engagement.name.as_str());
    }
}

#[test]
fn annihilation_engagement_match_runs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let eng = EngagementSpec {
        name: "mirror_smoke".into(),
        player: vec![FleetLine::new("destroyer_line", 1)],
        opponent: vec![FleetLine::new("destroyer_line", 1)],
    };
    let def = build_engagement_scenario(&eng, &FleetMapSpec::default(), 3).unwrap();
    let mut config = MatchConfig::from_scenario(
        root.join("simulation/suites/engagement:mirror_smoke"),
        3,
        "greedy".into(),
        "greedy".into(),
        30,
        20_000,
    );
    config.built_scenario = Some(def);
    config.data_root = Some(root.to_path_buf());
    config.engagement = Some(eng.name);
    let result = run_match(&config).expect("engagement match");
    assert_eq!(result.metrics.rejected_orders, 0);
}
