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
