# Plan: Apply `docs/UI-RUBRIC.md` to the TUI

Use the rubric as a **review instrument** on `frontend/tui/` (combat +
`--yard`). Do not treat it as a redesign brief. Every phase below names the
criterion it closes and a test that can fail.

**TUI only.** REPL and Love copy patterns later; they are not gates here.

Implemented through M5. This file remains the review instrument; the
scoreboard below was re-validated against HEAD after close-out.

## Review disposition

**Proceed with the revisions below.** The phase structure is sound, but the
first draft had three false premises that would have weakened its exit gates:

1. Movement over-allocation already logs an explanatory cap; shield and weapon
   clamps are the remaining silent allocate paths.
2. Repair is not merely missing a denominator. The TUI currently caps repair
   with `size >= 5 ? 2 : 1`, while the engine enforces `ceil(size / 3)` (sizes
   1–3 → 1, 4–6 → 2, 7 → 3). The plan must remove that rules divergence before
   displaying a cap.
3. Several proposed visibility tests could pass on unrelated text elsewhere in
   the buffer. New tests must identify the mode-specific row or exact copy they
   are intended to prove.

---

## How this plan uses the rubric

Walk lenses 1 → 5. File defects as `V2`, `E1`, … Severity:

| Severity | Merge |
|---|---|
| Critical / Major | Block |
| Minor / Polish | Log and fix in a later phase unless cheap |

Tensions resolve as the rubric says: **V2 beats P-anything**, **E1 beats E2**,
**R1 beats H1**, **G2 beats compactness**.

Mechanize with the harness the rubric already names: `render_to_string` +
`buffer_contains` for V/G/A, `handle_key` for E/C. Give each new check an id
(`T1`, `T2`, …) and a one-sentence invariant, same discipline as
`frontend/repl/screen_audit.py` I1–I4.

---

## Validation (TUI as of this writing)

Walked against `ui.rs`, `input.rs`, `yard.rs`, `protocol.rs`, and existing
`tests.rs`. This is the scoreboard the phases close. “Pass” means a reviewer
can point at a rendered 80×24 (combat) or 60×16 (yard) buffer and not fail
the criterion.

### Combat — allocate / path / volley

| Id | Sev | Status | Evidence |
|---|---|---|---|
| **V1** Movement power | — | **Pass** | Row is `Movement: n/cap pwr → N path`. |
| **V1** Weapon charge (allocate) | — | **Pass** | `charge {chg}/{max}` plus carried. |
| **V1** Budget | — | **Pass** | Fixed header `Budget cost/pool`. |
| **V1** Shield faces | — | **Pass** | Rows and face map show `current/cap` from `ship.shield_cap(i)`. |
| **V1** Fire charge | — | **Pass** | Fire rows show `chg=current/max`, with `UNCHARGED` or `OFFLINE` as status suffixes. |
| **V1 / V4 / E1** Repair / cloak | — | **Pass** | Repair uses the engine-projected `repair_cap`; missing fields refuse to guess. Cloak cost is shown; remaining pool is pinned. |
| **V1** Hull | Minor | **Accept** | Snapshot has no `hull_max`. Showing `hull 63` is honest; inventing `63/63` would fail V4. Revisit if the engine emits the cap. |
| **V2 / E1** Movement clamp | — | **Pass** | Input clamps at `movement_power_cap()`; dead engines are `Some(0)`, not “field missing.” |
| **V2** Binding constraint on screen | — | **Pass** | Movement, weapon, and shield rows show their enforced ceilings before commit. |
| **E2** Clamp announced | — | **Pass** | Movement, shield, weapon ceiling, and carried-charge clamp attempts log the exact field and reason. |
| **E3 / E4** Combat drafts | — | **Pass** | Allocate/path/volley commits are not destructive of a design. `q` then `y` names quit. |
| **C1** Exit key on the form | — | **Pass** | Allocate, movement, and fire reserve a fixed `Esc back` row. |
| **C2** Discard | — | **Pass** | Esc to Normal keeps the allocate/path draft. |
| **C3** Esc meaning | Polish | **Partial** | Combat allocate / movement / fire label `Esc back` (returns to Normal, draft kept). Yard edit also labels `Esc back` (dirty draft warns). Remaining issue is yard **browse**: Esc quits like `q` and is unlabeled (`q quit` only). |
| **H1–H4** | — | **Pass** | Allocate fields are grouped (movement / weapons / shields). Fire lists are small. `m` jumps to Movement (H4). |
| **P1 / P3** | — | **Pass** | Allocate scroll follows the cursor. |
| **P4** Movement vs shields | — | **Pass** | The pinned budget line includes the selected movement or shield current/cap and residual power. |
| **G2 / G4** Path ghost | — | **Pass** | Destination is `◇A1↗`, route is `◇`. Shape differs without color. |
| **A1 / A2** Keys at point of use | Minor | **Partial** | Movement and fire decision keys sit on their panels. Allocate footer is `Esc back · Enter commit · ↑/↓ field · ←/→ adjust · m movement`; cloak / repair / squad keys appear only on the conditional systems row. Remaining remainder: global `Tab` / `v` stay in the header / help. |
| **A3** Inert keys | — | **Pass** (recent) | PD queue logs; follow/cloak/repair log when illegal. |
| **A5 / P2** Legend | — | **Pass** | Compact glyph-first copy is fully visible at 80×24 and matches the rendered destination/route shapes. |
| **R2** | — | **Pass** | Callsign+arrow vs `◇` prefix. |
| **R3** | — | **Pass** | Mode is the panel title (`Allocate` / `Movement` / `Fire`). |
| **V3** | — | **Pass** | Combat log after volley; path fallback logged; allocate commit logged. |
| **V4** Previews | — | **Pass** | Fire preview line is the engine `fire_preview` (threshold, %, damage). Movement preview is `path_preview`. Repair authority is scored separately above. |

### Shipyard (`--yard`)

| Id | Sev | Status | Evidence |
|---|---|---|---|
| **R1 / H2** SKU picker | — | **Pass** | Columns: sku, kind, damage, space, cost, tags. `/` filter. |
| **H3** | — | **Pass** | Rows that do not fit the hull are marked (`fits`). |
| **H4** | — | **Pass** | Standards by size; user list defaults to recency; `o` cycles. |
| **E3 / E4** Weapon delete | — | **Pass** | `d` arms, `d` again confirms, cursor move cancels. |
| **C2** Unsaved Esc | — | **Pass** | Second Esc / `q` names unsaved work. |
| **V1** Preview | — | **Pass** | Header `cost / space used/cap / power`. |
| **A1** | — | **Pass** | Browse title co-locates the sort key; picker modal shows `/ filter`; weapon/system actions remain on their edit screen. |
| **A4** | — | **Pass** | Delete copy says “d again to confirm.” |

### Critical / Major

No Critical/Major issues remain. Documented non-blocking remainders:

- **A1 Minor:** global `Tab`/`v` stay in the header/help when the combat
  floor cannot fit them.
- **C3 Polish:** yard browse Esc quits and is unlabeled (`q quit` only).
- **V1 Hull Accept:** snapshot has no `hull_max`; do not invent `N/N`.

---

## Locked

- **TUI review only.** No Love or REPL UI work is gated here. Shared engine
  snapshot plumbing and its protocol documentation are allowed where the TUI
  otherwise would have to reimplement a rule.
- **One engine projection is allowed:** centralize the authoritative repair cap
  and emit it in the ship snapshot. The TUI must not copy a combat formula.
  This is protocol plumbing, not a rules change.
- **No panel redesign** to “fix” P4. Budget stays pinned; we add denominators
  and residual on the rows, not a new layout.
- **No typed `:` command line** (ADR-0023 leftover). Not a rubric fail.
- **Do not invent `hull N/N`** until the snapshot has `hull_max`.
- **Tests are the review.** Each closed criterion has a `T#` invariant. New
  test names use a searchable `rubric_tNN_...` prefix and their assertion
  identifies the exact row/copy being proved. Existing tests may be cited by
  name instead of renamed.
- **80×24 combat / 60×16 yard** remain the floors. New chrome must still
  render there.

---

## Phase 0 — Rubric harness

**Goal:** a numbered TUI invariant list a reviewer can fail.

**Work:**

1. Add a short comment block at the top of the TUI test module mapping
   `T1…` → rubric ids and concrete test-function names. Keep rubric tests in
   `tests.rs` for now so they can reuse its render fixtures without exporting
   test-only helpers; the file is already large, so do not create a second
   module merely to move the line count.
2. Existing tests that already prove a criterion get an id in that table
   (do not rewrite them). Examples:
   - movement `n/cap` row → T1 / V1
   - dead-engine clamp → T2 / E1
   - swarm ceil 3 not 2 → T3 / V2
   - `◇` destination → T4 / G2
   - yard `d` then `d` → T5 / E3+E4
   - unsaved Esc → T6 / C2
3. New phases only add new `T#`s, and each new function starts with
   `rubric_tNN_` so the invariant can be run directly.

**Exit (M0):**

| Check | Pass |
|---|---|
| Table of T1+ exists and cites existing tests | yes |
| `cargo test --manifest-path frontend/tui/Cargo.toml` | green |

**Commit:** `test(tui): number rubric invariants T1+`

---

## Phase 1 — V1 / V2 / V4 / E1 / E2 on allocate and fire

**Goal:** every spendable field shows current/max, every displayed cap is
authoritative, and a clamp is not silent.

**Work:**

1. Centralize repair capacity in the engine (`ceil(size / 3)`, minimum 1), use
   that single function for order validation, and emit `repair_cap` on each ship
   snapshot. Document the additive field in `docs/PROTOCOL.md`. The TUI protocol
   type consumes the field; it must not derive repair capacity from `size`.
2. For a snapshot missing `repair_cap`, disable repair increment and log
   `repair cap unavailable from engine` rather than guessing. This preserves V4
   under version skew.
3. Shield face rows and the face map: `F 3/12` using `ship.shield_cap(i)`.
4. Fire weapon list: `chg=4/4` (keep `OFFLINE` / `UNCHARGED` as suffixes, not
   replacements for the fraction).
5. Repair line: `z repair=1/2`, using snapshot `repair_cap`. Repair input uses
   the same field. Add size-4 and size-7 fixtures so the old 1/2 formula cannot
   return unnoticed.
6. Preserve the existing movement-cap explanation. Extend
   `set_allocate_field` to explain upper clamps for shields and weapons, and
   the carried-charge lower bound. Example copy:
   `shield F capped at 12`, `beam_1 capped at charge 4`, and
   `beam_1 carries 2; carried charge cannot be removed`. One message per key
   gesture is acceptable.
7. Keep the pinned budget header and its existing residual (`N free`). Do not
   add the selected-field comparison until Phase 3.

**Tests (new):**

| Id | Invariant |
|---|---|
| **T7** | Allocate shield row contains `/` and the face cap |
| **T8** | Fire panel charged gun shows `chg=N/M` |
| **T9** | Existing movement clamp remains unchanged and its log explains that surplus power would be lost |
| **T10** | `→` on a shield/weapon at cap leaves the value unchanged and the log names that exact field and cap |
| **T11** | Engine and TUI both report repair caps 2 for size 4 and 3 for size 7; repair status renders `current/cap` |
| **T12** | Missing snapshot `repair_cap` does not trigger a client-side size formula |

**Exit (M1):** T7–T12 pass; 80×24 allocate still shows Movement + budget;
`cargo test --all` proves engine validation and snapshot projection agree.

**Commits:**

- `fix(engine): expose authoritative repair cap`
- `fix(tui): show field caps and announce clamps`

---

## Phase 2 — A1 / A5 / C1 / C3 signifiers

**Goal:** keys live on the form; legends describe what is drawn.

**Work:**

1. Reserve mode-specific fixed action rows for allocate, movement, and fire;
   do not append `Esc` to a long line that ratatui clips. Each form must keep
   `Esc back` visible at 80×24 while its body scrolls.
2. Keep the fixed rows decision-local and conditional. Allocate must not
   advertise cloak/repair/squad keys for systems the focused ship does not
   have. Global `Tab focus` and `v map` may remain in the header/help when the
   80×24 form cannot fit them; record A1 as an explicit Minor remainder rather
   than claiming full compliance.
3. Replace the clipped map legend with compact glyph-first copy that fits its
   80×24 map column in full. Target copy:
   `A1→ ship · ◇A1→ end · ◇ route`.
4. Path panel: if `y` follow is illegal (not in a squad / you are leader),
   the help line still shows `y follow` but a press already logs why (A3).
   No extra chrome.

**Tests:**

| Id | Invariant |
|---|---|
| **T13** | The exact fixed action row for allocate, movement, and fire contains `Esc back` at 80×24 |
| **T14** | With a path preview at 80×24, the buffer contains the complete `A1→ ship · ◇A1→ end · ◇ route` legend (not merely a `◇` elsewhere) |

**Exit (M2):** T13–T14; fixed action rows neither clip nor overwrite scrollable
content at 80×24.

**Commit:** `fix(tui): show exits and clarify the map legend`

---

## Phase 3 — P4 without a redesign

**Goal:** the power trade between movement and shields is readable without
remembering a scrolled-off number.

**Work:**

1. Do **not** collapse the face list. V2 on the selected face stays.
2. On every allocate render, the pinned budget line already has `cost/pool`
   and free. Append the selected field to that same line when the cursor is on
   movement or a shield, for example
   `Budget 10/22 (12 free) · Movement 3/6 pwr`. That is the comparison P4
   asked for; do not replace the total budget with the field value.
3. Face map already shows all six values at once once you have scrolled
   there. Leave it.

**Tests:**

| Id | Invariant |
|---|---|
| **T15** | With cursor on the last shield, one pinned line contains both `Budget N/M (... free)` and `Shield FL N/M pwr` |
| **T16** | With cursor on Movement, one pinned line contains both total budget and `Movement N/M pwr` |

**Exit (M3):** T15–T16. Assertions inspect a single rendered line so separate,
scrolled values cannot produce a false pass.

**Commit:** `fix(tui): keep budget+cap visible while editing shields`

---

## Phase 4 — Yard pass

**Goal:** apply the same instrument to `--yard` without changing catalog
rules.

**Work:**

1. Browse help already has `o sort`, and the list title already has the current
   sort. Co-locate them in the title as `user sort: recency (o)` so A2 is
   proven rather than merely A1.
2. The picker already displays `/ filter` inside its modal and weapon columns
   already include damage and cost. Retag/add rendering coverage; do not move
   the hint or change production code unless the 60×16 render shows clipping.
   The current width calculation uses a fixed 72-column budget even when the
   modal is narrower; if T18 exposes clipping, derive the budget from the
   modal's inner width and preserve the decision columns (`sku`, `damage`,
   `space`, `cost`) before optional ammo/tags columns.
3. Lower the yard terminal floor to 60×16 while retaining the 80×24 combat
   floor. The shared terminal guard must choose the floor from the active
   surface; the 60×16 picker test must exercise the real modal, not a size-error
   screen.
4. No SKU deletions. No new sort modes.

**Tests:**

| Id | Invariant |
|---|---|
| **T17** | At 60×16, the browse list title itself contains the current sort label and `(o)` |
| **T18** | At 60×16, an open weapon picker visibly contains its damage and cost headers plus the exact `/ filter` signifier |

**Exit (M4):** T17–T18; existing yard tests still green. Do not create a
production-code commit for picker chrome if T18 proves it already passes.

**Commits:** `fix(yard): co-locate browse sort control`; `fix(yard): fit picker at 60x16 floor`

---

## Phase 5 — Rubric close-out

**Goal:** the validation table in this file is updated, and `docs/TODO.md`
points here for UI work.

**Work:**

1. Retest the checklist in `docs/UI-RUBRIC.md` against 80×24 combat and
   60×16 yard buffers. Update the tables above to Pass or an explicit
   non-blocking remainder with a reason.
2. One paragraph in `frontend/tui/README.md`: UI changes are reviewed
   against `docs/UI-RUBRIC.md`; invariants live as `T#` in TUI tests.
3. Do **not** claim Love/REPL compliance.

**Exit (M5):**

| Check | Pass |
|---|---|
| No open Critical/Major on TUI | yes |
| Remaining non-pass rows have a one-line reason | yes |
| `cargo test --all` | green |
| `cargo clippy --all-targets --all-features -- -D warnings` | green |
| `cargo test --manifest-path frontend/tui/Cargo.toml` | green |
| `cargo clippy --manifest-path frontend/tui/Cargo.toml --all-targets -- -D warnings` | green |

**Commit:** `docs(tui): close UI rubric TUI pass`

---

## Order

```text
M0 harness (tag existing tests)
  → M1 V1/V2/V4/E1/E2 (authoritative caps + clamp log)
    → M2 A1/A5/C1 (Esc, legend)
      → M3 P4 (budget stays visible)
        → M4 yard signposts
          → M5 close-out
```

M1 and M2 can ship as separate PRs. M3 depends on M1 (caps exist). M4 is
independent of M1–M3.

---

## Out of scope

- Love2D / REPL rubric passes (follow-up plans).
- REPL repair selection currently carries the same stale 1/2 cap formula. The
  engine field added in M1 makes a follow-up fix straightforward, but this TUI
  close-out must not claim that client is corrected.
- Medium/Large TUI tiers (ADR-0023).
- Emitting `hull_max` from the engine (needed before V1 hull can pass).
- Removing weapon SKUs or collapsing shield faces to “simplify” (H1 misuse).
- Typed `:` command line.
- Replacing the allocate panel layout.

---

## Commands

```bash
cargo test --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --manifest-path frontend/tui/Cargo.toml
cargo clippy --manifest-path frontend/tui/Cargo.toml --all-targets -- -D warnings
cargo run --manifest-path frontend/tui/Cargo.toml -- scenarios/battle.toml
cargo run --manifest-path frontend/tui/Cargo.toml -- --yard
```
