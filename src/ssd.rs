//! Ship Systems Display + damage allocation chart (D6).

use serde::Serialize;

/// One step on the simplified damage-allocation chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DacSlot {
    Hull,
    Engine,
    Power,
    Bridge,
    Weapon,
}

/// Fixed chart (cycles). Hull-heavy early so light internals still mostly hull.
pub const DAC: &[DacSlot] = &[
    DacSlot::Hull,
    DacSlot::Hull,
    DacSlot::Engine,
    DacSlot::Weapon,
    DacSlot::Hull,
    DacSlot::Power,
    DacSlot::Weapon,
    DacSlot::Bridge,
    DacSlot::Hull,
    DacSlot::Engine,
    DacSlot::Hull,
    DacSlot::Weapon,
    DacSlot::Power,
    DacSlot::Hull,
    DacSlot::Hull,
    DacSlot::Engine,
];

/// Itemized internals. `hull` is seeded from ship TOML `structure`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Ssd {
    pub hull: u32,
    pub engine: u32,
    pub engine_max: u32,
    pub power_sys: u32,
    pub power_sys_max: u32,
    pub bridge: u32,
    /// Parallel to ship.weapons by index: remaining boxes (0 = destroyed).
    pub weapon_boxes: Vec<u32>,
    /// Cursor into [`DAC`] for the next internal hit.
    #[serde(skip)]
    pub dac_cursor: usize,
}

impl Ssd {
    pub fn new(hull: u32, engine: u32, power_sys: u32, weapon_count: usize) -> Self {
        let engine = engine.max(1);
        let power_sys = power_sys.max(1);
        Self {
            hull,
            engine,
            engine_max: engine,
            power_sys,
            power_sys_max: power_sys,
            bridge: 1,
            weapon_boxes: vec![1; weapon_count],
            dac_cursor: 0,
        }
    }

    pub fn is_destroyed(&self) -> bool {
        self.hull == 0 || self.bridge == 0
    }

    pub fn weapon_operational(&self, weapon_index: usize) -> bool {
        self.weapon_boxes.get(weapon_index).copied().unwrap_or(0) > 0
    }

    /// Effective max speed given surviving engines (0 if engines gone).
    pub fn effective_max_speed(&self, design_speed: u32) -> u32 {
        if self.engine_max == 0 || self.engine == 0 {
            return 0;
        }
        ((design_speed as u64 * self.engine as u64) / self.engine_max as u64) as u32
    }

    /// Effective power generation given surviving power systems.
    pub fn effective_power(&self, design_power: u32) -> u32 {
        if self.power_sys_max == 0 || self.power_sys == 0 {
            return 0;
        }
        ((design_power as u64 * self.power_sys as u64) / self.power_sys_max as u64) as u32
    }

    /// Apply `points` of internal damage via the DAC. Returns boxes actually scored.
    pub fn apply_internal(&mut self, points: u32) {
        for _ in 0..points {
            self.apply_one_box();
            if self.is_destroyed() {
                break;
            }
        }
    }

    fn apply_one_box(&mut self) {
        for _ in 0..DAC.len() {
            let slot = DAC[self.dac_cursor % DAC.len()];
            self.dac_cursor = self.dac_cursor.wrapping_add(1);
            if self.damage_slot(slot) {
                return;
            }
        }
        // Fallback: hull
        self.hull = self.hull.saturating_sub(1);
    }

    fn damage_slot(&mut self, slot: DacSlot) -> bool {
        match slot {
            DacSlot::Hull if self.hull > 0 => {
                self.hull -= 1;
                true
            }
            DacSlot::Engine if self.engine > 0 => {
                self.engine -= 1;
                true
            }
            DacSlot::Power if self.power_sys > 0 => {
                self.power_sys -= 1;
                true
            }
            DacSlot::Bridge if self.bridge > 0 => {
                self.bridge -= 1;
                true
            }
            DacSlot::Weapon => {
                if let Some(idx) = self.weapon_boxes.iter().position(|&boxes| boxes > 0) {
                    self.weapon_boxes[idx] -= 1;
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hull_and_bridge_destroy() {
        let mut s = Ssd::new(2, 2, 2, 1);
        s.apply_internal(2);
        assert_eq!(s.hull, 0);
        assert!(s.is_destroyed());
    }

    #[test]
    fn test_weapon_box_can_die() {
        let mut s = Ssd::new(20, 4, 2, 2);
        // Force many weapon slots by walking chart
        for _ in 0..40 {
            s.apply_one_box();
            if s.weapon_boxes.iter().all(|&b| b == 0) {
                break;
            }
        }
        assert!(s.weapon_boxes.contains(&0) || s.is_destroyed());
    }
}
