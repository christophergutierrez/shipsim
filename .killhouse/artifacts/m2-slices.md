# M2 Slice Plan

- id: allocate-validation-state
  advances_gate: over-allocation soft error, valid allocation stored
  seam: `shipsim_core::movement::apply_order`
  red_command: `cargo test v2_allocate`
  green_condition: allocation tests pass
  scope_files: `src/movement.rs`, `src/game_state.rs`, `src/ship.rs`, `src/snapshot.rs`, `tests/v2_allocate.rs`
  contracts_used: M2 in `implementation-plan-combat-v2.md`
  contract_review: batch_standard
  invariants_touched: legacy FASA tests still green

- id: move-order-snapshot
  advances_gate: move order sorted by movement allocation and visible in snapshot
  seam: `StateSnapshot::from_game_state`
  red_command: `cargo test v2_allocate`
  green_condition: allocation and snapshot tests pass
  scope_files: `src/game_state.rs`, `src/snapshot.rs`, `tests/v2_allocate.rs`
  contracts_used: M2 in `implementation-plan-combat-v2.md`
  contract_review: batch_standard
  invariants_touched: deterministic PRNG state, legacy harness snapshots still serialize

## Contract Review Repair

Initial contract review found three blocking gaps:

- v2 weapon kind/mount was not visible in snapshots.
- M2 tests and catalog data still exercised only legacy weapon names.
- PRNG tie handling only randomized two-ship tied groups.

Repair added v2 catalog weapons, snapshot v2 kind/mount fields, M2 assertions for
`beam_1`, and Fisher-Yates tie ordering for any tied group size.
