pub mod ai;
pub mod board;
pub mod combat;
pub mod energy;
pub mod game_state;
pub mod hex;
pub mod impulse;
pub mod movement;
pub mod prng;
pub mod scenario;
pub mod schema;
pub mod ship;
pub mod snapshot;
pub mod ssd;
pub mod turn;

// Convenience re-export so harness/tests keep a short order-application path.
pub use movement::apply_order;
