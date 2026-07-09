use std::path::PathBuf;

use serde_json::Value;
use shipsim_core::combat::bearing_to;
use shipsim_core::hex::Hex;
use shipsim_core::movement::{apply_order, declare, Order, OrderError};
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
    game.configure_weapon_exact_damage(1, "phaser_1", damage)
        .expect("phaser_1 exists");
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
    game.set_ship_pos(1, Hex::new(1, 0)).unwrap();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, Hex::new(1 - range, 0)).unwrap();
    game.set_ship_facing(2, 0).unwrap();
    game.set_ship_shields(2, [100; 6]).unwrap();
    game.set_ship_structure(2, 100).unwrap();
    let before = game.ship(2).expect("defender exists").shields;

    apply_order(&mut game, Order::Fire {
        ship: 1,
        weapon: "phaser_1".to_string(),
        target: 2,
    })
    .expect("phaser fire succeeds");
    apply_order(&mut game, Order::RunTurn).expect("resolve fire at turn end");

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
fn test_fire_without_run_turn_no_damage() {
    let mut game = load_combat();
    let before = snapshot_json(&game);

    apply_order(&mut game, Order::Fire {
        ship: 1,
        weapon: "phaser_1".to_string(),
        target: 2,
    })
    .expect("fire declares");

    // Fire is deferred: no damage until RunTurn (ADR-0008).
    assert_eq!(snapshot_json(&game)["ships"][1]["shields"], before["ships"][1]["shields"]);
    assert_eq!(
        snapshot_json(&game)["ships"][1]["structure"],
        before["ships"][1]["structure"]
    );
}

#[test]
fn test_tracer_fire_damages_shield() {
    let mut game = load_combat();
    let before = snapshot_json(&game);
    let before_shield = before["ships"][1]["shields"][0]
        .as_u64()
        .expect("defender shield 0 before");

    apply_order(&mut game, Order::Fire {
        ship: 1,
        weapon: "phaser_1".to_string(),
        target: 2,
    })
    .expect("tracer fire order succeeds");
    apply_order(&mut game, Order::RunTurn).expect("resolve fire at turn end");

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
fn test_fire_skipped_when_target_moves_out_of_range() {
    // D1-fire: shot resolves on the first weapon fire window after movement on that impulse.
    // Target must leave range *before* the first fire window (phaser impulse 4).
    // Speed 16 moves on impulse 2, so it can step away before impulse 4.
    let mut game = load_combat();
    game.set_ship_pos(1, Hex::new(0, 0)).unwrap();
    game.set_ship_facing(1, 0).unwrap();
    game.configure_weapon_exact_damage(1, "phaser_1", 4).unwrap();
    game.configure_weapon_max_range(1, "phaser_1", 1).unwrap();
    game.set_ship_pos(2, Hex::new(1, 0)).unwrap();
    game.set_ship_facing(2, 0).unwrap();
    game.set_ship_shields(2, [50; 6]).unwrap();
    game.set_ship_structure(2, 50).unwrap();
    game.set_ship_power_profile(2, 16, 16).unwrap();
    let before_shields = game.ship(2).unwrap().shields;

    apply_order(&mut game, Order::Fire {
        ship: 1,
        weapon: "phaser_1".to_string(),
        target: 2,
    })
    .expect("fire legal at declare (range 1)");

    apply_order(&mut game, Order::Plot {
        ship: 2,
        path: vec![Hex::new(2, 0), Hex::new(3, 0)],
    })
    .expect("target plots away");
    apply_order(&mut game, Order::RunTurn).expect("run turn");

    assert_eq!(game.ship(2).unwrap().pos, Hex::new(3, 0));
    assert_eq!(
        game.ship(2).unwrap().shields,
        before_shields,
        "fire must skip when range exceeds max_range at first fire window"
    );
}

#[test]
fn test_unknown_weapon_rejected() {
    let game = load_combat();

    let error = declare(
        &game,
        Order::Fire {
            ship: 1,
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
            ship: 1,
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
            ship: 1,
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
    game.set_ship_pos(2, Hex::new(3, 3)).unwrap();

    let error = declare(
        &game,
        Order::Fire {
            ship: 1,
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
    game.set_ship_facing(1, 0).unwrap();

    let error = declare(
        &game,
        Order::Fire {
            ship: 1,
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
    apply_order(&mut game, Order::Fire {
        ship: 1,
        weapon: "phaser_1".to_string(),
        target: 2,
    })
    .expect("first fire succeeds");

    let error = declare(
        &game,
        Order::Fire {
            ship: 1,
            weapon: "phaser_1".to_string(),
            target: 2,
        },
    )
    .expect_err("refire should be rejected");

    assert_eq!(
        error,
        OrderError::WeaponAlreadyFired { ship: 1, weapon: "phaser_1".to_string() }
    );
}

#[test]
fn test_ssd_weapon_knockout_prevents_fire() {
    let mut game = load_combat();
    game.set_weapon_boxes(1, "phaser_1", 0).unwrap();
    let err = declare(
        &game,
        Order::Fire {
            ship: 1,
            weapon: "phaser_1".to_string(),
            target: 2,
        },
    )
    .expect_err("destroyed weapon cannot fire");
    assert!(matches!(err, OrderError::WeaponNotFound(_)));
}

#[test]
fn test_simultaneous_fire_mutual_kill() {
    // D2-fire: both ships shoot from a frozen pre-fire state. Sequential resolve would let
    // ship 1 kill ship 2 before ship 2's shot landed; simultaneous applies both hits.
    let mut game = load_combat();
    game.set_ship_shields(1, [0; 6]).unwrap();
    game.set_ship_structure(1, 1).unwrap();
    game.set_ship_shields(2, [0; 6]).unwrap();
    game.set_ship_structure(2, 1).unwrap();
    game.configure_weapon_exact_damage(1, "phaser_1", 1).unwrap();
    game.configure_weapon_exact_damage(2, "phaser_1", 1).unwrap();

    apply_order(
        &mut game,
        Order::Fire {
            ship: 1,
            weapon: "phaser_1".to_string(),
            target: 2,
        },
    )
    .expect("ship 1 fires");
    apply_order(
        &mut game,
        Order::Fire {
            ship: 2,
            weapon: "phaser_1".to_string(),
            target: 1,
        },
    )
    .expect("ship 2 fires");
    apply_order(&mut game, Order::RunTurn).expect("run turn");

    assert!(
        game.ship(1).unwrap().destroyed,
        "ship 1 must take ship 2's simultaneous hit"
    );
    assert!(
        game.ship(2).unwrap().destroyed,
        "ship 2 must take ship 1's simultaneous hit"
    );
}

#[test]
fn test_fire_requires_named_ship_and_weapon_on_that_ship() {
    let game = load_combat();
    // Same weapon id exists on ship 2's class, but ship 2 is the target here; firing as ship 99 fails.
    let err = declare(
        &game,
        Order::Fire {
            ship: 99,
            weapon: "phaser_1".to_string(),
            target: 2,
        },
    )
    .expect_err("unknown firer");
    assert_eq!(err, OrderError::ShipNotFound(99));

    // Ship 2 owns phaser_1 too — declaring fire *as* ship 2 is legal ownership-wise even if
    // geometry/arc may fail; ownership must not use global first-match.
    let owned = game.ship_owns_weapon(2, "phaser_1");
    assert!(owned, "escort also defines phaser_1");
}

#[test]
fn test_illegal_fire_no_mutation() {
    let mut game = load_combat();
    game.set_ship_facing(1, 0).unwrap();
    let before = snapshot_json(&game);

    let result = apply_order(&mut game, Order::Fire {
        ship: 1,
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
            ship: 1,
            weapon: "phaser_1".to_string(),
            target: 2,
        },
    )
    .expect("initial facing gives forward arc");

    game.set_ship_facing(1, 0).unwrap();

    let error = declare(
        &game,
        Order::Fire {
            ship: 1,
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
    let mut shields = game.ship(2).unwrap().shields;
    shields[0] = 3;
    game.set_ship_shields(2, shields).unwrap();
    game.set_ship_structure(2, 12).unwrap();

    apply_order(&mut game, Order::Fire {
        ship: 1,
        weapon: "phaser_1".to_string(),
        target: 2,
    })
    .expect("fire succeeds");
    apply_order(&mut game, Order::RunTurn).expect("resolve fire at turn end");

    let defender = game.ship(2).expect("defender exists");
    assert_eq!(defender.shields[0], 0);
    // 5 internal points via DAC: Hull,Hull,Engine,Weapon,Hull -> hull 12-3=9
    assert_eq!(defender.structure(), 9);
    assert_eq!(defender.ssd.engine, defender.speed - 1); // one engine box
}

#[test]
fn test_underflow_leaves_structure() {
    let mut game = load_combat();
    make_first_weapon_exact_damage(&mut game, 2);
    let mut shields = game.ship(2).unwrap().shields;
    shields[0] = 6;
    game.set_ship_shields(2, shields).unwrap();
    game.set_ship_structure(2, 12).unwrap();

    apply_order(&mut game, Order::Fire {
        ship: 1,
        weapon: "phaser_1".to_string(),
        target: 2,
    })
    .expect("fire succeeds");
    apply_order(&mut game, Order::RunTurn).expect("resolve fire at turn end");

    let defender = game.ship(2).expect("defender exists");
    assert_eq!(defender.shields[0], 4);
    assert_eq!(defender.structure(), 12);
}

#[test]
fn test_depleted_facing_stays_down() {
    let mut game = load_combat();
    make_first_weapon_exact_damage(&mut game, 4);
    let mut shields = game.ship(2).unwrap().shields;
    shields[0] = 3;
    game.set_ship_shields(2, shields).unwrap();
    game.set_ship_structure(2, 12).unwrap();

    apply_order(&mut game, Order::Fire {
        ship: 1,
        weapon: "phaser_1".to_string(),
        target: 2,
    })
    .expect("first fire succeeds");
    apply_order(&mut game, Order::RunTurn).expect("resolve first fire");
    apply_order(&mut game, Order::Fire {
        ship: 1,
        weapon: "phaser_1".to_string(),
        target: 2,
    })
    .expect("second fire succeeds");
    apply_order(&mut game, Order::RunTurn).expect("resolve second fire");

    let defender = game.ship(2).expect("defender exists");
    assert_eq!(defender.shields[0], 0);
    // Internals via DAC (not a pure structure pool): hull reduced but not 1:1 with overflow.
    assert!(defender.structure() < 12);
    assert!(defender.structure() >= 9);
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

    game.set_ship_pos(1, attacker_pos).unwrap();
    game.set_ship_facing(1, attacker_facing).unwrap();
    game.set_ship_facing(2, defender_facing).unwrap();
    game.set_ship_shields(2, [6; 6]).unwrap();
    game.set_ship_structure(2, 12).unwrap();

    apply_order(&mut game, Order::Fire {
        ship: 1,
        weapon: "phaser_1".to_string(),
        target: 2,
    })
    .expect("fire succeeds");
    apply_order(&mut game, Order::RunTurn).expect("resolve fire at turn end");

    let defender = game.ship(2).expect("defender exists");
    for (index, shield) in defender.shields.iter().enumerate() {
        if index == expected_shield {
            assert_eq!(*shield, 4);
        } else {
            assert_eq!(*shield, 6);
        }
    }
    assert_eq!(defender.structure(), 12);
}

#[test]
fn test_face_order_changes_hit_shield() {
    let mut baseline = load_combat();
    make_first_weapon_exact_damage(&mut baseline, 2);
    baseline.set_ship_shields(2, [6; 6]).unwrap();
    apply_order(&mut baseline, Order::Fire {
            ship: 1,
            weapon: "phaser_1".to_string(),
            target: 2,
        })
        .expect("baseline fire succeeds");
    apply_order(&mut baseline, Order::RunTurn).expect("resolve baseline fire");

    let mut turned = load_combat();
    make_first_weapon_exact_damage(&mut turned, 2);
    turned.set_ship_shields(2, [6; 6]).unwrap();
    turned.set_ship_facing(2, 1).unwrap();
    apply_order(&mut turned, Order::Fire {
            ship: 1,
            weapon: "phaser_1".to_string(),
            target: 2,
        })
        .expect("turned fire succeeds");
    apply_order(&mut turned, Order::RunTurn).expect("resolve turned fire");

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

        apply_order(&mut game, Order::Fire {
            ship: 1,
            weapon: "phaser_1".to_string(),
            target: 2,
        })
        .expect("attacker fire succeeds");
        apply_order(&mut game, Order::RunTurn).expect("resolve fire at turn end");

        if game.ship(2).expect("enemy exists").destroyed {
            fatal = true;
            break;
        }
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
    game.set_ship_pos(1, Hex::new(1, 0)).unwrap();
    game.set_ship_facing(1, 3).unwrap();
    game.set_ship_pos(2, Hex::new(-3, 0)).unwrap();
    game.set_ship_facing(2, 0).unwrap();
    game.set_ship_shields(2, [100; 6]).unwrap();
    game.set_ship_structure(2, 100).unwrap();
    let before_miss = game.ship(2).unwrap().shields;

    apply_order(&mut game, Order::Fire {
        ship: 1,
        weapon: "disruptor_1".to_string(),
        target: 2,
    })
    .expect("first disruptor fire succeeds");
    apply_order(&mut game, Order::RunTurn).expect("resolve miss fire");
    let after_miss = game.ship(2).expect("defender exists").shields;
    assert_eq!(shield_damage_taken(&before_miss, &after_miss), 0);

    game.set_ship_pos(2, Hex::new(0, 0)).unwrap();
    let before_hit = game.ship(2).expect("defender exists").shields;

    apply_order(&mut game, Order::Fire {
        ship: 1,
        weapon: "disruptor_1".to_string(),
        target: 2,
    })
    .expect("second disruptor fire succeeds");
    apply_order(&mut game, Order::RunTurn).expect("resolve hit fire");

    let after_hit = game.ship(2).expect("defender exists").shields;
    assert_eq!(shield_damage_taken(&before_hit, &after_hit), 4);
}

// Runs a fixed combat sequence from a freshly loaded scenario (same seed every
// time) and returns the serialized final snapshot as a byte string. The sequence
// issues real fire orders, each of which draws from the seeded PRNG, so the
// output would diverge run-to-run if any ambient (non-seeded) source entered the
// draw path.
fn run_seeded_fire_sequence() -> String {
    let mut game = load_combat();
    // Give the defender deep shields/structure so each shot's exact PRNG-rolled
    // damage lands on the toward-attacker facing without saturating to zero -- the
    // drawn value stays observable in the final snapshot, so any change to the
    // draw sequence changes the serialized output.
    game.set_ship_shields(2, [100; 6]).unwrap();
    game.set_ship_structure(2, 100).unwrap();
    for _ in 0..3 {
        apply_order(&mut game, Order::Fire {
            ship: 1,
            weapon: "phaser_1".to_string(),
            target: 2,
        })
        .expect("phaser fire succeeds");
        apply_order(&mut game, Order::RunTurn).expect("resolve fire at turn end");
    }
    serde_json::to_string(&StateSnapshot::from_game_state(&game)).expect("snapshot serializes")
}

#[test]
fn test_same_seed_same_orders_identical() {
    let first = run_seeded_fire_sequence();
    let second = run_seeded_fire_sequence();

    // Byte-identical serialized snapshots across two independent runs prove the
    // ADR-0005 reproducibility invariant: same seed + same order sequence yields
    // the same outcome. Compare the raw strings, not just parsed values.
    assert_eq!(first, second);

    // Guard against a vacuous pass: the sequence must actually draw from the PRNG
    // and land damage, so the compared snapshot is not the untouched initial state.
    // The sequence starts the defender at 6 facings of 100 (600 total); a real draw
    // strictly reduces that total.
    let final_state: Value = serde_json::from_str(&first).expect("snapshot parses");
    let defender_shield_total: u64 = final_state["ships"][1]["shields"]
        .as_array()
        .expect("defender shields array")
        .iter()
        .map(|value| value.as_u64().expect("shield is a number"))
        .sum();
    assert!(
        defender_shield_total < 6 * 100,
        "the seeded fire sequence must have drawn damage from the PRNG"
    );
}
