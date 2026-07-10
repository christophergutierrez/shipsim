use std::collections::BTreeMap;

use crate::combat::Weapon;
use crate::energy;
use crate::hex::Hex;
use crate::momentum::Keel;
use crate::ssd::Ssd;

#[derive(Debug, Clone)]
pub struct Ship {
    pub id: u32,
    pub class: String,
    pub pos: Hex,
    pub facing: u8,
    /// Design maximum movement speed (IMC cap before engine damage).
    pub speed: u32,
    /// Design energy generated each turn (before power_sys damage).
    pub power: u32,
    /// FASA: remaining power this turn (shared pool for move/fire/soak).
    pub power_remaining: u32,
    /// FASA: power cost of one basic move action.
    pub movement_point_ratio: u32,
    /// FASA: damage points absorbed per 1 power (SPR inverse; 2 => 1 power per 2 damage).
    pub shield_point_ratio_den: u32,
    /// Legacy SFB fields retained for content compatibility (unused in FASA path).
    pub turn_speed: u32,
    pub weapons_energy: u32,
    pub shield_reinforce: u32,
    pub turn_mode: u32,
    pub weapons: Vec<Weapon>,
    pub shields: [u32; 6],
    /// Combat v2: per-facing shield power bought during allocation.
    pub shields_powered: [u32; 6],
    /// Combat v2: remaining per-facing powered shields this turn.
    pub shields_remaining: [u32; 6],
    pub max_shield_per_facing: u32,
    /// Combat v2: movement budget bought this turn.
    pub movement_allocated: u32,
    pub move_remaining: u32,
    pub keel: Keel,
    /// Combat v2: weapon id -> charge bought this turn.
    pub weapon_charges: BTreeMap<String, u32>,
    /// Itemized internals (D6). `ssd.hull` replaces the old flat structure pool for internals.
    pub ssd: Ssd,
    pub destroyed: bool,
}

impl Ship {
    pub fn default_turn_speed(power: u32, max_speed: u32) -> u32 {
        energy::max_allocatable_speed(power, max_speed)
    }

    /// Hull boxes (legacy tests / snapshot "structure" field).
    pub fn structure(&self) -> u32 {
        self.ssd.hull
    }

    pub fn set_structure(&mut self, hull: u32) {
        self.ssd.hull = hull;
        self.destroyed = self.ssd.is_destroyed();
    }

    /// Apply multi-bucket allocation for this turn (respects crippled engine/power).
    pub fn apply_allocation(&mut self, movement: u32, weapons: u32, shields: u32) {
        let max_spd = self.effective_max_speed();
        let max_pow = self.effective_power();
        let movement = movement.min(max_spd);
        // Re-validate cost under crippled power: clamp weapons/shields if needed.
        let mut weapons = weapons;
        let mut shields = shields;
        let mut cost = movement.saturating_add(weapons).saturating_add(shields);
        if cost > max_pow {
            let over = cost - max_pow;
            let cut_w = weapons.min(over);
            weapons -= cut_w;
            cost -= cut_w;
            if cost > max_pow {
                shields = shields.saturating_sub(cost - max_pow);
            }
        }
        self.turn_speed = movement;
        self.weapons_energy = weapons;
        self.shield_reinforce = shields;
    }

    pub fn effective_max_speed(&self) -> u32 {
        self.ssd.effective_max_speed(self.speed)
    }

    pub fn effective_power(&self) -> u32 {
        self.ssd.effective_power(self.power)
    }

    pub fn reset_turn_energy(&mut self) {
        // FASA: restore full power pool for the turn.
        self.power_remaining = self.effective_power();
        // Keep legacy buckets loosely in sync for snapshot display.
        let max_spd = self.effective_max_speed();
        let max_pow = self.effective_power();
        let (movement, weapons, shields) = energy::default_buckets(max_pow, max_spd);
        self.apply_allocation(movement, weapons, shields);
    }

    pub fn reset_v2_allocation(&mut self) {
        self.shields_powered = [0; 6];
        self.shields_remaining = [0; 6];
        self.movement_allocated = 0;
        self.move_remaining = 0;
        self.keel = Keel::Stopped;
        self.weapon_charges.clear();
    }

    /// Apply damage: reinforce → shield facing → SSD allocation for overflow.
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
        if overflow > 0 {
            self.ssd.apply_internal(overflow);
        }
        self.destroyed = self.ssd.is_destroyed();
        // Clamp turn movement if engines just died mid-turn.
        let max_spd = self.effective_max_speed();
        if self.turn_speed > max_spd {
            self.turn_speed = max_spd;
        }
    }

    pub fn weapon(&self, weapon_id: &str) -> Option<&Weapon> {
        let (idx, w) = self
            .weapons
            .iter()
            .enumerate()
            .find(|(_, w)| w.id == weapon_id)?;
        if self.ssd.weapon_operational(idx) {
            Some(w)
        } else {
            None
        }
    }

    pub fn weapon_mut(&mut self, weapon_id: &str) -> Option<&mut Weapon> {
        let idx = self.weapons.iter().position(|w| w.id == weapon_id)?;
        if !self.ssd.weapon_operational(idx) {
            return None;
        }
        self.weapons.get_mut(idx)
    }

    pub fn can_afford_fire(&self) -> bool {
        self.can_afford_weapon_cost(energy::default_fire_energy_cost())
    }

    pub fn can_afford_weapon_cost(&self, cost: u32) -> bool {
        let c = energy::fire_energy_cost_for(cost);
        self.power_remaining >= c
    }

    pub fn spend_fire_energy(&mut self) -> bool {
        self.spend_weapon_energy(energy::default_fire_energy_cost())
    }

    pub fn spend_weapon_energy(&mut self, cost: u32) -> bool {
        let c = energy::fire_energy_cost_for(cost);
        if self.weapons_energy < c {
            return false;
        }
        self.weapons_energy -= c;
        true
    }
}
