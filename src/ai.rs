//! NPC tactics (D9). Greedy seek + opportunistic fire.

use crate::game_state::GameState;

/// Nearest living enemy on the opposing side (other id). Lowest id wins ties. Deterministic.
/// Side rule: player-controlled ships vs. all NPC ships (GreedySeek/Scripted).
pub fn seek_target(game: &GameState, ship_id: u32) -> Option<u32> {
    let origin = game.ship(ship_id)?.pos;
    let attacker_is_npc = game.npc(ship_id).is_some();
    let mut best: Option<(u32, u32, u32)> = None; // (dist, id, id)
    for other in game.ships() {
        if other.id == ship_id || other.destroyed {
            continue;
        }
        let target_is_npc = game.npc(other.id).is_some();
        if attacker_is_npc == target_is_npc {
            continue; // same side
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

use crate::combat_tables::WeaponKind as V2Kind;
use crate::motion::Maneuver;
use std::collections::BTreeMap;

/// Greedy v2 allocation for one ship: spend design power to close the gap, then
/// arm operational weapons, then power the forward shield facings. Deterministic
/// and always within `effective_power` (so `allocate_v2` accepts it).
pub fn v2_allocation(
    game: &GameState,
    ship_id: u32,
) -> Option<(u32, BTreeMap<String, u32>, [u32; 6])> {
    let ship = game.ship(ship_id)?;
    if ship.destroyed {
        return None;
    }
    let mut remaining = ship.effective_power();

    // Movement: buy enough engine power for the intended thrust, respecting hull
    // efficiency instead of treating reactor power as distance.
    let dist = seek_target(game, ship_id)
        .and_then(|tid| game.ship(tid))
        .map(|target| ship.pos.distance(target.pos))
        .unwrap_or(0);
    let desired_thrust = if ship.effective_max_speed() == 0 {
        0
    } else {
        ship.effective_max_speed().min(dist).max(1)
    };
    let movement = desired_thrust
        .saturating_mul(ship.thrust_conversion.power_per_thrust)
        .div_ceil(ship.thrust_conversion.thrust_per_power)
        .min(remaining);
    remaining -= movement;

    // Arm operational weapons. Charge carries (protocol 3): pay only for increases;
    // never request less than the carried charge.
    let mut weapons: BTreeMap<String, u32> = BTreeMap::new();
    for weapon in &ship.weapons {
        if ship.weapon(&weapon.id).is_none() {
            continue; // SSD-destroyed
        }
        let have = ship
            .weapon_charges
            .get(&weapon.id)
            .copied()
            .unwrap_or(0);
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

/// Preferred travel facing toward the seek target (one step from current pos).
pub fn preferred_course(game: &GameState, ship_id: u32, target_id: u32) -> Option<u8> {
    let ship = game.ship(ship_id)?;
    let target = game.ship(target_id)?;
    (0..6).min_by_key(|course| {
        let next = ship.pos + crate::hex::Hex::direction(*course).unwrap();
        (next.distance(target.pos), *course)
    })
}

/// Deterministic maneuver selection for the production NPC driver (protocol 3).
/// Prefer: turn nose toward target if needed, else accel when aligned, else coast.
pub fn v2_move_decision(game: &GameState, ship_id: u32) -> Option<Maneuver> {
    let ship = game.ship(ship_id)?;
    if ship.destroyed {
        return None;
    }
    let desired =
        seek_target(game, ship_id).and_then(|target| preferred_course(game, ship_id, target));
    let velocity = ship.velocity;
    let facing = ship.facing;
    let mut choices = Vec::new();
    if let Some(want) = desired {
        if facing != want {
            choices.push(Maneuver::Turn { facing: want });
        } else if velocity.speed == 0 || velocity.course == want {
            choices.push(Maneuver::Accel);
        } else if facing == crate::motion::opposite_dir(velocity.course) {
            // Facing reverse of momentum — cancel first.
            choices.push(Maneuver::Accel);
        } else {
            choices.push(Maneuver::Turn { facing: want });
        }
    } else if velocity.speed < ship.max_velocity {
        choices.push(Maneuver::Accel);
    }
    choices.push(Maneuver::Coast);
    choices.into_iter().find(|maneuver| {
        crate::motion::resolve_maneuver(velocity, facing, ship.max_velocity, *maneuver)
            .is_ok_and(|result| result.thrust_cost <= ship.thrust_remaining)
    })
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
    use crate::hex::Hex;
    use crate::ship::Ship;
    use crate::ssd::Ssd;
    use std::collections::BTreeMap;

    fn ship(id: u32, q: i32, r: i32) -> Ship {
        Ship {
            id,
            class: "t".into(),
            size: crate::combat_tables::BASELINE_TARGET_SIZE,
            pos: Hex::new(q, r),
            facing: 0,
            speed: 4,
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
            max_velocity: 4,
            thrust_conversion: crate::thrust::ThrustConversion::new(1, 1, 4).unwrap(),
            velocity: crate::motion::Velocity::stopped(0).unwrap(),
            thrust_remaining: 0,
        }
    }

    #[test]
    fn test_seek_nearest_lowest_id_tiebreak() {
        let mut npcs = std::collections::BTreeMap::new();
        npcs.insert(2, crate::game_state::NpcController::GreedySeek);
        npcs.insert(3, crate::game_state::NpcController::GreedySeek);
        let game = GameState::new_with_options(
            Board::new(10, 10),
            vec![ship(1, 0, 0), ship(2, 5, 0), ship(3, 3, 0)],
            Some(crate::game_state::Terminal::ReachHex(Hex::new(9, 9))),
            npcs,
            1,
        );
        assert_eq!(seek_target(&game, 2), Some(1));
    }
}
