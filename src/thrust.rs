//! Rational engine-power-to-thrust conversion (ADR-0022 §5, M1).
//!
//! Engine power is converted to thrust via a rational ratio
//! (`thrust_per_power` / `power_per_thrust`). All arithmetic is integer-only
//! and deterministic; remainders are truncated, not rounded.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Rational conversion between engine power and thrust.
///
/// `thrust = (power * thrust_per_power) / power_per_thrust`, truncated toward
/// zero. A ratio of `4:1` (tiny hull) yields 4 thrust per power; `1:2`
/// (capital hull) yields 1 thrust per 2 power.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThrustConversion {
    pub thrust_per_power: u32,
    pub power_per_thrust: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ConversionError {
    #[error("thrust_per_power and power_per_thrust are both zero")]
    ZeroRatio,
    #[error("mobile hull (max_velocity {0} > 0) must produce at least one thrust per power")]
    ImmobileRatioForMobile(u8),
    #[error("immobile hull (max_velocity 0) must not produce thrust (thrust_per_power {0} > 0)")]
    MobileRatioForImmobile(u32),
    #[error("power_per_thrust must be nonzero (got {0}); a zero denominator is not a valid ratio")]
    ZeroDenominator(u32),
    #[error("unsupported compound ratio {0}:{1}; at most one component may exceed one")]
    CompoundRatio(u32, u32),
}

impl ThrustConversion {
    /// Construct a conversion, validating the ratio is well-formed.
    ///
    /// The frozen model (ADR-0022 §5) permits ratios where exactly one
    /// conversion direction may exceed one, giving the forms `N:1` (tiny/escort
    /// hulls, N thrust per power), `1:1` (cruiser), and `1:N` (capital/huge
    /// hulls, one thrust per N power). Compound ratios where both components
    /// exceed one (e.g. `3:2`) are rejected: their integer "remainder power" is
    /// not meaningfully representable.
    ///
    /// Rules:
    /// - At least one of `thrust_per_power` / `power_per_thrust` must be nonzero.
    /// - `power_per_thrust` must be nonzero (a zero denominator is not a valid
    ///   ratio; `ThrustConversion::new(1, 0, 4)` is rejected).
    /// - At most one component may exceed one.
    /// - A mobile hull (`max_velocity > 0`) must be able to buy at least one
    ///   thrust with one power (`thrust_per_power >= 1`).
    /// - An immobile hull (`max_velocity == 0`) must produce no thrust
    ///   (`thrust_per_power == 0`).
    pub fn new(
        thrust_per_power: u32,
        power_per_thrust: u32,
        max_velocity: u8,
    ) -> Result<Self, ConversionError> {
        if thrust_per_power == 0 && power_per_thrust == 0 {
            return Err(ConversionError::ZeroRatio);
        }
        // A zero denominator is never a valid ratio, even for a mobile hull
        // that could otherwise produce "free" thrust.
        if power_per_thrust == 0 {
            return Err(ConversionError::ZeroDenominator(power_per_thrust));
        }
        // At most one component may exceed one. Compound ratios like 3:2 are
        // rejected because their integer remainder power is not meaningfully
        // representable.
        if thrust_per_power > 1 && power_per_thrust > 1 {
            return Err(ConversionError::CompoundRatio(
                thrust_per_power,
                power_per_thrust,
            ));
        }
        if max_velocity == 0 {
            // Immobile hulls produce no thrust.
            if thrust_per_power > 0 {
                return Err(ConversionError::MobileRatioForImmobile(thrust_per_power));
            }
        } else {
            // Mobile hulls must be able to buy at least one thrust per power.
            if thrust_per_power == 0 {
                return Err(ConversionError::ImmobileRatioForMobile(max_velocity));
            }
        }
        Ok(Self {
            thrust_per_power,
            power_per_thrust,
        })
    }

    /// Convert engine power into thrust, truncating the remainder.
    ///
    /// Returns `(thrust, remainder_power)` where `remainder_power` is the
    /// leftover power that did not contribute a full thrust this turn. When the
    /// conversion produces no thrust (e.g. an immobile hull), all power is
    /// unused and the remainder is zero — the power was never spent on thrust.
    pub fn convert(&self, power: u32) -> (u32, u32) {
        // Immobile or zero-yield conversion: no thrust, no power consumed.
        if self.thrust_per_power == 0 {
            return (0, 0);
        }
        // power_per_thrust is guaranteed nonzero by the constructor.
        let total_thrust_units = power.saturating_mul(self.thrust_per_power);
        let thrust = total_thrust_units / self.power_per_thrust;
        // Power consumed is the thrust produced scaled back to power units.
        let consumed = thrust
            .saturating_mul(self.power_per_thrust)
            .saturating_div(self.thrust_per_power);
        let remainder = power.saturating_sub(consumed);
        (thrust, remainder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tiny hull: 4:1 conversion. 1 power -> 4 thrust.
    #[test]
    fn tiny_hull_converts_four_to_one() {
        let conv = ThrustConversion::new(4, 1, 4).unwrap();
        let (thrust, rem) = conv.convert(1);
        assert_eq!(thrust, 4);
        assert_eq!(rem, 0);
    }

    /// Escort hull: 2:1 conversion. 1 power -> 2 thrust.
    #[test]
    fn escort_hull_converts_two_to_one() {
        let conv = ThrustConversion::new(2, 1, 4).unwrap();
        let (thrust, rem) = conv.convert(1);
        assert_eq!(thrust, 2);
        assert_eq!(rem, 0);
    }

    /// Cruiser hull: 1:1 conversion. 1 power -> 1 thrust.
    #[test]
    fn cruiser_hull_converts_one_to_one() {
        let conv = ThrustConversion::new(1, 1, 4).unwrap();
        let (thrust, rem) = conv.convert(1);
        assert_eq!(thrust, 1);
        assert_eq!(rem, 0);
    }

    /// Capital hull: 1:2 conversion. 1 power -> 0 thrust with remainder.
    #[test]
    fn capital_hull_converts_one_to_two_with_remainder() {
        let conv = ThrustConversion::new(1, 2, 4).unwrap();
        let (thrust, rem) = conv.convert(1);
        assert_eq!(thrust, 0);
        assert_eq!(rem, 1);
        // 2 power -> 1 thrust, no remainder.
        let (thrust, rem) = conv.convert(2);
        assert_eq!(thrust, 1);
        assert_eq!(rem, 0);
    }

    /// Huge hull: 1:4 conversion. 4 power -> 1 thrust.
    #[test]
    fn huge_hull_converts_one_to_four_with_remainder() {
        let conv = ThrustConversion::new(1, 4, 4).unwrap();
        // Power 1-3 yields zero thrust with remainder.
        let (thrust, rem) = conv.convert(1);
        assert_eq!(thrust, 0);
        assert_eq!(rem, 1);
        let (thrust, rem) = conv.convert(2);
        assert_eq!(thrust, 0);
        assert_eq!(rem, 2);
        let (thrust, rem) = conv.convert(3);
        assert_eq!(thrust, 0);
        assert_eq!(rem, 3);
        // Power 4 yields one thrust, no remainder.
        let (thrust, rem) = conv.convert(4);
        assert_eq!(thrust, 1);
        assert_eq!(rem, 0);
        // Power 5 yields one thrust, 1 remainder.
        let (thrust, rem) = conv.convert(5);
        assert_eq!(thrust, 1);
        assert_eq!(rem, 1);
    }

    /// Integer conversion truncates, never rounds.
    #[test]
    fn conversion_truncates_toward_zero() {
        let conv = ThrustConversion::new(1, 4, 4).unwrap();
        // 3 power -> 3/4 = 0 thrust, remainder 3.
        let (thrust, rem) = conv.convert(3);
        assert_eq!(thrust, 0);
        assert_eq!(rem, 3);
        // 6 power -> 6/4 = 1 thrust, remainder 2.
        let (thrust, rem) = conv.convert(6);
        assert_eq!(thrust, 1);
        assert_eq!(rem, 2);
    }

    /// Zero power yields zero thrust.
    #[test]
    fn zero_power_yields_zero_thrust() {
        let conv = ThrustConversion::new(4, 1, 4).unwrap();
        let (thrust, rem) = conv.convert(0);
        assert_eq!(thrust, 0);
        assert_eq!(rem, 0);
    }

    /// Both ratio components zero is rejected.
    #[test]
    fn zero_ratio_rejects() {
        assert!(matches!(
            ThrustConversion::new(0, 0, 4),
            Err(ConversionError::ZeroRatio)
        ));
    }

    /// A mobile hull with a zero thrust_per_power is rejected.
    #[test]
    fn mobile_hull_with_zero_thrust_per_power_rejects() {
        assert!(matches!(
            ThrustConversion::new(0, 2, 4),
            Err(ConversionError::ImmobileRatioForMobile(4))
        ));
    }

    /// An immobile hull (max_velocity 0) with nonzero thrust_per_power rejects.
    #[test]
    fn immobile_hull_with_nonzero_thrust_rejects() {
        assert!(matches!(
            ThrustConversion::new(1, 1, 0),
            Err(ConversionError::MobileRatioForImmobile(1))
        ));
    }

    /// An immobile hull with a zero thrust ratio is valid and produces nothing.
    #[test]
    fn immobile_hull_with_zero_thrust_is_valid() {
        let conv = ThrustConversion::new(0, 1, 0).unwrap();
        let (thrust, rem) = conv.convert(4);
        assert_eq!(thrust, 0);
        assert_eq!(rem, 0);
    }

    /// No floating-point arithmetic exists in the conversion.
    #[test]
    fn conversion_is_integer_only() {
        // Only well-formed ratios are constructible: power_per_thrust >= 1 and
        // at most one component exceeds one.
        for tpp in 1..=4u32 {
            for ppt in 1..=4u32 {
                // Skip compound ratios (both components exceed one).
                if tpp > 1 && ppt > 1 {
                    continue;
                }
                let conv = ThrustConversion::new(tpp, ppt, 4).unwrap();
                for power in 0..=8u32 {
                    let (thrust, rem) = conv.convert(power);
                    // Remainder is always less than the thrust cost per unit.
                    assert!(rem < ppt, "rem {rem} >= ppt {ppt} for {tpp}/{ppt} p{power}");
                    assert_eq!(thrust, power.saturating_mul(tpp) / ppt);
                }
            }
        }
    }

    /// A zero denominator (power_per_thrust == 0) is rejected even for a mobile
    /// hull that could otherwise produce "free" thrust.
    #[test]
    fn zero_denominator_rejects() {
        assert!(matches!(
            ThrustConversion::new(1, 0, 4),
            Err(ConversionError::ZeroDenominator(0))
        ));
        // The error message names the invalid denominator.
        let err = ThrustConversion::new(1, 0, 4).unwrap_err();
        assert!(err.to_string().contains("zero denominator"));
    }

    /// Compound ratios where both components exceed one are rejected.
    #[test]
    fn compound_ratios_reject() {
        assert!(matches!(
            ThrustConversion::new(2, 2, 4),
            Err(ConversionError::CompoundRatio(2, 2))
        ));
        assert!(matches!(
            ThrustConversion::new(3, 2, 4),
            Err(ConversionError::CompoundRatio(3, 2))
        ));
        assert!(matches!(
            ThrustConversion::new(2, 3, 4),
            Err(ConversionError::CompoundRatio(2, 3))
        ));
        // The error message names the unsupported ratio.
        let err = ThrustConversion::new(3, 2, 4).unwrap_err();
        assert!(err.to_string().contains("3:2"));
    }

    /// Ratios where exactly one component exceeds one are accepted.
    #[test]
    fn single_excess_ratios_accepted() {
        // N:1 forms (tiny/escort): thrust_per_power exceeds one.
        assert!(ThrustConversion::new(4, 1, 4).is_ok());
        assert!(ThrustConversion::new(2, 1, 4).is_ok());
        // 1:N forms (capital/huge): power_per_thrust exceeds one.
        assert!(ThrustConversion::new(1, 2, 4).is_ok());
        assert!(ThrustConversion::new(1, 4, 4).is_ok());
        // 1:1 is the boundary (neither exceeds one).
        assert!(ThrustConversion::new(1, 1, 4).is_ok());
    }
}
