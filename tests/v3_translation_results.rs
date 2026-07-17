//! Fable Phase 3–4: translation_results + fire_opportunity.

use shipsim_core::rules::Ruleset;
use shipsim_core::scenario::load_scenario_def_with_rules;
use shipsim_core::schema::ScenarioDef;

fn load_ai() -> shipsim_core::game_state::GameState {
    let rules = Ruleset::builtin();
    let text = std::fs::read_to_string("scenarios/ai.toml").expect("scenarios/ai.toml");
    let def: ScenarioDef = toml::from_str(&text).unwrap();
    load_scenario_def_with_rules(&def, std::path::Path::new("."), rules).unwrap()
}

#[test]
fn translation_results_empty_on_load() {
    let game = load_ai();
    assert!(game.translation_results().is_empty());
}

#[test]
fn end_turn_warning_equals_fire_opportunity_presence() {
    let game = load_ai();
    assert_eq!(
        game.end_turn_warning(),
        game.fire_opportunity().is_some(),
        "end_turn_warning must track fire_opportunity"
    );
}

#[test]
fn fire_opportunity_absent_on_allocate_with_uncharged_weapons() {
    // Fresh load: weapons uncharged → no legal fire opportunity.
    let game = load_ai();
    assert!(
        game.fire_opportunity().is_none(),
        "allocate with empty charges should have no fire_opportunity"
    );
}

#[test]
fn enemy_shots_are_not_player_fire_opportunities() {
    use shipsim_core::movement::{apply_order, Order};
    use std::collections::BTreeMap;

    let def: ScenarioDef = toml::from_str(
        r#"
width = 12
height = 6
seed = 1

[terminal]
type = "destruction"
target = 2

[[ships]]
id = 1
class = "escort"
q = 0
r = 0
facing = 0
controller = "player"

[[ships]]
id = 2
class = "escort"
q = 2
r = 0
facing = 3
controller = "scripted"
"#,
    )
    .expect("scenario parses");
    let rules = Ruleset::builtin();
    let mut game =
        load_scenario_def_with_rules(&def, std::path::Path::new("."), rules).expect("loads");

    // Charge only the ENEMY's beam. It then has a fully legal shot at the
    // player (charged, in range 2 <= 10, player in its forward arc) while the
    // player has none.
    apply_order(
        &mut game,
        Order::Allocate {
            ship: 1,
            movement: 0,
            weapons: BTreeMap::new(),
            shields: [0; 6],
        },
    )
    .expect("player allocate");
    apply_order(
        &mut game,
        Order::Allocate {
            ship: 2,
            movement: 0,
            weapons: BTreeMap::from([("beam_1".to_string(), 1)]),
            shields: [0; 6],
        },
    )
    .expect("enemy allocate");

    // The opportunity feed is player-advisory: an enemy's shot at the player
    // is a threat, not an opportunity, and must not drive the call-to-action
    // or end_turn_warning.
    assert!(
        game.fire_opportunity().is_none(),
        "an enemy's legal shot must not be advertised as a player fire opportunity"
    );
    assert!(!game.end_turn_warning());
}
