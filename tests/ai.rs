//! AI unit tests (seek target still valid).

use shipsim_core::ai::seek_target;
use shipsim_core::board::Board;
use shipsim_core::game_state::GameState;
use shipsim_core::hex::Hex;
use shipsim_core::momentum::Keel;
use shipsim_core::ship::Ship;
use shipsim_core::ssd::Ssd;
use std::collections::BTreeMap;

fn ship(id: u32, q: i32, r: i32) -> Ship {
    Ship {
        id,
        class: "t".into(),
        pos: Hex::new(q, r),
        facing: 0,
        speed: 4,
        power: 8,
        power_remaining: 8,
        movement_point_ratio: 1,
        shield_point_ratio_den: 1,
        turn_speed: 4,
        weapons_energy: 4,
        shield_reinforce: 0,
        turn_mode: 0,
        weapons: vec![],
        shields: [0; 6],
        shields_powered: [0; 6],
        shields_remaining: [0; 6],
        max_shield_per_facing: 6,
        movement_allocated: 0,
        move_remaining: 0,
        keel: Keel::Stopped,
        weapon_charges: BTreeMap::new(),
        ssd: Ssd::new(10, 4, 2, 0),
        destroyed: false,
    }
}

#[test]
fn test_seek_nearest() {
    let game = GameState::new(
        Board::new(10, 10),
        vec![ship(1, 0, 0), ship(2, 5, 0), ship(3, 3, 0)],
        Hex::new(9, 9),
    );
    assert_eq!(seek_target(&game, 1), Some(3));
}
