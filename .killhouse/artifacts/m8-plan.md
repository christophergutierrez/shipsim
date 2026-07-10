# M8 Implementation Plan — Love Client v2

## Spec (implementation-plan-combat-v2.md lines 446-460)

- Screen/phase: Allocate → Move → Fire commit → (auto resolve) → …
- Show move_order and whose move decision is next.
- Shield facing picker on commit.
- Projected beam damage for current charge.
- Hit/miss status strip; enemy status panel.
- End turn with warning dialog if `end_turn_warning`.
- Update `frontend/love/tests/run_all.lua` and replace
  `docs/PLAY-FASA.md` with `docs/PLAY-V2.md`.

## Gates

1. `luajit frontend/love/tests/run_all.lua` green (harness integration
   allocate+move).
2. Manual smoke checklist in PLAY-V2.md (human optional).
3. Pure Lua tests: order JSON shape; no game rules beyond projection
   formula if duplicated.

## Current state on entry

The Love frontend is fully FASA-shaped and will not pass the v2
snapshot contract:

- `orders.lua`: `move`, `fire`, `end_action`, `end_round` — all legacy.
- `phases.lua`: `MOVE`/`FIRE`/`END` — legacy 3-phase window, not the
  v2 Allocate→Move→Firing→TurnEnd machine.
- `tests/run_all.lua`: asserts `snap.round == 1` and
  `snap.active_ship ~= nil` — both removed in M7. Will fail.
- `harness.lua`: order-file replay; structurally fine, just needs the
  v2 order builders.
- `main.lua` (464 lines): drives the FASA phase window; needs the v2
  phase machine.
- `draw_hud.lua` (331 lines): FASA shield-face rail; needs v2
  shield-facing picker on commit + projected damage.

## Slices

### S1 — Order builders + phases + test (gate 1 + 3)

Files: `frontend/love/orders.lua`, `frontend/love/phases.lua`,
`frontend/love/tests/run_all.lua`.

- `orders.lua`: replace with v2 builders —
  `allocate(ship, movement, weapons, shields)`,
  `move(ship, mode)`, `pass_move(ship)`,
  `commit_fire(ship, weapon, target, shield_facing)`,
  `ready_fire(ship)`, `end_turn()`.
  JSON shape must match `src/movement.rs::Order` (snake_case tags).
- `phases.lua`: `ALLOCATE`/`MOVEMENT`/`FIRING`/`TURN_END` matching the
  core `Phase` enum (serde `snake_case`).
- `tests/run_all.lua`: assert v2 order JSON shape (type tags, fields);
  harness integration: load scenario, submit `allocate` for both
  ships, submit `pass_move`, assert snapshot advances. No
  `snap.round`/`snap.active_ship` asserts.

### S2 — Harness + main phase machine

Files: `frontend/love/harness.lua`, `frontend/love/main.lua`.

- `harness.lua`: no structural change; confirm it forwards v2 order
  tables as-is.
- `main.lua`: replace the FASA 3-phase window with the v2 phase
  machine driven by `snap.phase`:
  - Allocate: per-ship power sliders (movement, weapon charge, shield
    facings); `allocate` order.
  - Movement: show `move_order` and highlight the active mover
    (`snap.active_ship`); `move`/`pass_move` orders.
  - Firing: per-charged-weapon `commit_fire` with shield-facing
    picker; `ready_fire` after commit.
  - TurnEnd: `end_turn` order; if `snap.end_turn_warning`, show a
    confirm dialog.

### S3 — HUD: shield picker, projected damage, status strips

Files: `frontend/love/draw_hud.lua`, `frontend/love/ui_status.lua`.

- Shield-facing picker on the commit-fire panel (6 facings).
- Projected beam damage for the currently-charged weapon (uses
  `combat_tables` formula; may duplicate the projection formula in
  Lua per gate 3).
- Hit/miss status strip (from `snap.combat_log`); enemy status panel.

### S4 — Docs + final gate

Files: `docs/PLAY-V2.md` (new), delete `docs/PLAY-FASA.md`.

- Manual smoke checklist: launch, allocate, move, fire, end turn.
- `luajit frontend/love/tests/run_all.lua` green.
- `cargo test` still green (no core changes).

## Verification

```
luajit frontend/love/tests/run_all.lua   # gate 1 + 3
cargo test                                # no regressions
```
