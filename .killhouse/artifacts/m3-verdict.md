# M3 Verdict: Movement Phase + Momentum

Verdict: PASS

Scope reviewed against `implementation-plan-combat-v2.md` M3:

- `Move` / `PassMove` are accepted only during `Phase::Movement`.
- The active v2 mover is selected from frozen `move_order`.
- A ship cannot move or pass twice in the same movement phase.
- Zero-move ships auto-pass; all-zero movement allocations advance directly to firing.
- Forward/reverse/turn costs use v2 momentum rules.
- Reverse after forward costs 2; turns cost 1 and preserve keel.
- Off-map and occupied movement attempts return errors before position, keel, or `move_remaining` mutation.
- Movement phase advances to firing after all ships are moved, passed, or out of movement.
- `ships_moved_this_phase` is visible in snapshots while movement remains active and cleared on firing transition.
- Legacy FASA movement/fire tests remain green.

Contract review: PASS

Verification:

- `cargo test --test v2_movement`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`

Notes:

- During review, an all-zero movement allocation edge case was found and fixed before verdict. The fix routes movement-phase entry through `begin_v2_movement_phase`, which immediately transitions to firing when every living ship has `move_remaining == 0`.
- No blocking M3 findings remain.

