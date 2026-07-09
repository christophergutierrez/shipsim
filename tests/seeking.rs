use std::path::PathBuf;

use shipsim_core::hex::Hex;
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario;
use shipsim_core::snapshot::StateSnapshot;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

#[test]
fn test_drone_launches_and_impacts() {
    let mut game = load_scenario(&manifest_path("scenarios/combat.toml")).expect("combat");
    // Attacker facing target; place 3 hexes away so drone needs several impulses.
    game.set_ship_pos(1, Hex::new(0, 0)).unwrap();
    game.set_ship_facing(1, 0).unwrap();
    game.set_ship_pos(2, Hex::new(3, 0)).unwrap();
    game.set_ship_facing(2, 3).unwrap();
    game.set_ship_shields(2, [10; 6]).unwrap();
    game.set_ship_structure(2, 20).unwrap();
    let before = game.ship(2).unwrap().shields;

    apply_order(
        &mut game,
        Order::Fire {
            ship: 1,
            weapon: "drone_1".to_string(),
            target: 2,
        },
    )
    .expect("drone launch");

    assert_eq!(
        game.seeking_munitions().len(),
        1,
        "drone should be in flight after Fire"
    );
    assert_eq!(game.seeking_munitions()[0].pos, Hex::new(0, 0));

    // One RunTurn advances all 32 impulses — drone speed 1 hex/impulse closes 3 hexes easily.
    apply_order(&mut game, Order::RunTurn).unwrap();

    assert!(
        game.seeking_munitions().is_empty(),
        "drone should have impacted"
    );
    let after = game.ship(2).unwrap().shields;
    let damaged = before.iter().zip(after.iter()).any(|(b, a)| a < b)
        || game.ship(2).unwrap().structure() < 20;
    assert!(
        damaged,
        "target should take warhead damage; snap={}",
        serde_json::to_string(&StateSnapshot::from_game_state(&game)).unwrap()
    );
}

#[test]
fn test_drone_not_in_direct_fire_pending() {
    let mut game = load_scenario(&manifest_path("scenarios/combat.toml")).unwrap();
    game.set_ship_pos(1, Hex::new(1, 0)).unwrap();
    game.set_ship_facing(1, 3).unwrap();
    // Fire phaser still queues until window; drone launches immediately.
    apply_order(
        &mut game,
        Order::Fire {
            ship: 1,
            weapon: "drone_1".to_string(),
            target: 2,
        },
    )
    .unwrap();
    assert_eq!(game.seeking_munitions().len(), 1);
    // No damage yet until impact.
    let shields = game.ship(2).unwrap().shields;
    assert_eq!(shields, [6; 6]);
}
