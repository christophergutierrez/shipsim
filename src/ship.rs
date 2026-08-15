use std::collections::BTreeMap;

use crate::combat::Weapon;
use crate::hex::Hex;
use crate::schema::SystemKind;
use crate::ssd::Ssd;
use crate::thrust::ThrustConversion;

#[derive(Debug, Clone)]
pub struct Ship {
    pub id: u32,
    pub class: String,
    /// Canonical catalog key (ship-definition file stem). Distinct from the
    /// numeric scenario-instance `id` and the human-readable display `class`.
    /// Presentation clients use this to resolve data-driven art; the engine
    /// never reads art. See `frontend/love/assets/ship_art/README.md`.
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
    pub systems: Vec<SystemKind>,
    pub cloaked: bool,
    /// Per-facing shield power bought during allocation.
    pub shields_powered: [u32; 6],
    /// Remaining per-facing powered shields this turn.
    pub shields_remaining: [u32; 6],
    pub max_shield_per_facing: u32,
    /// Per-face shield cap. Catalog ships splat `max_shield_per_facing`.
    pub max_shields: [u32; 6],
    /// Motion power bought this turn (pre-conversion).
    pub movement_allocated: u32,
    /// Weapon id -> charge (carries across turns).
    pub weapon_charges: BTreeMap<String, u32>,
    /// Weapon id -> remaining ammo for magazine-tracked weapons (starts full).
    pub weapon_ammo: BTreeMap<String, u32>,
    /// Itemized internals (D6). `ssd.hull` replaces the old flat structure pool for internals.
    pub ssd: Ssd,
    pub destroyed: bool,
    /// Design maximum path actions per turn (hull cap, not momentum).
    pub max_maneuver_actions: u8,
    /// Rational engine-power-to-motion conversion for this hull.
    pub thrust_conversion: ThrustConversion,
    /// Usable motion points this turn after conversion and hull cap.
    pub motion_available: u32,
    /// Evasive motion points declared on the last resolved path this turn.
    /// Reset to 0 at allocate start; set when paths resolve; used during firing.
    pub evasion_committed: u32,
}

impl Ship {
    /// Maximum repair boxes an installed repair system may restore this turn.
    /// This is the authoritative rule projection used by order validation and
    /// protocol snapshots; clients must not derive it from `size` themselves.
    pub fn repair_cap_for_size(size: u32) -> u32 {
        size.div_ceil(3).max(1)
    }

    pub fn repair_cap(&self) -> u32 {
        Self::repair_cap_for_size(self.size)
    }

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

    pub fn shield_cap(&self, facing: usize) -> u32 {
        self.max_shields
            .get(facing)
            .copied()
            .unwrap_or(self.max_shield_per_facing)
    }

    pub fn reset_v2_allocation(&mut self) {
        // Shields never carry: every allocate starts faces at 0.
        self.shields_powered = [0; 6];
        self.shields_remaining = [0; 6];
        self.movement_allocated = 0;
        // Weapon charge carries; motion is re-bought each turn.
        self.motion_available = 0;
        // Evasion is path-declared each turn; clear stale value from last fire.
        self.evasion_committed = 0;
        self.cloaked = false;
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

#[cfg(test)]
mod tests {
    use super::Ship;

    #[test]
    fn repair_cap_matches_size_bands() {
        assert_eq!(Ship::repair_cap_for_size(1), 1);
        assert_eq!(Ship::repair_cap_for_size(3), 1);
        assert_eq!(Ship::repair_cap_for_size(4), 2);
        assert_eq!(Ship::repair_cap_for_size(6), 2);
        assert_eq!(Ship::repair_cap_for_size(7), 3);
    }
}
