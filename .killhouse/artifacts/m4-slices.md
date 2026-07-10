# M4 Slice Plan

- id: fire-commit-legality
  advances_gate: CommitFire/ReadyFire protocol, legal charged weapons, range/arc/facing validation
  seam: `shipsim_core::movement::apply_order` and `StateSnapshot::from_game_state`
  red_command: `cargo test --test v2_fire`
  green_condition: v2 fire legality tests pass
  scope_files: `src/movement.rs`, `src/game_state.rs`, `src/combat.rs`, `src/snapshot.rs`, `tests/v2_fire.rs`
  contracts_used: M4 in `implementation-plan-combat-v2.md`
  contract_review: batch_standard
  invariants_touched: legacy FASA tests still green

- id: simultaneous-resolution
  advances_gate: ReadyFire auto-resolves, deterministic hit/miss, shields/hull damage, weapon consumed
  seam: `shipsim_core::movement::apply_order` and `GameState` snapshot/log
  red_command: `cargo test --test v2_fire`
  green_condition: v2 simultaneous resolution tests pass
  scope_files: `src/game_state.rs`, `src/combat.rs`, `src/snapshot.rs`, `tests/v2_fire.rs`
  contracts_used: M4 in `implementation-plan-combat-v2.md`
  contract_review: batch_standard
  invariants_touched: `cargo test`, `cargo clippy --all-targets -- -D warnings`

## Contract Review Repair

Initial contract review found two blocking gaps:

- A resolved firing phase stayed in `firing`, allowing a second commit/ready
  batch in the same turn.
- Resolve-time legality re-check missed several pre-fire snapshot conditions:
  attacker destroyed, self-fire, charged weapon, already-fired weapon, and
  beam projected damage.

Repair closes the batch by transitioning to `turn_end` after M4 resolution
(M5 will replace this with the full loop decision), adds a regression test
against second-batch commits, and expands resolve-time snapshot validation.
