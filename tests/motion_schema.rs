//! M2: Schema-level motion data tests.
//!
//! Verifies that ship definitions carry explicit motion data (max_velocity,
//! thrust_per_power, power_per_thrust) and that invalid conversions reject at
//! load time.

use shipsim_core::scenario::load_scenario;
use shipsim_core::thrust::ThrustConversion;

fn manifest_path(relative: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

#[test]
fn escort_loads_with_tiny_hull_conversion() {
    let game = load_scenario(&manifest_path("scenarios/v2_duel.toml")).expect("v2_duel loads");
    let escort = game
        .ships()
        .iter()
        .find(|s| s.class == "Escort")
        .expect("escort present");
    assert_eq!(escort.max_velocity, 4);
    // Tiny hull: 4 thrust per power.
    assert_eq!(
        escort.thrust_conversion,
        ThrustConversion::new(4, 1, 4).unwrap()
    );
}

#[test]
fn heavy_cruiser_loads_with_cruiser_conversion() {
    let game = load_scenario(&manifest_path("scenarios/v2_duel.toml")).expect("v2_duel loads");
    let cruiser = game
        .ships()
        .iter()
        .find(|s| s.class == "Heavy Cruiser")
        .expect("cruiser present");
    assert_eq!(cruiser.max_velocity, 4);
    // Cruiser hull: 1:1 conversion.
    assert_eq!(
        cruiser.thrust_conversion,
        ThrustConversion::new(1, 1, 4).unwrap()
    );
}

#[test]
fn mobile_ship_unable_to_buy_one_thrust_rejects_at_load() {
    // A mobile hull (max_velocity > 0) with a conversion that produces zero
    // thrust per power must reject at load. We construct a temporary ship def
    // file and scenario to exercise this path.
    let tmp = tempfile::tempdir().expect("tempdir");
    // load_ship_def resolves ship data as <scenario_parent_parent>/data/ships/.
    // Put the scenario in a subdirectory so the data dir resolves correctly.
    let scenarios_dir = tmp.path().join("scenarios");
    std::fs::create_dir_all(&scenarios_dir).expect("mkdir scenarios");
    let data_ships = tmp.path().join("data").join("ships");
    std::fs::create_dir_all(&data_ships).expect("mkdir data/ships");

    // Broken hull: mobile (max_velocity 2) but produces zero thrust.
    std::fs::write(
        data_ships.join("broken.toml"),
        "id = \"broken\"\nname = \"Broken\"\nspeed = 2\npower = 6\nmax_velocity = 2\nthrust_per_power = 0\npower_per_thrust = 1\n",
    )
    .expect("write broken ship");

    let scenario = "width = 6\nheight = 6\nseed = 1\n[terminal]\ntype = \"destruction\"\ntarget = 1\n[[ships]]\nid = 1\nclass = \"broken\"\nq = 1\nr = 1\nfacing = 0\n";
    let scenario_path = scenarios_dir.join("broken_scenario.toml");
    std::fs::write(&scenario_path, scenario).expect("write scenario");

    let result = load_scenario(&scenario_path);
    assert!(result.is_err(), "mobile hull with zero thrust must reject");
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("invalid thrust conversion"),
        "error should mention thrust conversion, got: {err}"
    );
}
