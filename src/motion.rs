//! Pure motion rules for combat model v3 (protocol 3).
//!
//! Simplified inertial flight:
//! - Velocity is scalar speed (0..=MAX_VELOCITY) + course (0..=5).
//! - Facing is independent; thrust is applied only along facing.
//! - Each movement cycle: commit coast / accel / turn, then slide `speed`
//!   hexes along course (constant rate unless accel changes speed).
//! - Turn cost = hex-ring distance (1 adjacent, 2 for 120°, 3 for reverse).
//! - Accel (1 thrust) along facing: +speed if aligned with course, −speed if
//!   opposite (cancel momentum); from a stop, course becomes facing.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Global maximum velocity a ship may carry (hulls may cap lower via max_velocity).
pub const MAX_VELOCITY: u8 = 8;

/// Number of movement phases per turn.
pub const PHASES_PER_TURN: u8 = 4;

/// A ship's persistent velocity: translation speed and course direction.
///
/// When speed is zero, `course` retains its last value for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Velocity {
    pub speed: u8,
    pub course: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum VelocityError {
    #[error("speed {0} exceeds maximum velocity {1}")]
    SpeedTooHigh(u8, u8),
    #[error("course {0} is not a valid hex direction (0..=5)")]
    InvalidCourse(u8),
}

impl Velocity {
    pub fn new(speed: u8, course: u8) -> Result<Self, VelocityError> {
        if speed > MAX_VELOCITY {
            return Err(VelocityError::SpeedTooHigh(speed, MAX_VELOCITY));
        }
        if course > 5 {
            return Err(VelocityError::InvalidCourse(course));
        }
        Ok(Self { speed, course })
    }

    pub fn stopped(course: u8) -> Result<Self, VelocityError> {
        Self::new(0, course)
    }
}

/// Hex-ring distance between two facings (0..=3).
pub fn facing_turn_cost(from: u8, to: u8) -> u32 {
    let from = (from % 6) as i16;
    let to = (to % 6) as i16;
    let d = (from - to).unsigned_abs() as u32;
    d.min(6 - d)
}

/// Opposite hex direction.
pub fn opposite_dir(dir: u8) -> u8 {
    (dir + 3) % 6
}

/// A maneuver committed during a movement phase (protocol 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Maneuver {
    /// No thrust; keep velocity and facing.
    Coast,
    /// Spend 1 thrust; accelerate or cancel along current facing.
    Accel,
    /// Turn hull to absolute facing 0..=5. Cost = ring distance (1..=3).
    Turn {
        facing: u8,
    },
}

/// Outcome of resolving a maneuver against velocity and facing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManeuverResult {
    pub velocity: Velocity,
    pub facing: u8,
    pub thrust_cost: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ManeuverError {
    #[error("speed {0} already at maximum velocity {1}")]
    AlreadyMaxSpeed(u8, u8),
    #[error("already stopped; accel will start on facing")]
    // kept for clarity in tests — not raised for accel-from-stop
    AlreadyStopped(u8),
    #[error("facing {0} is not a valid hex direction (0..=5)")]
    InvalidFacing(u8),
    #[error("turn to same facing is a no-op; use coast")]
    TurnInPlaceNoOp,
    #[error(
        "accel only along current course or its reverse (facing {facing}, course {course}); turn first"
    )]
    AccelNotAligned { facing: u8, course: u8 },
    #[error("max_speed {0} exceeds global maximum velocity {1}")]
    MaxSpeedExceedsGlobal(u8, u8),
}

/// Resolve a maneuver. Does not consult thrust reserves (caller checks cost).
pub fn resolve_maneuver(
    velocity: Velocity,
    facing: u8,
    max_speed: u8,
    maneuver: Maneuver,
) -> Result<ManeuverResult, ManeuverError> {
    if max_speed > MAX_VELOCITY {
        return Err(ManeuverError::MaxSpeedExceedsGlobal(max_speed, MAX_VELOCITY));
    }
    if facing > 5 {
        return Err(ManeuverError::InvalidFacing(facing));
    }
    let max_speed = max_speed.min(MAX_VELOCITY);

    match maneuver {
        Maneuver::Coast => Ok(ManeuverResult {
            velocity,
            facing,
            thrust_cost: 0,
        }),
        Maneuver::Turn { facing: to } => {
            if to > 5 {
                return Err(ManeuverError::InvalidFacing(to));
            }
            let cost = facing_turn_cost(facing, to);
            if cost == 0 {
                return Err(ManeuverError::TurnInPlaceNoOp);
            }
            Ok(ManeuverResult {
                velocity,
                facing: to,
                thrust_cost: cost,
            })
        }
        Maneuver::Accel => {
            if velocity.speed == 0 {
                // Leave dock / restart: course becomes facing, speed 1.
                if max_speed == 0 {
                    return Err(ManeuverError::AlreadyMaxSpeed(0, 0));
                }
                let new_velocity = Velocity {
                    speed: 1,
                    course: facing,
                };
                return Ok(ManeuverResult {
                    velocity: new_velocity,
                    facing,
                    thrust_cost: 1,
                });
            }
            if facing == velocity.course {
                if velocity.speed >= max_speed {
                    return Err(ManeuverError::AlreadyMaxSpeed(velocity.speed, max_speed));
                }
                Ok(ManeuverResult {
                    velocity: Velocity {
                        speed: velocity.speed + 1,
                        course: velocity.course,
                    },
                    facing,
                    thrust_cost: 1,
                })
            } else if facing == opposite_dir(velocity.course) {
                let new_speed = velocity.speed - 1;
                let new_course = if new_speed == 0 {
                    facing // stopped facing the cancel direction (ready to go that way)
                } else {
                    velocity.course
                };
                Ok(ManeuverResult {
                    velocity: Velocity {
                        speed: new_speed,
                        course: new_course,
                    },
                    facing,
                    thrust_cost: 1,
                })
            } else {
                Err(ManeuverError::AccelNotAligned {
                    facing,
                    course: velocity.course,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_costs_are_ring_distance() {
        assert_eq!(facing_turn_cost(0, 0), 0);
        assert_eq!(facing_turn_cost(0, 1), 1);
        assert_eq!(facing_turn_cost(0, 5), 1);
        assert_eq!(facing_turn_cost(0, 2), 2);
        assert_eq!(facing_turn_cost(0, 4), 2);
        assert_eq!(facing_turn_cost(0, 3), 3);
    }

    #[test]
    fn accel_from_stop_sets_course_to_facing() {
        let v = Velocity::stopped(2).unwrap();
        let r = resolve_maneuver(v, 0, 8, Maneuver::Accel).unwrap();
        assert_eq!(r.velocity.speed, 1);
        assert_eq!(r.velocity.course, 0);
        assert_eq!(r.thrust_cost, 1);
    }

    #[test]
    fn accel_aligned_increases_speed() {
        let v = Velocity::new(2, 0).unwrap();
        let r = resolve_maneuver(v, 0, 8, Maneuver::Accel).unwrap();
        assert_eq!(r.velocity.speed, 3);
        assert_eq!(r.velocity.course, 0);
    }

    #[test]
    fn accel_opposite_cancels_then_stops_facing_reverse() {
        let v = Velocity::new(2, 0).unwrap();
        let r = resolve_maneuver(v, 3, 8, Maneuver::Accel).unwrap();
        assert_eq!(r.velocity.speed, 1);
        assert_eq!(r.velocity.course, 0);
        let r2 = resolve_maneuver(r.velocity, 3, 8, Maneuver::Accel).unwrap();
        assert_eq!(r2.velocity.speed, 0);
        assert_eq!(r2.velocity.course, 3);
    }

    #[test]
    fn accel_oblique_is_illegal() {
        let v = Velocity::new(2, 0).unwrap();
        let err = resolve_maneuver(v, 1, 8, Maneuver::Accel).unwrap_err();
        assert!(matches!(err, ManeuverError::AccelNotAligned { .. }));
    }

    #[test]
    fn turn_to_reverse_costs_three() {
        let v = Velocity::new(4, 0).unwrap();
        let r = resolve_maneuver(v, 0, 8, Maneuver::Turn { facing: 3 }).unwrap();
        assert_eq!(r.facing, 3);
        assert_eq!(r.thrust_cost, 3);
        assert_eq!(r.velocity.speed, 4); // still flying "backward"
        assert_eq!(r.velocity.course, 0);
    }

    #[test]
    fn progression_one_accel_then_coast_keeps_speed() {
        let mut v = Velocity::stopped(0).unwrap();
        let mut face = 0u8;
        // cycle 1: accel
        let r = resolve_maneuver(v, face, 8, Maneuver::Accel).unwrap();
        v = r.velocity;
        face = r.facing;
        assert_eq!(v.speed, 1);
        // cycles 2-4: coast — speed stays 1 (caller slides 1 hex each phase)
        for _ in 0..3 {
            let r = resolve_maneuver(v, face, 8, Maneuver::Coast).unwrap();
            v = r.velocity;
            assert_eq!(v.speed, 1);
        }
    }

    #[test]
    fn progression_accel_each_cycle_builds_to_four() {
        let mut v = Velocity::stopped(0).unwrap();
        let face = 0u8;
        for expect in 1..=4 {
            let r = resolve_maneuver(v, face, 8, Maneuver::Accel).unwrap();
            v = r.velocity;
            assert_eq!(v.speed, expect);
        }
    }

    #[test]
    fn max_velocity_eight() {
        let v = Velocity::new(7, 0).unwrap();
        let r = resolve_maneuver(v, 0, 8, Maneuver::Accel).unwrap();
        assert_eq!(r.velocity.speed, 8);
        let err = resolve_maneuver(r.velocity, 0, 8, Maneuver::Accel).unwrap_err();
        assert!(matches!(err, ManeuverError::AlreadyMaxSpeed(8, 8)));
    }
}
