# Plan: execute `tmp/haiku/FINDINGS.md`

**This is the single work order.** A coding agent implements it phase by
phase. Do not invent extra work. Do not implement the falsified claims.

Source: `tmp/haiku/FINDINGS.md` (Haiku docs-blind TUI playtest). That
file is evidence. **This file is what you execute.** If this file and
FINDINGS disagree on copy, this file wins.

**TUI combat only.** Do not change Love2D, REPL, yard, combat rules, or
balance. Do not add a tutorial document.

**One phase per commit.** Do not start the next phase until that phase’s
exit commands are green.

The tree has already landed parts of an earlier first-play pass. **Do
not revert those.** Several FINDINGS items are already closed; M0 proves
they stay closed. Later phases implement only what is still missing.

## How to execute

1. Read **Locked decisions** and **Do not implement**.
2. Run **M0**. If an M0 test fails, restore the locked behavior — do not
   reintroduce `OFFLINE`, `[DAMAGED]`, or Status `F:0/0`.
3. Then **M1 → M5** in order.
4. New tests live in `frontend/tui/src/tests.rs`, prefix
   `playtest_hNN_`. Each assertion names the exact row or copy.
5. After every phase:

```bash
cargo test --manifest-path frontend/tui/Cargo.toml
cargo clippy --manifest-path frontend/tui/Cargo.toml --all-targets -- -D warnings
```

6. After M5 also run `cargo test --all` and
   `cargo clippy --all-targets --all-features -- -D warnings`.
7. **Stop and ask** only if a locked decision cannot fit at 80×24.

---

## Locked decisions

1. **Engine owns rules.** Do not copy hex-arc or range math into the TUI
   for legality. Snapshot already has `weapon.arc` and `weapon.mount`.
   Fire legality uses `FireDecisionPreview` (`legal`, `reason`,
   `legal_shield_facings`). Map shading may keep its existing display
   helper; do not use it to accept or reject a shot.
2. **80×24 is the combat floor.** Every new paint test renders at 80×24.
3. **One truth per quantity.** During Allocate, Ship Status shows the
   **draft** for shields and weapon charge. Denominators are real caps
   (`ship.shield_cap(i)`, `weapon.max_charge`). Uncommitted values end
   with `*`. Never print shield denom `0` when the cap is nonzero.
4. **One word for a dead gun.** `operational == false` is `DESTROYED` in
   every panel. Never print `OFFLINE` or `[DAMAGED]`. Operational +
   `charge == 0` is `UNCHARGED`. DESTROYED rows must not show a ready
   `chg=N/M`.
5. **Illegal fire is unrepresentable.** Enter must not queue an
   uncharged, DESTROYED, dry, or `preview.legal == false` shot. Space
   must not send a known-illegal volley.
6. **Facing/arc tokens come from snapshot fields** (`arc`, `mount`),
   not a new client formula. Example copy: `arc F` or `mount=forward`.
7. **Do not invent `hull N/N`.**

## Do not implement

These were reported and **falsified** (FINDINGS “Falsified”). They are
symptoms of F1, not bugs.

| Id | False claim |
|---|---|
| **F-A** | Budget header stuck at `0/36` |
| **F-B** | Arrow keys do nothing; must type digits |
| **F-C** | Power bar saturates at 11/36 |
| **F-D** | No selection feedback (▶ plus yellow already exist) |

Also out of scope: Love/REPL/yard edits, a rules tutorial, changing
bearing/PD/damage, auto-spending a default loadout.

---

## Issue register

| Id | FINDINGS | Live status | Remediation status | This plan |
|---|---|---|---|---|
| **F1** | Status vs Allocate disagree; Status `F:0/0` | Closed (`*` draft + real cap; `playtest_p01`–`p03`) | **Pass** — M0 regression tests green | M0 verify |
| **F2** | `[DAMAGED]` vs `OFFLINE` | Closed (`DESTROYED` everywhere) | **Pass** — M0 regression tests green | M0 verify |
| **F3** | `chg=4/4 [DAMAGED]` looks ready | Closed (no `chg=` on DESTROYED) | **Pass** — M0/M1 tests green | M0 verify |
| **F5** | Enter queues uncharged → engine reject | Closed (`playtest_p05`–`p09`) | **Pass** — M0 regression tests green | M0 verify |
| **F9** | No-TTY prints raw `Os { code: 6 }` | Closed (`no_tty_message` + exit 1) | **Pass** — M0 regression tests green | M0 verify |
| **F4** | Movement cap drops with engines, unexplained | Allocate Movement row now names engine count; Movement phase keeps `engine loss` | **Pass** — H06/H07 | **M2** |
| **F6** | Arc fail unexplained; no row mark; not tied to map shade | Every preview-backed Fire row reports `BEARS`, `NO ARC`, `NO RANGE`, or engine reason; map retains `shade=arc` | **Pass** — H03/H04/H05 | **M1** |
| **F10** | *(FINDINGS supplemental)* While charging, no arc/mount on the weapon | Allocate, Status, and Fire weapon rows show snapshot-backed `arc` tokens | **Pass** — H01/H02 | **M1** |
| **F7** | `carried`, `profile=`, faces, `alloc 1/1`, `face=0→` undefined | Allocate ratios say `ships`; facing names a direction and exposes all six arrows; `profile=` remains absent | **Pass** — H08–H11 | **M3** |
| **F8** | No post-turn “you lost N / an engine” | Existing `Result:` summary is pinned and names hull/engine deltas at 80×24 | **Pass** — H12 and existing M41–M43 | **M4** confirm visible at 80×24 |

---

## M0 — Prove closed FINDINGS stay closed

**Goal:** F1, F2, F3, F5, F9 still pass. Do not change production code
unless a named test is red.

**Files:** none unless a test is red. Then only the failing behavior.

**Work:**

1. Run the TUI suite.
2. Confirm these exist and pass (names may be nearby if slightly
   renamed — grep `playtest_p0`):
   - `playtest_p01_status_and_allocate_shields_agree` (F1)
   - `playtest_p02_status_weapon_draft_matches_allocate` (F1)
   - `playtest_p03_uncommitted_marked_without_color` (F1)
   - `playtest_p04_destroyed_weapon_has_no_ready_charge` (F2/F3)
   - `playtest_p05_uncharged_enter_never_queues` (F5)
   - `playtest_p06_illegal_preview_enter_never_queues` (F5)
   - `playtest_p41_no_tty_message` (F9)
3. Grep `frontend/tui/src` for display `OFFLINE` or `[DAMAGED]`.
   Production UI must not paint those strings.

**Exit:**

| Check | Pass |
|---|---|
| Tests above green | yes |
| No production `OFFLINE` / `[DAMAGED]` on a rendered 80×24 Allocate or Fire buffer with a destroyed weapon | yes |
| `cargo test --manifest-path frontend/tui/Cargo.toml` | green |

**Commit:** only if you had to restore a regression:
`fix(tui): restore closed Haiku F1/F2/F5/F9 behavior`  
If already green: **no commit.** Go to M1.

---

## M1 — Show weapon facing before and at fire (F10, F6)

**Goal:** while **charging** and while **queuing**, the player can see
which way the gun faces and whether it currently bears. Map `shade=arc`
is the same idea, not a different one.

**Files:** `frontend/tui/src/ui.rs` (Status weapon lines, Allocate
weapon rows + spend preview, Fire weapon rows, map legend if needed).
Read `Weapon.arc` / `Weapon.mount` in `frontend/tui/src/protocol.rs`.
Do **not** add engine formulas.

**Work:**

1. **Allocate selected weapon row** (the `▶ beam_1 (Beam) charge …`
   line). Append a compact token from snapshot fields only, e.g.
   `arc F` if `mount`/`arc` is forward, otherwise `mount={mount}`
   (raw snapshot string is fine if you keep it short). Same token on
   the spend-preview fragment for that weapon.
2. **Ship Status weapon lines** get the same token
   (`beam_1 Beam rng≤10 arc F chg=0/4`).
3. **Fire list:** every weapon row (not only the selected one) shows
   `BEARS` or `NO ARC` from a **matching** `FireDecisionPreview`
   (`legal` / reason contains bear). If preview is missing, omit the
   mark (do not invent). Out of range must not say `NO ARC` — use
   `NO RANGE` when `reason` contains range, else the engine `reason`
   short form.
4. Map legend already has `shade=arc`. Keep that exact phrase on the
   80×24 map column (do not clip it to `S`). If the legend must
   shrink, keep `shade=arc` and drop a lower-value clause.
5. Do not compute whether a gun bears in Allocate (no target lock
   yet). Allocate only names the mount/arc. Fire names BEARS/NO ARC
   from the preview.

**Tests:**

| Id | Function | Invariant |
|---|---|---|
| **H01** | `playtest_h01_allocate_selected_weapon_names_arc` | 80×24 Allocate with cursor on `beam_1` contains `beam_1` and (`arc` or `mount`) on that row |
| **H02** | `playtest_h02_status_weapon_names_arc` | Same buffer Status region contains `beam_1` and (`arc` or `mount`) |
| **H03** | `playtest_h03_fire_every_row_marks_bear_or_not` | Fire fixture with two guns: injected previews, one `cannot bear` and one legal — buffer has `NO ARC` and `BEARS` |
| **H04** | `playtest_h04_range_reason_is_not_no_arc` | Preview reason `out of range` → row contains `RANGE` or the reason, and does **not** contain `NO ARC` |
| **H05** | `playtest_h05_map_legend_keeps_shade_arc` | 80×24 contains the full substring `shade=arc` |

**Exit:** H01–H05; existing fire-queue tests still green.

**Commit:** `fix(tui): show weapon arc while allocating and firing`

---

## M2 — Name why the movement cap shrank (F4)

**Goal:** when engines are damaged, the Allocate Movement row states
the cause next to the new cap.

**Files:** `frontend/tui/src/ui.rs` Allocate Movement row
(`max path {}` today). Snapshot already has `engine`, `motion_cap()`,
`max_maneuver_actions`.

**Work:**

1. If `motion_cap() < max_maneuver_actions` **or** `engine` is below
   the value implied by a full cap, append
   `(engines {engine})` or
   `(engines {engine}; max path {motion_cap()})`
   on the **Allocate** Movement row. Do not invent `engine_max` if
   the snapshot has no such field — then print `engines {n}` only.
2. Keep the existing Movement-phase `engine loss: n/m` line.
3. When engines are undamaged, do not add extra chrome.

**Tests:**

| Id | Function | Invariant |
|---|---|---|
| **H06** | `playtest_h06_allocate_movement_names_engine_loss` | Fixture `engine` damaged so `motion_cap < max_maneuver_actions`: Allocate 80×24 Movement row contains `engine` |
| **H07** | `playtest_h07_undamaged_movement_row_omits_engine_gloss` | Full engines: Movement row has `max path` and does not say `engine loss` |

**Exit:** H06–H07; existing motion-cap tests green.

**Commit:** `fix(tui): attribute movement cap to engine damage`

---

## M3 — Define remaining vocabulary (F7)

**Goal:** no FINDINGS jargon token appears bare on the first Allocate
paint.

**Files:** `frontend/tui/src/ui.rs` header (`stage_progress`), Status
facing line, Status shield labels, Help first line if needed.

**Work:**

1. Header `alloc 1/1` / `path 1/1` / `volley 1/1` →
   `alloc 1/1 ships` (or `1/1 ready`). The word `ships` or `ready`
   must sit on the same chip.
2. Status facing: `face=0→` becomes
   `face 0 → east` (use the existing direction table already used
   for hex bearings: 0 east, 1 northeast, 2 northwest, 3 west,
   4 southwest, 5 southeast).
3. Six facings must be discoverable on 80×24 without Help: either
   the Status/Allocate shield line already lists F FR RR R RL FL
   (it does) **and** a one-line
   `faces →↗↖←↙↘ = 0..5` on Allocate or Status, or Help’s first
   line lists all six arrows. Prefer the Status/Allocate line.
4. Status shield labels stay `F`/`FR`/… but the Allocate face list
   already spells `F  forward (nose)`. Keep that. If Status clips
   the second shield line at 80×24, shrink the power/engine line
   first so **FL** remains visible (FINDINGS F1 family).
5. `carried` already has `add N for ready`. Do not regress it.
6. Do not bring back `profile=`.

**Tests:**

| Id | Function | Invariant |
|---|---|---|
| **H08** | `playtest_h08_header_glosses_alloc_ratio` | 80×24 Allocate header contains `alloc` and (`ships` or `ready`) |
| **H09** | `playtest_h09_facing_names_a_direction` | Status contains `east` or `northeast` (or another of the six words) next to a facing arrow |
| **H10** | `playtest_h10_six_face_glyphs_or_labels_visible` | 80×24 contains `FL` and `RR` (or all six arrows `→↗↖←↙↘`) |
| **H11** | `playtest_h11_no_bare_profile_equals` | 80×24 Allocate has no `profile=` |

**Exit:** H08–H11.

**Commit:** `fix(tui): gloss remaining first-play vocabulary`

---

## M4 — Turn outcome stays on the next Allocate (F8)

**Goal:** after a damaging exchange, the player does not have to diff
hull numbers. A one-line summary is on the next Allocate paint.

**Files:** `frontend/tui/src/app.rs` `resolution_summary`,
`frontend/tui/src/ui.rs` (`Result: {summary}`).

**Work:**

1. Confirm `resolution_summary` still runs on snapshot transition and
   includes hull delta and engine/weapon loss when those change.
2. On **80×24 Allocate**, the `Result:` line must be visible after a
   fixture that drops hull and one engine box. If the reserved footer
   or spend preview pushes it off, pin `Result:` in the Allocate
   header/budget area (same pinned strip as Budget) so it cannot
   scroll away.
3. Do not invent per-hit combat-log rewrites if `Result:` already
   names `hull a→b` and `engines a→b`.

**Tests:**

| Id | Function | Invariant |
|---|---|---|
| **H12** | `playtest_h12_allocate_shows_turn_result` | After injecting a previous/current snap pair (hull 95→90, engine 3→2), 80×24 Allocate contains `Result` (or `resolved`) and `hull` and `engine` |
| **H13** | Existing resolution-summary unit test (`hull 12→9`) | still green |

**Exit:** H12–H13.

**Commit:** `fix(tui): keep turn result visible on allocate`

---

## M5 — Close-out

**Goal:** every FINDINGS confirmed defect is Pass or an explicit
leftover.

**Work:**

1. Add a **Status** column to the issue register above (Pass /
   leftover-with-reason). F-A–F-D stay “do not implement.”
2. One sentence in `frontend/tui/README.md`: Haiku first-contact
   invariants are `playtest_hNN_`.
3. Point `docs/TODO.md` at this file for remaining TUI first-contact
   work if a bullet still names the old playtest plans only.
4. Do not claim Love/REPL compliance. Do not run a new 10-agent
   playtest unless the operator asks.

**Exit:**

| Check | Pass |
|---|---|
| F1–F10 Pass or leftover-with-reason | yes |
| F-A–F-D not implemented | yes |
| `cargo test --manifest-path frontend/tui/Cargo.toml` | green |
| `cargo clippy --manifest-path frontend/tui/Cargo.toml --all-targets -- -D warnings` | green |
| `cargo test --all` | green |
| `cargo clippy --all-targets --all-features -- -D warnings` | green |

**Commit:** `docs(tui): close Haiku FINDINGS remediation`

---

## Order

```text
M0 verify closed F1/F2/F3/F5/F9
  → M1 facing on allocate + Fire BEARS/NO ARC   (F10, F6)
    → M2 movement cap names engines             (F4)
      → M3 leftover vocabulary                  (F7)
        → M4 turn result visible                (F8)
          → M5 docs close-out
```

## Recommended commits

1. *(none if M0 is already green)*
2. `fix(tui): show weapon arc while allocating and firing`
3. `fix(tui): attribute movement cap to engine damage`
4. `fix(tui): gloss remaining first-play vocabulary`
5. `fix(tui): keep turn result visible on allocate`
6. `docs(tui): close Haiku FINDINGS remediation`

## Commands

```bash
cargo test --manifest-path frontend/tui/Cargo.toml
cargo clippy --manifest-path frontend/tui/Cargo.toml --all-targets -- -D warnings
cargo test --all
cargo clippy --all-targets --all-features -- -D warnings
cargo run --manifest-path frontend/tui/Cargo.toml -- scenarios/battle.toml
```
