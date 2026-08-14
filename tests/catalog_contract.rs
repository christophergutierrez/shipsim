use std::collections::BTreeMap;
use std::path::Path;

use shipsim_core::scenario::load_ship_def;
use shipsim_core::simulation::fleet::{
    build_engagement_scenario, engagement_costs, validate_engagement_costs, FleetMapSpec,
};
use shipsim_core::simulation::SuiteSpec;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn yard_catalog_roles_and_costs_are_locked() {
    let expected = BTreeMap::from([
        ("yard_swarm", 74),
        ("yard_destroyer", 98),
        ("yard_light_cruiser", 216),
        ("yard_heavy_cruiser", 309),
        ("yard_battleship", 546),
        ("yard_dreadnought", 1322),
        ("yard_capital", 3470),
        ("yard_baseline", 86),
        ("yard_compact", 96),
        ("yard_potent", 96),
        ("yard_precise", 92),
    ]);
    for (class, cost) in expected {
        let ship = load_ship_def(root(), class).expect(class);
        assert_eq!(ship.cost, cost, "{class} cost");
        assert!(!ship.weapons.is_empty(), "{class} weapon battery");
    }

    let baseline = load_ship_def(root(), "yard_baseline").unwrap();
    let destroyer = load_ship_def(root(), "yard_destroyer").unwrap();
    assert_eq!(baseline.weapons.len(), 1);
    assert_eq!(destroyer.weapons.len(), 2);
    assert_eq!(
        destroyer
            .weapons
            .iter()
            .filter(|weapon| weapon.kind == "torp")
            .count(),
        1
    );

    let capital = load_ship_def(root(), "yard_capital").unwrap();
    let counts = capital.weapons.iter().fold(BTreeMap::new(), |mut counts, weapon| {
        *counts.entry(weapon.kind.as_str()).or_insert(0) += 1;
        counts
    });
    assert_eq!(counts.get("beam"), Some(&4));
    assert_eq!(counts.get("torp"), Some(&2));
    assert_eq!(counts.get("plasma"), Some(&2));
    assert!(capital.weapons.iter().any(|weapon| weapon.mount.as_deref() == Some("aft_port")));
}

#[test]
fn active_catalog_suites_validate_every_engagement() {
    for name in ["weapon_quality_matched.toml", "catalog_standard.toml", "cost_matched.toml"] {
        let text = std::fs::read_to_string(root().join("simulation/suites").join(name)).unwrap();
        let suite: SuiteSpec = toml::from_str(&text).expect(name);
        assert!(!suite.skip_cost_validation, "{name} must enforce costs");
        for engagement in &suite.engagements {
            let costs = engagement_costs(root(), engagement).expect(&engagement.name);
            validate_engagement_costs(
                &costs,
                &engagement.name,
                suite.budget,
                suite.cost_tolerance,
            )
            .expect(&engagement.name);
            let map = suite.map.clone().unwrap_or_else(FleetMapSpec::default);
            build_engagement_scenario(engagement, &map, 1).expect(&engagement.name);
            assert!(engagement.player.iter().all(|line| line.count > 0));
            assert!(engagement.opponent.iter().all(|line| line.count > 0));
        }
    }
}
