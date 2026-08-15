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
    let mut game = load_scenario(&path("fixtures/v4_conflict.toml")).unwrap();
    assert_eq!(game.phase(), Phase::Allocate);

    apply_order(
        &mut game,
        Order::Allocate {
            ship: 1,
            movement: 1,
            weapons: BTreeMap::new(),
            shields: [0; 6],
            cloak: false,
            repair: 0,
            unsquad: false,
            squad_leader: None,
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
            cloak: false,
            repair: 0,
            unsquad: false,
            squad_leader: None,
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
            evasive: 0,
            follow: false,
        },
    )
    .expect("path 1");
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 2,
            actions: vec![PathAction::MoveF],
            evasive: 0,
            follow: false,
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
        class: "test".into(),
        power: 10,
        power_available: 10,
        weapon_boxes: vec![1],
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
            ammo_remaining: None,
            max_ammo: None,
            accuracy_bonus: 0,
            damage_bonus: 0,
            repeat: false,
            pierce: false,
        }],
        ..ShipSnapshot::test_fixture(1)
    };
    let order = Order::Allocate {
        ship: 1,
        movement: 1,
        weapons: BTreeMap::from([("beam_1".into(), 4)]),
        shields: [1, 0, 0, 0, 0, 0],
        cloak: false,
        repair: 0,
        unsquad: false,
        squad_leader: None,
    };
    let mut metrics = MatchMetrics::default();
    metrics.record_allocation(&ship, &order);
    let hull = metrics.hull_efficiency.get("test").unwrap();
    // movement 1 + weapon increase 2 + shield 1 = 4
    assert_eq!(hull.power_spent_total, 4);
    assert_eq!(hull.weapon_power_allocated, 2);
    assert!(hull.power_spent_total <= hull.power_available_total);
}
