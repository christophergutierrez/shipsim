# M6 Slice Plan

- id: shared-fire-legality-and-fire-selection
  advances_gate: G1 (AI commits legal shots during the driven duel)
  seam: extract `GameState::v2_shot_shield_facing(attacker, weapon, target) -> Option<u8>` and
    route `weapon_has_legal_shot` through it; add `ai::v2_fire_commits`
  red_command: `cargo test --test v2_ai`
  green_condition: `weapon_has_legal_shot` delegates to the shared predicate (no 4th copy of
    fire legality); AI fire selection reuses it to pick weapon + legal shield facing
  scope_files: `src/game_state.rs`, `src/ai.rs`
  contracts_used: M6 "Reuse the existing v2 legality predicate"; M5 debt item #1 (reduce copies)
  contract_review: batch_standard
  invariants_touched: legality parity with commit-time `validate_fire_commit_v2`; adds an
    operational-weapon guard (stricter, still commit-legal)

- id: greedy-allocation-and-move
  advances_gate: G1 (duel closes range, arms weapons, terminates each turn)
  seam: `ai::v2_allocation` (movement->weapons->front shields, <= effective_power) and
    `ai::v2_move_decision` (turn toward enemy / forward / turn-in-place fallback)
  red_command: `cargo test --test v2_ai`
  green_condition: allocation is deterministic and always accepted by `allocate_v2`; every move
    decision the driver emits is legal and spends >= 1 move power (guarantees turn termination)
  scope_files: `src/ai.rs`
  contracts_used: M6 allocation/move bullets; frozen momentum + power rules
  contract_review: batch_standard
  invariants_touched: determinism (ship Vec / seek order, no unordered iteration)

- id: v2-npc-driver-and-scenarios
  advances_gate: G1 (AI-only duel reaches turn 3 / Won without panic), G2 (2v2 move_order)
  seam: `GameState::resolve_v2_npc_actions` phase-machine driver + `scenarios/v2_duel.toml`
  red_command: `cargo test --test v2_ai`
  green_condition: driver allocates AI ships, emits moves/passes for the active AI mover,
    commits+readies AI fire, and ends the turn when only AI remain; bounded by an 8192 step cap;
    stops for a human at any phase; G1/G2 pass
  scope_files: `src/game_state.rs`, `scenarios/v2_duel.toml`, `tests/v2_ai.rs`
  contracts_used: M6 driver bullet + "Turn state machine" in `implementation-plan-combat-v2.md`
  contract_review: batch_standard
  invariants_touched: legacy `resolve_npc_actions` untouched; no FASA order/test changes

## Notes / Repair

Fire legality now lives in three places, not four: the two error-producing validators
(`validate_fire_commit_v2`, `validate_fire_commit_against_v2_snapshot`) plus the shared
boolean/facing predicate `v2_shot_shield_facing`, which `weapon_has_legal_shot`,
`can_any_legal_fire`, `end_turn_warning`, and the AI (`ai::v2_fire_commits`) all funnel through.
This folds M5 debt item #1 (reduce copies) and #2 (the always-true `!is_empty()` guard is gone).

Termination: the driver's greedy move decision never passes while a ship has move power — a
turn-in-place always spends 1 and is always legal — so `move_remaining` strictly decreases each
movement phase and the intra-turn move/fire loop provably ends; weapons fire at most once per
turn (fired flag), so `can_any_legal_fire` drops to false after resolution. `EndTurn` advances
turns for an AI-only game. The 8192 step cap is a belt-and-suspenders bound, not the primary
termination argument.

Greedy allocation heuristic (deterministic): movement = min(design speed, distance to nearest
enemy, remaining power); then arm operational v2 weapons in id order (beams to full charge,
plasma/torps to 1); then pour the remainder into forward shield facings [0, 1, 5] capped at
`max_shield_per_facing`. Total is always <= `effective_power`, so `allocate_v2` never rejects it.
