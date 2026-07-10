//! NPC tactics (D9). Greedy seek + opportunistic fire.

use crate::combat;
use crate::game_state::GameState;
use crate::hex::Hex;
use crate::impulse::max_plot_steps;

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
pub fn build_path_toward(game: &GameState, ship_id: u32, goal: Hex, max_steps: u32) -> Vec<Hex> {
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

// ----- Combat v2 greedy AI (M6) -----
//
// These functions are the v2 counterparts of the FASA helpers above and are
// consumed by `GameState::resolve_v2_npc_actions`. They never mutate state.

use crate::combat_tables::WeaponKind as V2Kind;
use crate::momentum;
use crate::movement::MoveMode;
use std::collections::BTreeMap;

/// Greedy v2 allocation for one ship: spend design power to close the gap, then
/// arm operational weapons, then power the forward shield facings. Deterministic
/// and always within `effective_power` (so `allocate_v2` accepts it).
pub fn v2_allocation(game: &GameState, ship_id: u32) -> Option<(u32, BTreeMap<String, u32>, [u32; 6])> {
    let ship = game.ship(ship_id)?;
    if ship.destroyed {
        return None;
    }
    let mut remaining = ship.effective_power();

    // Movement: close toward the nearest enemy, capped by design speed.
    let dist = seek_target(game, ship_id)
        .and_then(|tid| game.ship(tid))
        .map(|target| ship.pos.distance(target.pos))
        .unwrap_or(0);
    let movement = ship.effective_max_speed().min(dist).min(remaining);
    remaining -= movement;

    // Arm each operational v2 weapon in id order: beams to full charge, single-shot
    // plasma/torps to 1.
    let mut weapons: BTreeMap<String, u32> = BTreeMap::new();
    for weapon in &ship.weapons {
        if remaining == 0 {
            break;
        }
        let Some(kind) = weapon.v2_kind else {
            continue;
        };
        if ship.weapon(&weapon.id).is_none() {
            continue; // SSD-destroyed
        }
        let charge = match kind {
            V2Kind::Beam => weapon.max_charge.min(remaining),
            V2Kind::Plasma | V2Kind::Torp => weapon.max_charge.min(1).min(remaining),
        };
        if charge > 0 {
            weapons.insert(weapon.id.clone(), charge);
            remaining -= charge;
        }
    }

    // Front-facing shields (0 = bow, then its two shoulders) soak the remainder.
    let mut shields = [0u32; 6];
    for facing in [0usize, 1, 5] {
        if remaining == 0 {
            break;
        }
        let add = remaining.min(ship.max_shield_per_facing);
        shields[facing] = add;
        remaining -= add;
    }

    Some((movement, weapons, shields))
}

/// Greedy v2 move decision for the active mover: turn toward the nearest enemy,
/// step forward when that is legal and affordable, else turn in place to stay
/// engaged. Returns a move the movement phase will always accept, or `None` when
/// the ship has no movement power left.
pub fn v2_move_decision(game: &GameState, ship_id: u32) -> Option<MoveMode> {
    let ship = game.ship(ship_id)?;
    if ship.destroyed || ship.move_remaining == 0 {
        return None;
    }
    // Turning in place always costs 1 and is always legal (no collision), so it is
    // the safe fallback that still spends move power.
    let fallback = MoveMode::TurnStarboard;

    let Some(target) = seek_target(game, ship_id).and_then(|tid| game.ship(tid)) else {
        return Some(fallback);
    };
    // `bearing_to` yields the nearest hex direction toward a possibly-distant target
    // (unlike `facing_between`, which only resolves adjacent hexes).
    let desired = crate::arc::bearing_to(ship.pos, target.pos);
    if desired != ship.facing {
        let clockwise = (desired + 6 - ship.facing) % 6;
        return Some(if clockwise <= 3 {
            MoveMode::TurnStarboard
        } else {
            MoveMode::TurnPort
        });
    }

    // Facing the target: step forward if the hex is open and affordable.
    let (cost, _) = momentum::move_cost(ship.keel, momentum::MoveMode::Forward);
    if cost > ship.move_remaining {
        return Some(fallback);
    }
    let Some(delta) = Hex::direction(ship.facing) else {
        return Some(fallback);
    };
    let next = ship.pos + delta;
    let on_board =
        game.board().mode != crate::board::MapMode::Hard || game.board().contains(next);
    if on_board && !game.is_occupied_by_other(ship_id, next) {
        Some(MoveMode::Forward)
    } else {
        Some(fallback)
    }
}

/// Legal v2 fire commits for a ship: every operational, charged weapon that can
/// bear on the seek target, paired with a legal shield facing. Reuses the shared
/// `GameState::v2_shot_shield_facing` legality predicate (no new copy).
pub fn v2_fire_commits(game: &GameState, ship_id: u32) -> Vec<(String, u32, u8)> {
    let Some(attacker) = game.ship(ship_id) else {
        return Vec::new();
    };
    if attacker.destroyed {
        return Vec::new();
    }
    let Some(target) = seek_target(game, ship_id).and_then(|tid| game.ship(tid)) else {
        return Vec::new();
    };
    let mut commits = Vec::new();
    for weapon in &attacker.weapons {
        if let Some(shield_facing) = game.v2_shot_shield_facing(attacker, weapon, target) {
            commits.push((weapon.id.clone(), target.id, shield_facing));
        }
    }
    commits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Board;
    use crate::game_state::GameState;
    use crate::momentum::Keel;
    use crate::ship::Ship;
    use crate::ssd::Ssd;
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
    fn test_seek_nearest_lowest_id_tiebreak() {
        let game = GameState::new(
            Board::new(10, 10),
            vec![ship(1, 0, 0), ship(2, 5, 0), ship(3, 3, 0)],
            Hex::new(9, 9),
        );
        assert_eq!(seek_target(&game, 1), Some(3));
    }
}
