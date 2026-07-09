//! Energy Allocation helpers (Slice 4 / D7 minimal).
//! Movement speed is chosen from a power budget; weapons/shields free this slice.

/// Maximum movement speed that can be allocated given available power and ship max speed.
pub fn max_allocatable_speed(power: u32, max_speed: u32) -> u32 {
    power.min(max_speed).min(31)
}

/// True if `requested` is a legal movement allocation for this turn.
pub fn is_legal_allocation(power: u32, max_speed: u32, requested: u32) -> bool {
    requested <= max_allocatable_speed(power, max_speed)
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
    fn test_legal_allocation() {
        assert!(is_legal_allocation(4, 4, 0));
        assert!(is_legal_allocation(4, 4, 4));
        assert!(!is_legal_allocation(4, 4, 5));
        assert!(!is_legal_allocation(2, 4, 3));
    }
}
