//! Energy Allocation (D7).
//!
//! Per-turn power is split across movement, weapons, and shield reinforcement.
//! Movement allocation is the IMC/plot speed. Weapon energy is spent per shot.
//! Shield reinforcement absorbs damage before hull shields.

/// Cap movement allocation by power, max speed, and IMC table (0..=31).
pub fn max_allocatable_speed(power: u32, max_speed: u32) -> u32 {
    power.min(max_speed).min(31)
}

/// Total energy spent by a multi-bucket allocation.
pub fn allocation_cost(movement: u32, weapons: u32, shields: u32) -> u32 {
    movement.saturating_add(weapons).saturating_add(shields)
}

/// Legal multi-bucket allocation: cost <= power and movement <= max speed (IMC-capped).
pub fn is_legal_multi_allocation(
    power: u32,
    max_speed: u32,
    movement: u32,
    weapons: u32,
    shields: u32,
) -> bool {
    movement <= max_allocatable_speed(power, max_speed)
        && allocation_cost(movement, weapons, shields) <= power
}

/// Default buckets: max movement first, remainder to weapons, shields 0.
pub fn default_buckets(power: u32, max_speed: u32) -> (u32, u32, u32) {
    let movement = max_allocatable_speed(power, max_speed);
    let weapons = power.saturating_sub(movement);
    (movement, weapons, 0)
}

/// Energy cost to fire one shot (Slice 4 deepen: flat 1 unless overridden later).
pub fn fire_energy_cost() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_allocatable_caps_by_power_and_speed() {
        assert_eq!(max_allocatable_speed(10, 4), 4);
        assert_eq!(max_allocatable_speed(2, 4), 2);
        assert_eq!(max_allocatable_speed(40, 40), 31);
    }

    #[test]
    fn test_multi_allocation_budget() {
        assert!(is_legal_multi_allocation(8, 4, 4, 4, 0));
        assert!(is_legal_multi_allocation(8, 4, 4, 3, 1));
        assert!(!is_legal_multi_allocation(8, 4, 4, 5, 0));
        assert!(!is_legal_multi_allocation(8, 4, 5, 0, 0));
    }

    #[test]
    fn test_default_buckets_prefer_movement() {
        assert_eq!(default_buckets(4, 4), (4, 0, 0));
        assert_eq!(default_buckets(8, 4), (4, 4, 0));
        assert_eq!(default_buckets(3, 4), (3, 0, 0));
    }
}
