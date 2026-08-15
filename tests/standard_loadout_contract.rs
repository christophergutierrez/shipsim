use std::collections::BTreeSet;
use std::path::Path;

use shipsim_core::combat_tables::{beam_damage, plasma_damage, torp_damage};
use shipsim_core::rules::Ruleset;
use shipsim_core::shipyard::{load_design, engine_spec, weapon_spec, BASIC_CLASS_IDS, BRUTE_CLASS_IDS, RANGE_CLASS_IDS};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn design(id: &str) -> shipsim_core::shipyard::Design {
    load_design(&root().join("data/designs").join(format!("{id}.toml"))).unwrap()
}

fn installed_power(design: &shipsim_core::shipyard::Design) -> u32 {
    let weapons = design
        .weapons
        .iter()
        .map(|weapon| weapon_spec(root(), &weapon.component).unwrap().max_charge)
        .sum::<u32>();
    weapons + design.shields.iter().map(|n| *n as u32).sum::<u32>()
}

fn close_damage(id: &str, rules: &shipsim_core::rules::CombatRules) -> u32 {
    design(id)
        .weapons
        .iter()
        .map(|weapon| {
            let spec = weapon_spec(root(), &weapon.component).unwrap();
            match spec.kind.as_str() {
                "beam" => beam_damage(rules, spec.max_charge, 1).unwrap_or(0),
                "plasma" => plasma_damage(rules, 1).unwrap_or(0),
                "torp" => torp_damage(rules, 1).unwrap_or(0),
                other => panic!("unexpected standard weapon kind {other}"),
            }
        })
        .sum()
}

#[test]
fn standard_catalog_has_three_complete_groupings() {
    assert_eq!(BASIC_CLASS_IDS.len(), 7);
    assert_eq!(RANGE_CLASS_IDS.len(), 7);
    assert_eq!(BRUTE_CLASS_IDS.len(), 7);
    for ids in [BASIC_CLASS_IDS, RANGE_CLASS_IDS, BRUTE_CLASS_IDS] {
        for (index, id) in ids.iter().enumerate() {
            let d = design(id);
            assert_eq!(d.size, (index + 1) as u32, "{id} size");
            assert_eq!(d.group, id.split('_').next().unwrap(), "{id} group");
        }
    }
}

#[test]
fn standard_sizes_one_through_five_meet_power_and_mount_contracts() {
    let caps = [2usize, 4, 8, 12, 16];
    for ids in [BASIC_CLASS_IDS, RANGE_CLASS_IDS, BRUTE_CLASS_IDS] {
        for (index, id) in ids[..5].iter().enumerate() {
            let d = design(id);
            let engine = engine_spec(root(), &d.engine, &d.engine_size).unwrap();
            assert_eq!(engine.power, [6, 14, 36, 80, 110][index], "{id} engine");
            assert_eq!(d.engine_size, ["s", "m", "l", "h", "h"][index]);
            assert!(d.weapons.len() <= caps[index], "{id} mount cap");
            assert!(installed_power(&d) * 100 >= engine.power * 60, "{id} power commitment");
            let mounts: BTreeSet<_> = d.weapons.iter().map(|w| w.mount.as_str()).collect();
            if d.weapons.len() > 1 {
                assert!(mounts.len() > 1, "{id} must spread weapons over arcs");
            }
            assert!(d.weapons.iter().all(|w| !w.component.contains("compact")), "{id} compact SKU");
            if id.starts_with("basic_") {
                assert!(d.weapons.iter().all(|w| w.component != "beam"), "{id} basic battery includes beam");
            }
        }
    }
}

#[test]
fn basic_shield_capacity_preserves_the_seventy_thirty_commitment() {
    // Shield banks are capacity, not a per-turn power bill. The contract is
    // therefore checked as a feasible 70:30 allocation: for every weapon
    // commitment, the installed capacity must cover the corresponding 30%.
    for id in BASIC_CLASS_IDS.iter().take(5) {
        let d = design(id);
        let weapon_power: u32 = d.weapons.iter().map(|w| weapon_spec(root(), &w.component).unwrap().max_charge).sum();
        let minimum_shields = (weapon_power * 3).div_ceil(7);
        assert!(d.shields.iter().map(|n| *n as u32).sum::<u32>() >= minimum_shields, "{id} shield commitment");
    }
}

#[test]
fn range_group_reaches_farther_and_brute_group_hits_harder() {
    let rules = Ruleset::load(root()).unwrap();
    let basic_range: u32 = BASIC_CLASS_IDS.iter().take(5).map(|id| {
        let d = design(id);
        d.weapons.iter().map(|w| weapon_spec(root(), &w.component).unwrap().max_range).sum::<u32>()
    }).sum();
    let long_range: u32 = RANGE_CLASS_IDS.iter().take(5).map(|id| {
        let d = design(id);
        d.weapons.iter().map(|w| weapon_spec(root(), &w.component).unwrap().max_range).sum::<u32>()
    }).sum();
    assert!(long_range > basic_range);
    for index in 1..5 {
        assert!(close_damage(BRUTE_CLASS_IDS[index], rules.combat()) > close_damage(BASIC_CLASS_IDS[index], rules.combat()), "size {} brute damage", index + 1);
    }
}

#[test]
fn capital_to_destroyer_cost_ratio_stays_in_locked_band() {
    let capital = shipsim_core::scenario::load_ship_def(root(), "basic_capital").unwrap();
    let destroyer = shipsim_core::scenario::load_ship_def(root(), "basic_destroyer").unwrap();
    let ratio = capital.cost as f64 / destroyer.cost as f64;
    assert!((30.0..=40.0).contains(&ratio), "capital/destroyer ratio {ratio:.2}");
}

#[test]
fn untouched_large_basic_hulls_retain_their_reference_batteries() {
    let dread = design("basic_dreadnought");
    let capital = design("basic_capital");
    assert_eq!(dread.engine_size, "l");
    assert_eq!(capital.engine_size, "h");
    assert_eq!(dread.weapons.len(), 7);
    assert_eq!(capital.weapons.len(), 8);
    assert!(dread.weapons.iter().any(|w| w.component == "torpedo" && w.mount == "aft_port"));
    assert!(capital.weapons.iter().any(|w| w.component == "plasma" && w.mount == "aft_starboard"));
}
