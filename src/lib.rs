pub mod ai;
pub mod arc;
pub mod board;
pub mod campaign;
pub mod combat;
pub mod combat_tables;
pub mod game_state;
pub mod hex;
pub mod motion;
pub mod movement;
pub mod movement_preview;
pub mod prng;
pub mod protocol;
pub mod rules;
pub mod save;
pub mod scenario;
pub mod schema;
pub mod ship;
pub mod simulation;
pub mod snapshot;
pub mod ssd;
pub mod thrust;
pub mod turn;

// Convenience re-export so harness/tests keep a short order-application path.
pub use movement::apply_order;
