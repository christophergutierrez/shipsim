# M7 Verdict: Delete FASA/Legacy Orders + Rewrite Tests

Verdict: PASS

## Scope reviewed

Against `implementation-plan-combat-v2.md` M7 (lines 435-444) and
`.killhouse/artifacts/m7-plan.md`.

## What was found on entry

M6 had over-deleted: the v2 D5a seeking-munition fields (`seeking`,
`next_seeking_id`) were removed from `GameState` along with the
genuinely-legacy `impulse`/`round`/`action_order` state, breaking the
lib build (10 errors). `snapshot.rs` still called the removed
`round()`/`active_ship()`/`action_order()`/`impulse()`/`seeking_munitions()`
accessors. The `Order` enum in `movement.rs` was already fully migrated
to v2 (Allocate/Move/PassMove/CommitFire/ReadyFire/EndTurn) — that part
of M7 was done by M6.

## What M7 did

1. **Restored v2 seeking-munition state** in `game_state.rs`: re-added
   `seeking: Vec<SeekingMunition>` and `next_seeking_id: u32` fields
   (D5a drones, still referenced by `launch_seeking`,
   `maybe_float_recenter`, `clamp_all_to_board`), initialized them in
   `new_with_options`, and added a `seeking_munitions()` accessor.

2. **Rewrote `snapshot.rs` for the v2 API**: dropped the legacy `round`,
   `action_order`, and `impulse` fields; repurposed `active_ship` as the
   v2 active mover (`Option<u32>`, `None` in the Allocate phase) via
   `active_v2_mover()`.

3. **Rewrote the d8 harness for v2 orders**: `d8_frontend_orders.jsonl`
   now uses allocate → pass_move → commit_fire → ready_fire → end_turn.
   `tests/harness.rs` assertions updated (turn/phase instead of
   round/active_ship); the soft-reject test now sends an illegal
   `commit_fire` in the Allocate phase instead of legacy `fire`/`end_action`.

4. **Regenerated** `tests/fixtures/d8/snapshots.jsonl` (v2-shaped: no
   round/action_order/impulse keys).

5. **Fixed `tests/fleet_campaign.rs`**: asserts on `status()`/`turn_number()`
   instead of the removed `active_ship()`.

6. `tests/fasa.rs` was already emptied by M6.

## Gates

1. `cargo test` — 56/56 green. ✅
2. `cargo clippy --all-targets` — clean (no errors; only pre-existing
   dead-code warnings). ✅
3. No `Order::EndAction`/`Fire`/`EndRound` or three-round FASA as a
   primary path in `src/`. ✅ (grep confirms zero hits.)

## Debt carried forward

- Pre-existing dead-code warnings (`rebuild_action_order`,
  `launch_seeking`, `clear_combat_log`, `next_seeking_id` never read,
  `waypoints`/`next_waypoint` never read). These are M6-era; not M7
  scope. The seeking-munition launch path exists but is not yet wired
  into a fire order — that is D5a integration debt, not M7.
- The Love frontend (`frontend/love/`) is still fully FASA-shaped
  (legacy `orders.fire`/`end_action`/`end_round`, `phases.MOVE/FIRE/END`,
  test asserts on `snap.round`/`snap.active_ship`). That is M8 scope.
