//! Pure motion rules for the persistent-velocity inertial movement system
//! (ADR-0022, M1).
//!
//! This module is deliberately independent of `GameState`, orders, scenarios,
//! protocol, and UI. All math is integer-only and deterministic.

use serde::{Deserialize, Serialize};

use thiserror::Error;

/// Maximum velocity a ship may carry.
pub const MAX_VELOCITY: u8 = 4;

/// Number of movement phases per turn.
pub const PHASES_PER_TURN: u8 = 4;

/// A ship's persistent velocity: translation speed and course direction.
///
/// `speed` is in `0..=MAX_VELOCITY` hexes per turn. `course` is one of six hex
/// directions (`0..=5`). When velocity is zero, `course` retains its last value
/// for display (ADR-0022 §2).
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
    /// Construct a velocity, validating bounds.
    pub fn new(speed: u8, course: u8) -> Result<Self, VelocityError> {
        if speed > MAX_VELOCITY {
            return Err(VelocityError::SpeedTooHigh(speed, MAX_VELOCITY));
        }
        if course > 5 {
            return Err(VelocityError::InvalidCourse(course));
        }
        Ok(Self { speed, course })
    }

    /// A stationary velocity keeping the given course for display.
    pub fn stopped(course: u8) -> Result<Self, VelocityError> {
        Self::new(0, course)
    }
}

/// The explicit translation-phase schedule (ADR-0022 §1).
///
/// Returns the 1-indexed movement phases in which a ship of the given velocity
/// translates. This is the exact table, not "the first V phases".
///
/// | Velocity | Translation phases |
/// |---:|---|
/// | 0 | none |
/// | 1 | 4 |
/// | 2 | 2, 4 |
/// | 3 | 1, 2, 4 |
/// | 4 | 1, 2, 3, 4 |
pub fn translation_phases(speed: u8) -> &'static [u8] {
    match speed {
        0 => &[],
        1 => &[4],
        2 => &[2, 4],
        3 => &[1, 2, 4],
        4 => &[1, 2, 3, 4],
        _ => &[],
    }
}

/// True when a ship of the given speed translates during the 1-indexed phase.
pub fn translates_in_phase(speed: u8, phase: u8) -> bool {
    translation_phases(speed).contains(&phase)
}

/// A maneuver a ship may commit during a movement phase (ADR-0022 §4–5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Maneuver {
    /// Translate without changing velocity or facing. Consumes no thrust.
    Coast,
    /// Increase speed by one. When stopped, an optional new course may be
    /// selected freely; when moving, the course is unchanged.
    Accelerate {
        #[serde(default)]
        course: Option<u8>,
    },
    /// Decrease speed by one.
    Decelerate,
    /// Change course one hex direction to port (counterclockwise).
    TurnCoursePort,
    /// Change course one hex direction to starboard (clockwise).
    TurnCourseStarboard,
    /// Rotate facing one hex direction to port without altering course.
    RotatePort,
    /// Rotate facing one hex direction to starboard without altering course.
    RotateStarboard,
}

/// Outcome of resolving a maneuver against a velocity and facing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManeuverResult {
    pub velocity: Velocity,
    pub facing: u8,
    /// Thrust consumed by this maneuver.
    pub thrust_cost: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ManeuverError {
    #[error("speed {0} already at maximum velocity {1}")]
    AlreadyMaxSpeed(u8, u8),
    #[error("speed {0} already at zero")]
    AlreadyStopped(u8),
    #[error("course {0} is not a valid hex direction (0..=5)")]
    InvalidCourse(u8),
    #[error("course selection is only allowed when stopped")]
    CourseChangeWhileMoving,
    #[error("max_speed {0} exceeds global maximum velocity {1}")]
    MaxSpeedExceedsGlobal(u8, u8),
}

/// Wrap a hex direction index into `0..=5`.
fn wrap_direction(direction: i8) -> u8 {
    let mut d = direction % 6;
    if d < 0 {
        d += 6;
    }
    d as u8
}

/// Rotate a direction one step to port (counterclockwise): 0 -> 5 -> 4 -> ...
fn rotate_port(direction: u8) -> u8 {
    wrap_direction(direction as i8 - 1)
}

/// Rotate a direction one step to starboard (clockwise): 0 -> 1 -> 2 -> ...
fn rotate_starboard(direction: u8) -> u8 {
    wrap_direction(direction as i8 + 1)
}

/// Course-change thrust cost: equals current speed, minimum 1 (ADR-0022 §5).
pub fn course_change_cost(current_speed: u8) -> u32 {
    current_speed.max(1) as u32
}

/// Facing-rotation thrust cost: always 1 (ADR-0022 §5).
pub const ROTATION_COST: u32 = 1;

/// Acceleration/deceleration thrust cost: always 1 per step.
pub const ACCEL_COST: u32 = 1;
pub const DECEL_COST: u32 = 1;

/// Resolve a maneuver against the current velocity and facing, returning the
/// new velocity, facing, and thrust cost.
///
/// Returns an error for illegal maneuvers: accelerating past the hull maximum,
/// decelerating below zero, or selecting a course while moving. A stopped ship
/// may select any course, including the opposite of its retained course.
pub fn resolve_maneuver(
    velocity: Velocity,
    facing: u8,
    max_speed: u8,
    maneuver: Maneuver,
) -> Result<ManeuverResult, ManeuverError> {
    // Defensive: the hull maximum must never exceed the global cap. A caller
    // that trusts an unvalidated `max_speed` (e.g. a hull with max_velocity 5)
    // could otherwise construct a velocity above MAX_VELOCITY.
    if max_speed > MAX_VELOCITY {
        return Err(ManeuverError::MaxSpeedExceedsGlobal(
            max_speed,
            MAX_VELOCITY,
        ));
    }
    match maneuver {
        Maneuver::Coast => Ok(ManeuverResult {
            velocity,
            facing,
            thrust_cost: 0,
        }),
        Maneuver::Accelerate { course } => {
            if velocity.speed >= max_speed {
                return Err(ManeuverError::AlreadyMaxSpeed(velocity.speed, max_speed));
            }
            let new_course = if velocity.speed == 0 {
                match course {
                    Some(c) => {
                        if c > 5 {
                            return Err(ManeuverError::InvalidCourse(c));
                        }
                        // A stopped ship may select any course, including the
                        // opposite of its retained course. Decelerating to zero
                        // and then accelerating opposite is the intended
                        // reversal mechanism (ADR-0022 §1–2). No single-maneuver
                        // reversal is possible because a moving ship cannot
                        // select a course during acceleration.
                        c
                    }
                    None => velocity.course,
                }
            } else {
                if course.is_some() {
                    return Err(ManeuverError::CourseChangeWhileMoving);
                }
                velocity.course
            };
            let new_velocity = Velocity {
                speed: velocity.speed + 1,
                course: new_course,
            };
            Ok(ManeuverResult {
                velocity: new_velocity,
                facing,
                thrust_cost: ACCEL_COST,
            })
        }
        Maneuver::Decelerate => {
            if velocity.speed == 0 {
                return Err(ManeuverError::AlreadyStopped(velocity.speed));
            }
            let new_velocity = Velocity {
                speed: velocity.speed - 1,
                course: velocity.course,
            };
            Ok(ManeuverResult {
                velocity: new_velocity,
                facing,
                thrust_cost: DECEL_COST,
            })
        }
        Maneuver::TurnCoursePort => {
            // A course change cannot reverse direction in a single maneuver.
            // Turning port by one step is never a reversal (that is three
            // steps), so this is always legal; the cost scales with speed.
            let new_course = rotate_port(velocity.course);
            Ok(ManeuverResult {
                velocity: Velocity {
                    speed: velocity.speed,
                    course: new_course,
                },
                facing,
                thrust_cost: course_change_cost(velocity.speed),
            })
        }
        Maneuver::TurnCourseStarboard => {
            let new_course = rotate_starboard(velocity.course);
            Ok(ManeuverResult {
                velocity: Velocity {
                    speed: velocity.speed,
                    course: new_course,
                },
                facing,
                thrust_cost: course_change_cost(velocity.speed),
            })
        }
        Maneuver::RotatePort => Ok(ManeuverResult {
            velocity,
            facing: rotate_port(facing),
            thrust_cost: ROTATION_COST,
        }),
        Maneuver::RotateStarboard => Ok(ManeuverResult {
            velocity,
            facing: rotate_starboard(facing),
            thrust_cost: ROTATION_COST,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn velocity_schedule_is_exact_table() {
        assert_eq!(translation_phases(0), &[] as &[u8]);
        assert_eq!(translation_phases(1), &[4]);
        assert_eq!(translation_phases(2), &[2, 4]);
        assert_eq!(translation_phases(3), &[1, 2, 4]);
        assert_eq!(translation_phases(4), &[1, 2, 3, 4]);
    }

    #[test]
    fn translates_in_phase_uses_schedule() {
        assert!(!translates_in_phase(0, 1));
        assert!(!translates_in_phase(0, 4));
        assert!(translates_in_phase(1, 4));
        assert!(!translates_in_phase(1, 1));
        assert!(translates_in_phase(3, 1));
        assert!(!translates_in_phase(3, 3));
        assert!(translates_in_phase(4, 3));
    }

    #[test]
    fn velocity_new_validates_bounds() {
        assert!(Velocity::new(0, 0).is_ok());
        assert!(Velocity::new(4, 5).is_ok());
        assert!(Velocity::new(5, 0).is_err());
        assert!(Velocity::new(0, 6).is_err());
    }

    #[test]
    fn velocity_new_reports_maximum_in_error() {
        // SpeedTooHigh must report the invalid speed and the maximum (4), not
        // the invalid speed as both values.
        let err = Velocity::new(5, 0).unwrap_err();
        assert_eq!(err, VelocityError::SpeedTooHigh(5, 4));
        assert_eq!(err.to_string(), "speed 5 exceeds maximum velocity 4");
    }

    #[test]
    fn acceleration_increases_speed_by_one() {
        let v = Velocity::new(2, 1).unwrap();
        let r = resolve_maneuver(v, 1, 4, Maneuver::Accelerate { course: None }).unwrap();
        assert_eq!(r.velocity.speed, 3);
        assert_eq!(r.velocity.course, 1);
        assert_eq!(r.thrust_cost, 1);
    }

    #[test]
    fn acceleration_rejects_above_max_speed() {
        let v = Velocity::new(4, 0).unwrap();
        assert!(matches!(
            resolve_maneuver(v, 0, 4, Maneuver::Accelerate { course: None }),
            Err(ManeuverError::AlreadyMaxSpeed(4, 4))
        ));
    }

    #[test]
    fn deceleration_decreases_speed_by_one() {
        let v = Velocity::new(3, 2).unwrap();
        let r = resolve_maneuver(v, 2, 4, Maneuver::Decelerate).unwrap();
        assert_eq!(r.velocity.speed, 2);
        assert_eq!(r.velocity.course, 2);
        assert_eq!(r.thrust_cost, 1);
    }

    #[test]
    fn deceleration_rejects_at_zero() {
        let v = Velocity::new(0, 0).unwrap();
        assert!(matches!(
            resolve_maneuver(v, 0, 4, Maneuver::Decelerate),
            Err(ManeuverError::AlreadyStopped(0))
        ));
    }

    #[test]
    fn course_wraps_between_zero_and_five() {
        // Port from 0 wraps to 5.
        let v = Velocity::new(2, 0).unwrap();
        let r = resolve_maneuver(v, 0, 4, Maneuver::TurnCoursePort).unwrap();
        assert_eq!(r.velocity.course, 5);
        // Starboard from 5 wraps to 0.
        let v = Velocity::new(2, 5).unwrap();
        let r = resolve_maneuver(v, 0, 4, Maneuver::TurnCourseStarboard).unwrap();
        assert_eq!(r.velocity.course, 0);
    }

    #[test]
    fn course_change_cost_equals_speed_minimum_one() {
        assert_eq!(course_change_cost(0), 1);
        assert_eq!(course_change_cost(1), 1);
        assert_eq!(course_change_cost(2), 2);
        assert_eq!(course_change_cost(3), 3);
        assert_eq!(course_change_cost(4), 4);
    }

    #[test]
    fn course_change_uses_current_speed_cost() {
        let v = Velocity::new(3, 1).unwrap();
        let r = resolve_maneuver(v, 1, 4, Maneuver::TurnCourseStarboard).unwrap();
        assert_eq!(r.thrust_cost, 3);
        assert_eq!(r.velocity.course, 2);
    }

    #[test]
    fn facing_rotation_costs_one_and_preserves_course() {
        let v = Velocity::new(2, 1).unwrap();
        let r = resolve_maneuver(v, 3, 4, Maneuver::RotatePort).unwrap();
        assert_eq!(r.thrust_cost, 1);
        assert_eq!(r.facing, 2);
        assert_eq!(r.velocity.course, 1);
        assert_eq!(r.velocity.speed, 2);

        let r = resolve_maneuver(v, 3, 4, Maneuver::RotateStarboard).unwrap();
        assert_eq!(r.thrust_cost, 1);
        assert_eq!(r.facing, 4);
        assert_eq!(r.velocity.course, 1);
    }

    #[test]
    fn facing_rotation_wraps() {
        let v = Velocity::new(0, 0).unwrap();
        let r = resolve_maneuver(v, 0, 4, Maneuver::RotatePort).unwrap();
        assert_eq!(r.facing, 5);
        let r = resolve_maneuver(v, 5, 4, Maneuver::RotateStarboard).unwrap();
        assert_eq!(r.facing, 0);
    }

    #[test]
    fn stopped_acceleration_can_select_any_course() {
        let v = Velocity::new(0, 0).unwrap();
        for course in 0..=5u8 {
            // A stopped ship may select any course, including the opposite of
            // its retained course (course 3 is opposite course 0).
            let r = resolve_maneuver(
                v,
                0,
                4,
                Maneuver::Accelerate {
                    course: Some(course),
                },
            )
            .unwrap();
            assert_eq!(r.velocity.speed, 1);
            assert_eq!(r.velocity.course, course);
        }
    }

    #[test]
    fn stopped_acceleration_without_course_keeps_current() {
        let v = Velocity::new(0, 2).unwrap();
        let r = resolve_maneuver(v, 2, 4, Maneuver::Accelerate { course: None }).unwrap();
        assert_eq!(r.velocity.speed, 1);
        assert_eq!(r.velocity.course, 2);
    }

    #[test]
    fn moving_acceleration_rejects_course_selection() {
        let v = Velocity::new(2, 1).unwrap();
        assert!(matches!(
            resolve_maneuver(v, 1, 4, Maneuver::Accelerate { course: Some(2) }),
            Err(ManeuverError::CourseChangeWhileMoving)
        ));
    }

    #[test]
    fn stopped_acceleration_can_select_opposite_retained_course() {
        // A stopped ship retaining course 0 may accelerate onto course 3 (the
        // exact opposite). Decelerating to zero and accelerating opposite is
        // the intended reversal mechanism (ADR-0022 §1–2).
        let v = Velocity::new(0, 0).unwrap();
        let r = resolve_maneuver(v, 0, 4, Maneuver::Accelerate { course: Some(3) }).unwrap();
        assert_eq!(r.velocity.speed, 1);
        assert_eq!(r.velocity.course, 3);
        assert_eq!(r.thrust_cost, ACCEL_COST);
    }

    #[test]
    fn moving_ship_cannot_reverse_in_single_maneuver() {
        // A moving ship cannot select a course during acceleration, so a
        // single-maneuver reversal is impossible. A single course turn is only
        // one step (60 degrees), never a 180-degree reversal.
        let v = Velocity::new(4, 0).unwrap();
        let r = resolve_maneuver(v, 0, 4, Maneuver::TurnCourseStarboard).unwrap();
        assert_eq!(r.velocity.course, 1);
        assert_ne!(r.velocity.course, 3);
    }

    #[test]
    fn coast_consumes_no_thrust_and_preserves_state() {
        let v = Velocity::new(3, 2).unwrap();
        let r = resolve_maneuver(v, 4, 4, Maneuver::Coast).unwrap();
        assert_eq!(r.thrust_cost, 0);
        assert_eq!(r.velocity, v);
        assert_eq!(r.facing, 4);
    }

    #[test]
    fn resolve_maneuver_rejects_max_speed_above_global_cap() {
        // Defensive: a caller passing an unvalidated max_speed (e.g. a hull with
        // max_velocity 5) must not be able to construct a velocity above
        // MAX_VELOCITY. Even a Coast maneuver is rejected so the API cannot be
        // used to launder an over-cap speed.
        let v = Velocity::new(0, 0).unwrap();
        let err = resolve_maneuver(v, 0, 5, Maneuver::Coast).unwrap_err();
        assert_eq!(err, ManeuverError::MaxSpeedExceedsGlobal(5, MAX_VELOCITY));
        // Accelerating toward the over-cap max must also reject before moving.
        assert!(matches!(
            resolve_maneuver(v, 0, 5, Maneuver::Accelerate { course: None }),
            Err(ManeuverError::MaxSpeedExceedsGlobal(5, 4))
        ));
    }
}
