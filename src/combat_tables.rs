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

/// Upper clamp for a size-adjusted threshold at a specific range.
///
/// A d20 roll can never exceed 20, so any threshold ≥ 20 is a guaranteed hit.
/// With the size-2 base table (r1 beam = 18) and `mult = size/2`, every hull
/// size 3+ hits threshold ≥ 27 at range 1 → clamped to 20 → auto-hit. That
/// voids the to-hit table's granularity for 6 of 7 size tiers and is the
/// structural root cause of the post-FF-fix swarm dominance.
///
/// The ceiling is never lower than the size-2 range-table threshold, so a
/// larger target cannot become harder to hit. At lower base chances the floor
/// of 15 still limits the size multiplier without flattening the range table.
pub fn to_hit_threshold_max(base_threshold: u8) -> u8 {
    base_threshold.max(15).min(19)
}

/// Scale the range-table d20 threshold by target silhouette.
///
/// ```text
/// mult = target_size / BASELINE_TARGET_SIZE   // size 2 → 1.0, size 7 → 3.5
/// adjusted = round_half_up(table × mult)
/// adjusted = clamp(adjusted, 1, to_hit_threshold_max(table threshold))
/// ```
/// The upper clamp is monotonic (`to_hit_threshold_max`): larger targets retain
/// their silhouette advantage without becoming automatic hits.
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
    let max = u32::from(to_hit_threshold_max(base as u8));
    Some(scaled.clamp(1, max) as u8)
}

/// Resolve the final d20 threshold, including catalog fire control.
///
/// Fire-control bonuses are intentionally limited to the size-2 baseline. This
/// keeps them from changing fighter or capital engagements unless a future rule
/// explicitly broadens their scope.
pub fn final_to_hit_threshold(
    kind: WeaponKind,
    range: u32,
    target_size: u32,
    attack_accuracy_bonus: u8,
) -> Option<u8> {
    let threshold = size_adjusted_to_hit_threshold(kind, range, target_size)?;
    let bonus = if target_size == BASELINE_TARGET_SIZE {
        attack_accuracy_bonus
    } else {
        0
    };
    Some(threshold.saturating_add(bonus).min(19))
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
        // size 4 → 15 × 2 = 30, clamped to max(base 15, floor 15).
        assert_eq!(
            size_adjusted_to_hit_threshold(WeaponKind::Beam, 3, 4),
            Some(15)
        );
        assert_eq!(
            size_adjusted_to_hit_threshold(WeaponKind::Beam, 10, 1),
            Some(2)
        );
        assert_eq!(
            size_adjusted_to_hit_threshold(WeaponKind::Beam, 10, 7),
            Some(14) // half_up(4 * 3.5) = 14, under the cap
        );
        assert_eq!(size_adjusted_to_hit_threshold(WeaponKind::Beam, 1, 0), None);
    }

    #[test]
    fn to_hit_threshold_cap_is_monotonic_and_never_guarantees_hits() {
        // At range 1, larger targets retain the size-2 table chance without
        // becoming guaranteed hits.
        assert_eq!(
            size_adjusted_to_hit_threshold(WeaponKind::Beam, 1, 2),
            Some(18)
        );
        assert_eq!(
            size_adjusted_to_hit_threshold(WeaponKind::Beam, 1, 3),
            Some(18)
        );
        assert_eq!(
            size_adjusted_to_hit_threshold(WeaponKind::Beam, 1, 4),
            Some(18)
        );
        assert_eq!(
            size_adjusted_to_hit_threshold(WeaponKind::Beam, 1, 5),
            Some(18)
        );
        assert_eq!(
            size_adjusted_to_hit_threshold(WeaponKind::Beam, 1, 7),
            Some(18)
        );
        // At range 4, size scaling can improve on the base 13, but only to 15.
        assert_eq!(
            size_adjusted_to_hit_threshold(WeaponKind::Beam, 4, 2),
            Some(13)
        );
        assert_eq!(
            size_adjusted_to_hit_threshold(WeaponKind::Beam, 4, 3),
            Some(15)
        );
    }

    #[test]
    fn target_size_never_reduces_hit_threshold() {
        for kind in [WeaponKind::Beam, WeaponKind::Plasma, WeaponKind::Torp] {
            for range in 1..=max_range(kind) {
                let mut previous = 0;
                for size in 1..=7 {
                    let threshold = size_adjusted_to_hit_threshold(kind, range, size).unwrap();
                    assert!(
                        threshold >= previous,
                        "{kind:?} range {range}: size {size} threshold {threshold} < {previous}"
                    );
                    assert!(threshold < 20);
                    previous = threshold;
                }
            }
        }
    }

    #[test]
    fn catalog_accuracy_applies_only_to_size_two_and_preserves_miss_chance() {
        assert_eq!(
            final_to_hit_threshold(WeaponKind::Beam, 10, 2, 10),
            Some(14)
        );
        assert_eq!(final_to_hit_threshold(WeaponKind::Beam, 1, 2, 10), Some(19));
        assert_eq!(final_to_hit_threshold(WeaponKind::Beam, 10, 1, 10), Some(2));
        assert_eq!(final_to_hit_threshold(WeaponKind::Beam, 10, 3, 10), Some(6));
    }

    #[test]
    fn partial_catalog_accuracy_preserves_each_weapon_range_curve() {
        for kind in [WeaponKind::Beam, WeaponKind::Plasma, WeaponKind::Torp] {
            let short = final_to_hit_threshold(kind, 1, 2, 10).unwrap();
            let long = final_to_hit_threshold(kind, max_range(kind), 2, 10).unwrap();
            assert_eq!(short, 19);
            assert!(long < short, "{kind:?} range curve was flattened");
        }
    }
}
