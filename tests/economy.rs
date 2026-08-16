use shipsim_core::game_state::{GameState, ScenarioStatus};
use shipsim_core::movement::{apply_order, Order, OrderError};
use shipsim_core::scenario::load_scenario;
use shipsim_core::schema::SideId;
use shipsim_core::snapshot::StateSnapshot;
use std::path::Path;

fn assault() -> GameState {
    load_scenario(Path::new("scenarios/shipyard_assault.toml")).expect("assault scenario")
}

#[test]
fn purchase_uses_snapshot_catalog_and_spawns_for_the_requested_side() {
    let mut game = assault();
    let snapshot = StateSnapshot::from_game_state(&game);
    let option = snapshot.purchasable.iter().find(|item| item.class == "basic_swarm").unwrap();
    assert_eq!(snapshot.credits.get(&SideId::A), Some(&100));
    apply_order(&mut game, Order::Purchase { side: SideId::A, class: option.class.clone() }).unwrap();
    assert_eq!(game.credits().get(&SideId::A), Some(&(100 - option.cost)));
    let purchased = game.ships().iter().find(|ship| ship.id > 10).unwrap();
    assert_eq!(purchased.side, SideId::A);
    assert_eq!(purchased.class_id, option.class);
    assert_eq!(purchased.pos.distance(game.ship(1).unwrap().pos), 1);
}

#[test]
fn purchase_rejects_over_budget_and_shipyard() {
    let mut game = assault();
    let error = apply_order(&mut game, Order::Purchase { side: SideId::A, class: "basic_light_cruiser".into() }).unwrap_err();
    assert!(matches!(error, OrderError::InsufficientCredits { .. }));
    assert_eq!(game.credits().get(&SideId::A), Some(&100));
    let error = apply_order(&mut game, Order::Purchase { side: SideId::A, class: "shipyard".into() }).unwrap_err();
    assert!(matches!(error, OrderError::NotPurchasable(_)));
    assert_eq!(game.status(), ScenarioStatus::InProgress);
}
