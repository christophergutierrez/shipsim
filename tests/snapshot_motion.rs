//! M2: Snapshot motion data tests.
//!
//! Verifies that the protocol snapshot exposes velocity, course, facing, thrust
//! conversion, and thrust remaining, and that turn reset preserves velocity
//! while clearing the per-turn thrust reserve.

use shipsim_core::motion::Velocity;
use shipsim_core::scenario::load_scenario;
use shipsim_core::ship::Ship;
use shipsim_core::snapshot::StateSnapshot;
use shipsim_core::thrust::ThrustConversion;

fn manifest_path(relative: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

#[test]
fn snapshot_exposes_motion_fields() {
    let game =
        load_scenario(&manifest_path("scenarios/m2_motion.toml")).expect("m2_motion loads");
    let snap = StateSnapshot::from_game_state(&game);

    let escort = snap
        .ships
        .iter()
        .find(|s| s.id == 1)
        .expect("escort in snapshot");
    assert_eq!(escort.max_velocity, 4);
    assert_eq!(escort.thrust_per_power, 4);
    assert_eq!(escort.power_per_thrust, 1);
    assert_eq!(escort.velocity, 2);
    assert_eq!(escort.course, 0);
    assert_eq!(escort.facing, 0);
    assert_eq!(escort.thrust_remaining, 0);

    let starbase = snap
        .ships
        .iter()
        .find(|s| s.id == 2)
        .expect("starbase in snapshot");
    assert_eq!(starbase.max_velocity, 0);
    assert_eq!(starbase.thrust_per_power, 0);
    assert_eq!(starbase.velocity, 0);
}

#[test]
fn turn_reset_preserves_velocity_but_clears_thrust_remaining() {
    // Build a ship directly to test reset_v2_allocation in isolation, since
    // ship_mut is pub(crate) and allocate_v2 does not yet write thrust_remaining
    // (that is M3's job).
    let mut ship = Ship {
        id: 1,
        class: "t".into(),
        pos: shipsim_core::hex::Hex::new(0, 0),
        facing: 0,
        speed: 4,
        power: 8,
        weapons: vec![],
        shields_powered: [0; 6],
        shields_remaining: [0; 6],
        max_shield_per_facing: 6,
        movement_allocated: 0,
        move_remaining: 0,
        keel: shipsim_core::momentum::Keel::Stopped,
        weapon_charges: std::collections::BTreeMap::new(),
        ssd: shipsim_core::ssd::Ssd::new(10, 4, 2, 0),
        destroyed: false,
        max_velocity: 4,
        thrust_conversion: ThrustConversion::new(1, 1, 4).unwrap(),
        velocity: Velocity::new(3, 2).unwrap(),
        thrust_remaining: 5,
    };

    ship.reset_v2_allocation();

    // Velocity persists across turns.
    assert_eq!(ship.velocity.speed, 3);
    assert_eq!(ship.velocity.course, 2);
    // Thrust reserve is cleared.
    assert_eq!(ship.thrust_remaining, 0);
    // Legacy fields are also cleared (bridge compatibility).
    assert_eq!(ship.move_remaining, 0);
    assert_eq!(ship.movement_allocated, 0);
}

#[test]
fn snapshot_motion_fields_serialize_as_json() {
    let game =
        load_scenario(&manifest_path("scenarios/m2_motion.toml")).expect("m2_motion loads");
    let snap = StateSnapshot::from_game_state(&game);
    let json = serde_json::to_string(&snap).expect("snapshot serializes");

    // Verify the new fields appear in the JSON output.
    assert!(json.contains("max_velocity"), "max_velocity in JSON");
    assert!(json.contains("thrust_per_power"), "thrust_per_power in JSON");
    assert!(json.contains("power_per_thrust"), "power_per_thrust in JSON");
    assert!(json.contains("\"velocity\""), "velocity in JSON");
    assert!(json.contains("\"course\""), "course in JSON");
    assert!(json.contains("thrust_remaining"), "thrust_remaining in JSON");
}
