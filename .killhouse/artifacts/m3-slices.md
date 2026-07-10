# M3 Slice Plan

- id: movement-phase-sequencing
  advances_gate: active sequencing, one decision per ship, auto zero-move pass, transition to firing
  seam: `shipsim_core::movement::apply_order` and `StateSnapshot::from_game_state`
  red_command: `cargo test --test v2_movement`
  green_condition: v2 movement sequencing tests pass
  scope_files: `src/movement.rs`, `src/game_state.rs`, `src/snapshot.rs`, `tests/v2_movement.rs`
  contracts_used: M3 in `implementation-plan-combat-v2.md`
  contract_review: batch_standard
  invariants_touched: legacy FASA tests still green

- id: momentum-costs-and-geometry
  advances_gate: forward/reverse/turn costs, off-map and occupied rejection without spend
  seam: `shipsim_core::movement::apply_order`
  red_command: `cargo test --test v2_movement`
  green_condition: v2 momentum/geometry tests pass
  scope_files: `src/movement.rs`, `src/game_state.rs`, `tests/v2_movement.rs`
  contracts_used: M3 in `implementation-plan-combat-v2.md`
  contract_review: batch_standard
  invariants_touched: full `cargo test`
