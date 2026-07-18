# Love2D Frontend Upgrade — Reviewer Handoff

**To:** Sol / Fable (reviewer)
**From:** Autopilot agent
**Date:** 2026-07-17
**Branch:** `master` (8 commits ahead of `origin/master`)
**Plan:** [`frontend/love/UPGRADE-PLAN.md`](UPGRADE-PLAN.md)
**Per-phase evidence log:** [`frontend/love/UPGRADE-LOG.md`](UPGRADE-LOG.md)

---

## TL;DR

All 7 phases of the Love2D UPGRADE-PLAN are implemented and committed
(phases −1 through 6). The Love2D client now reaches TUI parity on guidance
(previews, call-to-action, event feedback, dead-focus recovery, tutorial) and
exceeds it on visualization (endpoint clouds, arc fans, shield rings, threat
bearings, velocity vectors, tracers, slide interpolation, game-over panel).

**One stretch goal was deferred:** the replay viewer (Phase 6 item 2). The
tutorial port (Phase 6 item 1) is complete. See [Deferred Work](#deferred-work).

---

## How to verify everything in 60 seconds

```bash
cd /mnt/storage/git_home/shipsim

# 1. Engine builds clean (nothing outside frontend/love/ was touched)
cargo build -q

# 2. All 61 headless checks pass
luajit frontend/love/tests/run_all.lua

# 3. All modules load clean under plain luajit (no love.* needed)
for mod in tutorial events fx slide preview geom selection debounce \
           subprocess harness draw_board draw_hud ui hex paths orders \
           phases json; do
  luajit -e "require('$mod')" && echo "  $mod OK"
done

# 4. Grep gates from the plan (all should be empty or comment-only)
grep -rn '"request"' frontend/love/orders.lua              # empty = pass
grep -n 'legal\|can_fire\|affordable' frontend/love/draw_board.lua  # comment-only = pass
grep -n 'movement_preview' frontend/love/draw_board.lua    # empty = pass
```

For manual visual review:
```bash
love frontend/love
# Pick "tutorial_rear_attack" from the scenario picker for the full tutorial.
# Pick "ai" for a standard duel — tracers, floaters, game-over panel.
```

---

## Commit history (oldest → newest)

| Commit | Phase | Title |
|--------|-------|-------|
| `408c386` | −1 | Migrate harness to interactive `--stdin` pipe |
| `933718d` | 0 | Protocol catch-up and test scaffolding |
| `2369b7f` | 1 | Engine-authoritative previews (delete local rules math) |
| `7c7d67c` | 2 | Call-to-action banner + dead-focus recovery |
| `da3c7ae` | 3 | Event feedback: ticker, floaters, translation callouts |
| `4b324e8` | 4 | Board visualization: endpoint cloud, arc fans, shield ring, threats, velocity |
| `5a35b3e` | 5 | Resolution theater: tracers, slide interpolation, game-over panel |
| `162f77d` | 6 | Tutorial: rear-attack step-gate machine, coach panel, tests |

Each commit message contains a detailed breakdown of what changed and why.

---

## Files changed (16 files, +4810 / −157 lines)

### New modules (pure Lua, no `love.*` APIs — testable headless)

| File | Lines | Phase | Purpose |
|------|-------|-------|---------|
| `subprocess.lua` | 161 | −1 | Bidirectional subprocess via FFI (`pipe`+`fork`+`execvp`) |
| `events.lua` | 194 | 0 | Ring buffer of structured combat events (diffs `combat_log`) |
| `preview.lua` | 183 | 1 | Debounced read-only request manager (`fire_preview`, `maneuver_options`, `movement_preview`) |
| `selection.lua` | 121 | 2 | Auto-advance + dead-focus recovery logic (pure, testable) |
| `geom.lua` | 147 | 4 | Pixel-only geometry: arc fans, shield ring segments, velocity arrows |
| `fx.lua` | 278 | 3+5 | Particles/tweens: floaters, damage pulses, tracers, torps, plasma |
| `slide.lua` | 126 | 5 | Position interpolation (ships lerp between snapshots over 0.3s) |
| `tutorial.lua` | 519 | 6 | 26-step tutorial gate machine (ported from `tutorial.rs`) |

### Modified files

| File | Lines | Phases | What changed |
|------|-------|--------|--------------|
| `harness.lua` | 280 | −1, 0 | Rewrote to long-lived `--stdin` pipe; added `harness.request` for read-only requests |
| `main.lua` | 1255 | all | App state, input handling, preview wiring, tutorial gates, session lifecycle |
| `draw_board.lua` | 387 | 1, 4 | Deleted local arc/bearing legality math; added endpoint cloud, arc fans, shield ring, threats, velocity vectors |
| `draw_hud.lua` | 853 | 1, 2, 3, 5, 6 | Fire preview line, call-to-action banner, event ticker, game-over panel, tutorial coach panel |
| `tests/run_all.lua` | 1302 | all | 61 checks (13 pre-existing + 48 new across phases) |
| `UPGRADE-PLAN.md` | 275 | — | The plan itself (written before phase 0) |
| `UPGRADE-LOG.md` | 103 | — | Per-phase evidence log with milestone checklists |

---

## What to review (by risk priority)

### 🔴 High priority — correctness and anti-goal compliance

1. **No rules in the client (ground rule #1).** The plan's core invariant: hit
   odds, arcs, ranges, costs, and legality all come from the engine via
   `fire_preview` / `maneuver_options` / `movement_preview`. Phase 1 deleted
   the client's homegrown `arc_ok` / `relative_bearing` legality math from
   `draw_board.lua`. Verify:
   - `grep -n 'legal\|can_fire\|affordable' frontend/love/draw_board.lua` →
     only the comment at line 178 ("display-only geometry; legality comes from
     fire_preview").
   - `draw_board.lua` does not issue requests directly
     (`grep -n 'movement_preview' frontend/love/draw_board.lua` → empty).
     Requests live in `preview.lua` / `main.lua`; the board only draws state.

2. **Read-only requests never mutate game state (ground rule #1).** The
   `harness.request` path sends a `request` envelope, not an order. Verify:
   - `grep -rn '"request"' frontend/love/orders.lua` → empty (orders never
     carry a `request` field).
   - `tests/run_all.lua` has a check: `request envelope round-trip` (skipped
     without `LOVE_LIVE=1` since it needs a live engine, but the code path is
     there).

3. **Destroyed-ship guards (Phase 2, blocking bug from TUI playtests).** Every
   order-submit path must guard against destroyed ships. Verify:
   - `grep -n 'destroyed' frontend/love/main.lua` shows guards at lines 232,
     245, 280, 322, 366, 371, 403, 1174 — covering allocate, movement, fire,
     ready-fire, end-turn, and board click selection.

4. **Tutorial port faithfulness (Phase 6).** `tutorial.lua` is a port of
   `frontend/tui/src/tutorial.rs`. Verify the 26 step titles match exactly:
   ```bash
   diff <(grep 'title: "' frontend/tui/src/tutorial.rs | sed 's/.*title: "//;s/".*//') \
        <(grep 'title = "' frontend/love/tutorial.lua | sed 's/.*title = "//;s/".*//')
   # Should produce no output (identical titles, same order).
   ```
   The `OBJECTIVE` string also matches: "Race past the escort, inspect the
   map, and destroy it from behind with all weapons."

### 🟡 Medium priority — integration wiring

5. **Tutorial gate integration in `main.lua`.** The tutorial intercepts input
   before normal dispatch. The flow for order-backed steps is:
   - Gate validates the action → sets `tutorial_order_candidate` → returns
     `false` (allow) → normal handler emits the order → `mark_tutorial_order_emitted()`
     moves candidate → pending → `submit()` confirms → `tutorial.advance()`.
   - This mirrors the TUI's `confirm_tutorial_order` (`app.rs:735-744`).
   - Verify the 5 `mark_tutorial_order_emitted()` calls are in `do_allocate`,
     `do_movement`, `do_commit_fire`, `do_ready_fire`, `do_end_turn`.
   - Verify `tutorial_gate_ui` is called at the top of `handle_ui_hit` and
     `tutorial_gate_key` at the top of `love.keypressed`.

6. **Slide interpolation (Phase 5).** `slide.lua` stores previous positions per
   ship and lerps over 0.3s. Ships that vanish between snapshots (destroyed)
   are dropped, not held at stale positions. Verify the `slide drops vanished
   ships` test.

7. **Game-over stats (Phase 5).** The game-over panel computes stats from the
   `events.lua` history (structured), not by parsing log strings. Verify the
   `game over stats match event history` test feeds synthetic events and
   asserts exact counts.

### 🟢 Low priority — style and polish

8. **Module isolation.** Every new module is luajit-clean (no `love.*` APIs in
   `events.lua`, `fx.lua` state math, `slide.lua`, `preview.lua`, `geom.lua`,
   `selection.lua`, `tutorial.lua`). Only `main.lua`, `draw_*.lua`, and
   `fx.lua`'s draw half touch Love APIs. The test suite runs entirely under
   plain `luajit` (no Love runtime needed).

9. **Test coverage.** 61 checks covering: order builders, harness parsing,
   phase transitions, hex math, events ring buffer + classification, fire
   preview legality, maneuver options, call-to-action banner, dead-focus
   recovery, auto-advance, fx tweens/particles, ticker, preview debounce, arc
   fan geometry, game-over stats, lerp, slide, tracers/torps/plasma, and the
   full 26-step tutorial walkthrough.

---

## Per-phase milestone status

| Phase | Milestone | Status | Evidence |
|-------|-----------|--------|----------|
| −1 | 13 existing checks pass with new harness | ✅ | `run_all.lua` passes |
| −1 | `harness.request` round-trips `movement_preview` | ✅ | `request envelope round-trip` check (live) |
| −1 | Requests don't pollute order log | ✅ | grep gate empty |
| 0 | `events ring buffer caps and orders` | ✅ | check passes |
| 0 | `snapshot exposes fire_opportunity fields` | ✅ | check passes |
| 0 | `rules provenance label format` | ✅ | check passes |
| 0 | Grep gate: no `"request"` in orders.lua | ✅ | empty |
| 1 | `fire_preview drives target legality` | ✅ | check passes |
| 1 | `maneuver options disable unaffordable turns` | ✅ | check passes |
| 1 | Grep gate: no legality words in draw_board.lua | ✅ | comment-only |
| 2 | `banner names pending fleetmate` / `skips ready-locked` | ✅ | `phase_call_to_action mirrors TUI` check |
| 2 | `dead selection recovers to survivor` | ✅ | check passes |
| 2 | Grep gate: `destroyed` guards in main.lua | ✅ | 8 guard sites |
| 3 | `fx tweens complete and free themselves` | ✅ | check passes |
| 3 | `ticker colors by event kind` | ✅ | check passes |
| 3 | `blocked translation becomes an event` | ✅ | check passes |
| 4 | `preview debounce coalesces bursts` | ✅ | check passes |
| 4 | `arc fan geometry spans correct hex count` | ✅ | check passes |
| 4 | Grep gate: no `movement_preview` in draw_board.lua | ✅ | empty |
| 5 | `game over stats match event history` | ✅ | check passes |
| 5 | `lerp reaches target within duration` | ✅ | check passes |
| 5 | Total check count ≥ 30 | ✅ | 61 checks |
| 6 | Tutorial ported (26 steps, same scenario/order) | ✅ | step titles match TUI exactly |
| 6 | Tutorial gate logic tested | ✅ | 10 Phase 6 checks pass |
| 6 | Full 26-step walkthrough reaches completion | ✅ | check passes |

**Manual milestones** (requiring `love frontend/love` with a display) were not
executed by the autopilot agent — these need a human to visually confirm:
- P0: status strip shows `rules: default fnv1a-…` on load.
- P1: hovering an enemy in firing phase shows hit % + damage within 150 ms.
- P2: kill A1 via console cheat — selection jumps to A2, allocate works.
- P3: every enemy hit visible without reading the log (floater + pulse + ticker).
- P3: same-course tailgate shows ⊘ marker.
- P3: `love.timer.getFPS()` ≥ 60 with 20 active floaters.
- P4: endpoint cloud updates while dragging; coast marker distinct; bare shield
  face obvious; arc fans change color on fire/death; velocity arrows present.
- P5: tracers visible, no input lag during animation, game-over panel shows
  non-zero stats, `fx_enabled=false` disables all of it.
- P6: tutorial coach panel renders with yellow border, prompt strip, narration;
  gates block wrong-field allocation clicks; steps advance on correct actions.

---

## Deferred work

### Replay viewer (Phase 6, stretch goal)

**Status:** Not started. Explicitly marked "stretch, needs explicit go-ahead"
in the plan.

**What it would be:** Load a save's order stream and scrub turn-by-turn with
Phase 5 animations. Read-only; reuses the engine's `--resume` replay semantics.

**Feasibility:** The engine supports `--resume <save.json>` (verified in
`src/bin/shipsim.rs:201` and `src/save.rs:133` `replay()`). The Love2D harness
would need a new mode that spawns with `--resume` instead of `--scenario`, and
a scrub UI (timeline slider + step forward/back). The Phase 5 animation modules
(`fx.lua`, `slide.lua`) are already reusable for this.

**Why deferred:** It was the second item in a stretch phase. The tutorial port
(the first item) was the core deliverable and is complete. The replay viewer is
a new feature, not a parity gap.

---

## Architecture notes for the reviewer

### Isolation boundary

All work is under `frontend/love/`. No files outside that directory were
modified. The engine (`src/`), TUI (`frontend/tui/`), and REPL
(`frontend/repl/`) are untouched. `cargo build -q` confirms the engine is clean.

### Harness architecture (Phase −1)

The old harness used `--orders <file>` batch replay (re-running the engine from
scratch per submit). The new harness spawns `shipsim --scenario <path> --stdin`
once and keeps a bidirectional pipe open via `subprocess.lua` (FFI:
`pipe`+`fork`+`execvp`). This was a prerequisite for read-only requests
(`movement_preview`, `maneuver_options`, `fire_preview`), which only work over
a long-lived `--stdin` pipe.

Public API preserved: `harness.new`, `harness.load_scenario`,
`harness.submit`, `session.snapshot`, `harness.parse_stream`. Added:
`harness.request(session, tbl)` for read-only requests, `harness.kill(session)`
for teardown. All three session-teardown paths (menu, escape, end-screen) and
`love.quit` call `harness.kill` so the child is reaped.

### Tutorial architecture (Phase 6)

`tutorial.lua` is a pure-Lua step-gate machine with no `love.*` APIs. It
exports: `new`, `step_count`, `current_step`, `is_complete`, `advance`,
`set_error`, `check_action` (discrete — advances on match), `validate_action`
(order-backed — validates but does NOT advance), `check_reach_value` (field/
target gate), `do_now_line` (prompt strip), `narration` (coach body),
`state_error` (unexpected game-over mid-lesson).

The integration in `main.lua` mirrors the TUI's flow:
- `tutorial_gate_ui` / `tutorial_gate_key` intercept input before normal
  dispatch (mirrors `input.rs:55 tutorial_gate`).
- Order-backed steps validate → set `tutorial_order_candidate` → return false
  (allow) → normal handler emits order → `mark_tutorial_order_emitted()` →
  `submit()` → `confirm_tutorial_order` advances.
- Discrete steps advance immediately via `check_action`.
- `sync_phase` checks `state_error` on every snapshot to surface unexpected
  game-over mid-lesson.
- `start_scenario` detects `tutorial_rear_attack.toml` by basename and
  activates the controller.

The coach panel in `draw_hud.lua` mirrors the TUI's `render_tutorial_panel`
(`ui.rs:2009-2077`): yellow-bordered box with title line, prompt strip, and
narration body.

---

## Questions for the reviewer

1. **Manual visual review:** The autopilot agent cannot run Love2D with a
   display. The manual milestones (listed above) need a human to confirm. Are
   there any visual issues that need fixing before merge?

2. **Replay viewer:** Should the stretch goal be attempted now, or deferred to
   a separate effort? The engine supports it; the work is ~1 new harness mode
   + a scrub UI.

3. **Test live-engine checks:** The `request envelope round-trip` check is
   skipped unless `LOVE_LIVE=1` is set (it needs a running engine). Should we
   wire it into CI, or leave it as a manual check?

4. **Merge strategy:** 8 commits, one per phase. Squash-merge or keep the
   per-phase history? The per-phase commits have detailed messages that serve
   as documentation.
