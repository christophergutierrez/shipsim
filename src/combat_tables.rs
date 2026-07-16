//! Combat Model v2 range, hit, and damage tables.
//!
//! These constants mirror `docs/combat-v2-tables.md` and ADR-0020.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponKind {
    Beam,
    Plasma,
    Torp,
}

const BEAM_FACTORS: [f64; 10] = [2.0, 1.9, 1.7, 1.6, 1.4, 1.3, 1.2, 1.1, 1.0, 1.0];
const PLASMA_DAMAGE: [u32; 14] = [8, 6, 5, 4, 3, 3, 2, 2, 1, 1, 1, 1, 1, 1];
const BEAM_TO_HIT: [u8; 10] = [18, 17, 15, 13, 11, 10, 8, 7, 5, 4];
const PLASMA_TO_HIT: [u8; 14] = [16, 14, 12, 10, 8, 6, 5, 4, 3, 2, 2, 2, 1, 1];
const TORP_TO_HIT: [u8; 12] = [14, 13, 12, 11, 10, 9, 7, 6, 5, 4, 3, 3];

/// Hull size whose hit chance is exactly the range-table chance.
/// Matches size tier 2 (Destroyer) in `data/sizes.toml`.
pub const BASELINE_TARGET_SIZE: u32 = 2;

pub fn half_up(value: f64) -> u32 {
    (value + 0.5).floor() as u32
}

pub fn max_range(kind: WeaponKind) -> u32 {
    match kind {
        WeaponKind::Beam => 10,
        WeaponKind::Plasma => 14,
        WeaponKind::Torp => 12,
    }
}

pub fn beam_factor(range: u32) -> Option<f64> {
    table_value(&BEAM_FACTORS, range)
}

pub fn beam_damage(charge: u32, range: u32) -> Option<u32> {
    if charge == 0 {
        return None;
    }
    let damage = half_up(charge as f64 * beam_factor(range)?);
    (damage >= 1).then_some(damage)
}

pub fn plasma_damage(range: u32) -> Option<u32> {
    table_value(&PLASMA_DAMAGE, range)
}

pub fn torp_damage(range: u32) -> Option<u32> {
    (range >= 1 && range <= max_range(WeaponKind::Torp)).then_some(4)
}

pub fn to_hit_threshold(kind: WeaponKind, range: u32) -> Option<u8> {
    match kind {
        WeaponKind::Beam => table_value(&BEAM_TO_HIT, range),
        WeaponKind::Plasma => table_value(&PLASMA_TO_HIT, range),
        WeaponKind::Torp => table_value(&TORP_TO_HIT, range),
    }
}

/// Scale the range-table d20 threshold by target silhouette.
///
/// ```text
/// mult = target_size / BASELINE_TARGET_SIZE   // size 2 → 1.0, size 7 → 3.5
/// adjusted = round_half_up(table × mult), clamp 1..=20
/// ```
/// Lever #6 trials (soft ×2.25, medium ×2.67, mild capital ×3.0) all shifted
/// abc claim B to capital stomps at n=1k; keep classic size/2 for the #1–#5 lock.
pub fn size_adjusted_to_hit_threshold(
    kind: WeaponKind,
    range: u32,
    target_size: u32,
) -> Option<u8> {
    if target_size == 0 {
        return None;
    }
    let base = u32::from(to_hit_threshold(kind, range)?);
    let scaled = base
        .saturating_mul(target_size)
        .saturating_add(BASELINE_TARGET_SIZE / 2)
        / BASELINE_TARGET_SIZE;
    Some(scaled.clamp(1, 20) as u8)
}

fn table_value<T: Copy>(values: &[T], range: u32) -> Option<T> {
    if range == 0 {
        return None;
    }
    values.get((range - 1) as usize).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_up_rounds_positive_values() {
        assert_eq!(half_up(0.4), 0);
        assert_eq!(half_up(0.5), 1);
        assert_eq!(half_up(0.8), 1);
        assert_eq!(half_up(2.3), 2);
    }

    #[test]
    fn beam_damage_uses_charge_and_range_factor() {
        assert_eq!(beam_damage(1, 1), Some(2));
        assert_eq!(beam_damage(3, 1), Some(6));
        assert_eq!(beam_damage(1, 10), Some(1));
        assert_eq!(beam_damage(1, 11), None);
        assert_eq!(beam_damage(0, 1), None);
    }

    #[test]
    fn fixed_damage_tables_match_v2_contract() {
        assert_eq!(torp_damage(1), Some(4));
        assert_eq!(torp_damage(12), Some(4));
        assert_eq!(torp_damage(13), None);
        assert_eq!(plasma_damage(1), Some(8));
        assert_eq!(plasma_damage(10), Some(1));
    }

    #[test]
    fn to_hit_tables_are_range_limited() {
        assert_eq!(to_hit_threshold(WeaponKind::Beam, 1), Some(18));
        assert_eq!(to_hit_threshold(WeaponKind::Beam, 10), Some(4));
        assert_eq!(to_hit_threshold(WeaponKind::Beam, 11), None);
        assert_eq!(to_hit_threshold(WeaponKind::Plasma, 14), Some(1));
        assert_eq!(to_hit_threshold(WeaponKind::Torp, 12), Some(3));
    }

    #[test]
    fn target_size_scales_threshold_from_size_two_baseline() {
        // mult = size/2 (classic). Beam r3 base 15.
        assert_eq!(
            size_adjusted_to_hit_threshold(WeaponKind::Beam, 3, 1),
            Some(8)
        );
        assert_eq!(
            size_adjusted_to_hit_threshold(WeaponKind::Beam, 3, 2),
            Some(15)
        );
        assert_eq!(
            size_adjusted_to_hit_threshold(WeaponKind::Beam, 3, 4),
            Some(20)
        );
        assert_eq!(
            size_adjusted_to_hit_threshold(WeaponKind::Beam, 10, 1),
            Some(2)
        );
        assert_eq!(
            size_adjusted_to_hit_threshold(WeaponKind::Beam, 10, 7),
            Some(14) // half_up(4 * 3.5)
        );
        assert_eq!(size_adjusted_to_hit_threshold(WeaponKind::Beam, 1, 0), None);
    }
}
