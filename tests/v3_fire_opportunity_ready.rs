//! fire_opportunity must ignore ready-locked ships and already-queued weapons.

use shipsim_core::movement::{apply_order, Order};
use shipsim_core::motion::Maneuver;
use shipsim_core::rules::Ruleset;
use shipsim_core::scenario::load_scenario_def_with_rules;
use shipsim_core::schema::ScenarioDef;
use std::collections::BTreeMap;

/// Player vs AI duel. AI does not allocate in-process without the harness NPC
/// pump — so only the player needs orders; we still need the AI allocated to
/// leave allocate phase. Use two player controllers for phase machine, but
/// fire_opportunity only targets non-player, so ship 2 must be `ai` and we
/// pre-resolve NPC allocates by driving both as player then... 
///
/// Simpler approach: both ships are player for phase advancement; for
/// fire_opportunity tests we use a temporary same-side-not-allowed design by
/// marking ship 2 as `ai` and calling the internal allocate path for the AI
/// via the same Order::Allocate (engine accepts allocate for any living ship).
fn load_duel() -> shipsim_core::game_state::GameState {
    let rules = Ruleset::builtin();
    let toml = r#"
width = 12
height = 10
seed = 1
map_mode = "hard"
[terminal]
type = "destruction"
target = 2
[[ships]]
id = 1
class = "heavy_cruiser"
q = 2
r = 4
facing = 0
controller = "player"
[[ships]]
id = 2
class = "escort"
q = 8
r = 4
facing = 3
controller = "ai"
"#;
    let def: ScenarioDef = toml::from_str(toml).unwrap();
    load_scenario_def_with_rules(&def, std::path::Path::new("."), rules).unwrap()
}

fn alloc(game: &mut shipsim_core::game_state::GameState, ship: u32, mov: u32, beam: u32) {
    let mut weapons = BTreeMap::new();
    weapons.insert("beam_1".into(), beam);
    apply_order(
        game,
        Order::Allocate {
            ship,
            movement: mov,
            weapons,
            shields: [4, 0, 0, 0, 0, 0],
        },
    )
    .unwrap_or_else(|e| panic!("allocate ship {ship}: {e}"));
}

fn coast_into_fire(game: &mut shipsim_core::game_state::GameState) {
    assert_eq!(game.phase_name(), "movement");
    // One cycle: both commit → slide → firing. AI commit is accepted via order API.
    apply_order(
        game,
        Order::CommitManeuver {
            ship: 1,
            maneuver: Maneuver::Coast,
        },
    )
    .unwrap();
    apply_order(
        game,
        Order::CommitManeuver {
            ship: 2,
            maneuver: Maneuver::Coast,
        },
    )
    .unwrap();
    assert_eq!(game.phase_name(), "firing");
}

#[test]
fn fire_opportunity_skips_ready_ship() {
    let mut game = load_duel();
    alloc(&mut game, 1, 2, 4);
    alloc(&mut game, 2, 2, 4); // AI hull also allocated via order API
    coast_into_fire(&mut game);

    let before = game.fire_opportunity();
    assert!(
        before.is_some(),
        "expected a legal fire opportunity before ready"
    );
    let ready_ship = before.as_ref().unwrap().ship;
    assert_eq!(ready_ship, 1, "only the player is advertised");
    apply_order(&mut game, Order::ReadyFire { ship: ready_ship }).unwrap();
    // Player is ready — no remaining player opportunity (only one player ship).
    assert!(
        game.fire_opportunity().is_none(),
        "ready-locked sole player ship must clear fire_opportunity"
    );
    assert!(!game.end_turn_warning());
}

#[test]
fn fire_opportunity_skips_already_queued_weapon() {
    let mut game = load_duel();
    alloc(&mut game, 1, 2, 4);
    alloc(&mut game, 2, 2, 0);
    coast_into_fire(&mut game);

    let opp = game
        .fire_opportunity()
        .expect("ship 1 should have a charged beam opportunity");
    assert_eq!(opp.ship, 1);
    assert_eq!(opp.weapon, "beam_1");
    apply_order(
        &mut game,
        Order::CommitFire {
            ship: 1,
            weapon: "beam_1".into(),
            target: 2,
            shield_facing: opp.legal_shield_facings[0],
        },
    )
    .unwrap();
    // beam_1 is queued — opportunity should not re-offer the same weapon.
    // (HC has torp/plasma too; they may still be offered if charged.)
    if let Some(next) = game.fire_opportunity() {
        assert!(
            !(next.ship == 1 && next.weapon == "beam_1"),
            "must not re-offer already queued beam_1; got {next:?}"
        );
    } else {
        // Fine if nothing else is charged/legal.
    }
}
