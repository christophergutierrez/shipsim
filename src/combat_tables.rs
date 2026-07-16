//! Combat Model v2 range, hit, and damage tables.
//!
//! Typed evaluators for the configured combat rules (ADR-0020).

use crate::rules::{
    BeamDamageModel, CombatRules, RangeDamageModel, SizeMultiplier, TorpDamageModel,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponKind {
    Beam,
    Plasma,
    Torp,
}

pub fn half_up(value: f64) -> u32 {
    (value + 0.5).floor() as u32
}

pub fn max_range(rules: &CombatRules, kind: WeaponKind) -> u32 {
    match kind {
        WeaponKind::Beam => rules.weapons().beam().to_hit().len() as u32,
        WeaponKind::Plasma => rules.weapons().plasma().to_hit().len() as u32,
        WeaponKind::Torp => rules.weapons().torp().to_hit().len() as u32,
    }
}

pub fn beam_factor(rules: &CombatRules, range: u32) -> Option<f64> {
    table_value(rules.weapons().beam().range_factors(), range)
}

pub fn beam_damage(rules: &CombatRules, charge: u32, range: u32) -> Option<u32> {
    if charge == 0 {
        return None;
    }
    let factor = beam_factor(rules, range)?;
    let damage = match rules.weapons().beam().damage_model() {
        BeamDamageModel::ChargeTimesRangeFactor => half_up(charge as f64 * factor),
    };
    (damage >= 1).then_some(damage)
}

pub fn plasma_damage(rules: &CombatRules, range: u32) -> Option<u32> {
    match rules.weapons().plasma().damage_model() {
        RangeDamageModel::RangeTable => table_value(rules.weapons().plasma().damage(), range),
    }
}

pub fn torp_damage(rules: &CombatRules, range: u32) -> Option<u32> {
    let in_range = range >= 1 && range <= max_range(rules, WeaponKind::Torp);
    match rules.weapons().torp().damage_model() {
        TorpDamageModel::Flat => in_range.then_some(rules.weapons().torp().flat_damage()),
    }
}

pub fn to_hit_threshold(rules: &CombatRules, kind: WeaponKind, range: u32) -> Option<u8> {
    match kind {
        WeaponKind::Beam => table_value(rules.weapons().beam().to_hit(), range),
        WeaponKind::Plasma => table_value(rules.weapons().plasma().to_hit(), range),
        WeaponKind::Torp => table_value(rules.weapons().torp().to_hit(), range),
    }
}

/// Upper clamp for a size-adjusted threshold at a specific range, from the
/// configured `combat.accuracy.ceiling_floor`/`ceiling_max`
/// (`data/rules/default.toml`, ADR-0024).
///
/// The ceiling is never lower than the unscaled range-table threshold at this
/// range (`base_threshold.max(ceiling_floor)`), so a larger target cannot
/// become *harder* to hit than the size-2 baseline; it is capped at
/// `ceiling_max`, so a larger target cannot become an automatic hit either.
///
/// Historical note: an earlier revision clamped this directly to the die
/// maximum (20 for d20) with no size-aware ceiling. Every hull size 3+ scaled
/// past that clamp at short range (e.g. beam r1 base 18 × mult ≥ 1.5 → ≥ 27),
/// making them auto-hit and erasing the to-hit table's granularity for 6 of 7
/// size tiers. `ceiling_floor`/`ceiling_max` are what fix that; see
/// `docs/combat-v2-tables.md` for the current formula.
pub fn to_hit_threshold_max(rules: &CombatRules, base_threshold: u8) -> u8 {
    base_threshold
        .max(rules.accuracy().ceiling_floor())
        .min(rules.accuracy().ceiling_max())
}

/// Scale the range-table d20 threshold by target silhouette.
///
/// ```text
/// mult = target_size / baseline_target_size   // size 2 → 1.0, size 7 → 3.5
/// adjusted = round_half_up(table × mult)
/// adjusted = clamp(adjusted, 1, to_hit_threshold_max(table threshold))
/// ```
/// The upper clamp is monotonic (`to_hit_threshold_max`): larger targets retain
/// their silhouette advantage without becoming automatic hits.
pub fn size_adjusted_to_hit_threshold(
    rules: &CombatRules,
    kind: WeaponKind,
    range: u32,
    target_size: u32,
) -> Option<u8> {
    if target_size == 0 {
        return None;
    }
    let base = u32::from(to_hit_threshold(rules, kind, range)?);
    let baseline = rules.accuracy().baseline_target_size();
    let scaled = match rules.accuracy().size_multiplier() {
        SizeMultiplier::TargetOverBaseline => {
            base.saturating_mul(target_size)
                .saturating_add(baseline / 2)
                / baseline
        }
    };
    let max = u32::from(to_hit_threshold_max(rules, base as u8));
    Some(scaled.clamp(1, max) as u8)
}

/// Resolve the final d20 threshold, including catalog fire control.
///
/// Fire-control bonuses are intentionally limited to the size-2 baseline. This
/// keeps them from changing fighter or capital engagements unless a future rule
/// explicitly broadens their scope.
pub fn final_to_hit_threshold(
    rules: &CombatRules,
    kind: WeaponKind,
    range: u32,
    target_size: u32,
    attack_accuracy_bonus: u8,
) -> Option<u8> {
    let threshold = size_adjusted_to_hit_threshold(rules, kind, range, target_size)?;
    let bonus = if target_size == rules.accuracy().fire_control_target_size() {
        attack_accuracy_bonus
    } else {
        0
    };
    // Fire control can never push the final threshold past either configured
    // limit: the accuracy ceiling itself, or one below the die maximum (no
    // unmodified *or* fire-control-boosted attack becomes a guaranteed hit).
    let final_cap = rules.accuracy().ceiling_max().min(rules.die_sides() - 1);
    Some(threshold.saturating_add(bonus).min(final_cap))
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

    fn combat_rules() -> crate::rules::CombatRules {
        crate::rules::Ruleset::builtin().combat().clone()
    }

    #[test]
    fn half_up_rounds_positive_values() {
        assert_eq!(half_up(0.4), 0);
        assert_eq!(half_up(0.5), 1);
        assert_eq!(half_up(0.8), 1);
        assert_eq!(half_up(2.3), 2);
    }

    #[test]
    fn beam_damage_uses_charge_and_range_factor() {
        let rules = combat_rules();
        assert_eq!(beam_damage(&rules, 1, 1), Some(2));
        assert_eq!(beam_damage(&rules, 3, 1), Some(6));
        assert_eq!(beam_damage(&rules, 1, 10), Some(1));
        assert_eq!(beam_damage(&rules, 1, 11), None);
        assert_eq!(beam_damage(&rules, 0, 1), None);
    }

    #[test]
    fn fixed_damage_tables_match_v2_contract() {
        let rules = combat_rules();
        assert_eq!(torp_damage(&rules, 1), Some(4));
        assert_eq!(torp_damage(&rules, 12), Some(4));
        assert_eq!(torp_damage(&rules, 13), None);
        assert_eq!(plasma_damage(&rules, 1), Some(8));
        assert_eq!(plasma_damage(&rules, 10), Some(1));
    }

    #[test]
    fn to_hit_tables_are_range_limited() {
        let rules = combat_rules();
        assert_eq!(to_hit_threshold(&rules, WeaponKind::Beam, 1), Some(18));
        assert_eq!(to_hit_threshold(&rules, WeaponKind::Beam, 10), Some(4));
        assert_eq!(to_hit_threshold(&rules, WeaponKind::Beam, 11), None);
        assert_eq!(to_hit_threshold(&rules, WeaponKind::Plasma, 14), Some(1));
        assert_eq!(to_hit_threshold(&rules, WeaponKind::Torp, 12), Some(3));
    }

    #[test]
    fn target_size_scales_threshold_from_size_two_baseline() {
        let rules = combat_rules();
        // mult = size/2 (classic). Beam r3 base 15.
        assert_eq!(
            size_adjusted_to_hit_threshold(&rules, WeaponKind::Beam, 3, 1),
            Some(8)
        );
        assert_eq!(
            size_adjusted_to_hit_threshold(&rules, WeaponKind::Beam, 3, 2),
            Some(15)
        );
        // size 4 → 15 × 2 = 30, clamped to max(base 15, floor 15).
        assert_eq!(
            size_adjusted_to_hit_threshold(&rules, WeaponKind::Beam, 3, 4),
            Some(15)
        );
        assert_eq!(
            size_adjusted_to_hit_threshold(&rules, WeaponKind::Beam, 10, 1),
            Some(2)
        );
        assert_eq!(
            size_adjusted_to_hit_threshold(&rules, WeaponKind::Beam, 10, 7),
            Some(14) // half_up(4 * 3.5) = 14, under the cap
        );
        assert_eq!(
            size_adjusted_to_hit_threshold(&rules, WeaponKind::Beam, 1, 0),
            None
        );
    }

    #[test]
    fn to_hit_threshold_cap_is_monotonic_and_never_guarantees_hits() {
        let rules = combat_rules();
        // At range 1, larger targets retain the size-2 table chance without
        // becoming guaranteed hits.
        assert_eq!(
            size_adjusted_to_hit_threshold(&rules, WeaponKind::Beam, 1, 2),
            Some(18)
        );
        assert_eq!(
            size_adjusted_to_hit_threshold(&rules, WeaponKind::Beam, 1, 3),
            Some(18)
        );
        assert_eq!(
            size_adjusted_to_hit_threshold(&rules, WeaponKind::Beam, 1, 4),
            Some(18)
        );
        assert_eq!(
            size_adjusted_to_hit_threshold(&rules, WeaponKind::Beam, 1, 5),
            Some(18)
        );
        assert_eq!(
            size_adjusted_to_hit_threshold(&rules, WeaponKind::Beam, 1, 7),
            Some(18)
        );
        // At range 4, size scaling can improve on the base 13, but only to 15.
        assert_eq!(
            size_adjusted_to_hit_threshold(&rules, WeaponKind::Beam, 4, 2),
            Some(13)
        );
        assert_eq!(
            size_adjusted_to_hit_threshold(&rules, WeaponKind::Beam, 4, 3),
            Some(15)
        );
    }

    #[test]
    fn target_size_never_reduces_hit_threshold() {
        let rules = combat_rules();
        for kind in [WeaponKind::Beam, WeaponKind::Plasma, WeaponKind::Torp] {
            for range in 1..=max_range(&rules, kind) {
                let mut previous = 0;
                for size in 1..=7 {
                    let threshold =
                        size_adjusted_to_hit_threshold(&rules, kind, range, size).unwrap();
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
        let rules = combat_rules();
        assert_eq!(
            final_to_hit_threshold(&rules, WeaponKind::Beam, 10, 2, 10),
            Some(14)
        );
        assert_eq!(
            final_to_hit_threshold(&rules, WeaponKind::Beam, 1, 2, 10),
            Some(19)
        );
        assert_eq!(
            final_to_hit_threshold(&rules, WeaponKind::Beam, 10, 1, 10),
            Some(2)
        );
        assert_eq!(
            final_to_hit_threshold(&rules, WeaponKind::Beam, 10, 3, 10),
            Some(6)
        );
    }

    #[test]
    fn partial_catalog_accuracy_preserves_each_weapon_range_curve() {
        let rules = combat_rules();
        for kind in [WeaponKind::Beam, WeaponKind::Plasma, WeaponKind::Torp] {
            let short = final_to_hit_threshold(&rules, kind, 1, 2, 10).unwrap();
            let long =
                final_to_hit_threshold(&rules, kind, max_range(&rules, kind), 2, 10).unwrap();
            assert_eq!(short, 19);
            assert!(long < short, "{kind:?} range curve was flattened");
        }
    }

    #[test]
    fn final_threshold_cannot_exceed_a_lowered_configured_ceiling() {
        // ceiling_max (15) here is well below die_sides - 1 (19); a large
        // fire-control bonus must still respect the configured ceiling, not
        // just the die maximum.
        let builtin = crate::rules::Ruleset::builtin();
        assert_eq!(builtin.combat().accuracy().ceiling_max(), 19);

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/rules/default.toml");
        let original = std::fs::read_to_string(&path).expect("read default rules for fixture");
        let lowered = original.replace("ceiling_max = 19", "ceiling_max = 15");
        let rules = crate::rules::Ruleset::from_text(&path, &lowered)
            .expect("lowered ceiling_max is still a valid ruleset")
            .combat()
            .clone();
        assert_eq!(rules.accuracy().ceiling_max(), 15);

        let huge_bonus = 200;
        let threshold = final_to_hit_threshold(&rules, WeaponKind::Beam, 1, 2, huge_bonus).unwrap();
        assert_eq!(
            threshold, 15,
            "fire control must not push the threshold past the configured ceiling_max"
        );
    }

    #[test]
    fn each_configured_damage_model_reaches_its_matching_evaluator() {
        let rules = combat_rules();
        // BeamDamageModel::ChargeTimesRangeFactor
        assert_eq!(beam_damage(&rules, 2, 1), Some(4));
        // RangeDamageModel::RangeTable (plasma)
        assert_eq!(plasma_damage(&rules, 1), Some(8));
        // TorpDamageModel::Flat
        assert_eq!(torp_damage(&rules, 1), Some(4));
        assert_eq!(
            torp_damage(&rules, 1),
            torp_damage(&rules, max_range(&rules, WeaponKind::Torp))
        );
        // SizeMultiplier::TargetOverBaseline
        assert_eq!(
            size_adjusted_to_hit_threshold(&rules, WeaponKind::Beam, 3, 2),
            Some(15)
        );
    }
}
