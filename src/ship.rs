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
    /// Energy generated each turn.
    pub power: u32,
    /// Allocated movement speed for the current turn (drives IMC + plot length).
    pub turn_speed: u32,
    /// Remaining weapon energy this turn (spent when firing).
    pub weapons_energy: u32,
    /// Temporary shield reinforcement pool (absorbs before facing shields).
    pub shield_reinforce: u32,
    pub turn_mode: u32,
    pub weapons: Vec<Weapon>,
    pub shields: [u32; 6],
    pub structure: u32,
    pub destroyed: bool,
}

impl Ship {
    pub fn default_turn_speed(power: u32, max_speed: u32) -> u32 {
        energy::max_allocatable_speed(power, max_speed)
    }

    /// Apply multi-bucket allocation for this turn.
    pub fn apply_allocation(&mut self, movement: u32, weapons: u32, shields: u32) {
        self.turn_speed = movement;
        self.weapons_energy = weapons;
        self.shield_reinforce = shields;
    }

    pub fn reset_turn_energy(&mut self) {
        let (movement, weapons, shields) = energy::default_buckets(self.power, self.speed);
        self.apply_allocation(movement, weapons, shields);
    }

    /// Apply damage: reinforce pool first, then shield facing, then structure.
    pub fn apply_hit(&mut self, shield_facing: usize, damage: u32) {
        let mut remaining = damage;
        if self.shield_reinforce > 0 && remaining > 0 {
            let absorbed = self.shield_reinforce.min(remaining);
            self.shield_reinforce -= absorbed;
            remaining -= absorbed;
        }
        if remaining == 0 {
            return;
        }
        let facing = shield_facing % 6;
        let absorbed = self.shields[facing].min(remaining);
        self.shields[facing] -= absorbed;
        let overflow = remaining - absorbed;
        self.structure = self.structure.saturating_sub(overflow);
        self.destroyed = self.structure == 0;
    }

    pub fn weapon(&self, weapon_id: &str) -> Option<&Weapon> {
        self.weapons.iter().find(|w| w.id == weapon_id)
    }

    pub fn weapon_mut(&mut self, weapon_id: &str) -> Option<&mut Weapon> {
        self.weapons.iter_mut().find(|w| w.id == weapon_id)
    }

    pub fn can_afford_fire(&self) -> bool {
        self.weapons_energy >= energy::fire_energy_cost()
    }

    pub fn spend_fire_energy(&mut self) -> bool {
        let cost = energy::fire_energy_cost();
        if self.weapons_energy < cost {
            return false;
        }
        self.weapons_energy -= cost;
        true
    }
}
