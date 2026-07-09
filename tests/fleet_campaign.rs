use std::path::PathBuf;

use shipsim_core::board::MapMode;
use shipsim_core::campaign::Campaign;
use shipsim_core::hex::Hex;
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

#[test]
fn test_fleet_scenario_loads_four_ships() {
    let game = load_scenario(&manifest_path("scenarios/fleet.toml")).expect("fleet");
    assert_eq!(game.ships().len(), 4);
    assert_eq!(game.board().mode, MapMode::Floating);
    assert_eq!(game.npc_ids().len(), 2);
}

#[test]
fn test_floating_map_recenters() {
    let mut game = load_scenario(&manifest_path("scenarios/fleet.toml")).unwrap();
    // Push player ships far "off" via floating-allowed plot path outside 0..width.
    // Use hard-mode would reject; floating allows then recenters.
    game.set_ship_pos(1, Hex::new(0, 0)).unwrap();
    apply_order(
        &mut game,
        Order::Allocate {
            ship: 1,
            movement: 4,
            weapons: 4,
            shields: 0,
        },
    )
    .unwrap();
    // Plot off the right edge of a 12-wide map starting near edge
    game.set_ship_pos(1, Hex::new(10, 0)).unwrap();
    apply_order(
        &mut game,
        Order::Plot {
            ship: 1,
            path: vec![
                Hex::new(11, 0),
                Hex::new(12, 0),
                Hex::new(13, 0),
                Hex::new(14, 0),
            ],
        },
    )
    .expect("floating allows off-map plot");
    apply_order(&mut game, Order::RunTurn).unwrap();
    // After float recenter, all ships should be on-board.
    for s in game.ships() {
        assert!(
            game.board().contains(s.pos),
            "ship {} at {:?} should be on board after float",
            s.id,
            s.pos
        );
    }
}

#[test]
fn test_campaign_loads_and_lists_scenarios() {
    let camp = Campaign::load(&manifest_path("campaigns/demo.toml")).expect("campaign");
    assert_eq!(camp.name, "Demo Campaign");
    assert_eq!(camp.scenario_paths.len(), 3);
    let game = camp.load_current().expect("first scenario");
    assert!(!game.ships().is_empty());
}

#[test]
fn test_combat_log_records_direct_fire() {
    let mut game = load_scenario(&manifest_path("scenarios/combat.toml")).unwrap();
    apply_order(
        &mut game,
        Order::Fire {
            ship: 1,
            weapon: "phaser_1".to_string(),
            target: 2,
        },
    )
    .unwrap();
    apply_order(&mut game, Order::RunTurn).unwrap();
    assert!(
        !game.combat_log().is_empty(),
        "expected combat log entries after fire"
    );
    assert_eq!(game.combat_log()[0].kind, "direct");
}
