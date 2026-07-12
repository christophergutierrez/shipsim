//! M2: Scenario-level motion data tests.
//!
//! Verifies that scenarios load with explicit motion data, initial velocity is
//! validated against hull maximum, and the immobile starbase accepts
//! max_velocity = 0 but rejects nonzero initial velocity.

use shipsim_core::scenario::load_scenario;

fn manifest_path(relative: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

#[test]
fn m2_scenario_loads_with_initial_velocity() {
    let game = load_scenario(&manifest_path("scenarios/m2_motion.toml")).expect("m2_motion loads");
    let escort = game
        .ships()
        .iter()
        .find(|s| s.id == 1)
        .expect("escort present");
    assert_eq!(escort.velocity.speed, 2);
    assert_eq!(escort.velocity.course, 0);
    assert_eq!(escort.thrust_remaining, 0);
}

#[test]
fn starbase_loads_immobile_with_zero_velocity() {
    let game = load_scenario(&manifest_path("scenarios/m2_motion.toml")).expect("m2_motion loads");
    let starbase = game
        .ships()
        .iter()
        .find(|s| s.id == 2)
        .expect("starbase present");
    assert_eq!(starbase.max_velocity, 0);
    assert_eq!(starbase.velocity.speed, 0);
    // Immobile hull: thrust_per_power must be 0.
    assert_eq!(starbase.thrust_conversion.thrust_per_power, 0);
}

#[test]
fn starbase_rejects_nonzero_initial_velocity() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let scenarios_dir = tmp.path().join("scenarios");
    std::fs::create_dir_all(&scenarios_dir).expect("mkdir scenarios");
    let data_ships = tmp.path().join("data").join("ships");
    std::fs::create_dir_all(&data_ships).expect("mkdir data/ships");

    // Copy the starbase definition.
    let real_starbase = manifest_path("data/ships/starbase.toml");
    std::fs::copy(&real_starbase, data_ships.join("starbase.toml")).expect("copy starbase");

    // Scenario giving the starbase an initial velocity of 1.
    let scenario = "width = 6\nheight = 6\nseed = 1\n[terminal]\ntype = \"destruction\"\ntarget = 1\n[[ships]]\nid = 1\nclass = \"starbase\"\nq = 1\nr = 1\nfacing = 0\nvelocity = 1\n";
    let scenario_path = scenarios_dir.join("starbase_vel.toml");
    std::fs::write(&scenario_path, scenario).expect("write scenario");

    let result = load_scenario(&scenario_path);
    assert!(
        result.is_err(),
        "starbase with nonzero initial velocity must reject"
    );
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("exceeds max_velocity"),
        "error should mention velocity exceeds max, got: {err}"
    );
}

#[test]
fn initial_velocity_above_hull_maximum_rejects() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let scenarios_dir = tmp.path().join("scenarios");
    std::fs::create_dir_all(&scenarios_dir).expect("mkdir scenarios");
    let data_ships = tmp.path().join("data").join("ships");
    std::fs::create_dir_all(&data_ships).expect("mkdir data/ships");

    // Copy the escort definition (max_velocity = 4).
    let real_escort = manifest_path("data/ships/escort.toml");
    std::fs::copy(&real_escort, data_ships.join("escort.toml")).expect("copy escort");

    // Scenario giving the escort an initial velocity of 5 (above max 4).
    let scenario = "width = 6\nheight = 6\nseed = 1\n[terminal]\ntype = \"destruction\"\ntarget = 1\n[[ships]]\nid = 1\nclass = \"escort\"\nq = 1\nr = 1\nfacing = 0\nvelocity = 5\n";
    let scenario_path = scenarios_dir.join("overmax.toml");
    std::fs::write(&scenario_path, scenario).expect("write scenario");

    let result = load_scenario(&scenario_path);
    assert!(
        result.is_err(),
        "initial velocity above hull max must reject"
    );
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("exceeds max_velocity"),
        "error should mention velocity exceeds max, got: {err}"
    );
}

#[test]
fn initial_course_defaults_to_facing_when_omitted() {
    let game = load_scenario(&manifest_path("scenarios/m2_motion.toml")).expect("m2_motion loads");
    let starbase = game
        .ships()
        .iter()
        .find(|s| s.id == 2)
        .expect("starbase present");
    // Starbase has facing 0 and no explicit course; course should default to 0.
    assert_eq!(starbase.velocity.course, 0);
}

/// Build a temp scenario tree (data/ships + scenarios) rooted at `tmp` and write
/// a ship def + scenario. Returns the scenario path.
fn write_temp_scenario(
    tmp: &tempfile::TempDir,
    ship_def: &str,
    scenario: &str,
) -> std::path::PathBuf {
    let scenarios_dir = tmp.path().join("scenarios");
    std::fs::create_dir_all(&scenarios_dir).expect("mkdir scenarios");
    let data_ships = tmp.path().join("data").join("ships");
    std::fs::create_dir_all(&data_ships).expect("mkdir data/ships");

    // load_ship_def resolves ship data as <scenario_parent_parent>/data/ships/.
    std::fs::write(data_ships.join("hull.toml"), ship_def).expect("write ship def");
    let scenario_path = scenarios_dir.join("hull_scenario.toml");
    std::fs::write(&scenario_path, scenario).expect("write scenario");
    scenario_path
}

#[test]
fn hull_max_velocity_above_global_cap_rejects_even_with_zero_initial_velocity() {
    // Finding 1: a hull with max_velocity = 5 must reject at load even when the
    // ship starts at velocity 0. The global cap (MAX_VELOCITY = 4) bounds the
    // hull value itself, not just the initial velocity.
    let tmp = tempfile::tempdir().expect("tempdir");
    let ship_def = "id = \"fast\"\nname = \"Fast\"\nspeed = 3\npower = 14\nmax_velocity = 5\nthrust_per_power = 4\npower_per_thrust = 1\n";
    let scenario = "width = 6\nheight = 6\nseed = 1\n[terminal]\ntype = \"destruction\"\ntarget = 1\n[[ships]]\nid = 1\nclass = \"hull\"\nq = 1\nr = 1\nfacing = 0\n";
    let path = write_temp_scenario(&tmp, ship_def, scenario);

    let result = load_scenario(&path);
    assert!(
        result.is_err(),
        "hull max_velocity above global cap must reject"
    );
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("exceeds global maximum"),
        "error should mention global maximum, got: {err}"
    );
}

#[test]
fn mobile_hull_that_cannot_buy_one_thrust_with_design_power_rejects() {
    // Finding 2: a mobile hull with design power 14 and a 1:15 conversion
    // produces zero thrust even at full reactor output (14 / 15 = 0). It must
    // reject at load. This is distinct from thrust_per_power = 0, which is
    // caught earlier as an invalid conversion.
    let tmp = tempfile::tempdir().expect("tempdir");
    let ship_def = "id = \"underpowered\"\nname = \"Underpowered\"\nspeed = 3\npower = 14\nmax_velocity = 4\nthrust_per_power = 1\npower_per_thrust = 15\n";
    let scenario = "width = 6\nheight = 6\nseed = 1\n[terminal]\ntype = \"destruction\"\ntarget = 1\n[[ships]]\nid = 1\nclass = \"hull\"\nq = 1\nr = 1\nfacing = 0\n";
    let path = write_temp_scenario(&tmp, ship_def, scenario);

    let result = load_scenario(&path);
    assert!(
        result.is_err(),
        "mobile hull that cannot buy one thrust with design power must reject"
    );
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("cannot buy one thrust"),
        "error should mention cannot buy one thrust, got: {err}"
    );
}

#[test]
fn omitted_max_velocity_resolves_from_legacy_speed() {
    // Finding 3: when max_velocity is omitted, it is derived from legacy speed.
    // A speed-1 hull resolves to max_velocity 1; a speed-3 hull resolves to 3.
    for (speed, expected_max) in [(1u32, 1u8), (3, 3)] {
        let tmp = tempfile::tempdir().expect("tempdir");
        // power must be enough to buy one thrust at 1:1.
        let ship_def = format!(
            "id = \"legacy\"\nname = \"Legacy\"\nspeed = {speed}\npower = 6\nthrust_per_power = 1\npower_per_thrust = 1\n"
        );
        let scenario = "width = 6\nheight = 6\nseed = 1\n[terminal]\ntype = \"destruction\"\ntarget = 1\n[[ships]]\nid = 1\nclass = \"hull\"\nq = 1\nr = 1\nfacing = 0\n";
        let path = write_temp_scenario(&tmp, &ship_def, scenario);

        let game = load_scenario(&path).expect("legacy hull loads");
        let ship = game
            .ships()
            .iter()
            .find(|s| s.id == 1)
            .expect("ship present");
        assert_eq!(
            ship.max_velocity, expected_max,
            "omitted max_velocity should resolve from legacy speed {speed}"
        );
    }
}

#[test]
fn explicit_max_velocity_overrides_legacy_speed() {
    // Finding 3: an explicit max_velocity overrides the legacy speed derivation.
    // A speed-3 hull with explicit max_velocity = 2 resolves to 2, not 3.
    let tmp = tempfile::tempdir().expect("tempdir");
    let ship_def = "id = \"override\"\nname = \"Override\"\nspeed = 3\npower = 6\nmax_velocity = 2\nthrust_per_power = 1\npower_per_thrust = 1\n";
    let scenario = "width = 6\nheight = 6\nseed = 1\n[terminal]\ntype = \"destruction\"\ntarget = 1\n[[ships]]\nid = 1\nclass = \"hull\"\nq = 1\nr = 1\nfacing = 0\n";
    let path = write_temp_scenario(&tmp, ship_def, scenario);

    let game = load_scenario(&path).expect("override hull loads");
    let ship = game
        .ships()
        .iter()
        .find(|s| s.id == 1)
        .expect("ship present");
    assert_eq!(
        ship.max_velocity, 2,
        "explicit max_velocity should override legacy speed"
    );
}
