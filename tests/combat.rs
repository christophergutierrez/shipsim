use std::path::PathBuf;

use serde_json::Value;
use shipsim_core::combat::{bearing_to, WeaponKind};
use shipsim_core::hex::Hex;
use shipsim_core::movement::{declare, Order, OrderError};
use shipsim_core::scenario::load_scenario;
use shipsim_core::snapshot::StateSnapshot;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn load_combat() -> shipsim_core::game_state::GameState {
    load_scenario(&manifest_path("scenarios/combat.toml")).expect("combat scenario loads")
}

fn snapshot_json(game: &shipsim_core::game_state::GameState) -> Value {
    serde_json::to_value(StateSnapshot::from_game_state(game)).expect("snapshot serializes")
}

fn make_first_weapon_exact_damage(game: &mut shipsim_core::game_state::GameState, damage: u32) {
    let attacker = game.ship_mut(1).expect("attacker exists");
    let weapon = attacker
        .weapons
        .iter_mut()
        .find(|weapon| weapon.id == "phaser_1")
        .expect("phaser_1 exists");
    weapon.kind = WeaponKind::Disruptor;
    weapon.damage = damage;
}

fn shield_damage_taken(before: &[u32; 6], after: &[u32; 6]) -> u32 {
    before
        .iter()
        .zip(after.iter())
        .map(|(before, after)| before - after)
        .sum()
}

fn phaser_damage_at_range(range: i32) -> u32 {
    let mut game = load_combat();
    let attacker = game.ship_mut(1).expect("attacker exists");
    attacker.pos = Hex::new(1, 0);
    attacker.facing = 3;

    let defender = game.ship_mut(2).expect("defender exists");
    defender.pos = Hex::new(1 - range, 0);
    defender.facing = 0;
    defender.shields = [100; 6];
    defender.structure = 100;
    let before = defender.shields;

    game.apply_order(Order::Fire {
        weapon: "phaser_1".to_string(),
        target: 2,
    })
    .expect("phaser fire succeeds");

    shield_damage_taken(&before, &game.ship(2).expect("defender exists").shields)
}

#[test]
fn test_combat_scenario_loads_weapons() {
    let game = load_combat();
    let attacker = game.ship(1).expect("attacker exists");

    let weapon = attacker
        .weapons
        .iter()
        .find(|weapon| weapon.id == "phaser_1")
        .expect("phaser_1 loaded");
    assert_eq!(format!("{:?}", weapon.kind), "Phaser");
    assert_eq!(format!("{:?}", weapon.arc), "Forward");
    assert_eq!(weapon.max_range, 4);
}

#[test]
fn test_combat_scenario_starts_in_progress() {
    let game = load_combat();
    let json = snapshot_json(&game);

    assert_eq!(json["status"], "InProgress");
    assert!(json["objective"].is_null());
}

#[test]
fn test_combat_snapshot_shape() {
    let game = load_combat();
    let json = snapshot_json(&game);

    assert_eq!(json["seed"], 4242);
    assert!(json["objective"].is_null());

    let ship = &json["ships"][0];
    assert_eq!(ship["shields"].as_array().expect("shields array").len(), 6);
    assert_eq!(ship["structure"], 12);
    assert_eq!(ship["destroyed"], false);

    let weapons = ship["weapons"].as_array().expect("weapons array");
    assert_eq!(weapons[0]["id"], "phaser_1");
    assert_eq!(weapons[0]["kind"], "Phaser");
    assert_eq!(weapons[0]["arc"], "Forward");
    assert_eq!(weapons[0]["max_range"], 4);
}

#[test]
fn test_tracer_fire_damages_shield() {
    let mut game = load_combat();
    let before = snapshot_json(&game);
    let before_shield = before["ships"][1]["shields"][0]
        .as_u64()
        .expect("defender shield 0 before");

    game.apply_order(Order::Fire {
        weapon: "phaser_1".to_string(),
        target: 2,
    })
    .expect("tracer fire order succeeds");

    let after_snapshot = StateSnapshot::from_game_state(&game);
    let after = serde_json::to_value(&after_snapshot).expect("snapshot serializes");
    println!("{}", serde_json::to_string(&after_snapshot).unwrap());

    let after_shield = after["ships"][1]["shields"][0]
        .as_u64()
        .expect("defender shield 0 after");
    assert!(
        after_shield < before_shield,
        "defender toward-attacker shield should drop"
    );
    assert_eq!(after["status"], "InProgress");
}

#[test]
fn test_unknown_weapon_rejected() {
    let game = load_combat();

    let error = declare(
        &game,
        Order::Fire {
            weapon: "missing_weapon".to_string(),
            target: 2,
        },
    )
    .expect_err("unknown weapon should be rejected");

    assert_eq!(
        error,
        OrderError::WeaponNotFound("missing_weapon".to_string())
    );
}

#[test]
fn test_unknown_target_rejected() {
    let game = load_combat();

    let error = declare(
        &game,
        Order::Fire {
            weapon: "phaser_1".to_string(),
            target: 99,
        },
    )
    .expect_err("unknown target should be rejected");

    assert_eq!(error, OrderError::TargetNotFound(99));
}

#[test]
fn test_fire_at_self_rejected() {
    let game = load_combat();

    let error = declare(
        &game,
        Order::Fire {
            weapon: "phaser_1".to_string(),
            target: 1,
        },
    )
    .expect_err("self fire should be rejected");

    assert_eq!(error, OrderError::FireAtSelf(1));
}

#[test]
fn test_out_of_range_rejected() {
    let mut game = load_combat();
    game.ship_mut(2).expect("target exists").pos = Hex::new(3, 3);

    let error = declare(
        &game,
        Order::Fire {
            weapon: "phaser_1".to_string(),
            target: 2,
        },
    )
    .expect_err("out of range fire should be rejected");

    assert_eq!(
        error,
        OrderError::OutOfRange {
            weapon: "phaser_1".to_string(),
            range: 5,
            max_range: 4,
        }
    );
}

#[test]
fn test_out_of_arc_rejected() {
    let mut game = load_combat();
    game.apply_order(Order::Face { ship: 1, facing: 0 })
        .expect("face order succeeds");

    let error = declare(
        &game,
        Order::Fire {
            weapon: "phaser_1".to_string(),
            target: 2,
        },
    )
    .expect_err("out of arc fire should be rejected");

    assert_eq!(
        error,
        OrderError::OutOfArc {
            weapon: "phaser_1".to_string(),
            target: 2,
        }
    );
}

#[test]
fn test_refire_rejected() {
    let mut game = load_combat();
    game.apply_order(Order::Fire {
        weapon: "phaser_1".to_string(),
        target: 2,
    })
    .expect("first fire succeeds");

    let error = declare(
        &game,
        Order::Fire {
            weapon: "phaser_1".to_string(),
            target: 2,
        },
    )
    .expect_err("refire should be rejected");

    assert_eq!(
        error,
        OrderError::WeaponAlreadyFired("phaser_1".to_string())
    );
}

#[test]
fn test_illegal_fire_no_mutation() {
    let mut game = load_combat();
    game.apply_order(Order::Face { ship: 1, facing: 0 })
        .expect("face order succeeds");
    let before = snapshot_json(&game);

    let result = game.apply_order(Order::Fire {
        weapon: "phaser_1".to_string(),
        target: 2,
    });

    assert!(matches!(result, Err(OrderError::OutOfArc { .. })));
    assert_eq!(snapshot_json(&game), before);
}

#[test]
fn test_bearing_delta_to_facing() {
    let origin = Hex::ORIGIN;

    assert_eq!(bearing_to(origin, Hex::new(3, 0)), 0);
    assert_eq!(bearing_to(origin, Hex::new(3, -3)), 1);
    assert_eq!(bearing_to(origin, Hex::new(0, -3)), 2);
    assert_eq!(bearing_to(origin, Hex::new(-3, 0)), 3);
    assert_eq!(bearing_to(origin, Hex::new(-3, 3)), 4);
    assert_eq!(bearing_to(origin, Hex::new(0, 3)), 5);
    assert_eq!(bearing_to(origin, Hex::new(2, -1)), 0);
}

#[test]
fn test_face_order_changes_arc_eligibility() {
    let mut game = load_combat();

    declare(
        &game,
        Order::Fire {
            weapon: "phaser_1".to_string(),
            target: 2,
        },
    )
    .expect("initial facing gives forward arc");

    game.apply_order(Order::Face { ship: 1, facing: 0 })
        .expect("face order succeeds");

    let error = declare(
        &game,
        Order::Fire {
            weapon: "phaser_1".to_string(),
            target: 2,
        },
    )
    .expect_err("new facing should put target outside forward arc");

    assert!(matches!(error, OrderError::OutOfArc { .. }));
}

#[test]
fn test_overflow_bleeds_then_stops() {
    let mut game = load_combat();
    make_first_weapon_exact_damage(&mut game, 8);
    let defender = game.ship_mut(2).expect("defender exists");
    defender.shields[0] = 3;
    defender.structure = 12;

    game.apply_order(Order::Fire {
        weapon: "phaser_1".to_string(),
        target: 2,
    })
    .expect("fire succeeds");

    let defender = game.ship(2).expect("defender exists");
    assert_eq!(defender.shields[0], 0);
    assert_eq!(defender.structure, 7);
}

#[test]
fn test_underflow_leaves_structure() {
    let mut game = load_combat();
    make_first_weapon_exact_damage(&mut game, 2);
    let defender = game.ship_mut(2).expect("defender exists");
    defender.shields[0] = 6;
    defender.structure = 12;

    game.apply_order(Order::Fire {
        weapon: "phaser_1".to_string(),
        target: 2,
    })
    .expect("fire succeeds");

    let defender = game.ship(2).expect("defender exists");
    assert_eq!(defender.shields[0], 4);
    assert_eq!(defender.structure, 12);
}

#[test]
fn test_depleted_facing_stays_down() {
    let mut game = load_combat();
    make_first_weapon_exact_damage(&mut game, 4);
    let defender = game.ship_mut(2).expect("defender exists");
    defender.shields[0] = 3;
    defender.structure = 12;

    game.apply_order(Order::Fire {
        weapon: "phaser_1".to_string(),
        target: 2,
    })
    .expect("first fire succeeds");
    game.apply_order(Order::EndTurn).expect("turn ends");
    game.apply_order(Order::Fire {
        weapon: "phaser_1".to_string(),
        target: 2,
    })
    .expect("second fire succeeds");

    let defender = game.ship(2).expect("defender exists");
    assert_eq!(defender.shields[0], 0);
    assert_eq!(defender.structure, 7);
}

#[test]
fn test_damage_hits_bearing_facing() {
    let mut game = load_combat();
    make_first_weapon_exact_damage(&mut game, 2);

    let attacker_pos = Hex::new(2, -1);
    let target_pos = game.ship(2).expect("defender exists").pos;
    let attacker_facing = bearing_to(attacker_pos, target_pos);
    let defender_facing = 2;
    let expected_shield =
        ((bearing_to(target_pos, attacker_pos) + 6 - defender_facing) % 6) as usize;

    let attacker = game.ship_mut(1).expect("attacker exists");
    attacker.pos = attacker_pos;
    attacker.facing = attacker_facing;
    let defender = game.ship_mut(2).expect("defender exists");
    defender.facing = defender_facing;
    defender.shields = [6; 6];
    defender.structure = 12;

    game.apply_order(Order::Fire {
        weapon: "phaser_1".to_string(),
        target: 2,
    })
    .expect("fire succeeds");

    let defender = game.ship(2).expect("defender exists");
    for (index, shield) in defender.shields.iter().enumerate() {
        if index == expected_shield {
            assert_eq!(*shield, 4);
        } else {
            assert_eq!(*shield, 6);
        }
    }
    assert_eq!(defender.structure, 12);
}

#[test]
fn test_face_order_changes_hit_shield() {
    let mut baseline = load_combat();
    make_first_weapon_exact_damage(&mut baseline, 2);
    baseline.ship_mut(2).expect("defender exists").shields = [6; 6];
    baseline
        .apply_order(Order::Fire {
            weapon: "phaser_1".to_string(),
            target: 2,
        })
        .expect("baseline fire succeeds");

    let mut turned = load_combat();
    make_first_weapon_exact_damage(&mut turned, 2);
    turned.ship_mut(2).expect("defender exists").shields = [6; 6];
    turned
        .apply_order(Order::Face { ship: 2, facing: 1 })
        .expect("defender face order succeeds");
    turned
        .apply_order(Order::Fire {
            weapon: "phaser_1".to_string(),
            target: 2,
        })
        .expect("turned fire succeeds");

    let baseline_shields = baseline.ship(2).expect("defender exists").shields;
    let turned_shields = turned.ship(2).expect("defender exists").shields;

    assert_eq!(baseline_shields[0], 4);
    assert_eq!(baseline_shields[5], 6);
    assert_eq!(turned_shields[0], 6);
    assert_eq!(turned_shields[5], 4);
}

#[test]
fn test_phaser_damage_by_range_pinned_seed() {
    let range_one = phaser_damage_at_range(1);
    let mid_range = phaser_damage_at_range(2);
    let max_range = phaser_damage_at_range(4);

    assert_eq!(range_one, 9);
    assert_eq!(mid_range, 7);
    assert_eq!(max_range, 2);
    assert_ne!(range_one, max_range);
}

#[test]
fn test_fire_until_destroyed_wins() {
    let mut game = load_combat();

    // The combat scenario has no objective hex, so any Won must come from the
    // destruction terminal, never a coincidental objective-hex match.
    let start = snapshot_json(&game);
    assert!(start["objective"].is_null());
    assert_eq!(start["status"], "InProgress");

    let mut fatal = false;
    for _ in 0..50 {
        // Before the fatal shot the enemy is alive and the scenario is unresolved.
        assert!(!game.ship(2).expect("enemy exists").destroyed);
        assert_eq!(snapshot_json(&game)["status"], "InProgress");

        game.apply_order(Order::Fire {
            weapon: "phaser_1".to_string(),
            target: 2,
        })
        .expect("attacker fire succeeds");

        if game.ship(2).expect("enemy exists").destroyed {
            fatal = true;
            break;
        }
        game.apply_order(Order::EndTurn).expect("turn ends");
    }

    assert!(fatal, "enemy should be destroyed within the volley budget");

    let end = snapshot_json(&game);
    assert_eq!(end["status"], "Won");
    assert_eq!(end["ships"][1]["destroyed"], true);
    assert!(end["objective"].is_null());
}

#[test]
fn test_disruptor_miss_then_hit_pinned_seed() {
    let mut game = load_combat();
    let attacker = game.ship_mut(1).expect("attacker exists");
    attacker.pos = Hex::new(1, 0);
    attacker.facing = 3;

    let defender = game.ship_mut(2).expect("defender exists");
    defender.pos = Hex::new(-3, 0);
    defender.facing = 0;
    defender.shields = [100; 6];
    defender.structure = 100;
    let before_miss = defender.shields;

    game.apply_order(Order::Fire {
        weapon: "disruptor_1".to_string(),
        target: 2,
    })
    .expect("first disruptor fire succeeds");
    let after_miss = game.ship(2).expect("defender exists").shields;
    assert_eq!(shield_damage_taken(&before_miss, &after_miss), 0);

    game.apply_order(Order::EndTurn).expect("turn ends");
    game.ship_mut(2).expect("defender exists").pos = Hex::new(0, 0);
    let before_hit = game.ship(2).expect("defender exists").shields;

    game.apply_order(Order::Fire {
        weapon: "disruptor_1".to_string(),
        target: 2,
    })
    .expect("second disruptor fire succeeds");

    let after_hit = game.ship(2).expect("defender exists").shields;
    assert_eq!(shield_damage_taken(&before_hit, &after_hit), 4);
}
