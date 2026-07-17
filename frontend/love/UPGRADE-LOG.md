# Love2D client upgrade log

Per-phase evidence log for `frontend/love/UPGRADE-PLAN.md`. One section per
phase, with milestone checklists and machine-checked evidence.

## Phase −1 — Harness migration to interactive `--stdin` pipe (prerequisite)

**Why:** The plan's Phases 0, 1, and 4 all depend on read-only requests
(`movement_preview`, `maneuver_options`, `fire_preview`). These only work over
a long-lived `--stdin` pipe; the old harness used `--orders <file>` batch
replay (re-running the engine from scratch per submit), which has no way to
receive a mid-game request. This phase was not in the original plan but is
required by it.

**What changed:**
- New `frontend/love/subprocess.lua`: bidirectional subprocess via
  `pipe(2)`+`fork(2)`+`execvp(3)` (luajit FFI). Lua's `io.popen` is
  unidirectional and glibc `popen("r+")` does not work on Linux, so a real
  two-pipe subprocess is required — the same mechanism Rust's `Stdio::piped`
  uses in `frontend/tui/src/harness.rs`.
- Rewrote `frontend/love/harness.lua` to spawn `shipsim --scenario <p> --stdin`
  once and keep the pipes open. Public API preserved exactly:
  `harness.new`, `harness.load_scenario`, `harness.submit`, `session.snapshot`,
  `harness.parse_stream`. Added `harness.request(session, tbl)` for read-only
  requests and `harness.kill(session)` for teardown.
- `main.lua`: all three session-teardown paths (menu, escape, end-screen) and
  a new `love.quit` handler now call `harness.kill` so the child is reaped.

**Engine line protocol (empirically verified):**
- On spawn: 1 initial snapshot.
- Per order: 1 snapshot (or error envelope).
- Per read-only request: 1 typed response envelope; does NOT mutate state.

**Milestones:**
- [x] All 13 existing `tests/run_all.lua` checks pass unchanged with the new
      harness (API preserved — no test edits needed).
- [x] `harness.request` round-trips a `movement_preview` against the live
      engine: `type == "movement_preview"`, `ok == true`, `endpoints`
      non-empty.
- [x] Read-only requests do NOT pollute the order log (order count unchanged
      across a request) and do NOT mutate game state (order after request
      still applies).
- [x] No references to removed harness internals (`write_orders`, `harness.run`)
      remain in `main.lua`, `scripted_pump.lua`, or `tests/run_all.lua`.

**Evidence:**
```
$ luajit frontend/love/tests/run_all.lua
... (13 OK lines) ...
All 13 checks passed.
```
