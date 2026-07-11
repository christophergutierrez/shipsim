use std::collections::BTreeMap;

use crate::combat::Weapon;
use crate::hex::Hex;
use crate::momentum::Keel;
use crate::ssd::Ssd;

#[derive(Debug, Clone)]
pub struct Ship {
    pub id: u32,
    pub class: String,
    pub pos: Hex,
    pub facing: u8,
    /// Design maximum movement speed before engine damage.
    pub speed: u32,
    /// Design energy generated each turn (before power_sys damage).
    pub power: u32,
    pub weapons: Vec<Weapon>,
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
    /// Hull boxes exposed as snapshot structure.
    pub fn structure(&self) -> u32 {
        self.ssd.hull
    }

    pub fn set_structure(&mut self, hull: u32) {
        self.ssd.hull = hull;
        self.destroyed = self.ssd.is_destroyed();
    }

    pub fn effective_max_speed(&self) -> u32 {
        self.ssd.effective_max_speed(self.speed)
    }

    pub fn effective_power(&self) -> u32 {
        self.ssd.effective_power(self.power)
    }

    pub fn reset_v2_allocation(&mut self) {
        self.shields_powered = [0; 6];
        self.shields_remaining = [0; 6];
        self.movement_allocated = 0;
        self.move_remaining = 0;
        self.keel = Keel::Stopped;
        self.weapon_charges.clear();
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
}
