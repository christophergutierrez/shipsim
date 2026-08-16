use std::collections::BTreeMap;

use shipsim_core::game_state::Phase;
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario_def;
use shipsim_core::schema::{ScenarioDef, ShipPlacementDef};

fn squad_game() -> shipsim_core::game_state::GameState {
    let def = ScenarioDef {
        width: 12,
        height: 12,
        seed: 7,
        map_mode: None,
        objective: None,
        terminal: None,
        ships: vec![
            ShipPlacementDef {
                id: 1,
                class: "basic_destroyer".into(),
                q: 2,
                r: 2,
                facing: 0,
                controller: "player".into(),
                side: None,
                power: None,
                structure: None,
                max_shield_per_facing: None,
                squad: Some(9),
                leader: Some(1),
            },
            ShipPlacementDef {
                id: 2,
                class: "basic_destroyer".into(),
                q: 2,
                r: 2,
                facing: 0,
                controller: "player".into(),
                side: None,
                power: None,
                structure: None,
                max_shield_per_facing: None,
                squad: Some(9),
                leader: Some(1),
            },
        ],
    };
    load_scenario_def(&def, std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn allocate(game: &mut shipsim_core::game_state::GameState, ship: u32, unsquad: bool) {
    apply_order(
        game,
        Order::Allocate {
            ship,
            movement: 1,
            weapons: BTreeMap::new(),
            shields: [0; 6],
            cloak: false,
            repair: 0,
            unsquad,
            squad_leader: None,
        },
    )
    .unwrap();
}

#[test]
fn declared_squad_stacks_and_follower_can_follow_leader_path() {
    let mut game = squad_game();
    assert_eq!(game.squads()[&9].members, vec![1, 2]);
    allocate(&mut game, 1, false);
    allocate(&mut game, 2, false);
    assert_eq!(game.phase(), Phase::Movement);
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 1,
            actions: vec![shipsim_core::path::PathAction::MoveF],
            evasive: 0,
            follow: false,
        },
    )
    .unwrap();
    apply_order(
        &mut game,
        Order::CommitPath {
            ship: 2,
            actions: vec![],
            evasive: 0,
            follow: true,
        },
    )
    .unwrap();
    assert_eq!(game.ship(1).unwrap().pos, game.ship(2).unwrap().pos);
}

#[test]
fn unsquad_allows_independent_members_next_turn() {
    let mut game = squad_game();
    allocate(&mut game, 1, true);
    allocate(&mut game, 2, false);
    assert_eq!(game.squads()[&9].members, vec![2]);
}
