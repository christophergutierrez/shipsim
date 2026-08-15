use std::collections::BTreeMap;
use std::path::Path;

use shipsim_core::scenario::load_ship_def;
use shipsim_core::shipyard::STANDARD_CLASS_IDS;
use shipsim_core::simulation::fleet::{
    build_engagement_scenario, engagement_costs, validate_engagement_costs,
};
use shipsim_core::simulation::SuiteSpec;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn yard_catalog_roles_and_costs_are_locked() {
    let mut expected = BTreeMap::from([
        ("yard_baseline", 86),
        ("yard_compact", 96),
        ("yard_potent", 96),
        ("yard_precise", 92),
    ]);
    let standard_costs = [74, 98, 216, 309, 546, 1322, 3470];
    expected.extend(STANDARD_CLASS_IDS.iter().copied().zip(standard_costs));
    for (class, cost) in expected {
        let ship = load_ship_def(root(), class).expect(class);
        assert_eq!(ship.cost, cost, "{class} cost");
        assert!(!ship.weapons.is_empty(), "{class} weapon battery");
    }

    let baseline = load_ship_def(root(), "yard_baseline").unwrap();
    let destroyer = load_ship_def(root(), "basic_destroyer").unwrap();
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

    let capital = load_ship_def(root(), "basic_capital").unwrap();
    let counts = capital
        .weapons
        .iter()
        .fold(BTreeMap::new(), |mut counts, weapon| {
            *counts.entry(weapon.kind.as_str()).or_insert(0) += 1;
            counts
        });
    assert_eq!(counts.get("beam"), Some(&4));
    assert_eq!(counts.get("torp"), Some(&2));
    assert_eq!(counts.get("plasma"), Some(&2));
    assert!(capital
        .weapons
        .iter()
        .any(|weapon| weapon.mount.as_deref() == Some("aft_port")));
}

/// Every suite under `simulation/suites/` — not a hand-picked subset. A suite
/// that declares `skip_cost_validation = false` (the default) is asserting its
/// own engagements are cost-matched, so that declaration is what gates the
/// cost check here; no separate allowlist of "which suites count" is needed,
/// and a newly added suite is covered automatically.
fn all_suite_paths() -> Vec<std::path::PathBuf> {
    let dir = root().join("simulation/suites");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .expect("read simulation/suites")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("toml"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "expected suite files under {dir:?}");
    paths
}

#[test]
fn active_catalog_suites_validate_every_engagement() {
    for path in all_suite_paths() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(&path).unwrap();
        let suite: SuiteSpec = toml::from_str(&text).expect(&name);

        for engagement in &suite.engagements {
            assert!(
                engagement.player.iter().all(|line| line.count > 0),
                "{name}/{}: zero-count player line",
                engagement.name
            );
            assert!(
                engagement.opponent.iter().all(|line| line.count > 0),
                "{name}/{}: zero-count opponent line",
                engagement.name
            );
            let map = suite.map.clone().unwrap_or_default();
            build_engagement_scenario(engagement, &map, 1)
                .unwrap_or_else(|e| panic!("{name}/{}: {e}", engagement.name));

            // A suite that has not opted out of cost enforcement must have
            // engagements that actually pass it — the comment-only promise
            // ("must not be described as equal-cost") is not a substitute for
            // a suite that plainly enforces it, e.g. via skip_cost_validation.
            if !suite.skip_cost_validation {
                let costs = engagement_costs(root(), engagement)
                    .unwrap_or_else(|e| panic!("{name}/{}: {e}", engagement.name));
                validate_engagement_costs(
                    &costs,
                    &engagement.name,
                    suite.budget,
                    suite.cost_tolerance,
                )
                .unwrap_or_else(|e| panic!("{name}/{}: {e}", engagement.name));
            }
        }
    }
}

/// Held-out seed discipline (catalog remediation review; see docs/BALANCE-PROTOCOL.md):
/// 264-327 is virgin sign-off data and 328-391 is eval data. Neither may
/// appear in an ordinary suite — burning either ahead of its evidence phase
/// is exactly the mistake the ranges exist to prevent. `abc_claims_signoff.toml`
/// is the sole declared exception: a pre-existing, explicitly named record
/// already certifying the legacy catalog on 264-327, not a leak from this
/// work. A suite gains that exemption only by carrying `_signoff` in its own
/// filename — this catches an accidental/early reference in any other suite,
/// including ones added later, without needing to update an allowlist by hand.
#[test]
fn no_suite_touches_held_out_seed_pools_without_a_declared_signoff_name() {
    for path in all_suite_paths() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.contains("_signoff") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let suite: SuiteSpec = toml::from_str(&text).expect(&name);
        for seed in &suite.seeds {
            assert!(
                !(264..=327).contains(seed),
                "{name}: seed {seed} is in the virgin sign-off range 264-327 \
                 (rename the suite to include `_signoff` if this is a real record)"
            );
            assert!(
                !(328..=391).contains(seed),
                "{name}: seed {seed} is in the evaluation range 328-391"
            );
        }
    }
}
