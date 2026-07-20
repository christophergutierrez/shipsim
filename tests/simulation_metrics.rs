//! Simulation metrics under protocol v4 paths/volleys.

use std::collections::BTreeMap;
use std::path::PathBuf;

use shipsim_core::game_state::Phase;
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::path::PathAction;
use shipsim_core::scenario::load_scenario;
use shipsim_core::simulation::metrics::MatchMetrics;
use shipsim_core::snapshot::StateSnapshot;

fn path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

#[test]
fn metrics_record_path_resolution_and_actual_translation() {
    let mut game = load_scenario(&path("scenarios/v4_conflict.toml")).unwrap();
    assert_eq!(game.phase(), Phase::Allocate);

    apply_order(
        &mut game,
        Order::Allocate {
            ship: 1,
            movement: 1,
            weapons: BTreeMap::new(),
            shields: [0; 6],
        },
    )
    .expect("allocate 1");
    apply_order(
        &mut game,
        Order::Allocate {
            ship: 2,
            movement: 1,
            weapons: BTreeMap::new(),
            shields: [0; 6],
        },
    )
    .expect("allocate 2");
    assert_eq!(game.phase(), Phase::Movement);

    let before = StateSnapshot::from_game_state(&game);
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 1,
            actions: vec![PathAction::MoveF],
        },
    )
    .expect("path 1");
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 2,
            actions: vec![PathAction::MoveF],
        },
    )
    .expect("path 2");
    assert_eq!(game.phase(), Phase::Firing);

    let after = StateSnapshot::from_game_state(&game);
    assert!(
        !after.path_results.is_empty(),
        "path_results must be populated after resolution"
    );

    let mut metrics = MatchMetrics::default();
    metrics.record_path_resolution(&before, &after.path_results);
    assert!(metrics.path_observations >= 2);
    // At least one ship in a conflict ends without net translation when it
    // falls back to start — translated_steps must reflect actual final travel.
    let loser_or_stationary = after.path_results.iter().any(|r| {
        r.final_q == before.ships.iter().find(|s| s.id == r.ship).unwrap().q
            && r.final_r == before.ships.iter().find(|s| s.id == r.ship).unwrap().r
            && r.submitted_cost > 0
    });
    if loser_or_stationary {
        assert!(
            after
                .path_results
                .iter()
                .any(|r| r.submitted_cost > 0 && r.translated_steps == 0),
            "fallback-to-start must report translated_steps=0: {:?}",
            after.path_results
        );
    }
}

#[test]
fn weapon_power_metric_counts_increases_only() {
    use shipsim_core::snapshot::ShipSnapshot;

    // Synthetic: ship already carries charge 2, allocates want 4 → spent 2.
    let ship = ShipSnapshot {
        id: 1,
        class: "test".into(),
        class_id: "test".into(),
        size: 2,
        controller: "player".into(),
        q: 0,
        r: 0,
        facing: 0,
        power: 10,
        attack_accuracy_bonus: 0,
        power_available: 10,
        movement_allocated: 0,
        shields_powered: [0; 6],
        shields_remaining: [0; 6],
        max_shield_per_facing: 4,
        structure: 10,
        engine: 2,
        power_sys: 2,
        bridge: 1,
        weapon_boxes: vec![1],
        destroyed: false,
        max_maneuver_actions: 4,
        thrust_per_power: 1,
        power_per_thrust: 1,
        motion_available: 0,
        weapons: vec![shipsim_core::snapshot::WeaponSnapshot {
            id: "beam_1".into(),
            kind: "Beam".into(),
            arc: "Forward".into(),
            mount: Some("forward".into()),
            max_range: 10,
            charge: 2,
            fired: false,
            max_charge: 4,
            operational: true,
        }],
    };
    let order = Order::Allocate {
        ship: 1,
        movement: 1,
        weapons: BTreeMap::from([("beam_1".into(), 4)]),
        shields: [1, 0, 0, 0, 0, 0],
    };
    let mut metrics = MatchMetrics::default();
    metrics.record_allocation(&ship, &order);
    let hull = metrics.hull_efficiency.get("test").unwrap();
    // movement 1 + weapon increase 2 + shield 1 = 4
    assert_eq!(hull.power_spent_total, 4);
    assert_eq!(hull.weapon_power_allocated, 2);
    assert!(hull.power_spent_total <= hull.power_available_total);
}
