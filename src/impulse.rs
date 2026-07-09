//! Impulse Movement Chart (IMC) — pure SFB-style movement schedule.
//!
//! Speeds 0..=31, impulses 1..=32. Schedule via even distribution:
//! `moves_on_impulse(s, i) <=> ((i-1)*s)/32 != (i*s)/32`.

/// Whether a ship at `speed` moves on `impulse` (1..=32).
/// Speeds outside 0..=31 and impulses outside 1..=32 never move.
pub fn moves_on_impulse(speed: u8, impulse: u8) -> bool {
    if speed == 0 || impulse == 0 || impulse > 32 || speed > 31 {
        return false;
    }
    let s = u32::from(speed);
    let i = u32::from(impulse);
    ((i - 1) * s) / 32 != (i * s) / 32
}

/// Number of move-impulses in a turn for `speed` (equals speed when speed <= 31).
pub fn move_count(speed: u8) -> u8 {
    if speed > 31 {
        return 31;
    }
    (1u8..=32)
        .filter(|&impulse| moves_on_impulse(speed, impulse))
        .count() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_imc_speed_zero_never_moves() {
        for impulse in 1u8..=32 {
            assert!(!moves_on_impulse(0, impulse));
        }
        assert_eq!(move_count(0), 0);
    }

    #[test]
    fn test_imc_speed_one_only_impulse_32() {
        for impulse in 1u8..=31 {
            assert!(
                !moves_on_impulse(1, impulse),
                "speed 1 must not move on impulse {impulse}"
            );
        }
        assert!(moves_on_impulse(1, 32));
        assert_eq!(move_count(1), 1);
    }

    #[test]
    fn test_imc_impulse_one_never_for_sub32() {
        for speed in 1u8..=31 {
            assert!(
                !moves_on_impulse(speed, 1),
                "speed {speed} must not move on impulse 1"
            );
            assert!(
                moves_on_impulse(speed, 32),
                "speed {speed} must move on impulse 32"
            );
        }
    }

    #[test]
    fn test_imc_move_count_equals_speed() {
        for speed in 0u8..=31 {
            assert_eq!(move_count(speed), speed, "speed {speed}");
        }
    }

    #[test]
    fn test_imc_speed_16_even_impulses() {
        for impulse in 1u8..=32 {
            let expected = impulse % 2 == 0;
            assert_eq!(
                moves_on_impulse(16, impulse),
                expected,
                "speed 16 impulse {impulse}"
            );
        }
    }

    #[test]
    fn test_imc_out_of_range_false() {
        assert!(!moves_on_impulse(1, 0));
        assert!(!moves_on_impulse(1, 33));
        assert!(!moves_on_impulse(32, 32));
        assert!(!moves_on_impulse(100, 1));
    }
}
