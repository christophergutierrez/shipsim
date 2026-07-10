# M8 Verdict: Love Client v2

Verdict: PASS

## Scope reviewed

M8 spec (`implementation-plan-combat-v2.md` lines 447-463): retarget the Love
frontend from the FASA/impulse interaction model to the Combat Model v2 phase
machine. Plan: `.killhouse/artifacts/m8-plan.md`.

## Critical state note

HEAD at review time (commit 6dafb1b, "feat(m7)") did **not** compile: 123
errors. The M7 commit's `game_state.rs` referenced `crate::momentum::Keel` but
`src/lib.rs` declared no `momentum` module and `src/momentum.rs` did not exist
in HEAD. The entire v2 combat model (M0-M8: `momentum.rs`, `arc.rs`,
`combat_tables.rs`, rewritten `movement.rs`/`game_state.rs`/`snapshot.rs`,
v2 tests, PRD, plan) was uncommitted in the working tree.

The working tree builds and passes all gates. This commit captures the full v2
codebase as a single atomic commit so HEAD is buildable. The M8 verdict below
applies to the v2 frontend work within that commit.

## Spec items (lines 449-457)

1. Screen/phase: Allocate -> Move -> Fire commit -> (auto resolve) -> ...
   - `frontend/love/phases.lua`: v2 phase machine (allocate, movement, firing,
     turn_end) matching core `Phase` enum (serde snake_case). `main.lua`
     drives the UI from `snap.phase` via `sync_phase()`. PASS.

2. Show move_order and whose move decision is next.
   - `main.lua` reads `snap.active_ship` (the v2 active mover; None in
     Allocate) and `snap.move_order`. Movement panel highlights the active
     ship. PASS.

3. Shield facing picker on commit.
   - `draw_hud.lua`: 6-facing shield picker (F, FR, RR, R, RL, FL) on the
     commit-fire panel; `app.shield_facing` 0..5; `pick_shield_facing` button
     action. PASS.

4. Projected beam damage for current charge.
   - `draw_hud.lua::projected_damage()`: dice * per_die * charge. Explicitly
     duplicates the core `combat_tables` formula for UI preview; core is
     authoritative. Allowed by gate 3. PASS.

5. Hit/miss status strip; enemy status panel.
   - `draw_hud.lua`: combat-log strip reads `snap.combat_log`, renders
     `attacker -> target HIT/MISS <face> dmg <n>`. Enemy status panel
     present. PASS.

6. End turn with warning dialog if `end_turn_warning`.
   - `main.lua`: `snap.end_turn_warning` sets `app.show_end_warning`; confirm
     dialog (`end_warning_confirm` / cancel). EndTurn order submitted only on
     confirm. PASS.

7. Update `run_all.lua` and replace `docs/PLAY-FASA.md` with `docs/PLAY-V2.md`.
   - `run_all.lua`: v2 gate 1 (harness allocate+move) + gate 3 (order JSON
     shape). `docs/PLAY-V2.md` exists; `docs/PLAY-FASA.md` deleted. PASS.

## Order JSON shape parity (gate 3)

`frontend/love/orders.lua` builders vs `src/movement.rs::Order`
(`#[serde(tag="type", rename_all="snake_case")]`):

| Lua builder        | Rust variant   | Fields match |
|--------------------|----------------|--------------|
| allocate(s,mv,w,sh)| Allocate       | ship, movement, weapons (map), shields ([u32;6]) OK |
| move(s,mode)       | Move           | ship, mode (MoveMode snake_case) OK |
| pass_move(s)       | PassMove       | ship OK |
| commit_fire(s,w,t,f)| CommitFire    | ship, weapon, target, shield_facing (u8) OK |
| ready_fire(s)      | ReadyFire      | ship OK |
| end_turn()         | EndTurn        | (no fields) OK |

All six match field-for-field. PASS.

## Gates (lines 461-463)

1. `luajit frontend/love/tests/run_all.lua` green (harness integration
   allocate+move). -> 7/7 checks passed. PASS.
2. Manual smoke checklist in PLAY-V2.md (human optional). -> PLAY-V2.md
   documents launch, allocate, move, fire, end turn. PASS (doc gate).
3. Pure Lua tests: order JSON shape; no game rules beyond projection formula
   if duplicated. -> order-builder tests assert JSON shape; only
   `projected_damage` duplicates a formula, as permitted. PASS.

## Additional gates run

- `cargo test`: all test binaries green (v2_fire 6/6, v2_movement 6/6,
  v2_turn_loop 6/6, plus harness/fleet_campaign/etc). No regressions.
- `cargo clippy --all-targets`: no errors; 6 pre-existing dead-code warnings
  (rebuild_action_order, launch_seeking, clear_combat_log, next_seeking_id,
  ScriptedPlan fields) carried over from M7. Not M8 scope.

## Notes / debt

- The commit is larger than M8 alone because HEAD was broken; the full v2
  codebase is committed atomically to make HEAD buildable.
- `harness.lua` forwards v2 order tables as-is via `json.encode` (no
  structural change), as the plan required.
- Pre-existing dead-code warnings are D5a integration debt, not M8.
