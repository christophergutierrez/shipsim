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

    fn choose_order(&mut self, context: &DecisionContext<'_>) -> Order;
}
