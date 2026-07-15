//! Read-only four-cycle movement preview (ADR-0022 preview contract).
//!
//! Computes the reachable endpoint set for a single ship over the four
//! movement phases of a turn, given its current state and a complete local
//! allocation draft. This is a *pure projection*: it shares the exact same
//! rules as live movement (`motion::resolve_maneuver`) but never mutates the
//! engine, the PRNG, the combat log, or save/replay data.
//!
//! Contract (see `docs/PROTOCOL.md`):
//! - Enumerate four cycles of legal maneuvers (coast / accel / turn / turn_accel).
//! - Track full hypothetical state: position, facing, course, speed, thrust.
//! - Deduplicate full states, not just positions.
//! - Return sorted, deterministic endpoint data.
//! - Enemy ships remain at their current positions for display; their future
//!   movement is unknown and is NOT predicted.
//! - Currently occupied destinations are reported separately, not silently
//!   removed from the geometric envelope.
//! - Zero thrust still returns the coast endpoint.
//! - World coordinates are unbounded in this projection (board edges are not
//!   enforced here; the live engine owns edge/collision behavior unchanged).

use std::collections::BTreeSet;

use crate::board::MapMode;
use crate::hex::Hex;
use crate::motion::{self, Maneuver, ManeuverError, Velocity};

/// A complete hypothetical ship state used during preview enumeration.
///
/// Two states are equal iff every field matches; this is what we deduplicate
/// on (not just position), so two different maneuver sequences that arrive at
/// the same hex with different facing/course/speed are both retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HypotheticalState {
    pub pos: Hex,
    pub facing: u8,
    pub velocity: Velocity,
    pub thrust_remaining: u32,
}

impl PartialOrd for HypotheticalState {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HypotheticalState {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (
            self.pos.q,
            self.pos.r,
            self.facing,
            self.velocity.speed,
            self.velocity.course,
            self.thrust_remaining,
        )
            .cmp(&(
                other.pos.q,
                other.pos.r,
                other.facing,
                other.velocity.speed,
                other.velocity.course,
                other.thrust_remaining,
            ))
    }
}

/// One reachable endpoint after four cycles.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PreviewEndpoint {
    pub q: i32,
    pub r: i32,
    pub facing: u8,
    pub course: u8,
    pub speed: u8,
    /// Thrust left unspent at the end of four cycles.
    pub thrust_remaining: u32,
}

/// The full preview result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewResult {
    /// Sorted, deduplicated reachable endpoints (full states).
    pub endpoints: Vec<PreviewEndpoint>,
    /// The single coast endpoint (zero-thrust / always-coast trajectory).
    pub coast: PreviewEndpoint,
    /// Endpoints (as (q,r)) that coincide with a currently occupied hex.
    pub occupied: Vec<(i32, i32)>,
}

/// Inputs needed to run a preview. The caller (engine) assembles these from the
/// live ship state plus the complete allocation draft.
#[derive(Debug, Clone)]
pub struct PreviewInputs {
    pub start: Hex,
    pub facing: u8,
    pub velocity: Velocity,
    pub max_velocity: u8,
    /// Thrust bought this turn from the allocation draft (engine owns the
    /// power→thrust conversion; the TUI sends the full draft).
    pub thrust_remaining: u32,
    /// Hexes currently occupied by other ships (for the separate occupied list).
    pub occupied_hexes: Vec<Hex>,
    /// The active board policy. Preview must obey hard-map edges just as live
    /// movement does; unbounded and floating worlds permit leaving the nominal
    /// rectangle.
    pub map_mode: MapMode,
    pub board_width: u32,
    pub board_height: u32,
}

impl PreviewInputs {
    /// Number of movement cycles in a turn (ADR-0022 M4).
    pub const CYCLES: u8 = 4;
}

/// Compute the four-cycle reachable endpoint set.
///
/// This is pure: it takes inputs by value and returns a result. It does not
/// touch `&mut GameState`, the PRNG, or any log.
pub fn preview(inputs: PreviewInputs) -> Result<PreviewResult, ManeuverError> {
    // `Velocity::new` already enforces speed <= MAX_VELOCITY, and
    // `resolve_maneuver` enforces max_speed <= MAX_VELOCITY, so no extra
    // up-front speed check is needed here.

    let mut current: BTreeSet<HypotheticalState> = BTreeSet::new();
    current.insert(HypotheticalState {
        pos: inputs.start,
        facing: inputs.facing,
        velocity: inputs.velocity,
        thrust_remaining: inputs.thrust_remaining,
    });

    for _ in 0..PreviewInputs::CYCLES {
        let mut next: BTreeSet<HypotheticalState> = BTreeSet::new();
        for state in &current {
            for branch in legal_branches(*state, &inputs) {
                next.insert(branch);
            }
        }
        // If every branch was pruned (e.g. all thrust exhausted and no legal
        // maneuver), carry the current states forward so the envelope never
        // shrinks below what we already reached.
        if next.is_empty() {
            next = current;
        }
        current = next;
    }

    // The coast endpoint: always-coast for four cycles. Coast is always legal
    // (zero thrust cost) so it is always present in the reachable set, but we
    // compute it explicitly so callers can render it distinctly even when the
    // reachable set is large.
    let coast_state = coast_trajectory(&inputs)?;

    let mut endpoints: Vec<PreviewEndpoint> = current
        .iter()
        .map(|s| PreviewEndpoint {
            q: s.pos.q,
            r: s.pos.r,
            facing: s.facing,
            course: s.velocity.course,
            speed: s.velocity.speed,
            thrust_remaining: s.thrust_remaining,
        })
        .collect();
    endpoints.sort();

    let occupied_set: BTreeSet<(i32, i32)> =
        inputs.occupied_hexes.iter().map(|h| (h.q, h.r)).collect();
    let mut occupied: Vec<(i32, i32)> = endpoints
        .iter()
        .map(|e| (e.q, e.r))
        .filter(|p| occupied_set.contains(p))
        .collect();
    occupied.sort();
    occupied.dedup();

    Ok(PreviewResult {
        endpoints,
        coast: PreviewEndpoint {
            q: coast_state.pos.q,
            r: coast_state.pos.r,
            facing: coast_state.facing,
            course: coast_state.velocity.course,
            speed: coast_state.velocity.speed,
            thrust_remaining: coast_state.thrust_remaining,
        },
        occupied,
    })
}

/// Enumerate every legal single-cycle maneuver from `state` and return the
/// resulting hypothetical states. Coast and accel are always considered; turns
/// and turn_accel are generated for every legal facing.
fn legal_branches(state: HypotheticalState, inputs: &PreviewInputs) -> Vec<HypotheticalState> {
    let mut out = Vec::new();
    let maneuvers = all_maneuvers(state.facing);
    for maneuver in maneuvers {
        let Ok(result) =
            motion::resolve_maneuver(state.velocity, state.facing, inputs.max_velocity, maneuver)
        else {
            continue;
        };
        if result.thrust_cost > state.thrust_remaining {
            continue;
        }
        // Slide `speed` hexes along the new course (constant-rate translation,
        // matching live `resolve_movement_phase`). Preview reports current
        // occupied endpoints separately because other ships' future maneuvers
        // are unknown, but it does enforce hard-map edges.
        let new_pos = slide(state.pos, result.velocity, inputs);
        out.push(HypotheticalState {
            pos: new_pos,
            facing: result.facing,
            velocity: result.velocity,
            thrust_remaining: state.thrust_remaining - result.thrust_cost,
        });
    }
    out
}

/// Every maneuver to consider from a given facing: coast, accel, a turn to
/// each of the six facings, and a turn_accel to each of the six facings.
/// `motion::resolve_maneuver` decides legality/cost per facing (e.g. a turn to
/// the current facing is a no-op error and is pruned).
fn all_maneuvers(_facing: u8) -> Vec<Maneuver> {
    let mut v = vec![Maneuver::Coast, Maneuver::Accel];
    for new_facing in 0u8..=5 {
        v.push(Maneuver::Turn { facing: new_facing });
        v.push(Maneuver::TurnAccel { facing: new_facing });
    }
    v
}

/// Slide `speed` hexes along `course`. Speed 0 ⇒ no movement.
fn slide(pos: Hex, velocity: Velocity, inputs: &PreviewInputs) -> Hex {
    let Some(delta) = Hex::direction(velocity.course) else {
        return pos;
    };
    let mut p = pos;
    for _ in 0..velocity.speed {
        let next = p + delta;
        if inputs.map_mode.blocks_edges()
            && (next.q < 0
                || next.r < 0
                || next.q >= inputs.board_width as i32
                || next.r >= inputs.board_height as i32)
        {
            // Live hard-map movement leaves the ship at its last legal hex;
            // later substeps attempt the same illegal move and remain blocked.
            break;
        }
        p = next;
    }
    p
}

/// The pure coast trajectory over four cycles (zero thrust spent).
fn coast_trajectory(inputs: &PreviewInputs) -> Result<HypotheticalState, ManeuverError> {
    let mut state = HypotheticalState {
        pos: inputs.start,
        facing: inputs.facing,
        velocity: inputs.velocity,
        thrust_remaining: inputs.thrust_remaining,
    };
    for _ in 0..PreviewInputs::CYCLES {
        let result = motion::resolve_maneuver(
            state.velocity,
            state.facing,
            inputs.max_velocity,
            Maneuver::Coast,
        )?;
        state.pos = slide(state.pos, result.velocity, inputs);
        state.facing = result.facing;
        state.velocity = result.velocity;
        // Coast costs no thrust.
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stopped(facing: u8) -> Velocity {
        Velocity::new(0, facing).unwrap()
    }

    fn inputs_at(start: Hex, facing: u8, thrust: u32) -> PreviewInputs {
        PreviewInputs {
            start,
            facing,
            velocity: stopped(facing),
            max_velocity: crate::motion::MAX_VELOCITY,
            thrust_remaining: thrust,
            occupied_hexes: vec![],
            map_mode: MapMode::Unbounded,
            board_width: 0,
            board_height: 0,
        }
    }

    #[test]
    fn zero_thrust_produces_coast_endpoint() {
        // A stopped ship with zero thrust: coast endpoint == start.
        let res = preview(inputs_at(Hex::new(3, 3), 0, 0)).unwrap();
        assert_eq!(res.coast.q, 3);
        assert_eq!(res.coast.r, 3);
        assert_eq!(res.coast.speed, 0);
        // The coast endpoint must be in the reachable set.
        assert!(res
            .endpoints
            .iter()
            .any(|e| e.q == 3 && e.r == 3 && e.speed == 0));
    }

    #[test]
    fn moving_ship_coasts_along_course() {
        // Speed 2, course 0 (+q): four coast cycles slide 2 hexes each = 8 total.
        let v = Velocity::new(2, 0).unwrap();
        let inputs = PreviewInputs {
            start: Hex::new(0, 0),
            facing: 0,
            velocity: v,
            max_velocity: crate::motion::MAX_VELOCITY,
            thrust_remaining: 0,
            occupied_hexes: vec![],
            map_mode: MapMode::Unbounded,
            board_width: 0,
            board_height: 0,
        };
        let res = preview(inputs).unwrap();
        assert_eq!(res.coast.q, 8);
        assert_eq!(res.coast.r, 0);
        assert_eq!(res.coast.speed, 2);
    }

    #[test]
    fn more_thrust_never_shrinks_reachable_set() {
        // The reachable endpoint count with more thrust must be >= the count
        // with less thrust (monotonicity).
        let base = preview(inputs_at(Hex::new(5, 5), 0, 0)).unwrap();
        let more = preview(inputs_at(Hex::new(5, 5), 0, 4)).unwrap();
        assert!(
            more.endpoints.len() >= base.endpoints.len(),
            "more thrust reduced reachable set: {} < {}",
            more.endpoints.len(),
            base.endpoints.len()
        );
    }

    #[test]
    fn endpoints_are_sorted_and_deduplicated() {
        let res = preview(inputs_at(Hex::new(5, 5), 0, 6)).unwrap();
        let mut sorted = res.endpoints.clone();
        sorted.sort();
        assert_eq!(res.endpoints, sorted, "endpoints not sorted");
        let mut seen = std::collections::BTreeSet::new();
        for e in &res.endpoints {
            assert!(seen.insert(e.clone()), "duplicate endpoint: {:?}", e);
        }
    }

    #[test]
    fn occupied_endpoints_reported_separately() {
        // Place an occupied hex on a reachable cell.
        let res = preview(PreviewInputs {
            start: Hex::new(0, 0),
            facing: 0,
            velocity: stopped(0),
            max_velocity: crate::motion::MAX_VELOCITY,
            thrust_remaining: 2,
            occupied_hexes: vec![Hex::new(1, 0)],
            map_mode: MapMode::Unbounded,
            board_width: 0,
            board_height: 0,
        })
        .unwrap();
        // The occupied hex must appear in the occupied list if it is reachable.
        // (1,0) is reachable with one accel from a stopped ship.
        assert!(
            res.occupied.contains(&(1, 0)),
            "occupied hex (1,0) not reported; endpoints={:?}",
            res.endpoints.iter().map(|e| (e.q, e.r)).collect::<Vec<_>>()
        );
        // And it must NOT be removed from the geometric envelope.
        assert!(res.endpoints.iter().any(|e| e.q == 1 && e.r == 0));
    }

    #[test]
    fn turn_changes_facing() {
        // A ship with thrust can turn to a new facing.
        let res = preview(inputs_at(Hex::new(5, 5), 0, 4)).unwrap();
        assert!(
            res.endpoints.iter().any(|e| e.facing != 0),
            "no endpoint with changed facing: {:?}",
            res.endpoints
        );
    }

    #[test]
    fn turn_accel_changes_facing_and_speed() {
        // turn_accel must be reachable: an endpoint with both a changed facing
        // and speed > 0 should exist when thrust is available.
        let res = preview(inputs_at(Hex::new(5, 5), 0, 4)).unwrap();
        assert!(
            res.endpoints.iter().any(|e| e.facing != 0 && e.speed > 0),
            "no turn_accel endpoint (facing!=0 && speed>0): {:?}",
            res.endpoints
        );
    }

    #[test]
    fn accel_increases_speed() {
        let res = preview(inputs_at(Hex::new(5, 5), 0, 4)).unwrap();
        assert!(
            res.endpoints.iter().any(|e| e.speed > 0),
            "no endpoint with increased speed: {:?}",
            res.endpoints
        );
    }

    #[test]
    fn preview_is_pure_no_mutation_needed() {
        // This is a structural guarantee: preview() takes inputs by value and
        // returns a result. Re-running with the same inputs yields the same
        // result (determinism).
        let inputs = inputs_at(Hex::new(2, 2), 1, 3);
        let a = preview(inputs.clone()).unwrap();
        let b = preview(inputs).unwrap();
        assert_eq!(a, b);
    }
}
