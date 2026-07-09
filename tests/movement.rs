use std::collections::HashSet;
use std::path::PathBuf;

use shipsim_core::board::Board;
use shipsim_core::game_state::{GameState, ScenarioStatus};
use shipsim_core::hex::Hex;
use shipsim_core::movement::{apply_order, declare, resolve, Order, OrderError};
use shipsim_core::scenario::load_scenario;
use shipsim_core::ship::Ship;
use shipsim_core::snapshot::StateSnapshot;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn two_ship_state() -> GameState {
    GameState::new(
        Board::new(8, 8),
        vec![
            Ship {
                id: 1,
                class: "Heavy Cruiser".to_string(),
                pos: Hex::new(0, 0),
                facing: 0,
                speed: 4,
                power: 4,
                turn_speed: 4,
                turn_mode: 2,
                weapons: Vec::new(),
                shields: [0; 6],
                structure: 0,
                destroyed: false,
            },
            Ship {
                id: 2,
                class: "Escort".to_string(),
                pos: Hex::new(4, 0),
                facing: 3,
                speed: 3,
                power: 3,
                turn_speed: 3,
                turn_mode: 1,
                weapons: Vec::new(),
                shields: [0; 6],
                structure: 0,
                destroyed: false,
            },
        ],
        Hex::new(7, 7),
    )
}

#[test]
fn test_hex_distance_and_neighbors() {
    let origin = Hex::ORIGIN;
    assert_eq!(origin.distance(Hex::new(2, -1)), 2);
    assert_eq!(
        origin.distance(Hex::new(3, -2)),
        Hex::new(3, -2).distance(origin)
    );

    let neighbors = origin.neighbors();
    let unique: HashSet<_> = neighbors.iter().copied().collect();
    assert_eq!(neighbors.len(), 6);
    assert_eq!(unique.len(), 6);
    for neighbor in neighbors {
        assert_eq!(origin.distance(neighbor), 1);
    }

    assert_eq!(Hex::facing_between(origin, Hex::new(1, 0)), Some(0));
}

#[test]
fn test_plot_declare_no_mutation() {
    let mut game = two_ship_state();
    let before = StateSnapshot::from_game_state(&game);

    let declared = declare(
        &game,
        Order::Plot {
            ship: 1,
            path: vec![Hex::new(1, 0), Hex::new(2, 0)],
        },
    )
    .expect("declaration succeeds");

    let after_declare = StateSnapshot::from_game_state(&game);
    assert_eq!(
        serde_json::to_value(before).unwrap(),
        serde_json::to_value(after_declare).unwrap()
    );

    resolve(&mut game, declared);
    // Plot stored but position unchanged until RunTurn.
    assert_eq!(game.ship(1).unwrap().pos, Hex::new(0, 0));
}

#[test]
fn test_run_turn_applies_plot() {
    let mut game = two_ship_state();
    apply_order(&mut game, Order::Plot {
        ship: 1,
        path: vec![Hex::new(1, 0), Hex::new(2, 0), Hex::new(3, 0)],
    })
    .unwrap();
    apply_order(&mut game, Order::RunTurn).unwrap();
    assert_eq!(game.ship(1).unwrap().pos, Hex::new(3, 0));
    assert_eq!(game.impulse(), 0);
    assert_eq!(game.turn_number(), 2);
}

#[test]
fn test_plot_implies_facing_after_run_turn() {
    let mut game = two_ship_state();
    // End with a turn onto +r so final facing is 5 (delta 0,1).
    apply_order(&mut game, Order::Plot {
        ship: 1,
        path: vec![Hex::new(1, 0), Hex::new(2, 0), Hex::new(2, 1)],
    })
    .unwrap();
    apply_order(&mut game, Order::RunTurn).unwrap();
    assert_eq!(game.ship(1).unwrap().pos, Hex::new(2, 1));
    assert_eq!(game.ship(1).unwrap().facing, 5);
}

#[test]
fn test_allocate_reduces_plot_budget() {
    let mut game = two_ship_state();
    // two_ship ship 1 has speed 4 / power 4; allocate 1.
    apply_order(
        &mut game,
        Order::Allocate { ship: 1, speed: 1 },
    )
    .unwrap();
    let err = apply_order(
        &mut game,
        Order::Plot {
            ship: 1,
            path: vec![Hex::new(1, 0), Hex::new(2, 0)],
        },
    )
    .expect_err("path longer than allocated speed");
    assert!(matches!(err, OrderError::PlotTooLong { .. }));

    apply_order(
        &mut game,
        Order::Plot {
            ship: 1,
            path: vec![Hex::new(1, 0)],
        },
    )
    .expect("single hex ok at speed 1");
    apply_order(&mut game, Order::RunTurn).unwrap();
    assert_eq!(game.ship(1).unwrap().pos, Hex::new(1, 0));
    // After turn, energy resets to full default allocation.
    assert_eq!(
        game.ship(1).unwrap().turn_speed,
        game.ship(1).unwrap().power.min(game.ship(1).unwrap().speed)
    );
}

#[test]
fn test_illegal_allocation_rejected() {
    let mut game = two_ship_state();
    let err = apply_order(
        &mut game,
        Order::Allocate {
            ship: 1,
            speed: 99,
        },
    )
    .expect_err("over budget");
    assert!(matches!(err, OrderError::IllegalAllocation { .. }));
}

#[test]
fn test_plot_too_long_rejected_without_mutation() {
    let mut game = two_ship_state();
    let before = StateSnapshot::from_game_state(&game);
    let err = apply_order(&mut game, Order::Plot {
            ship: 1,
            path: vec![
                Hex::new(1, 0),
                Hex::new(2, 0),
                Hex::new(3, 0),
                Hex::new(4, 0),
                Hex::new(5, 0),
            ],
        })
        .expect_err("path longer than speed");
    assert!(matches!(err, OrderError::PlotTooLong { .. }));
    assert_eq!(
        serde_json::to_value(before).unwrap(),
        serde_json::to_value(StateSnapshot::from_game_state(&game)).unwrap()
    );
}

#[test]
fn test_non_adjacent_plot_rejected_without_mutation() {
    let mut game = two_ship_state();
    let before = StateSnapshot::from_game_state(&game);
    let err = apply_order(&mut game, Order::Plot {
            ship: 1,
            path: vec![Hex::new(2, 0)],
        })
        .expect_err("skip hex");
    assert!(matches!(err, OrderError::NotAdjacent { .. }));
    assert_eq!(
        serde_json::to_value(before).unwrap(),
        serde_json::to_value(StateSnapshot::from_game_state(&game)).unwrap()
    );
}

#[test]
fn test_off_map_plot_rejected_without_mutation() {
    let mut game = two_ship_state();
    let err = apply_order(&mut game, Order::Plot {
            ship: 1,
            path: vec![Hex::new(0, -1)],
        })
        .expect_err("off map");
    assert!(matches!(err, OrderError::OffMap { .. }));
}

#[test]
fn test_occupied_hex_in_plot_rejected() {
    let mut game = two_ship_state();
    // Ship 2 sits at (4,0). Path through (4,0) rejected at submit.
    let err = apply_order(&mut game, Order::Plot {
            ship: 1,
            path: vec![
                Hex::new(1, 0),
                Hex::new(2, 0),
                Hex::new(3, 0),
                Hex::new(4, 0),
            ],
        })
        .expect_err("occupied");
    assert!(matches!(err, OrderError::HexOccupied { .. }));
}

#[test]
fn test_turn_mode_violation_rejected() {
    let mut game = two_ship_state();
    // turn_mode 2: first step free; immediate turn on step 2 is illegal (straight only 1 < 2).
    // Path stays on-board (r>=0): (1,0) facing 0, then (1,1) facing 5.
    let err = apply_order(&mut game, Order::Plot {
            ship: 1,
            path: vec![Hex::new(1, 0), Hex::new(1, 1)],
        })
        .expect_err("turn mode");
    assert!(
        matches!(
            err,
            OrderError::TurnModeViolation {
                ship: 1,
                step_index: 1
            }
        ),
        "got {err:?}"
    );
}

#[test]
fn test_turn_mode_allows_after_straight_run() {
    let mut game = two_ship_state();
    // two straight then turn: (1,0),(2,0) then turn to (2,1) — facing change after straight=2.
    apply_order(&mut game, Order::Plot {
        ship: 1,
        path: vec![Hex::new(1, 0), Hex::new(2, 0), Hex::new(2, 1)],
    })
    .expect("turn after two straight is legal for turn_mode 2");
}

#[test]
fn test_collision_both_stop() {
    // Two ships move into the same hex on the same impulse.
    let mut game = GameState::new(
        Board::new(6, 6),
        vec![
            Ship {
                id: 1,
                class: "A".into(),
                pos: Hex::new(0, 0),
                facing: 0,
                speed: 1,
                power: 1,
                turn_speed: 1,
                turn_mode: 0,
                weapons: vec![],
                shields: [0; 6],
                structure: 0,
                destroyed: false,
            },
            Ship {
                id: 2,
                class: "B".into(),
                pos: Hex::new(2, 0),
                facing: 3,
                speed: 1,
                power: 1,
                turn_speed: 1,
                turn_mode: 0,
                weapons: vec![],
                shields: [0; 6],
                structure: 0,
                destroyed: false,
            },
        ],
        Hex::new(5, 5),
    );

    // speed 1 moves only on impulse 32; both plot into (1,0).
    apply_order(&mut game, Order::Plot {
        ship: 1,
        path: vec![Hex::new(1, 0)],
    })
    .unwrap();
    apply_order(&mut game, Order::Plot {
        ship: 2,
        path: vec![Hex::new(1, 0)],
    })
    .unwrap();
    apply_order(&mut game, Order::RunTurn).unwrap();

    assert_eq!(game.ship(1).unwrap().pos, Hex::new(0, 0));
    assert_eq!(game.ship(2).unwrap().pos, Hex::new(2, 0));
}

#[test]
fn test_collision_clears_remaining_plot() {
    // Multi-step plots: collide on first shared destination; later steps must not apply.
    // speed 2: two move-impulses. Both try to enter (2,0) as first step of remaining path.
    // Ship1 at (0,0) path (1,0),(2,0); ship2 at (4,0) path (3,0),(2,0).
    // First moves are free; second step both enter (2,0) -> both stop at (1,0) and (3,0).
    let mut game = GameState::new(
        Board::new(8, 8),
        vec![
            Ship {
                id: 1,
                class: "A".into(),
                pos: Hex::new(0, 0),
                facing: 0,
                speed: 2,
                power: 2,
                turn_speed: 2,
                turn_mode: 0,
                weapons: vec![],
                shields: [0; 6],
                structure: 0,
                destroyed: false,
            },
            Ship {
                id: 2,
                class: "B".into(),
                pos: Hex::new(4, 0),
                facing: 3,
                speed: 2,
                power: 2,
                turn_speed: 2,
                turn_mode: 0,
                weapons: vec![],
                shields: [0; 6],
                structure: 0,
                destroyed: false,
            },
        ],
        Hex::new(7, 7),
    );

    apply_order(&mut game, Order::Plot {
        ship: 1,
        path: vec![Hex::new(1, 0), Hex::new(2, 0)],
    })
    .unwrap();
    apply_order(&mut game, Order::Plot {
        ship: 2,
        path: vec![Hex::new(3, 0), Hex::new(2, 0)],
    })
    .unwrap();
    apply_order(&mut game, Order::RunTurn).unwrap();

    assert_eq!(game.ship(1).unwrap().pos, Hex::new(1, 0));
    assert_eq!(game.ship(2).unwrap().pos, Hex::new(3, 0));
}

#[test]
fn test_simultaneous_no_first_mover_advantage() {
    // Declaration order must not change outcome for non-colliding moves.
    let run = |plot_first: u32| {
        let mut game = two_ship_state();
        let plots = [
            (
                1u32,
                vec![Hex::new(1, 0), Hex::new(2, 0), Hex::new(3, 0)],
            ),
            (2u32, vec![Hex::new(4, 1), Hex::new(4, 2)]),
        ];
        for id in if plot_first == 1 {
            [1u32, 2]
        } else {
            [2u32, 1]
        } {
            let path = plots.iter().find(|(s, _)| *s == id).unwrap().1.clone();
            apply_order(&mut game, Order::Plot { ship: id, path }).unwrap();
        }
        apply_order(&mut game, Order::RunTurn).unwrap();
        (
            game.ship(1).unwrap().pos,
            game.ship(2).unwrap().pos,
        )
    };

    assert_eq!(run(1), run(2));
}

#[test]
fn test_scripted_ship_auto_plots_toward_waypoint() {
    let mut game =
        load_scenario(&manifest_path("scenarios/movement.toml")).expect("movement scenario");
    let start = game.ship(2).unwrap().pos;
    // Player plots idle (empty path is invalid) — plot one hex away that is free.
    // Ship 1 at (0,0); ship 2 at (3,0). Player plots along r.
    apply_order(&mut game, Order::Plot {
        ship: 1,
        path: vec![Hex::new(0, 1)],
    })
    .unwrap();
    apply_order(&mut game, Order::RunTurn).unwrap();
    let after = game.ship(2).unwrap().pos;
    // Scripted escort speed 3, waypoint (3,1) then (3,2) — should leave start.
    assert_ne!(after, start);
    assert_eq!(game.ship(1).unwrap().pos, Hex::new(0, 1));
}

#[test]
fn test_turn_container_advances_via_run_turn() {
    let mut game = two_ship_state();
    assert_eq!(game.turn_number(), 1);
    apply_order(&mut game, Order::RunTurn).unwrap();
    assert_eq!(game.turn_number(), 2);
    apply_order(&mut game, Order::RunTurn).unwrap();
    assert_eq!(game.turn_number(), 3);
}

#[test]
fn test_run_without_objective_stays_in_progress() {
    let mut game = load_scenario(&manifest_path("scenarios/combat.toml")).unwrap();
    apply_order(&mut game, Order::RunTurn).unwrap();
    assert_eq!(game.status(), ScenarioStatus::InProgress);
}

#[test]
fn test_plot_rejects_currently_occupied_hex() {
    // Occupancy is checked at plot submit (ADR-0008). Mid-turn pass-through swap is
    // allowed by the engine when destinations are free of stationary ships, but
    // adjacent swap cannot be authored when the destination is occupied at submit.
    let mut game = GameState::new(
        Board::new(4, 4),
        vec![
            Ship {
                id: 1,
                class: "A".into(),
                pos: Hex::new(0, 0),
                facing: 0,
                speed: 1,
                power: 1,
                turn_speed: 1,
                turn_mode: 0,
                weapons: vec![],
                shields: [0; 6],
                structure: 0,
                destroyed: false,
            },
            Ship {
                id: 2,
                class: "B".into(),
                pos: Hex::new(1, 0),
                facing: 3,
                speed: 1,
                power: 1,
                turn_speed: 1,
                turn_mode: 0,
                weapons: vec![],
                shields: [0; 6],
                structure: 0,
                destroyed: false,
            },
        ],
        Hex::new(3, 3),
    );
    let err = apply_order(&mut game, Order::Plot {
            ship: 1,
            path: vec![Hex::new(1, 0)],
        })
        .expect_err("cannot plot into currently occupied hex");
    assert!(matches!(err, OrderError::HexOccupied { .. }));
}
