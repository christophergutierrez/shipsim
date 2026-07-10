//! Campaign load still works under FASA state init.

use std::path::PathBuf;

use shipsim_core::campaign::Campaign;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

#[test]
fn test_campaign_loads() {
    let c = Campaign::load(&manifest_path("campaigns/demo.toml")).expect("campaign");
    let game = c.load_current().expect("scenario");
    assert_eq!(game.status(), shipsim_core::game_state::ScenarioStatus::InProgress);
    assert_eq!(game.turn_number(), 1);
}
