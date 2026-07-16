# TUI play-feedback plan — phased milestones

**Date:** 2026-07-16  
**Source:** UI play on ratatui free-play (`scenarios/ai.toml`) + rear-attack tutorial  
**Session logs:** `frontend/tui/local/agent-play-*.md`  
**Related:** [UI-RECOMMENDATIONS.md](UI-RECOMMENDATIONS.md), [UX-IMPLEMENTATION-PLAN.md](UX-IMPLEMENTATION-PLAN.md)

## Status (as of uncommitted TUI UX work)

| Phase | Status | Notes |
|---|---|---|
| 0 Baseline | done | Floor 80×24; tutorial win + free-play harness still green |
| 1 Map truth | done | Finest-fit auto-zoom; `d=N` range title priority; multipin `+N`; off-map strip; **1.2** style assert still optional |
| 2 Weapon health | **done** | Edit lock + soft notice; **allocate/preview omit offline ids** (2.2); fire skip + `FireDraft::for_ship` snap; quiet previews |
| 3 Allocate form | done | Sticky Budget header, ▶ row marker, honest hull |
| 4 Fire queue + coach | done | Header/panel queue parity; `Cycle N/4` coach (assert exact substring); no-charge coach |
| 5 Shield / move copy | partial | Shield mini-diagram already in allocate panel; movement copy polish optional |
| 6 Tutorial density | partial | Charge steps denser than original 29-step busywork; further collapse optional |

Open follow-ups (non-blocking): map `Buffer` style assertion (1.2), deeper tutorial step collapse (6.1), movement-panel copy audit (5.3).

## Goal

Make free-play and tutorial **readable at a glance** for range, weapon health,
fire queue, and the 4-cycle turn structure — without reimplementing combat
rules in the client.

## Constraints (binding)

| Rule | Implication |
|---|---|
| Thin client | No to-hit, arc, thrust-cost, or shield-face rules in TUI |
| Protocol | Prefer fields already on the snapshot; additive protocol only when derivation would copy a rule |
| Scope | `frontend/tui/` only unless a phase explicitly opens engine/protocol |
| Verification | `TestBackend` required; one live harness/UI-play check per phase |

## Verification (every phase)

```bash
cargo test -q                                          # engine untouched / still green
cargo test --manifest-path frontend/tui/Cargo.toml
cargo clippy --manifest-path frontend/tui/Cargo.toml -- -D warnings
```

Plus the **phase-specific** checks in each milestone.  
Manual UI play when noted:

```bash
cargo run --manifest-path frontend/tui/Cargo.toml -- scenarios/ai.toml
cargo run --manifest-path frontend/tui/Cargo.toml -- --tutorial
```

---

## Issue → phase map

| ID | Severity | Issue | Phase |
|---|---|---|---|
| M1 | P0 | Map distance / scale illegible; ships look adjacent when far | 1 |
| M2 | P0 | Damaged weapons still powerable → `weapon … not found` spam | 2 |
| M3 | P1 | Allocate is a scrolled text dump | 3 |
| M4 | P1 | No off-map enemy cue | 1 (with M1) |
| M5 | P1 | Fire queue header vs panel disagree; weak “pending” UX | 4 |
| M6 | P1 | 4× move/fire under-signposted in free play | 4 |
| M7 | P2 | Shield faces abstract (no diagram) | 5 |
| M8 | P2 | Turn thrust cost invisible (engine reject only) | 5* |
| M9 | P2 | Movement preview on every weapon digit/arrow | 2 (with M2) |
| M10 | P2 | Tutorial micro-step density | 6 |
| B1 | P0† | Map ship colors discarded (`Span` → plain `String`) | 1 |
| B2 | P1† | Hull shown as fake `N/N` | 3 |

† From [UI-RECOMMENDATIONS.md](UI-RECOMMENDATIONS.md) §3 — fix while touching those panels.  
\* M8: **display-only** if cost is not on the wire; do not invent costs. Prefer soft wording or engine advisory later.

---

## Phase 0 — Baseline freeze (½ day)

### Goal

Lock reproducible play paths and acceptance scripts so later phases are measurable.

### Work

1. Record terminal floor used for play: **80×24** (existing `MIN_*`) and a wide case **100×30**.
2. Note two acceptance scenarios:
   - Free play: `scenarios/ai.toml` — complete allocate → 4×(move/fire) → end_turn at least once without soft-error spam.
   - Tutorial: `--tutorial` — still wins (existing `tutorial_rear_attack_wins_against_engine`).
3. Optional: keep play frame dumps under `frontend/tui/local/` (gitignored) as before/after evidence.

### Exit criteria

| # | Testable check |
|---|---|
| 0.1 | `cargo test --manifest-path frontend/tui/Cargo.toml` green on current tree |
| 0.2 | `tutorial_rear_attack_wins_against_engine` still passes |
| 0.3 | Written baseline: “map shows A1/B2 as adjacent at range 8” and “damaged weapon allocate produces ENGINE: weapon … not found” still true if re-checked (document, don’t “fix” yet) |

**No product code required** unless baseline tests fail.

---

## Phase 1 — Map truth (P0 + off-map)

### Goal

Players can judge **relative position and range** from the map (or adjacent chrome) without computing q/r mentally.

### Work

1. **Fix map cell styling** (B1): build rows as `Vec<Span>` / `Line::from(spans)` so player/enemy/focus colors survive.
2. **Scale / labeling:**
   - Prefer auto-zoom that keeps **all living ships** (or focused + nearest enemy) with **true 1 cell = 1 hex** when they fit; only step to coarser scale when necessary.
   - When scale > 1 hex/cell, show scale in the map title (`z=-1 · 2 hex/cell`) and avoid packing distinct ships into one glyph without a multipin cue.
3. **Range readout (presentation):** axial distance between focused ship and selected/first enemy — pure geometry from snapshot `q,r` (not a combat rule). Show in map title or ship status: `→ B2 d=6`.
4. **Off-map contacts (M4):** if a living non-focused ship falls outside the viewport, show a one-line strip: `B2 off-map · west · d=14` (bearing as hex-neighbor direction from focus, not arc legality).

### Out of scope

Range rings, weapon arcs, “behind” stern geometry (needs arc rules or protocol later).

### Exit criteria

| # | Testable check |
|---|---|
| 1.1 | `TestBackend` fixture: two ships at axial distance 8 → buffer shows **either** ≥8 map steps between callsigns **or** explicit `d=8` in chrome (assert string present) |
| 1.2 | Color test: player/enemy callsign cells use distinct styles (inspect `Buffer` cell styles, not only text) |
| 1.3 | Off-map: snapshot with enemy outside viewport rect → buffer contains `off-map` (or agreed synonym) and enemy callsign |
| 1.4 | Tutorial win test still green |
| 1.5 | Manual: `ai.toml` at 80×24 — can tell A1 and B2 are not co-located at load |

### Risk

Auto-zoom thrashing as ships move. Mitigate: hysteresis or only re-auto when pan/zoom is `None` (already the model).

---

## Phase 2 — Weapon health + quiet allocate previews (P0 / P2)

### Goal

Dead guns are obvious and **cannot** drive illegal allocate/preview spam.

### Work

1. **Allocate draft:**
   - Skip or hard-lock fields for `!operational` weapons (cursor may land on them only to show status; ←/→/digits no-ops or blocked with soft notice).
   - On commit, omit non-operational weapons from the weapons map **or** send charge equal to current engine charge without top-up — match whatever the engine already accepts; prefer not listing dead ids if that triggers `not found`.
2. **UI copy:** damaged rows: `beam_1 Beam  OFFLINE` (not charge 4/4 editable).
3. **Fire mode:** do not offer Enter queue on offline weapons; ↓ skips or shows blocked.
4. **Preview (M9):** call `request_movement_preview` only when **movement or shields** change, not when only weapon charge digits change. (Weapons do not affect coast envelope.)

### Protocol note

If engine still requires dead weapons in allocate JSON, confirm with one API-play probe before changing omit behavior. No engine change required if omit works.

### Exit criteria

| # | Testable check |
|---|---|
| 2.1 | Synthetic snapshot: one weapon `operational: false` → after Right/digit on that row, draft charge unchanged |
| 2.2 | Commit allocate with damaged beam → **no** `weapon beam_1 was not found` soft error (harness or unit with mock) |
| 2.3 | Weapon-only digit entry → `pending_preview` stays `None` |
| 2.4 | Movement field Right → `pending_preview` is `Some` |
| 2.5 | Fire mode: Enter on damaged weapon does not emit `commit_fire` |
| 2.6 | Buffer contains `OFFLINE` / `DAMAGED` and does not show an editable charge prompt for that row |

---

## Phase 3 — Allocate as a form (P1)

### Goal

Allocate reads as a **form**, not a log dump; budget stays visible while editing shields.

### Work

1. Render allocate with a fixed header: `Budget spent/pool (free) · status`.
2. Scrollable field list (or `List`/`Table`) with **sticky** selection: selected row always on-screen (`allocate_scroll`).
3. Columns (text is fine): `name | value | max/note` e.g. `beam_1 | 4 | max 4 · carried 2`.
4. Fix hull display (B2): `hull {structure}` only (no fake max) until protocol adds max structure.
5. Keep carried-charge semantics visible: `carried N` vs `top-up cost`.

### Exit criteria

| # | Testable check |
|---|---|
| 3.1 | 10-field draft, cursor on last shield → buffer still contains `Budget` / `power` line |
| 3.2 | Selected field shows ▶ (or inverted row) on the **same** row as the field name |
| 3.3 | Hull line matches `hull \d+` without `\d+/\d+` fake max (unless max field added) |
| 3.4 | Tutorial allocate steps still advance (rear-attack win test) |
| 3.5 | Manual: set mov/weapons/shields without losing budget visibility at 80×24 |

---

## Phase 4 — Fire queue + cycle coach (P1)

### Goal

Player always knows **what will fire on Space** and **where they are in the 4-cycle loop**.

### Work

1. **Single source of truth for queue count:** header `queued=N` = `fire_commits` (or client pending list) for the focused ship; fire panel `Queued:` must match.
2. Fire panel: list pending commits (`beam_1 → B2 face F`) from snapshot `fire_commits` + local unsent draft if any.
3. Free-play phase coach (one line under header or in movement/fire panel):
   - Movement: `Cycle 2/4 · commit maneuver (c/t/0–5)`
   - Fire: `Cycle 2/4 · queue shots then Space (ready); charge only refills on allocate`
   - After guns empty mid-turn: `No charge left this turn — Space to pass fire`
4. Clarify Space vs `e`: end-turn only in `turn_end` (already true); coach line when phase is `turn_end`.

### Exit criteria

| # | Testable check |
|---|---|
| 4.1 | After three `commit_fire` keys, header and fire panel both show the same queue count (buffer + app state) |
| 4.2 | Snapshot with empty weapons charge mid-turn → buffer matches coach substring `No charge` or `Space to pass` |
| 4.3 | Phase `movement` with `movement_phase=3` → buffer contains coach substring `Cycle 3/4` (not only header `Move 3/4`) |
| 4.4 | Tutorial win still green |
| 4.5 | Manual free-play: one full turn without confusing “why can’t I shoot again?” |

---

## Phase 5 — Movement & shield clarity (P2)

### Goal

Reduce mis-keys on facing/shields without inventing rules.

### Work

1. **Shield mini-diagram** in allocate (and fire target face):

   ```text
     FL  F  FR
     RL  ·  RR
         R
   ```

   Highlight selected face; values optional on the diagram.

2. **Movement panel copy:**
   - One facing line: `0–5 turn to facing (costs thrust; engine rejects if short)`.
   - Remove contradictory “digits set facing” duplicates.
   - Do **not** show a guessed thrust cost unless protocol/preview exposes it (M8). Soft text only.

3. Optional later (separate engine ADR): `turn_cost` advisory on movement preview — **not** required to close this phase.

### Exit criteria

| # | Testable check |
|---|---|
| 5.1 | Allocate mode buffer contains multi-line shield diagram tokens (`FL`, `FR`, `R`) |
| 5.2 | Changing shield cursor updates which face is marked selected (buffer or app assert) |
| 5.3 | Movement help contains a single facing instruction (assert no duplicate conflicting lines if previously tested) |
| 5.4 | No new hard-coded thrust table in `frontend/tui/` (grep / review) |

---

## Phase 6 — Tutorial density (P2)

### Goal

Same lesson, fewer busywork steps; keep win smoke.

### Work

1. Collapse “nav to field + set value” into one step when auto-cursor already lands on the field (allocate).
2. Collapse repeated shield rebuild steps on later turns if tutorial still has multi-turn path (rear-attack is ~1 turn — focus on allocate micro-steps).
3. Coach `text`: first occurrence of a concept ≥ 2 sentences; repeats may be short.
4. Audit tutorial strings for wrong `Tab` / outdated key hints.

### Exit criteria

| # | Testable check |
|---|---|
| 6.1 | `REAR_ATTACK_STEPS.len()` reduced vs baseline (record N_before → N_after in PR) |
| 6.2 | `tutorial_rear_attack_wins_against_engine` still **Won** |
| 6.3 | Unit: auto-cursor + → still powers beam after select (existing regression tests) |
| 6.4 | Manual `--tutorial` completable without dead-end soft blocks |

---

## Suggested schedule

| Phase | Focus | Effort (rough) | Depends on |
|---|---|---|---|
| 0 | Baseline | 0.5 d | — |
| 1 | Map truth + off-map | 1–2 d | 0 |
| 2 | Damaged weapons + preview quiet | 1 d | 0 |
| 3 | Allocate form | 1–2 d | 2 (fields already correct) |
| 4 | Fire queue + cycle coach | 1 d | 0 |
| 5 | Shield diagram + move copy | 0.5–1 d | 3 |
| 6 | Tutorial trim | 0.5–1 d | 2, 3 |

**Parallelizable:** Phase 1 ∥ Phase 2 ∥ Phase 4 after Phase 0.  
**Serial:** 3 after 2; 5 after 3; 6 after allocate behavior is stable.

---

## Definition of done (whole plan)

1. All phase exit criteria green.
2. Free-play `ai.toml`: full game to Won/Lost with **no** repeated `weapon … not found` during normal play.
3. Tutorial still wins under `tutorial_rear_attack_wins_against_engine`.
4. At 80×24: player can answer without reading q/r alone:
   - How far is the nearest enemy?
   - Which of my guns work?
   - What fires when I press Space?
   - Which of 4 cycles am I on?

## Explicit non-goals (this plan)

- Medium/Large layout tiers / fleet multi-ship UX (see [UX-IMPLEMENTATION-PLAN.md](UX-IMPLEMENTATION-PLAN.md)).
- To-hit / damage previews.
- Engine combat balance.
- Love2D or REPL feature parity beyond shared vocabulary.

## Handoff

Implement **one phase per PR** (or tightly stacked commits). Each PR description lists the exit-criteria table rows checked and the manual scenario used.
