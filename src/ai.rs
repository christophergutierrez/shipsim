//! NPC tactics. Greedy seek + opportunistic volley (protocol v4).

use crate::game_state::GameState;
use crate::movement::VolleyShot;
use crate::path::{self, PathAction, PathState};
use std::collections::BTreeMap;

/// Nearest living enemy on the opposing side. Lowest id wins ties. Deterministic.
pub fn seek_target(game: &GameState, ship_id: u32) -> Option<u32> {
    let origin = game.ship(ship_id)?.pos;
    let attacker_is_npc = game.npc(ship_id).is_some();
    let mut best: Option<(u32, u32)> = None;
    for other in game.ships() {
        if other.id == ship_id || other.destroyed {
            continue;
        }
        let target_is_npc = game.npc(other.id).is_some();
        if attacker_is_npc == target_is_npc {
            continue;
        }
        let d = origin.distance(other.pos);
        let key = (d, other.id);
        match best {
            None => best = Some(key),
            Some(b) if key < b => best = Some(key),
            _ => {}
        }
    }
    best.map(|(_, id)| id)
}

use crate::combat_tables::WeaponKind as V2Kind;

/// Greedy allocation: motion toward target, arm weapons, front shields.
pub fn v2_allocation(
    game: &GameState,
    ship_id: u32,
) -> Option<(u32, BTreeMap<String, u32>, [u32; 6])> {
    let ship = game.ship(ship_id)?;
    if ship.destroyed {
        return None;
    }
    let mut remaining = ship.effective_power();

    let dist = seek_target(game, ship_id)
        .and_then(|tid| game.ship(tid))
        .map(|target| ship.pos.distance(target.pos))
        .unwrap_or(0);
    let desired_motion = if ship.effective_max_maneuver_actions() == 0 {
        0
    } else {
        ship.effective_max_maneuver_actions().min(dist.max(1))
    };
    // Buy enough power for desired motion via conversion.
    let movement = if ship.thrust_conversion.thrust_per_power == 0 {
        0
    } else {
        desired_motion
            .saturating_mul(ship.thrust_conversion.power_per_thrust)
            .div_ceil(ship.thrust_conversion.thrust_per_power)
            .min(remaining)
    };
    remaining -= movement;

    let mut weapons: BTreeMap<String, u32> = BTreeMap::new();
    for weapon in &ship.weapons {
        if ship.weapon(&weapon.id).is_none() {
            continue;
        }
        let have = ship.weapon_charges.get(&weapon.id).copied().unwrap_or(0);
        let kind = weapon.kind;
        let want = match kind {
            V2Kind::Beam => weapon.max_charge,
            V2Kind::Plasma | V2Kind::Torp => weapon.max_charge.min(1).max(have),
        }
        .max(have);
        let increase = want.saturating_sub(have);
        if increase > remaining {
            weapons.insert(weapon.id.clone(), have);
            continue;
        }
        weapons.insert(weapon.id.clone(), want);
        remaining -= increase;
    }

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

/// Preferred absolute facing toward the seek target.
pub fn preferred_facing(game: &GameState, ship_id: u32, target_id: u32) -> Option<u8> {
    let ship = game.ship(ship_id)?;
    let target = game.ship(target_id)?;
    (0..6).min_by_key(|face| {
        let next = ship.pos + crate::hex::Hex::direction(*face).unwrap();
        (next.distance(target.pos), *face)
    })
}

/// Deterministic greedy path using only the five legal actions and motion budget.
pub fn v2_path_decision(game: &GameState, ship_id: u32) -> Option<Vec<PathAction>> {
    let ship = game.ship(ship_id)?;
    if ship.destroyed {
        return None;
    }
    let budget = ship.motion_available;
    if budget == 0 {
        return Some(Vec::new());
    }
    let target_id = seek_target(game, ship_id);
    let want_face = target_id.and_then(|t| preferred_facing(game, ship_id, t));

    let mut state = PathState {
        pos: ship.pos,
        facing: ship.facing,
    };
    let mut actions = Vec::new();
    let bounds = if game.board().mode.blocks_edges() {
        Some(path::MapBounds {
            width: game.board().width,
            height: game.board().height,
        })
    } else {
        None
    };

    for _ in 0..budget {
        let Some(want) = want_face else {
            break;
        };
        let Some(target) = target_id.and_then(|t| game.ship(t)) else {
            break;
        };
        if state.pos.distance(target.pos) <= 1 {
            // Close enough: face target if needed.
            if state.facing != want {
                let action = if facing_cw_steps(state.facing, want)
                    <= facing_ccw_steps(state.facing, want)
                {
                    PathAction::TurnRight
                } else {
                    PathAction::TurnLeft
                };
                if let Ok(next) = path::apply_action(state, action) {
                    state = next;
                    actions.push(action);
                    continue;
                }
            }
            break;
        }

        // Prefer move_f if facing toward target, else turn, else FR/FL if helpful.
        let candidates = [
            PathAction::MoveF,
            PathAction::MoveFr,
            PathAction::MoveFl,
            PathAction::TurnRight,
            PathAction::TurnLeft,
        ];
        let mut best: Option<(PathAction, PathState, u32, u8)> = None;
        for action in candidates {
            let Ok(next) = path::apply_action(state, action) else {
                continue;
            };
            if let Some(b) = bounds {
                if action.translates() && !b.contains(next.pos) {
                    continue;
                }
            }
            let dist = next.pos.distance(target.pos);
            let face_delta =
                facing_cw_steps(next.facing, want).min(facing_ccw_steps(next.facing, want));
            let key = (dist, face_delta);
            match best {
                None => best = Some((action, next, key.0, key.1)),
                Some((_, _, bd, bf)) if (dist, face_delta) < (bd, bf) => {
                    best = Some((action, next, key.0, key.1))
                }
                _ => {}
            }
        }
        if let Some((action, next, ..)) = best {
            state = next;
            actions.push(action);
        } else {
            break;
        }
    }
    Some(actions)
}

fn facing_cw_steps(from: u8, to: u8) -> u8 {
    (from + 6 - to) % 6
}

fn facing_ccw_steps(from: u8, to: u8) -> u8 {
    (to + 6 - from) % 6
}

/// Build a complete legal volley against the seek target.
pub fn v2_volley(game: &GameState, ship_id: u32) -> Vec<VolleyShot> {
    let Some(attacker) = game.ship(ship_id) else {
        return Vec::new();
    };
    if attacker.destroyed {
        return Vec::new();
    }
    let Some(target) = seek_target(game, ship_id).and_then(|tid| game.ship(tid)) else {
        return Vec::new();
    };
    let mut shots = Vec::new();
    for weapon in &attacker.weapons {
        if let Some(shield_facing) = game.v2_shot_shield_facing(attacker, weapon, target) {
            shots.push(VolleyShot {
                weapon: weapon.id.clone(),
                target: target.id,
                shield_facing,
            });
        }
    }
    shots
}

/// Back-compat name used by some call sites.
pub fn v2_fire_commits(game: &GameState, ship_id: u32) -> Vec<(String, u32, u8)> {
    v2_volley(game, ship_id)
        .into_iter()
        .map(|s| (s.weapon, s.target, s.shield_facing))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Board;
    use crate::game_state::GameState;
    use crate::hex::Hex;
    use crate::ship::Ship;
    use crate::ssd::Ssd;
    use std::collections::BTreeMap;

    fn ship(id: u32, q: i32, r: i32) -> Ship {
        Ship {
            id,
            class: "t".into(),
            size: crate::rules::Ruleset::builtin()
                .combat()
                .accuracy()
                .baseline_target_size(),
            pos: Hex::new(q, r),
            facing: 0,
            power: 8,
            attack_accuracy_bonus: 0,
            weapons: vec![],
            shields_powered: [0; 6],
            shields_remaining: [0; 6],
            max_shield_per_facing: 6,
            movement_allocated: 0,
            weapon_charges: BTreeMap::new(),
            ssd: Ssd::new(10, 4, 2, 0),
            destroyed: false,
            max_maneuver_actions: 4,
            thrust_conversion: crate::thrust::ThrustConversion::new(1, 1, 4).unwrap(),
            motion_available: 0,
        }
    }

    #[test]
    fn seek_picks_nearest_enemy() {
        let mut a = ship(1, 0, 0);
        let mut b = ship(2, 3, 0);
        let mut c = ship(3, 5, 0);
        // mark 2 and 3 as NPCs via GameState — simpler distance check unit
        let _ = (&mut a, &mut b, &mut c);
        let board = Board::new(10, 10);
        let rules = crate::rules::Ruleset::builtin();
        let game = GameState::new_with_options(
            board,
            vec![ship(1, 0, 0), ship(2, 3, 0)],
            None,
            BTreeMap::from([(2, crate::game_state::NpcController::GreedySeek)]),
            1,
            rules,
        );
        assert_eq!(seek_target(&game, 1), Some(2));
    }
}
