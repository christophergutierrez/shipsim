//! Pure path kernel for protocol v4 simplified simultaneous turns (ADR-0025).
//!
//! Five path actions, each costing exactly one motion point. No velocity or
//! course — only position + facing. Not connected to `GameState`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::hex::Hex;

/// One atomic path action. Every action costs exactly one motion point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathAction {
    /// Translate one hex through current forward; facing unchanged.
    MoveF,
    /// Translate through forward-right; then rotate one face right.
    MoveFr,
    /// Translate through forward-left; then rotate one face left.
    MoveFl,
    /// In-place rotate one face right.
    TurnRight,
    /// In-place rotate one face left.
    TurnLeft,
}

impl PathAction {
    pub const ALL: [PathAction; 5] = [
        PathAction::MoveF,
        PathAction::MoveFr,
        PathAction::MoveFl,
        PathAction::TurnRight,
        PathAction::TurnLeft,
    ];

    /// True when this action translates the ship one hex.
    pub fn translates(self) -> bool {
        matches!(
            self,
            PathAction::MoveF | PathAction::MoveFr | PathAction::MoveFl
        )
    }

    /// Wire / serde name.
    pub fn as_str(self) -> &'static str {
        match self {
            PathAction::MoveF => "move_f",
            PathAction::MoveFr => "move_fr",
            PathAction::MoveFl => "move_fl",
            PathAction::TurnRight => "turn_right",
            PathAction::TurnLeft => "turn_left",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "move_f" | "f" => Some(PathAction::MoveF),
            "move_fr" | "fr" => Some(PathAction::MoveFr),
            "move_fl" | "fl" => Some(PathAction::MoveFl),
            "turn_right" | "tr" | "r" => Some(PathAction::TurnRight),
            "turn_left" | "tl" | "l" => Some(PathAction::TurnLeft),
            _ => None,
        }
    }
}

/// Position + facing at one step of a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PathState {
    pub pos: Hex,
    pub facing: u8,
}

impl PathState {
    pub fn new(pos: Hex, facing: u8) -> Result<Self, PathError> {
        if !Hex::is_valid_facing(facing) {
            return Err(PathError::InvalidFacing(facing));
        }
        Ok(Self { pos, facing })
    }

    /// Absolute hex direction for a relative offset from facing.
    /// `rel`: 0 = F, 1 = FR, 5 = FL (mod 6).
    pub fn absolute_dir(self, rel: u8) -> u8 {
        (self.facing + rel) % 6
    }
}

/// Why a path is illegal. Index is the first bad action (0-based).
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PathError {
    #[error("facing {0} is not a valid hex direction (0..=5)")]
    InvalidFacing(u8),
    #[error(
        "path costs {cost} motion points but only {budget} available (illegal at action {index})"
    )]
    OverBudget {
        cost: u32,
        budget: u32,
        index: usize,
    },
    #[error("action {index} translates off the hard map to ({q},{r})")]
    OffBoard { index: usize, q: i32, r: i32 },
    #[error("action {index} uses an invalid facing after prior steps")]
    InvalidFacingAt { index: usize, facing: u8 },
}

/// One step in a traced path (state *after* the action at this index).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathStep {
    pub action: PathAction,
    pub pos: Hex,
    pub facing: u8,
    /// True when this action translated the ship.
    pub translated: bool,
}

/// Full trace of a legal path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathTrace {
    pub start: PathState,
    pub steps: Vec<PathStep>,
    pub cost: u32,
    pub final_state: PathState,
    /// Positions after each translating action, in order (for conflict fallback).
    pub translated_positions: Vec<Hex>,
}

impl PathTrace {
    /// True when the path contains no translating actions.
    pub fn is_stationary(&self) -> bool {
        self.translated_positions.is_empty()
    }
}

/// Optional hard-map bounds. When set, any translation landing outside fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapBounds {
    pub width: u32,
    pub height: u32,
}

impl MapBounds {
    pub fn contains(self, hex: Hex) -> bool {
        hex.q >= 0 && hex.r >= 0 && (hex.q as u32) < self.width && (hex.r as u32) < self.height
    }
}

/// Apply one action to a state. Returns the new state.
pub fn apply_action(state: PathState, action: PathAction) -> Result<PathState, PathError> {
    if !Hex::is_valid_facing(state.facing) {
        return Err(PathError::InvalidFacing(state.facing));
    }
    match action {
        PathAction::MoveF => {
            let dir = state.absolute_dir(0);
            let delta = Hex::direction(dir).expect("facing validated");
            Ok(PathState {
                pos: state.pos + delta,
                facing: state.facing,
            })
        }
        PathAction::MoveFr => {
            let dir = state.absolute_dir(1);
            let delta = Hex::direction(dir).expect("facing validated");
            Ok(PathState {
                pos: state.pos + delta,
                facing: (state.facing + 1) % 6,
            })
        }
        PathAction::MoveFl => {
            let dir = state.absolute_dir(5);
            let delta = Hex::direction(dir).expect("facing validated");
            Ok(PathState {
                pos: state.pos + delta,
                facing: (state.facing + 5) % 6,
            })
        }
        PathAction::TurnRight => Ok(PathState {
            pos: state.pos,
            facing: (state.facing + 1) % 6,
        }),
        PathAction::TurnLeft => Ok(PathState {
            pos: state.pos,
            facing: (state.facing + 5) % 6,
        }),
    }
}

/// Trace and validate an ordered action list against budget and optional bounds.
pub fn trace_path(
    start: PathState,
    actions: &[PathAction],
    budget: u32,
    bounds: Option<MapBounds>,
) -> Result<PathTrace, PathError> {
    if !Hex::is_valid_facing(start.facing) {
        return Err(PathError::InvalidFacing(start.facing));
    }
    let cost = actions.len() as u32;
    if cost > budget {
        // First illegal action is the one that exceeds budget.
        return Err(PathError::OverBudget {
            cost,
            budget,
            index: budget as usize,
        });
    }

    let mut current = start;
    let mut steps = Vec::with_capacity(actions.len());
    let mut translated_positions = Vec::new();

    for (index, &action) in actions.iter().enumerate() {
        let next = apply_action(current, action).map_err(|err| match err {
            PathError::InvalidFacing(f) => PathError::InvalidFacingAt { index, facing: f },
            other => other,
        })?;
        let translated = action.translates();
        if translated {
            if let Some(b) = bounds {
                if !b.contains(next.pos) {
                    return Err(PathError::OffBoard {
                        index,
                        q: next.pos.q,
                        r: next.pos.r,
                    });
                }
            }
            translated_positions.push(next.pos);
        }
        steps.push(PathStep {
            action,
            pos: next.pos,
            facing: next.facing,
            translated,
        });
        current = next;
    }

    Ok(PathTrace {
        start,
        steps,
        cost,
        final_state: current,
        translated_positions,
    })
}

/// Cost of a path (always equal to action count).
pub fn path_cost(actions: &[PathAction]) -> u32 {
    actions.len() as u32
}

/// Usable motion pool: lesser of converted allocation and effective maneuver cap.
pub fn usable_motion(converted: u32, effective_cap: u32) -> u32 {
    converted.min(effective_cap)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin_facing(f: u8) -> PathState {
        PathState::new(Hex::ORIGIN, f).unwrap()
    }

    #[test]
    fn all_five_actions_cost_exactly_one() {
        let start = origin_facing(0);
        for action in PathAction::ALL {
            let t = trace_path(start, &[action], 1, None).unwrap();
            assert_eq!(t.cost, 1);
            assert_eq!(path_cost(&[action]), 1);
        }
    }

    #[test]
    fn six_turns_return_to_start() {
        let start = origin_facing(2);
        let rights: Vec<_> = (0..6).map(|_| PathAction::TurnRight).collect();
        let t = trace_path(start, &rights, 6, None).unwrap();
        assert_eq!(t.final_state, start);
        assert!(t.is_stationary());

        let lefts: Vec<_> = (0..6).map(|_| PathAction::TurnLeft).collect();
        let t = trace_path(start, &lefts, 6, None).unwrap();
        assert_eq!(t.final_state, start);
    }

    #[test]
    fn move_f_preserves_facing() {
        let start = origin_facing(0);
        let t = trace_path(start, &[PathAction::MoveF], 1, None).unwrap();
        assert_eq!(t.final_state.facing, 0);
        assert_eq!(t.final_state.pos, Hex::new(1, 0));
        assert_eq!(t.translated_positions, vec![Hex::new(1, 0)]);
    }

    #[test]
    fn move_fr_moves_and_rotates_right() {
        // Facing 0: FR is dir 1 → (1,-1), then facing becomes 1.
        let start = origin_facing(0);
        let t = trace_path(start, &[PathAction::MoveFr], 1, None).unwrap();
        assert_eq!(t.final_state.pos, Hex::new(1, -1));
        assert_eq!(t.final_state.facing, 1);
    }

    #[test]
    fn move_fl_moves_and_rotates_left() {
        // Facing 0: FL is dir 5 → (0,1), then facing becomes 5.
        let start = origin_facing(0);
        let t = trace_path(start, &[PathAction::MoveFl], 1, None).unwrap();
        assert_eq!(t.final_state.pos, Hex::new(0, 1));
        assert_eq!(t.final_state.facing, 5);
    }

    #[test]
    fn three_move_fr_trace() {
        let start = origin_facing(0);
        let actions = [PathAction::MoveFr, PathAction::MoveFr, PathAction::MoveFr];
        let t = trace_path(start, &actions, 3, None).unwrap();
        assert_eq!(t.cost, 3);
        // Step 1: (1,-1) face 1
        assert_eq!(t.steps[0].pos, Hex::new(1, -1));
        assert_eq!(t.steps[0].facing, 1);
        // Step 2 from face 1 FR = dir 2 → (0,-1) relative → (1,-2), face 2
        assert_eq!(t.steps[1].pos, Hex::new(1, -2));
        assert_eq!(t.steps[1].facing, 2);
        // Step 3 from face 2 FR = dir 3 → (-1,0) → (0,-2), face 3
        assert_eq!(t.steps[2].pos, Hex::new(0, -2));
        assert_eq!(t.steps[2].facing, 3);
        assert_eq!(t.final_state.facing, 3);
    }

    #[test]
    fn section_2_4_examples_differ() {
        let start = origin_facing(0);
        // move_fr, turn_right, turn_right
        let a = [
            PathAction::MoveFr,
            PathAction::TurnRight,
            PathAction::TurnRight,
        ];
        // turn_right, turn_right, move_fr
        let b = [
            PathAction::TurnRight,
            PathAction::TurnRight,
            PathAction::MoveFr,
        ];
        let ta = trace_path(start, &a, 3, None).unwrap();
        let tb = trace_path(start, &b, 3, None).unwrap();
        assert_eq!(ta.cost, 3);
        assert_eq!(tb.cost, 3);
        assert_ne!(ta.final_state, tb.final_state);
        // A: after FR at (1,-1) f1, TR→f2, TR→f3 → end (1,-1) f3
        assert_eq!(
            ta.final_state,
            PathState {
                pos: Hex::new(1, -1),
                facing: 3,
            }
        );
        // B: TR→f1, TR→f2, FR from f2: dir 3 → (-1,0), face 3 → end (-1,0) f3
        assert_eq!(
            tb.final_state,
            PathState {
                pos: Hex::new(-1, 0),
                facing: 3,
            }
        );
    }

    #[test]
    fn budget_at_limit_ok_over_rejected() {
        let start = origin_facing(0);
        let actions = [PathAction::MoveF, PathAction::MoveF];
        assert!(trace_path(start, &actions, 2, None).is_ok());
        let err = trace_path(start, &actions, 1, None).unwrap_err();
        assert_eq!(
            err,
            PathError::OverBudget {
                cost: 2,
                budget: 1,
                index: 1
            }
        );
    }

    #[test]
    fn hard_bounds_reject_first_outside() {
        let start = PathState::new(Hex::new(0, 0), 3).unwrap(); // face west
        let bounds = MapBounds {
            width: 2,
            height: 2,
        };
        // move_f goes to (-1, 0) — off board
        let err = trace_path(start, &[PathAction::MoveF], 1, Some(bounds)).unwrap_err();
        assert_eq!(
            err,
            PathError::OffBoard {
                index: 0,
                q: -1,
                r: 0
            }
        );
        // turn only stays on board
        assert!(trace_path(start, &[PathAction::TurnRight], 1, Some(bounds)).is_ok());
    }

    #[test]
    fn empty_path_is_legal_and_stationary() {
        let start = origin_facing(4);
        let t = trace_path(start, &[], 0, None).unwrap();
        assert_eq!(t.cost, 0);
        assert_eq!(t.final_state, start);
        assert!(t.is_stationary());
    }

    #[test]
    fn no_velocity_or_course_in_api() {
        // Compile-time / API smoke: PathState only has pos + facing.
        let s = origin_facing(0);
        let _ = (s.pos, s.facing);
    }
}
