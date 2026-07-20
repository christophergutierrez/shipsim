use crate::movement::Order;
use crate::rules::Ruleset;
use crate::snapshot::{ShipSnapshot, StateSnapshot};

pub struct DecisionContext<'a> {
    pub snapshot: &'a StateSnapshot,
    pub ship: &'a ShipSnapshot,
    /// The exact immutable ruleset enforced by this match.
    pub rules: &'a Ruleset,
    /// Optional candidate orders. Protocol v4 policies typically plan a full
    /// `CommitPath` / `CommitVolley` from the snapshot rather than picking from
    /// an exhaustively enumerated legal set.
    pub legal_orders: &'a [Order],
}

pub trait Policy {
    fn name(&self) -> &str;

    fn allocate(&mut self, ship: &ShipSnapshot) -> Order;

    /// Context-aware allocation hook. Existing policies can keep the compact
    /// `allocate` implementation; policies that govern motion spend use the full
    /// fleet snapshot to calculate engagement range.
    fn allocate_with_context(&mut self, ship: &ShipSnapshot, _snapshot: &StateSnapshot) -> Order {
        self.allocate(ship)
    }

    fn choose_order(&mut self, context: &DecisionContext<'_>) -> Order;
}
