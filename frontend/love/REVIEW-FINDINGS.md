# Love2D Frontend Code & Documentation Review

**Reviewer:** Killhouse tribunal (parallel subagents + verification)
**Date:** 2026-07-19
**Scope:** All `frontend/love/` code and documentation
**Original verdict:** **BLOCK** — 2 blocking, 9 major, 8 minor, 5 info
**Follow-up (same day):** B1/m7/m8 code fixes landed; suite **92** green.
**Cleanup (same day):** M9 SUPERSEDED banners; m1 dead preview state removed;
m4/m6 and comment hygiene closed. Residual: optional module inventory polish
already in README.

## Executive summary

The Love2D frontend was migrated to **protocol v4** (simultaneous turns,
`path_preview`/`reach_preview`/`fire_preview`, path editor) *after* the
UPGRADE-PLAN cycle completed. Four documents from that cycle
(UPGRADE-PLAN, UPGRADE-LOG, HANDOFF-REVIEW, REVIEW-VERDICT) described the
v3-era design and were actively misleading — they now carry **SUPERSEDED**
headers pointing at [`README.md`](README.md).

**B1 fixed:** `status_fmt.order_echo` had a missing `turn` branch (FIX-PLAN F2
claimed done; code never updated). Suite no longer crashes; **All 92 checks
passed.** **m8 fixed:** shield ring off-by-one. **m7 fixed:** harness
docstrings use v4 request names.

**What's working well:**
- Grep gates all pass (no rules in the client, no requests in orders.lua)
- All 18 modules load clean under plain luajit
- Engine builds clean (`cargo build -q`)
- Protocol version is correct (v4) in all code
- `love.resize` handler exists
- Subprocess lifecycle is correct (child reaped on quit)

---

## BLOCKING findings

### B1 — Test suite crashes: `status_fmt.order_echo` missing `turn` branch

**File:** `frontend/love/status_fmt.lua:7-29`
**Test:** `frontend/love/tests/run_all.lua:1544`

```
luajit: frontend/love/tests/run_all.lua:1544: turn echo
stack traceback:
    [C]: in function 'assert'
```

The test asserts `status_fmt.order_echo(2, "turn", 3):match("facing 3")`.
The function has no `turn` branch — it discards `facing` with
`local _ = facing` and falls through to the generic format
`"Ship #2 turn"`. The FIX-PLAN F2 spec (`FIX-PLAN.md:73`) says `turn 3` →
`"turned to facing 3"`, and the FIX-PLAN verdict checkbox (`:155`) marks
this as done `[x]` — but the code was never updated.

**Impact:** The test suite exits code 1 and never prints "All N checks
passed." Every doc claiming a green suite is wrong. The `turn` action is
not currently called from `main.lua` (v4 uses `commit_path`), but the
`order_echo` contract and test expect it.

**Fix:** Add a `turn` branch to `status_fmt.order_echo`:
```lua
if action == "turn" then
    return string.format("Ship #%s turned to facing %s", id, tostring(facing))
end
```
Then remove `local _ = facing` and re-run the suite to get the true check
count.

**Status:** FIXED during this review. The suite now passes with **92 checks**.

### B2 — Docs reference retired request names as live engine API

**Docs:** UPGRADE-PLAN.md, UPGRADE-LOG.md, HANDOFF-REVIEW.md, REVIEW-VERDICT.md

All four docs instructed the reader to issue `movement_preview` and
`maneuver_options` requests. The engine explicitly rejects both
(`retired_request` → use `path_preview` / `reach_preview`).

**Status:** CLOSED via M9 — SUPERSEDED banners on all four docs; live
API is documented only in README / PROTOCOL / harness comments.

---

## MAJOR findings

### M1 — Tutorial step count wrong; titles do not match TUI

**Docs:** HANDOFF-REVIEW.md (lines 88, 135, 139-140, 179, 209, 211),
REVIEW-VERDICT.md (line 28) — all claim "26 steps" and "titles match exactly"

**Code:** TUI `tutorial.rs` has **24** steps; LOVE `tutorial.lua` has
**28** steps. The diff command from HANDOFF-REVIEW.md:137-139 produces
**8 lines of diff**, not "no output." The LOVE port intentionally decomposes
compound TUI steps for the graphical path editor (e.g., one TUI "Draw the
run east" → three LOVE "Path east" steps), but the docs claim exact parity.

**Fix:** Update docs to say 28 steps (LOVE) / 24 (TUI), and describe the
intentional decomposition. Soften the `tutorial.lua:81` comment "Mirrors
REAR_ATTACK_STEPS."

### M2 — Test check count wrong everywhere

**Docs:** HANDOFF-REVIEW.md (61), REVIEW-VERDICT.md (61), FIX-PLAN.md (78),
UPGRADE-LOG.md (20), UPGRADE-PLAN.md (≥30)

**Code:** Suite crashes (B1). Counting `OK` lines before the crash gives 73,
but the suite is not green so no count is valid. Once B1 is fixed, re-measure.

### M3 — File line counts in HANDOFF-REVIEW are stale

**Doc:** HANDOFF-REVIEW.md:81-98

| File | Doc says | Actual |
|------|----------|--------|
| events.lua | 194 | 243 |
| preview.lua | 183 | 134 |
| selection.lua | 121 | 197 |
| geom.lua | 147 | 156 |
| tutorial.lua | 519 | 552 |
| harness.lua | 280 | 281 |
| main.lua | 1255 | 1915 |
| draw_board.lua | 387 | 383 |
| draw_hud.lua | 853 | 1072 |
| run_all.lua | 1302 | 1799 |

**Fix:** Refresh or drop line counts (they rot fast).

### M4 — `destroyed` guard line numbers wrong

**Doc:** HANDOFF-REVIEW.md:130-132 claims guards at lines 232, 245, 280,
322, 366, 371, 403, 1174 (8 sites)

**Code:** `grep -n 'destroyed' frontend/love/main.lua` returns lines 299,
347, 360, 465, 509, 514, 546, 918, 1790 (9 sites, none at cited lines)

### M5 — TUI source line references wrong

**Doc:** HANDOFF-REVIEW.md:151 (`app.rs:735-744` for `confirm_tutorial_order`),
:297 (`ui.rs:2009-2077` for `render_tutorial_panel`)

**Code:** `confirm_tutorial_order` is at `app.rs:763` (line 735-744 is
fire_preview request building). `render_tutorial_panel` is at `ui.rs:1986`.
Also `preview.lua:54` cites `ui.rs:1279` for `fire_preview_line`; actual is
`ui.rs:1242`.

### M6 — REVIEW-VERDICT findings reference stale lines, appear already fixed

**Doc:** REVIEW-VERDICT.md cites `geom.lua:41`, `draw_hud.lua:242/247/255/775`,
`main.lua:668` — none contain the described code. The angle-basis fix (Phase 4
FAIL) appears done (`geom.lua:41` now documents the corrected basis; test
`fan geometry follows board-facing axes` passes). The "Active #nil" fix (D8)
was done by FIX-PLAN F2. `love.resize` exists at `main.lua:997`.

**Fix:** Deprecate REVIEW-VERDICT.md (see M9).

### M7 — "8 commits ahead of origin/master" is stale

**Doc:** HANDOFF-REVIEW.md:6

**Code:** `git rev-list --count origin/master..HEAD` = **0**. The per-phase
commits exist in history but are buried under 7+ later commits.

### M8 — UPGRADE-PLAN phase model contradicts v4 turn flow

**Doc:** UPGRADE-PLAN.md:131-140 describes allocate/movement/firing/**turn_end**
with manual "End turn" and `end_turn_warning`.

**Code:** README.md:41-48 (v4-aware) states the turn flow is
allocate/path/volley with **no manual end-turn**. `main.lua` has no
`turn_end` phase; `phases.lua` defines ALLOCATE/MOVEMENT/FIRING only.

### M9 — Four docs from superseded review cycle should be deprecated

**Docs:** UPGRADE-PLAN.md, UPGRADE-LOG.md, HANDOFF-REVIEW.md,
REVIEW-VERDICT.md — all date to 2026-07-17/18, describe v3-era design.

**Status:** FIXED — SUPERSEDED banners added; README + FIX-PLAN + this
file remain the live surface. Stale line counts / ahead-of-origin claims
(M2–M7) are discharged by deprecation rather than line-by-line rewrites.

---

## MINOR findings

### m1 — `app.maneuver_options` is dead state

**File:** `main.lua` (field, reset, no-op stub)

**Status:** FIXED — field, stub, and call site removed; reach issuer
renamed `request_reach_preview`; preview/debounce comments updated.

### m2 — FIX-PLAN-created modules undocumented outside FIX-PLAN

`layout.lua`, `status_fmt.lua`, `settings.lua`, `toast.lua`, `camera.lua`,
`input_policy.lua`, `allocation.lua`, `debounce.lua` exist but appear in no
doc except FIX-PLAN (and not all there).

**Status:** FIXED — module inventory added to README.md.

### m3 — UPGRADE-LOG only documents Phase −1 and Phase 0

**Doc:** UPGRADE-LOG.md header promises "One section per phase" but contains
only Phase −1 and Phase 0. Phases 1–6 were implemented but never logged.

### m4 — README controls table omits some bindings

**Status:** FIXED — README controls match production help/dispatcher
(ship select by click; `+`/`−` allocate; `R` Commit Volley own row; PageUp/
PageDown; C/F camera). Note: Love never had Tab ship-switch or digit allocate
(those were TUI habits mis-attributed in the original finding).

### m5 — `subprocess.lua:128-130` drops trailing bytes after newline

```lua
if nl then
    chunks[#chunks + 1] = chunk:sub(1, nl - 1)
    -- Note: any bytes after the newline in this chunk are dropped.
    return table.concat(chunks)
end
```

The comment explains this is acceptable because "the shipsim engine emits
exactly one line per order/request." This is true for the current engine,
but is a latent fragility if the engine ever pipelines multiple lines in one
write. Not blocking, but worth a TODO.

### m6 — `tutorial_gate_key` confusing `c` key handling

**File:** `main.lua` tutorial key map

**Status:** FIXED — `c` always maps to `RecenterMap` (any phase); redundant
movement-branch mapping and misleading comment removed.

### m7 — `harness.lua` docstrings referenced retired request names (FIXED)

**File:** `harness.lua` (lines 6-8, 229-232, 268)

**Status:** Fixed during this review. The docstrings now correctly reference
`path_preview`/`reach_preview`/`fire_preview` with a note that
`movement_preview`/`maneuver_options` are retired.

### m8 — Shield ring off-by-one in `draw_board.lua` (FIXED)

**File:** `draw_board.lua:230`

**Status:** Fixed during this review. The shield-ring segment loop dropped
the final arc edge. Changed `#pts - 2` to `#pts - 3` so all segments draw.

---

## INFO findings

### i1 — Grep gates all pass ✅
- `grep -rn '"request"' frontend/love/orders.lua` → empty
- `grep -n 'legal\|can_fire\|affordable' frontend/love/draw_board.lua` → comment-only
- `grep -n 'movement_preview' frontend/love/draw_board.lua` → empty

### i2 — All 18 modules load clean under plain luajit ✅

### i3 — `--resume` / `replay()` references accurate ✅
`src/bin/shipsim.rs:208` (`--resume`), `src/save.rs:133` (`replay()`)

### i4 — README launch commands correct ✅
`play.sh` exists and is executable.

### i5 — No v2/v3 protocol-version strings in docs ✅
The only stale-version issue is retired request names (B2), not version strings.

---

## Recommended action order

1. ~~**Fix B1**~~ — done (`turn` branch; suite 92 green).
2. ~~**Deprecate M9**~~ — SUPERSEDED banners; discharges B2 and M2–M7 hygiene.
3. ~~**Clean up m1 / m6 / m4 / m2**~~ — dead state, `c` map, README controls +
   modules.
4. **Optional:** leave historical FIX-PLAN check-count table as archaeology;
   live count is 92 (see FIX-PLAN status note and README).
