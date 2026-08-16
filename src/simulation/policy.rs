use crate::movement::Order;
use crate::rules::Ruleset;
use crate::schema::SideId;
use crate::snapshot::{ShipSnapshot, StateSnapshot};
use serde::Serialize;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PolicyMetadata {
    pub id: &'static str,
    pub label: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct PurchaseContext<'a> {
    pub snapshot: &'a StateSnapshot,
    pub side: SideId,
    pub turn: u32,
}

pub trait Policy {
    fn name(&self) -> &str;

    fn metadata(&self) -> PolicyMetadata;

    fn allocate(&mut self, ship: &ShipSnapshot) -> Order;

    /// Context-aware allocation hook. Existing policies can keep the compact
    /// `allocate` implementation; policies that govern motion spend use the full
    /// fleet snapshot to calculate engagement range.
    fn allocate_with_context(&mut self, ship: &ShipSnapshot, _snapshot: &StateSnapshot) -> Order {
        self.allocate(ship)
    }

    fn choose_order(&mut self, context: &DecisionContext<'_>) -> Order;

    /// Return a bounded list of ordinary purchase orders for this side and
    /// allocation stage. The runner invokes this exactly once per side/turn.
    fn purchase_orders(&mut self, context: &PurchaseContext<'_>) -> Vec<Order> {
        let _ = context;
        Vec::new()
    }
}
