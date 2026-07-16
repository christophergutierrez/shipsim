use crate::movement::Order;
use crate::snapshot::{ShipSnapshot, StateSnapshot};

pub struct DecisionContext<'a> {
    pub snapshot: &'a StateSnapshot,
    pub ship: &'a ShipSnapshot,
    pub legal_orders: &'a [Order],
}

pub trait Policy {
    fn name(&self) -> &str;

    fn allocate(&mut self, ship: &ShipSnapshot) -> Order;

    /// Context-aware allocation hook. Existing policies can keep the compact
    /// `allocate` implementation; policies that govern velocity use the full
    /// fleet snapshot to calculate engagement range.
    fn allocate_with_context(&mut self, ship: &ShipSnapshot, _snapshot: &StateSnapshot) -> Order {
        self.allocate(ship)
    }

    fn choose_order(&mut self, context: &DecisionContext<'_>) -> Order;
}
