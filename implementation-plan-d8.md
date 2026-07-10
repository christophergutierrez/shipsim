# Implementation Plan: shipsim D8 Love Frontend

## Planning Verdict
- verdict: READY
- task_tier: full
- tier_trigger: New public client surface + harness wire-format extension (error NDJSON, snapshot field).
- execution_policy: cost_optimized
- model_routing: current-model-only
- autonomy: autopilot
- reason: PRD-d8 + ADR-0017/0018; core deltas small; Love thin client with headless gates.

## Repository State
- VCS: `/mnt/storage/git_home/shipsim`, branch `master`
- HEAD at plan time: see `git rev-parse HEAD`
- Core through D10 realized; no `frontend/` tree yet
- Harness: post-load snapshot; hard-exit on order error (to be soft-reject)
- Snapshot ships lack `controller`

## Test Commands
```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo test --test harness
luajit frontend/love/tests/run_all.lua
```

## Tracer Bullet
First green path: soft-reject harness + controller field + Love spawn via orders-file replay + text dump of post-load snapshot (no art).

## Integration Pattern (decision)
Love does **not** need a long-lived stdin pipe. Session keeps a successful-orders list; each submit writes NDJSON orders file and runs:
`shipsim --scenario <path> --orders <tmp>` then parses NDJSON (post-load + per order).
On `type=error` for the tentative last order, drop it and keep prior snapshot. Fully headless-testable.

## Milestones

### M1 — Harness soft-reject + controller on snapshot
**Scope:** `src/bin/shipsim.rs`, `src/snapshot.rs`, `src/game_state.rs` (controller label), harness tests, regenerate `tests/fixtures/d8/`.

**Gates:**
1. Illegal order line emits `{"type":"error",...}`; process exit 0 when further legal orders follow in same file.
2. Ship snapshots include `"controller":"player"|"ai"|"scripted"`.
3. `cargo test --test harness` green including updated D8 fixture.

### M2 — Love scaffold + pure Lua tests + harness adapter
**Scope:** `frontend/love/**` skeleton: conf, main, json, harness (orders replay), paths, status strip data, luajit tests.

**Gates:**
1. `luajit frontend/love/tests/run_all.lua` passes (hex math, order build, defeat derive, json roundtrip).
2. Integration: run built `shipsim` from adapter against `combat.toml` empty orders; parse post-load snapshot with 2 ships and controllers.

### M3 — Board + HUD debug geometry + phases UI state
**Scope:** hex draw, ship markers, HUD text, phase state machine, multi-ship selection pure logic.

**Gates:**
1. Pure tests: phase transitions; player-only selection; flat-top pixel helpers.
2. Manual optional: love opens without error (not required for gate).

### M4 — Input + play loop to end
**Scope:** mouse/keyboard input, energy/plot/fire/resolve sending orders, win/defeat screens, scenario picker listing `scenarios/*.toml`.

**Gates:**
1. Pure tests: scenario list helper; end-condition Won vs derived defeat.
2. Headless script: replay `d8_frontend_orders.jsonl` via adapter reaches turn>=2 with combat_log; illegal order soft path tested via adapter unit test invoking shipsim.

### M5 — Polish gates + docs sync
**Scope:** README in `frontend/love/`, ROADMAP/CONTEXT/D8 brief sync, full `cargo test` + luajit.

**Gates:**
1. All test commands green.
2. README documents launch: `cargo build && love frontend/love` from repo root (or documented cwd).

## File Contracts (summary)

| Path | Responsibility |
|------|----------------|
| `src/bin/shipsim.rs` | Soft error lines; keep alive on OrderError/parse error for order lines |
| `src/snapshot.rs` | `controller` field on ShipSnapshot |
| `src/game_state.rs` | `controller_label(ship_id)` |
| `frontend/love/harness.lua` | Binary find, orders replay, parse stream |
| `frontend/love/hex.lua` | Flat-top layout pure |
| `frontend/love/phases.lua` | Phase enum + transitions pure |
| `frontend/love/orders.lua` | Build order tables |
| `frontend/love/end_condition.lua` | Won / defeat pure |
| `frontend/love/main.lua` | Screens + love callbacks |
| `frontend/love/draw_*.lua` | Debug draw |
| `frontend/love/input.lua` | Mouse/keyboard |
| `frontend/love/tests/*` | luajit |

## Out of Scope
Per PRD: FFI, Godot, core Lost, art, campaign UI, impulse animation, full EA form.

## Blast Radius
- Snapshot JSON gains field (additive; clients ignore unknown fields usually; golden fixtures update).
- Harness no longer exits on illegal order (behavior change; tests update).

## Budget
max_milestones_unattended: 8 (using 5)
