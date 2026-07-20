use std::collections::BTreeMap;

use crate::combat::Weapon;
use crate::hex::Hex;
use crate::ssd::Ssd;
use crate::thrust::ThrustConversion;

#[derive(Debug, Clone)]
pub struct Ship {
    pub id: u32,
    pub class: String,
    /// Canonical catalog key (ship-definition file stem). Distinct from the
    /// numeric scenario-instance `id` and the human-readable display `class`.
    /// Presentation clients use this to resolve data-driven art; the engine
    /// never reads art. See `docs/SHIP-ART-IMPLEMENTATION-PLAN.md` Phase 1.
    pub class_id: String,
    /// Relative target silhouette used to scale d20 hit thresholds (1..=7 tiers in `data/sizes.toml`).
    pub size: u32,
    pub pos: Hex,
    pub facing: u8,
    /// Design energy generated each turn (before power_sys damage).
    pub power: u32,
    /// Catalog fire-control bonus applied after target-size accuracy.
    pub attack_accuracy_bonus: u8,
    pub weapons: Vec<Weapon>,
    /// Per-facing shield power bought during allocation.
    pub shields_powered: [u32; 6],
    /// Remaining per-facing powered shields this turn.
    pub shields_remaining: [u32; 6],
    pub max_shield_per_facing: u32,
    /// Motion power bought this turn (pre-conversion).
    pub movement_allocated: u32,
    /// Weapon id -> charge (carries across turns).
    pub weapon_charges: BTreeMap<String, u32>,
    /// Itemized internals (D6). `ssd.hull` replaces the old flat structure pool for internals.
    pub ssd: Ssd,
    pub destroyed: bool,
    /// Design maximum path actions per turn (hull cap, not momentum).
    pub max_maneuver_actions: u8,
    /// Rational engine-power-to-motion conversion for this hull.
    pub thrust_conversion: ThrustConversion,
    /// Usable motion points this turn after conversion and hull cap.
    pub motion_available: u32,
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

    /// Effective per-turn maneuver cap after engine SSD damage.
    pub fn effective_max_maneuver_actions(&self) -> u32 {
        self.ssd
            .effective_max_speed(u32::from(self.max_maneuver_actions))
    }

    pub fn effective_power(&self) -> u32 {
        self.ssd.effective_power(self.power)
    }

    pub fn reset_v2_allocation(&mut self) {
        // Shields never carry: every allocate starts faces at 0.
        self.shields_powered = [0; 6];
        self.shields_remaining = [0; 6];
        self.movement_allocated = 0;
        // Weapon charge carries; motion is re-bought each turn.
        self.motion_available = 0;
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
