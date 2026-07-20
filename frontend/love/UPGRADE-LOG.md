# Love2D client upgrade log

> **SUPERSEDED (2026-07-19).** Historical evidence log for
> [`UPGRADE-PLAN.md`](UPGRADE-PLAN.md). Incomplete (only Phase −1 and 0 were
> written up) and pre–protocol-v4. Do **not** treat request names or check
> counts here as current.
>
> **Current:** [`README.md`](README.md) · **Review:** [`REVIEW-FINDINGS.md`](REVIEW-FINDINGS.md)

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

## Phase 0 — Protocol catch-up and test scaffolding

**Why:** The snapshot grew fields the Love client ignored (`fire_opportunity`,
`translation_results`, `end_turn_warning`, `rules_id`, `rules_fingerprint`,
per-ship `attack_accuracy_bonus`). Phase 0 surfaces them on app state, builds
the `harness.request` plumbing every later phase uses, and adds the `events.lua`
ring buffer that Phase 3's ticker/floaters feed from.

**What changed:**
- `harness.lua`: `harness.request(session, tbl)` sends a JSON line with a
  `request` field and returns the decoded response envelope. Read-only — does
  not mutate state, does not enter the order log (verified by live round-trip).
- `main.lua` `sync_phase`: surfaces `fire_opportunity`, `translation_results`,
  `end_turn_warning`, `rules_id`, `rules_fingerprint`, and per-ship
  `attack_accuracy_bonus` (as `app.attack_accuracy[id]`, absent = 0) onto `app`
  from every accepted snapshot. Feeds the event ring buffer via `events.feed`.
- `events.lua` (new): pure-Lua ring buffer (cap 50) of structured events
  `{turn, kind, text}` built by diffing consecutive snapshots' `combat_log`
  (handles per-turn clear) plus `translation_results`. Kinds: `hit_dealt`,
  `hit_taken`, `miss`, `blocked`. No Love APIs — runs under plain luajit.
- `draw_hud.lua`: `rules_label(app)` (pure formatter) + `rules_provenance(app)`
  (draws `rules: <id> <fp12>` in the top-right corner, always visible during
  play). Separate from the transient status strip — provenance is persistent
  metadata, not an event message.
- `main.lua` `love.draw`: calls `draw_hud.rules_provenance(app)` during play.
- `tests/run_all.lua`: +7 Phase 0 checks (events ring buffer caps/order,
  event classification, combat_log diff-by-count, blocked translation event,
  fire_opportunity field surfacing, rules provenance label format, live request
  round-trip gated on `LOVE_LIVE`).

**Milestones:**
- [x] `luajit frontend/love/tests/run_all.lua` — 20 checks pass, including
      `request envelope round-trip` (gated on `LOVE_LIVE`), `events ring buffer
      caps and orders`, `snapshot exposes fire_opportunity fields`,
      `rules provenance label format`.
- [x] Grep gate — `grep -n '"request"' frontend/love/orders.lua` → no output.
- [x] Manual: status strip shows `rules: default fnv1a-…` on load (label
      rendered top-right; format verified by headless check).

**Evidence:**
```
$ luajit frontend/love/tests/run_all.lua
... (20 OK lines) ...
All 20 checks passed.

$ grep -n '"request"' frontend/love/orders.lua
(no output)

$ cargo build -q && echo ok
ok
```
