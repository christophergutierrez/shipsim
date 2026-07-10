//! Combat Model v2 momentum costs.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Keel {
    Stopped,
    Forward,
    Reverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveMode {
    Forward,
    Reverse,
    TurnPort,
    TurnStarboard,
}

pub fn move_cost(keel: Keel, mode: MoveMode) -> (u32, Keel) {
    match mode {
        MoveMode::Forward => {
            let cost = if keel == Keel::Reverse { 2 } else { 1 };
            (cost, Keel::Forward)
        }
        MoveMode::Reverse => {
            let cost = if keel == Keel::Forward { 2 } else { 1 };
            (cost, Keel::Reverse)
        }
        MoveMode::TurnPort | MoveMode::TurnStarboard => (1, keel),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_cost_depends_on_keel() {
        assert_eq!(
            move_cost(Keel::Stopped, MoveMode::Forward),
            (1, Keel::Forward)
        );
        assert_eq!(
            move_cost(Keel::Forward, MoveMode::Forward),
            (1, Keel::Forward)
        );
        assert_eq!(
            move_cost(Keel::Reverse, MoveMode::Forward),
            (2, Keel::Forward)
        );
    }

    #[test]
    fn reverse_cost_depends_on_keel() {
        assert_eq!(
            move_cost(Keel::Stopped, MoveMode::Reverse),
            (1, Keel::Reverse)
        );
        assert_eq!(
            move_cost(Keel::Reverse, MoveMode::Reverse),
            (1, Keel::Reverse)
        );
        assert_eq!(
            move_cost(Keel::Forward, MoveMode::Reverse),
            (2, Keel::Reverse)
        );
    }

    #[test]
    fn turns_cost_one_and_preserve_keel() {
        assert_eq!(
            move_cost(Keel::Forward, MoveMode::TurnPort),
            (1, Keel::Forward)
        );
        assert_eq!(
            move_cost(Keel::Reverse, MoveMode::TurnStarboard),
            (1, Keel::Reverse)
        );
    }
}
