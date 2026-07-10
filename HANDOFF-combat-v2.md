# Combat v2 Handoff

Date: 2026-07-10

## Current State

Combat Model v2 is mid-implementation under Killhouse checkpoint autonomy.

Use these as authority, in order:

1. `implementation-plan-combat-v2.md`
2. `docs/adr/0020-combat-model-v2-momentum-power.md`
3. `docs/PRD-combat-model-v2.md`
4. `.killhouse/run-state.json`
5. Latest verdict artifact in `.killhouse/artifacts/`

Killhouse state:

- `classification`: major
- `stage`: `IMPLEMENT_MILESTONE`
- `autonomy`: checkpoint
- `execution_policy`: cost_optimized
- completed milestones: M0, M1, M2, M3, M4, M5, M6
- current milestone: M7
- model routing: current-model-only

Next task: implement M7 only, then stop at checkpoint.

## Completed Milestones

### M0/M1

Pure v2 rule modules are in place:

- `src/combat_tables.rs`
- `src/momentum.rs`
- `src/arc.rs`

These are wired through `src/lib.rs`.

### M2

Implemented v2 allocation/data model:

- `Order::Allocate`
- v2 phase starts at `allocate`
- allocation validates movement, weapon charges, and shield facings
- phase advances to `movement` after all living ships allocate
- move order sorted by movement allocation, with PRNG tie handling
- snapshot exposes phase, move order, allocation state, and v2 weapon metadata
- ship catalog now has v2 power budgets and v2 weapon metadata while preserving legacy weapons

Verdict: `.killhouse/artifacts/m2-verdict.md`

### M3

Implemented v2 movement phase:

- `MoveMode::Reverse`
- `Order::PassMove`
- one move/pass decision per ship per movement phase
- zero-move ships auto-pass, including all-zero allocations
- momentum costs from `src/momentum.rs`
- off-map/occupied movement rejects without spending move
- phase advances to `firing` when all ships decide or cannot move
- snapshot exposes `ships_moved_this_phase`

Verdict: `.killhouse/artifacts/m3-verdict.md`

### M4

Implemented v2 firing phase:

- `Order::CommitFire`
- `Order::ReadyFire`
- commit legality checks charged v2 weapon, target, range, arc, legal shield facing, and beam no-damage
- all living ships ready auto-resolves the batch
- deterministic resolution order: `(ship, weapon)`
- d20 hit checks and v2 damage tables
- powered shield facing depletion and hull overflow
- misses consume charge and mark weapon fired
- multi-weapon commits in one phase
- snapshot exposes `fire_commits` and `ships_ready_fire`

Important M4 caveat (resolved in M5):

- After resolution, M4 set phase to `turn_end` to close the firing batch.
- M5 replaced that placeholder with the full loop decision: return to
  `movement` if any ship can still move or fire legally, otherwise turn end.

Verdict: `.killhouse/artifacts/m4-verdict.md`

### M5

Implemented v2 turn loop / EndTurn / multi-cycle / win:

- `Order::EndTurn` (always advances the turn; rejected only during `Allocate`)
- loop decision at resolve: return to `Movement` if any ship can move or fire
  legally, otherwise `TurnEnd`
- `end_turn_warning` advisory snapshot field (never blocks EndTurn)
- new-turn reset zeroes shields, charges, fired flags, movement; keel `Stopped`
- destruction win reuses existing `refresh_status` / `Terminal::DestroyShip`

Verdict: `.killhouse/artifacts/m5-verdict.md`

### M6

Implemented v2 AI + multi-ship scenarios:

- shared fire-legality predicate `v2_shot_shield_facing` (folds M5 debt #1/#2;
  legality copies reduced from four to three)
- greedy AI: `v2_allocation` (move toward, arm weapons, front shields, <=
  effective_power), `v2_move_decision` (turn toward enemy / forward /
  turn-in-place fallback), `v2_fire_commits` (reuses shared predicate)
- `resolve_v2_npc_actions` phase-machine driver (allocates AI ships, emits
  moves/passes, commits+readies fire, ends turn when only AI remain; 8192-step
  cap; stops for a human at any phase)
- `scenarios/v2_duel.toml` AI-only duel on a 24x20 map

Verdict: `.killhouse/artifacts/m6-verdict.md`

## Verification At Handoff

The latest completed checkpoint verified:

```bash
cargo test --test v2_ai
cargo test
cargo clippy --all-targets -- -D warnings
```

All passed (62 tests, 16 suites; clippy clean).

## Working Tree Warning

The worktree is intentionally dirty and contains many pre-existing uncommitted/untracked files from prior FASA/D8/v2 work. Do not reset or revert unrelated changes.

Relevant new v2 files include:

- `.killhouse/`
- `src/arc.rs`
- `src/combat_tables.rs`
- `src/momentum.rs`
- `tests/v2_allocate.rs`
- `tests/v2_movement.rs`
- `tests/v2_fire.rs`
- `tests/v2_turn_loop.rs`
- `tests/v2_ai.rs`
- `scenarios/v2_duel.toml`
- `HANDOFF-combat-v2.md`

Several existing files have both older dirty changes and v2 changes. Work with the current state; do not attempt broad cleanup before M7.

## Next Milestone: M7

Plan section: `implementation-plan-combat-v2.md`, milestone M7.

Scope:

- Remove FASA `EndAction`/3-round fields as product API.
- Delete/empty `tests/fasa.rs` content.
- Harness fixtures for v2.
- `rg` for old order types in `src/`.

Expected gates from plan:

1. `cargo test` full green.
2. `cargo clippy --all-targets -- -D warnings` clean.
3. No `Order::EndAction` / three-round FASA as primary path in `src/`.

Non-blocking debt carried into M7 from M5/M6 verdicts:

1. Fire legality still lives in three places
   (`validate_fire_commit_v2`, `validate_fire_commit_against_v2_snapshot`,
   `v2_shot_shield_facing`). Candidate to consolidate during the M7 cleanup.
2. `v2_move_decision` fallback is always `TurnStarboard` regardless of which
   direction is shorter when blocked. Harmless; smarter fallback is optional.

## Killhouse Procedure For Next Session

1. Read this handoff.
2. Read `.killhouse/run-state.json`.
3. Read M7 in `implementation-plan-combat-v2.md`.
4. Add/update `.killhouse/artifacts/m7-slices.md`.
5. Add red tests first.
6. Implement M7 only.
7. Run:

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

8. Run a Killhouse contract review for M7.
9. Repair blockers.
10. Write `.killhouse/artifacts/m7-verdict.md`.
11. Update `.killhouse/run-state.json` to completed M7 and current M8.
12. Stop at checkpoint.

## Notes

- Legacy FASA tests are still expected to pass until M7 purge.
- Do not start Love client work before M8.
- Do not delete old FASA orders/tests until M7.
- Do not commit unless the user explicitly asks and you can safely stage only the intended milestone changes.
