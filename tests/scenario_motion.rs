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
    let game =
        load_scenario(&manifest_path("scenarios/m2_motion.toml")).expect("m2_motion loads");
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
    let game =
        load_scenario(&manifest_path("scenarios/m2_motion.toml")).expect("m2_motion loads");
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
    let game =
        load_scenario(&manifest_path("scenarios/m2_motion.toml")).expect("m2_motion loads");
    let starbase = game
        .ships()
        .iter()
        .find(|s| s.id == 2)
        .expect("starbase present");
    // Starbase has facing 0 and no explicit course; course should default to 0.
    assert_eq!(starbase.velocity.course, 0);
}
