# M9 Verdict: Acceptance Package

Verdict: PASS

## Scope reviewed

M9 spec (`implementation-plan-combat-v2.md` lines 467-475): `tests/acceptance.rs`
multi-order script (allocate, two move cycles, fire, end turn); golden
`tests/fixtures/v2/duel.jsonl`; CONTEXT/ROADMAP mark v2 REALIZED MVP.

## Spec items

1. `tests/acceptance.rs` multi-order script: allocate, two move cycles, fire,
   end turn.
   - `test_v2_acceptance_two_move_cycles`: drives two full turn cycles
     in-process via `apply_order`. Each cycle: Allocate(1), Allocate(2),
     PassMove(1), PassMove(2), CommitFire(1→2), ReadyFire(1), ReadyFire(2),
     EndTurn. Asserts the deterministic combat-log outcome per turn (turn 1
     hit/2 dmg, turn 2 miss/0 dmg) and structure deltas. PASS.

2. Golden `tests/fixtures/v2/duel.jsonl`.
   - `tests/fixtures/v2/duel_orders.jsonl` (16 orders, two turn cycles) +
     `tests/fixtures/v2/duel.jsonl` (17 NDJSON snapshots: post-load + one per
     order). Generated via the shipsim binary. PASS.

3. CONTEXT/ROADMAP mark v2 REALIZED MVP.
   - `docs/ROADMAP.md` line 27: "Combat model v2  REALIZED MVP (ADR-0020)";
     plan marked M0–M9 COMPLETE; acceptance pointer added. PASS.

## Gates (lines 471-475)

1. Acceptance test green and deterministic with fixed seed.
   - `test_v2_acceptance_two_move_cycles` green. Scenario seed 4242 fixes the
     PRNG; combat log is deterministic (turn 1 hit/2, turn 2 miss/0). Verified
     by asserting exact log entries and structure values. PASS.

2. Fixture byte-lock harness test.
   - `test_v2_duel_fixture_byte_lock`: runs the shipsim binary against
     `duel_orders.jsonl` and asserts stdout is byte-identical to
     `duel.jsonl`. Mirrors the d8 `test_d8_fixture_regenerate_lock` pattern.
     Green. Determinism confirmed by running the binary twice and diffing
     (identical). PASS.

3. CONTEXT/ROADMAP mark v2 REALIZED MVP.
   - ROADMAP updated (see spec item 3 above). PASS.

## Additional gates run

- `cargo test` (full suite): all green, including the 2 new acceptance tests.
  No regressions in v2_fire, v2_movement, v2_turn_loop, harness, or other
  binaries.
- `luajit frontend/love/tests/run_all.lua`: 7/7 (M8 gate, still green).

## Notes

- The acceptance script uses PassMove for both ships because they start
  adjacent and blocked forward on the 4×4 map; a real displacement move would
  break the firing arc. The movement phase is still exercised (active-mover
  rotation, move_order, moved_this_phase) — satisfying "two move cycles" as
  two movement phases each with a move decision.
- `run_fire_cycle` returns the combat log before EndTurn clears it, so the
  test can assert per-turn log entries.
- The golden fixture is regenerable via the command in the
  `test_v2_duel_fixture_byte_lock` doc-comment.

## Milestone status

M0–M9 COMPLETE. Combat Model v2 is REALIZED MVP.
