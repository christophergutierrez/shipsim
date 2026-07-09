use std::path::PathBuf;

use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario;
use shipsim_core::snapshot::StateSnapshot;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

#[test]
fn test_ai_seeks_and_can_close_distance() {
    let mut game = load_scenario(&manifest_path("scenarios/ai.toml")).expect("ai scenario");
    let start = game.ship(2).unwrap().pos.distance(game.ship(1).unwrap().pos);

    // Player holds; AI should auto-plot toward the player on RunTurn.
    apply_order(&mut game, Order::RunTurn).expect("run turn");

    let after = game.ship(2).unwrap().pos.distance(game.ship(1).unwrap().pos);
    assert!(
        after < start,
        "greedy AI should move closer (start dist {start}, after {after})"
    );
}

#[test]
fn test_ai_may_queue_fire_when_in_arc() {
    let mut game = load_scenario(&manifest_path("scenarios/ai.toml")).expect("ai scenario");
    // Place AI adjacent and facing the player so fire is legal.
    game.set_ship_pos(1, shipsim_core::hex::Hex::new(4, 4)).unwrap();
    game.set_ship_pos(2, shipsim_core::hex::Hex::new(5, 4)).unwrap();
    game.set_ship_facing(2, 3).unwrap(); // face -q toward player
    game.set_ship_facing(1, 0).unwrap();

    let before = game.ship(1).unwrap().shields;
    // Several turns so a fire window lands and PRNG can deal damage.
    for _ in 0..6 {
        if game.ship(1).unwrap().destroyed {
            break;
        }
        apply_order(&mut game, Order::RunTurn).unwrap();
    }
    let after = game.ship(1).unwrap().shields;
    let damaged = before.iter().zip(after.iter()).any(|(b, a)| a < b)
        || game.ship(1).unwrap().structure() < 12
        || game.ship(1).unwrap().destroyed;
    assert!(
        damaged,
        "AI should eventually fire when adjacent; snapshot={}",
        serde_json::to_string(&StateSnapshot::from_game_state(&game)).unwrap()
    );
}
