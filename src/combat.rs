use crate::hex::Hex;
use crate::prng::Prng;
use crate::ship::Ship;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeaponKind {
    Phaser,
    Disruptor,
}

/// Impulse Fire Frequency (simplified): which impulses a weapon class may discharge (D1-fire).
/// Impulses are 1..=32. Phasers every 4th impulse; disruptors every 8th (both include 32).
pub fn fires_on_impulse(kind: &WeaponKind, impulse: u8) -> bool {
    if impulse == 0 || impulse > 32 {
        return false;
    }
    match kind {
        WeaponKind::Phaser => impulse.is_multiple_of(4),
        WeaponKind::Disruptor => impulse.is_multiple_of(8),
    }
}

/// Why a shot is illegal at a given pair of ship positions (declare-time or post-move).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FireIllegal {
    WeaponNotFound,
    TargetDestroyed,
    FireAtSelf,
    OutOfRange { range: u32, max_range: u32 },
    OutOfArc,
}

/// Shared fire legality at the given attacker/target positions (ADR-0008 pre- and post-move).
pub fn fire_legality<'a>(
    attacker: &'a Ship,
    weapon_id: &str,
    target: &Ship,
) -> Result<&'a Weapon, FireIllegal> {
    if target.destroyed {
        return Err(FireIllegal::TargetDestroyed);
    }
    if attacker.id == target.id {
        return Err(FireIllegal::FireAtSelf);
    }
    let weapon = attacker
        .weapon(weapon_id)
        .ok_or(FireIllegal::WeaponNotFound)?;
    let range = attacker.pos.distance(target.pos);
    if range > weapon.max_range {
        return Err(FireIllegal::OutOfRange {
            range,
            max_range: weapon.max_range,
        });
    }
    let relative = relative_bearing(attacker.facing, attacker.pos, target.pos);
    if !arc_contains(&weapon.arc, relative) {
        return Err(FireIllegal::OutOfArc);
    }
    Ok(weapon)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arc {
    Forward,
    Rear,
    Left,
    Right,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weapon {
    pub id: String,
    pub kind: WeaponKind,
    pub arc: Arc,
    pub max_range: u32,
    pub damage: u32,
    pub phaser_dice_by_range: Vec<u32>,
    pub to_hit_by_range: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FireOutcome {
    pub attacker: u32,
    pub target: u32,
    pub shield: usize,
    pub damage: u32,
}

pub fn bearing_to(from: Hex, to: Hex) -> u8 {
    let neighbors = from.neighbors();
    neighbors
        .iter()
        .enumerate()
        .min_by_key(|(facing, neighbor)| (neighbor.distance(to), *facing))
        .map(|(facing, _)| facing as u8)
        .unwrap_or(0)
}

pub fn arc_contains(arc: &Arc, relative_bearing: u8) -> bool {
    match arc {
        Arc::Forward => relative_bearing == 0,
        Arc::Rear => relative_bearing == 3,
        Arc::Left => matches!(relative_bearing, 1 | 2),
        Arc::Right => matches!(relative_bearing, 4 | 5),
        Arc::All => true,
    }
}

pub fn relative_bearing(origin_facing: u8, from: Hex, to: Hex) -> u8 {
    (bearing_to(from, to) + 6 - origin_facing) % 6
}

fn ranged_value(values: &[u32], range: u32) -> Option<u32> {
    if range == 0 {
        return values.first().copied();
    }
    values
        .get((range - 1) as usize)
        .copied()
        .or_else(|| values.last().copied())
}

fn resolve_weapon_damage(weapon: &Weapon, range: u32, prng: &mut Prng) -> u32 {
    match weapon.kind {
        WeaponKind::Phaser => {
            if let Some(dice) = ranged_value(&weapon.phaser_dice_by_range, range) {
                (0..dice).map(|_| prng.roll(6)).sum()
            } else {
                weapon.damage + prng.roll(2) - 1
            }
        }
        WeaponKind::Disruptor => {
            if let Some(threshold) = ranged_value(&weapon.to_hit_by_range, range) {
                if prng.roll(6) <= threshold {
                    weapon.damage
                } else {
                    0
                }
            } else {
                weapon.damage
            }
        }
    }
}

/// Resolve a direct-fire shot against a target ship. Pure of `GameState` (ADR encapsulation).
pub fn resolve_fire(
    attacker: &Ship,
    weapon_id: &str,
    target: &mut Ship,
    prng: &mut Prng,
) -> Option<FireOutcome> {
    let weapon = attacker.weapon(weapon_id)?.clone();
    let range = attacker.pos.distance(target.pos);
    let shield = relative_bearing(target.facing, target.pos, attacker.pos) as usize;
    let damage = resolve_weapon_damage(&weapon, range, prng);
    target.apply_hit(shield, damage);
    Some(FireOutcome {
        attacker: attacker.id,
        target: target.id,
        shield,
        damage,
    })
}

#[cfg(test)]
mod fire_window_tests {
    use super::*;

    #[test]
    fn test_phaser_windows() {
        assert!(!fires_on_impulse(&WeaponKind::Phaser, 1));
        assert!(fires_on_impulse(&WeaponKind::Phaser, 4));
        assert!(fires_on_impulse(&WeaponKind::Phaser, 32));
        assert!(!fires_on_impulse(&WeaponKind::Phaser, 3));
    }

    #[test]
    fn test_disruptor_windows() {
        assert!(!fires_on_impulse(&WeaponKind::Disruptor, 4));
        assert!(fires_on_impulse(&WeaponKind::Disruptor, 8));
        assert!(fires_on_impulse(&WeaponKind::Disruptor, 32));
    }
}
