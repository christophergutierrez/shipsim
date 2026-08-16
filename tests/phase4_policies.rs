use std::path::PathBuf;

use shipsim_core::game_state::GameState;
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario;
use shipsim_core::schema::SideId;
use shipsim_core::simulation::{
    build_policy_for_side, policy_catalog, policy_seed, run_match, MatchConfig, PurchaseContext,
};
use shipsim_core::snapshot::StateSnapshot;

fn assault() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scenarios/shipyard_assault.toml")
}

#[test]
fn advertised_policy_metadata_is_stable() {
    let ids: Vec<_> = policy_catalog().iter().map(|meta| meta.id).collect();
    assert_eq!(
        ids,
        ["random", "greedy", "aggressive", "defensive", "mobility"]
    );
    let labels: Vec<_> = policy_catalog().iter().map(|meta| meta.label).collect();
    assert_eq!(
        labels,
        ["Random", "Greedy", "Aggressive", "Defensive", "Mobility"]
    );
    assert_ne!(
        policy_seed(7, SideId::A, "random"),
        policy_seed(7, SideId::B, "random")
    );
    assert_ne!(
        policy_seed(7, SideId::A, "random"),
        policy_seed(7, SideId::A, "greedy")
    );
}

#[test]
fn policies_make_bounded_side_owned_purchase_decisions() {
    let game = load_scenario(&assault()).expect("assault scenario");
    let snapshot = StateSnapshot::from_game_state(&game);
    for side in [SideId::A, SideId::B] {
        for meta in policy_catalog() {
            let mut policy = build_policy_for_side(meta.id, 11, side).expect("known policy");
            let orders = policy.purchase_orders(&PurchaseContext {
                snapshot: &snapshot,
                side,
                turn: snapshot.turn,
            });
            assert!(orders.len() <= 1, "{} exceeded purchase bound", meta.id);
            for order in orders {
                match order {
                    Order::Purchase {
                        side: order_side, ..
                    }
                    | Order::PurchaseCustom {
                        side: order_side, ..
                    } => {
                        assert_eq!(order_side, side)
                    }
                    other => panic!("policy emitted non-purchase order: {other:?}"),
                }
            }
        }
    }
}

#[test]
fn policies_complete_fast_legality_gate_for_both_sides() {
    for meta in policy_catalog() {
        for seed in 1..=2 {
            let config = MatchConfig::from_scenario(
                assault(),
                seed,
                meta.id.into(),
                meta.id.into(),
                5,
                2_000,
            );
            let result = run_match(&config).unwrap_or_else(|error| {
                panic!("{} seed {seed} rejected an order: {error}", meta.id)
            });
            assert!(result.metrics.rejected_orders == 0);
            assert!(result
                .trace
                .iter()
                .any(|event| matches!(event.order, Order::Allocate { .. })));
        }
    }
}

#[test]
fn purchases_are_applied_through_the_normal_order_path_for_both_sides() {
    let catalog = load_scenario(&assault())
        .expect("assault scenario")
        .purchase_catalog();
    for item in catalog
        .into_iter()
        .filter(|item| item.class != "shipyard" && item.cost <= 100)
    {
        for side in [SideId::A, SideId::B] {
            let mut game: GameState = load_scenario(&assault()).expect("assault scenario");
            let before = game.ships().len();
            apply_order(
                &mut game,
                Order::Purchase {
                    side,
                    class: item.class.clone(),
                },
            )
            .unwrap_or_else(|error| panic!("{} purchase for {side:?}: {error}", item.class));
            assert_eq!(game.ships().len(), before + 1);
            let purchased = game.ships().iter().max_by_key(|ship| ship.id).unwrap();
            assert_eq!(purchased.side, side);
        }
    }
}

#[test]
#[ignore = "500-pair legality soak; run explicitly in release mode"]
fn shipyard_assault_five_by_five_by_twenty_legality_soak() {
    for player in policy_catalog() {
        for opponent in policy_catalog() {
            for seed in 1..=20 {
                let config = MatchConfig::from_scenario(
                    assault(),
                    seed,
                    player.id.into(),
                    opponent.id.into(),
                    50,
                    20_000,
                );
                let result = run_match(&config).expect("policy legality/liveness");
                assert_eq!(result.metrics.rejected_orders, 0);
            }
        }
    }
}
