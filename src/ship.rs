use crate::combat::Weapon;
use crate::energy;
use crate::hex::Hex;

#[derive(Debug, Clone)]
pub struct Ship {
    pub id: u32,
    pub class: String,
    pub pos: Hex,
    pub facing: u8,
    /// Maximum legal movement speed (IMC cap).
    pub speed: u32,
    /// Energy generated each turn (Slice 4).
    pub power: u32,
    /// Allocated movement speed for the current turn (drives IMC + plot length).
    pub turn_speed: u32,
    pub turn_mode: u32,
    pub weapons: Vec<Weapon>,
    pub shields: [u32; 6],
    pub structure: u32,
    pub destroyed: bool,
}

impl Ship {
    /// Default turn allocation: spend full movement budget under power.
    pub fn default_turn_speed(power: u32, max_speed: u32) -> u32 {
        energy::max_allocatable_speed(power, max_speed)
    }

    /// Apply damage to a shield facing; overflow bleeds to structure and may destroy the ship.
    pub fn apply_hit(&mut self, shield_facing: usize, damage: u32) {
        let facing = shield_facing % 6;
        let absorbed = self.shields[facing].min(damage);
        self.shields[facing] -= absorbed;
        let overflow = damage - absorbed;
        self.structure = self.structure.saturating_sub(overflow);
        self.destroyed = self.structure == 0;
    }

    pub fn weapon(&self, weapon_id: &str) -> Option<&Weapon> {
        self.weapons.iter().find(|w| w.id == weapon_id)
    }

    pub fn weapon_mut(&mut self, weapon_id: &str) -> Option<&mut Weapon> {
        self.weapons.iter_mut().find(|w| w.id == weapon_id)
    }

    pub fn reset_turn_energy(&mut self) {
        self.turn_speed = Self::default_turn_speed(self.power, self.speed);
    }
}
