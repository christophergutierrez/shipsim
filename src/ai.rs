//! NPC tactics (D9). Greedy seek + opportunistic fire.

use crate::combat;
use crate::game_state::GameState;
use crate::hex::Hex;
use crate::impulse::max_plot_steps;
use crate::movement;

/// Nearest living enemy (other id). Lowest id wins ties. Deterministic.
pub fn seek_target(game: &GameState, ship_id: u32) -> Option<u32> {
    let origin = game.ship(ship_id)?.pos;
    let mut best: Option<(u32, u32, u32)> = None; // (dist, id, id)
    for other in game.ships() {
        if other.id == ship_id || other.destroyed {
            continue;
        }
        let d = origin.distance(other.pos);
        let key = (d, other.id);
        match best {
            None => best = Some((d, other.id, other.id)),
            Some((bd, bid, _)) if key < (bd, bid) => best = Some((d, other.id, other.id)),
            _ => {}
        }
    }
    best.map(|(_, id, _)| id)
}

/// Greedy path of up to `max_steps` toward `goal`, shortened to a turn-mode-valid plot.
pub fn build_path_toward(
    game: &GameState,
    ship_id: u32,
    goal: Hex,
    max_steps: u32,
) -> Vec<Hex> {
    let Some(ship) = game.ship(ship_id) else {
        return Vec::new();
    };
    if ship.destroyed || max_steps == 0 {
        return Vec::new();
    }

    let mut path: Vec<Hex> = Vec::new();
    let mut pos = ship.pos;

    while (path.len() as u32) < max_steps {
        if pos.distance(goal) == 0 {
            break;
        }
        let next = if pos.distance(goal) == 1
            && game.board().contains(goal)
            && !game.is_occupied_by_other(ship_id, goal)
            && !path.contains(&goal)
        {
            Some(goal)
        } else {
            greedy_neighbor(game, ship_id, pos, goal, &path)
        };
        let Some(step) = next else {
            break;
        };
        path.push(step);
        pos = step;
    }

    while !path.is_empty() {
        if movement::validate_plot(game, ship_id, &path).is_ok() {
            break;
        }
        path.pop();
    }
    path
}

fn greedy_neighbor(
    game: &GameState,
    ship_id: u32,
    pos: Hex,
    goal: Hex,
    path: &[Hex],
) -> Option<Hex> {
    let mut best: Option<(u8, Hex)> = None;
    for facing in 0u8..=5 {
        let Some(delta) = Hex::direction(facing) else {
            continue;
        };
        let candidate = pos + delta;
        if !game.board().contains(candidate) {
            continue;
        }
        if game.is_occupied_by_other(ship_id, candidate) {
            continue;
        }
        if path.contains(&candidate) {
            continue;
        }
        if candidate.distance(goal) >= pos.distance(goal) {
            continue;
        }
        match best {
            None => best = Some((facing, candidate)),
            Some((bf, _)) if facing < bf => best = Some((facing, candidate)),
            _ => {}
        }
    }
    best.map(|(_, h)| h)
}

/// Plot for a greedy AI: path toward nearest enemy (empty if none / cannot move).
pub fn greedy_plot(game: &GameState, ship_id: u32) -> Vec<Hex> {
    let Some(ship) = game.ship(ship_id) else {
        return Vec::new();
    };
    let max_steps = max_plot_steps(ship.turn_speed);
    let Some(tid) = seek_target(game, ship_id) else {
        return Vec::new();
    };
    let goal = game.ship(tid).map(|s| s.pos).unwrap_or(ship.pos);
    build_path_toward(game, ship_id, goal, max_steps)
}

/// First operational weapon that can legally fire at the seek target.
pub fn choose_fire(game: &GameState, ship_id: u32) -> Option<(String, u32)> {
    let ship = game.ship(ship_id)?;
    if ship.destroyed || !ship.can_afford_fire() {
        return None;
    }
    let target_id = seek_target(game, ship_id)?;
    let target = game.ship(target_id)?;
    for w in &ship.weapons {
        if game.weapon_fired_this_turn(ship_id, &w.id) {
            continue;
        }
        if ship.weapon(&w.id).is_none() {
            continue; // SSD destroyed
        }
        if combat::fire_legality(ship, &w.id, target).is_ok() {
            return Some((w.id.clone(), target_id));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Board;
    use crate::game_state::GameState;
    use crate::ship::Ship;
    use crate::ssd::Ssd;

    fn ship(id: u32, q: i32, r: i32) -> Ship {
        Ship {
            id,
            class: "t".into(),
            pos: Hex::new(q, r),
            facing: 0,
            speed: 4,
            power: 8,
            turn_speed: 4,
            weapons_energy: 4,
            shield_reinforce: 0,
            turn_mode: 0,
            weapons: vec![],
            shields: [0; 6],
            ssd: Ssd::new(10, 4, 2, 0),
            destroyed: false,
        }
    }

    #[test]
    fn test_seek_nearest_lowest_id_tiebreak() {
        let game = GameState::new(
            Board::new(10, 10),
            vec![ship(1, 0, 0), ship(2, 5, 0), ship(3, 3, 0)],
            Hex::new(9, 9),
        );
        assert_eq!(seek_target(&game, 1), Some(3));
    }
}
