# M5 Verdict: Turn Loop / EndTurn / Multi-cycle / Win

Verdict: PASS

Scope reviewed against `implementation-plan-combat-v2.md` M5 and `HANDOFF-combat-v2.md`:

- Replaced the M4 `Phase::TurnEnd` placeholder in `resolve_fire_phase_v2` with the
  frozen loop decision: `Won` parks at `TurnEnd`; otherwise if any living ship
  `can_any_move()` OR `can_any_legal_fire()` -> fresh `Movement` phase via
  `begin_v2_movement_phase()`; else `TurnEnd`.
- Loop-back reuses the frozen `move_order` (no rebuild, no PRNG consumed) and starts
  with `fire_commits` / `ready_fire` cleared, so a reopened firing phase awaits fresh
  client `ReadyFire` and never auto-resolves or self-hangs.
- `Order::EndTurn` (serde `"end_turn"`) added alongside legacy FASA orders. It always
  advances the turn via `advance_turn_counter()` + `reset_all_power()` (NOT the FASA
  `advance_round_or_turn` 3-round path). Rejected only during `Allocate`
  (`OrderError::EndTurnDuringAllocation`), checked before any mutation.
- `end_turn_warning` is a derived, advisory snapshot field
  (`can_any_move() || can_any_legal_fire()`); it never blocks `EndTurn` — the client
  owns any confirm dialog (product decision confirmed with the user).
- `can_any_legal_fire` / `weapon_has_legal_shot` mirror commit-time legality
  (charge > 0, not fired, range <= max, beam projected damage >= 1, arc, legal shield
  facings), including identical `mount: Option` handling.
- Destruction win reuses existing `refresh_status()` / `Terminal::DestroyShip`; no new
  win logic.
- New-turn reset (via EndTurn) zeroes shields powered/remaining, weapon charges,
  fired flags, movement, and sets keel `Stopped`; clears commits/ready/log.
- Legacy FASA orders and tests untouched.

## Gates (independently re-run)

- `cargo test --test v2_turn_loop` -> 6 passed (G1-G6)
- `cargo test` -> 60 passed (15 suites; +6 vs M4 baseline of 54)
- `cargo clippy --all-targets -- -D warnings` -> exit 0, clean

Gate mapping:

1. Resolve with attacker `move_remaining > 0` -> `phase == movement`, commits/ready
   cleared, `ships_moved_this_phase` empty. (G1)
2. Resolve with no move and no charged legal weapon -> `phase == turn_end`. (G2)
3. Mid-turn with usable actions -> `end_turn_warning == true`; `EndTurn` advances the
   turn (number increments, phase `allocate`). (G3)
4. No legal actions -> `end_turn_warning == false`. (G4)
5. Destroy terminal target with v2 fire -> `status == Won`. (G5)
6. New turn: allocations cleared, keel `Stopped`, shields zero, charges zero, fired
   flags clear. (G6)

## Contract review (CODE_REVIEW_TRIBUNAL)

PASS — no blocking findings. Six scrutiny points cleared: legality parity, termination
(no hang), soft-reject-on-error, determinism, reset completeness, no FASA regression /
scope creep.

Non-blocking findings recorded as debt (not fixed in M5 per one-milestone scope):

1. **(debt)** Fire legality is now encoded in three places —
   `validate_fire_commit_v2`, `validate_fire_commit_against_v2_snapshot`, and
   `weapon_has_legal_shot`. Consistent today, but a future arc/beam-damage rule change
   must touch all three or `can_any_legal_fire` / `end_turn_warning` will silently
   drift from what is actually committable. Recommend extracting a shared
   `is_shot_legal(attacker, weapon, target, &ships)` predicate (candidate for M7
   cleanup or a dedicated refactor).
2. **(cleanup)** `!legal_shield_facings(...).is_empty()` in `weapon_has_legal_shot` is
   always true; redundant guard kept for parity intent. Harmless.
3. **(info)** `reset_all_power` skips destroyed ships (stale allocation on corpses).
   Not observable; consistent with existing behavior.

## Files changed

- `src/movement.rs` — `Order::EndTurn`, dispatch arm, `OrderError::EndTurnDuringAllocation`
- `src/game_state.rs` — loop decision at resolve; `can_any_move`, `can_any_legal_fire`,
  private `weapon_has_legal_shot`, `end_turn_warning`, `end_turn_v2`
- `src/snapshot.rs` — `end_turn_warning: bool` field
- `tests/v2_turn_loop.rs` — NEW, 6 gates (G1-G6)
- `tests/v2_fire.rs` — retargeted the one M4 test that asserted the placeholder to
  assert the M5 loop-back (stronger: verifies the still-charged torp is committable in
  the reopened firing phase)
- `.killhouse/artifacts/m5-slices.md` — slice plan

## Notes

- No commit made (checkpoint policy; commit only on explicit user request).
- Worked in place in the intentionally-dirty tree; no worktree, no reset, no unrelated
  files touched.
- M6 (AI + multi-ship scenarios) is next.
