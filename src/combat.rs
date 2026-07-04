use crate::game_state::GameState;
use crate::hex::Hex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeaponKind {
    Phaser,
    Disruptor,
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

pub fn resolve_fire(game: &mut GameState, weapon_id: &str, target_id: u32) -> Option<FireOutcome> {
    let attacker_index = game.weapon_owner_index(weapon_id)?;
    let target_index = game.ship_index(target_id)?;

    let attacker = game.ships[attacker_index].clone();
    let weapon = attacker
        .weapons
        .iter()
        .find(|weapon| weapon.id == weapon_id)?
        .clone();
    let target_pos = game.ships[target_index].pos;
    let shield =
        relative_bearing(game.ships[target_index].facing, target_pos, attacker.pos) as usize;
    let damage = match weapon.kind {
        WeaponKind::Phaser => weapon.damage + game.prng.roll(2) - 1,
        WeaponKind::Disruptor => weapon.damage,
    };

    let target = &mut game.ships[target_index];
    let absorbed = target.shields[shield].min(damage);
    target.shields[shield] -= absorbed;
    let overflow = damage - absorbed;
    target.structure = target.structure.saturating_sub(overflow);
    target.destroyed = target.structure == 0;

    Some(FireOutcome {
        attacker: attacker.id,
        target: target_id,
        shield,
        damage,
    })
}
