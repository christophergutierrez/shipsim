# Plan: TUI first-play fixes (naive playtest)

**Superseded.** Execute
[`docs/plans/tui-first-play.md`](tui-first-play.md). That file merges this
Grok cohort with the Luna and Haiku playtests and is the work order for
implementation.

Use this file as historical evidence for the Grok 4.5 cohort only. It is
**not** the implementation plan.

**TUI combat only.** Yard, REPL, and Love2D are not gates here.

**Not coding yet.** This file is the plan.

## How the playtest was run

Ten Grok 4.5 small-effort agents. They were forbidden to read documentation,
source, or tests. Each launched the real client:

```bash
cargo run --manifest-path frontend/tui/Cargo.toml -- scenarios/battle.toml
```

A PTY helper only allocated an 80×24 tmux pane and sent the keys they chose.
It did not explain the game.

| Id | Persona | Reached | Confused | Stuck | Play again |
|---:|---|---|---|---|---|
| 01 | Keys written on screen only | 2 full turns, quit | Yes (OFFLINE, arcs, log) | No | Yes, if arcs/shields clearer |
| 02 | Move toward the enemy | T3 disabled, never fired | Somewhat | For fighting back | Yes — movement was learnable |
| 03 | Fire as soon as possible | T1 hit; T2+ disabled | Yes (queue, Help trap) | T2+ | Yes, if queue/Help fixed |
| 04 | Hunt on-screen help | T1 cycle into T2 | Yes, early | No | Yes |
| 05 | Arrows / Enter / Esc / Space | T8 **DEFEAT** | Yes | Briefly (illegal volley) | Maybe |
| 06 | Letter keys first (wasd/hjkl/?) | T5 disabled | Yes | Briefly (illegal volley) | Yes |
| 07 | Understand every number first | T1 into T2 | Yes, from screen 1 | No | Maybe |
| 08 | Commit defaults (Enter/Space) | T8 **DEFEAT**, 0 shots | Yes | No | Yes, with intentional alloc |
| 09 | Find quit, then restart | 1 full turn | Yes | No | Yes |
| 10 | Complete two useful turns | 2 turns | Yes | Briefly (illegal volley) | Maybe |

**Score.** 10/10 launched. 10/10 were confused. 8/10 finished at least one
Allocate → Movement → Fire cycle from on-screen footers. 2/10 finished a
match; both lost. 0/10 won. 4/10 had a hard fail on `Space` because the
queue accepted an illegal shot. 1/10 was trapped in Help. Movement toward
the enemy was learnable. Recovering from the first exchange was not.

Raw reports: `/tmp/tui-playtest/01-report.md` … `10-report.md`.

## Review disposition

The rubric TUI pass (`docs/plans/ui-rubric-tui.md`) is still correct about
**caps, clamp logs, and labeled form footers**. This playtest falsifies
three close-out assumptions at the 80×24 floor:

1. **P1 / first-screen allocate.** The reserved `Esc back` row plus the
   Weapons header means the first allocate paint often shows **no weapon
   row and no shield row**. Testers treated Movement as the only spend.
2. **E1 on fire.** The engine preview already says `cannot bear` / uncharged,
   but Enter still queues the shot. Space then fails the **whole** volley
   (`ENGINE: order_illegal`). That is a silent-then-punish loop.
3. **C1 / C3 on Esc.** Footer copy is `Esc back`. Global Esc leaves the form
   for `Mode::Normal`, which **is** the Help panel. Help does not say how to
   return; testers needed `f` / `a` / `m`. That is not “back.”

Do not relitigate hull `N/N` or Love/REPL here.

---

## Issue catalog

Severity uses `docs/UI-RUBRIC.md`. **n** is how many of the ten named it.

### Block (Critical / Major)

| Id | Sev | n | Rubric | Live evidence |
|---|---|---:|---|---|
| **I1** Illegal queue | Major | 5 | E1, E2, A3 | `input.rs` Enter queues if the gun is operational. It does not check charge or `fire_preview.legal`. Testers queued `cannot bear` and UNCHARGED guns; Space sent the volley; engine rejected everything. |
| **I2** First-screen spend hidden | Major | 8 | P1, V2, P4 | 80×24 allocate body shows Budget + Movement + `Weapons (` header + fixed footer. Shield faces and weapon charge rows are below the fold. Status still shows `shields F:0/0`. Testers never funded shields; several never charged guns. |
| **I3** Zero spend looks valid | Major | 2+ | V2, E1 | Default draft is all zeros. Header/budget can read `power 0/36 ok`. Enter commits a turn with no path and no charge. Tester 08 lost 8–0 without leaving the default loop. |
| **I4** Esc → Help trap | Major | 3 | C1, C3, A5 | Footer: `Esc back`. Handler: `app.mode = Mode::Normal`. Normal title is `Help`. Esc/Space from Help do not restore Fire/Allocate. Return keys (`a`/`m`/`f`) are only on that Help page. |

### Log (Minor / Polish)

| Id | Sev | n | Rubric | Live evidence |
|---|---|---:|---|---|
| **I5** Mid-word truncation | Minor | 10 | V1, A5 | Ship line is `profile={size}` next to a long class name; hull line is `(current structure boxes)`. Map legend and `Space passes` CTA clip to `S…` / `Space p`. |
| **I6** OFFLINE unexplained | Minor | 6 | V3, A3 | After a volley, beams are non-operational. Allocate row is `{id} OFFLINE`. ←/→ no-ops; a log line exists (`cannot charge`) but testers did not see it. Destroyed vs “already fired” is not distinguished on the row. |
| **I7** Disabled death-walk | Minor | 5 | H3, C1 | `A1 disabled; Space passes` still walks Allocate → Movement → Fire each turn until hull dies. Testers mashed Space for many empty phases. |
| **I8** Combat-log jargon | Minor | 8 | V3, R1 | `HIT +7 sh-7 int-0` has no on-panel legend. Testers could not tell shields from internals. |
| **I9** Path cap vs motion | Minor | 4 | V1 | Allocate prints `0/6 pwr → 6 path` while power is 0. Testers thought path length was fixed at 6. |
| **I10** Enter toggle unlabeled | Minor | 4 | A4 | Enter both queues and removes. Footer is only `Enter queue`. |
| **I11** Sticky ENGINE banner | Minor | 2 | V3 | `last_error` stays in the header and combat log after the player unqueues the bad shot. Cleared on Esc-to-Normal, which is the Help trap. |
| **I12** Help keys silent | Polish | 2 | A1, A3 | `h` and `?` do nothing. Help is Esc (to Normal) or only visible after leaving the form. |
| **I13** Quit not on the form | Polish | 2 | C1, A1 | Allocate footer has no `q`. Tester 06 hit `q` while exploring letters and got a quit dialog mid-fight. Tester 09 had to Esc into Help to discover quit. |
| **I14** Stack / zoom | Polish | 4 | R2, G2 | Closing range paints `A1+1` and jumps `4 hex/cell` → `2` with no sentence. Status coords lagged testers’ reading of the stack. |
| **I15** Space fire vs pass | Polish | 3 | A5 | Fixed footer is `Space fire` while the CTA is `No legal shot; Space passes fire`. |
| **I16** Clamp-log spam | Polish | 2 | E2 | Holding Right on a 1-charge torp writes `capped at 1` into the combat log every tick. |
| **I17** Movement keys scroll away | Polish | 1 | A1 | After a few path steps the `w/a/d` line is pushed above the reserved footer. Path keys still work; the signifier vanishes. |

### Not a TUI defect

- **F1 bumped Movement (tester 04).** There is no `KeyCode::F` handler. Treat as PTY/tmux sending an escape sequence, not a bind to add.
- **They died because they skipped shields.** That is a rules outcome. The UI defect is that shields were not on the first allocate screen (I2), not that the engine applied damage.

---

## Locked

- **TUI combat only.** No Love/REPL/yard work is gated here.
- **Engine owns rules.** Do not change bearing, charge, or offline semantics. The TUI must stop offering values the engine will reject (E1).
- **80×24 combat floor stays.** New chrome must still render there. If a reserved footer and the weapon list cannot both fit, the spendable rows win over a second help sentence (V2 beats compactness).
- **No panel redesign** beyond first-screen allocate density and fire-queue gating. Do not collapse shield faces or delete SKUs.
- **Tests are the review.** Each closed issue has a `P#` invariant. New tests use `playtest_pNN_...`. Assertions name the exact row/copy.
- **Do not invent `hull N/N`.** Still no `hull_max` on the snapshot.

---

## Phase 0 — Playtest harness

**Goal:** a numbered first-play invariant list a reviewer can fail.

**Work:**

1. Add a short comment block in `frontend/tui/src/tests.rs` mapping `P1…` to
   issue ids I1–I17 and test function names. Keep the tests in `tests.rs`.
2. Do not rewrite existing rubric `T#` tests. Cite them when they already
   cover a slice (OFFLINE row, clamp log, `Esc back` footer).

**Exit (M0):**

| Check | Pass |
|---|---|
| Table of P1+ exists and cites functions | yes |
| `cargo test --manifest-path frontend/tui/Cargo.toml` | green |

**Commit:** `test(tui): number first-play invariants P1+`

---

## Phase 1 — Do not offer an illegal volley (I1, I10, I11, I15)

**Goal:** Enter never queues a shot the engine will reject; Space never
submits one; the player can see why.

**Work:**

1. On Fire Enter, refuse to queue when any of these is true:
   - weapon is not operational
   - `charge == 0`
   - current `fire_preview` for that weapon/target/face has `legal == false`
   If preview is missing, request it and do not queue yet.
2. Announce the refusal on the Fire panel (selected-row suffix or the existing
   preview line), not only in `app.log`. Examples: `cannot bear`, `UNCHARGED`,
   `OFFLINE`.
3. `commit_volley` must drop or refuse any still-illegal shot before send.
   An all-illegal or empty queue is a pass (hold fire), never
   `order_illegal`.
4. Footer: `Enter queue` when the selected gun is not queued; `Enter remove`
   when it is. When the CTA is pass-only, footer `Space fire` becomes
   `Space pass`.
5. Clear `last_error` when the player unqueues a shot or queues a legal one.

**Tests:**

| Id | Invariant |
|---|---|
| **P1** | Enter on a `cannot bear` preview does not increase `fire_draft.shots` and the 80×24 Fire buffer contains `cannot bear` |
| **P2** | Enter on an operational `charge == 0` weapon does not queue and the selected row or preview names `UNCHARGED` |
| **P3** | Space with only illegal drafts in the client state sends no volley order and does not set `last_error` to `order_illegal` |
| **P4** | While a legal shot is `[QUEUED]`, the Fire footer contains `Enter remove` |
| **P5** | After an injected `last_error`, unqueueing the focused shot clears `last_error` |

**Exit (M1):** P1–P5; existing fire/tutorial tests still green.

**Commit:** `fix(tui): refuse illegal fire queues`

---

## Phase 2 — First-screen allocate shows every spend (I2, I3, I9)

**Goal:** at 80×24, a new player can see movement, at least one weapon, and
shields without pressing Down. Zero spend does not read as success.

**Work:**

1. Keep the reserved exit row. Shrink the allocate *body* so the first paint
   includes:
   - pinned budget line
   - Movement `n/cap`
   - at least the first weapon `charge n/m` (or `OFFLINE`)
   - a shields signpost with numbers, e.g. `Shields F:0/12 FR:0/12 … (↓)`
     or one compact face-map line. Full face list may still scroll.
2. If that does not fit with the current Weapons header + hull line, drop or
   compress the decorative lines first (`power 0/36 ok`, long hull gloss).
   Do not drop the weapon row to keep the footer.
3. Replace happy zero-spend copy. `power 0/36 ok` must not appear for an
   all-zero draft. Use copy that names the consequence, e.g.
   `Budget 0/36 (36 free) · no movement · no charge · no shields`.
4. Allocate Movement row: do not print `→ 6 path` as if it were purchased
   path. Print the cap as a cap: `pwr n/cap (max path N)` or omit path
   length until Movement phase.

**Tests:**

| Id | Invariant |
|---|---|
| **P6** | 80×24 Allocate at cursor 0 contains a weapon id and `charge` or `OFFLINE` on a visible row (not merely the word `Weapons`) |
| **P7** | The same buffer contains `Shield` or a face label (`F`/`FR`/…) plus a `n/n` fraction |
| **P8** | All-zero draft buffer does not contain `ok` on the budget/power line and does contain `no charge` or equivalent |
| **P9** | Movement row does not use `→ N path` with N equal to the unused cap while `pwr` is 0 |

**Exit (M2):** P6–P9; T15/T16 (pinned budget + selected cap) still pass.

**Commit:** `fix(tui): show weapons and shields on first allocate`

---

## Phase 3 — Esc, Help, quit (I4, I12, I13)

**Goal:** Esc means one labeled thing; Help is reachable and leavable.

**Work:**

1. Keep Esc = leave form to Normal. Change the form footer from `Esc back`
   to `Esc help` **or** make Normal a real back-stack (Esc from Help
   restores the phase form). Pick one and put that word on the form.
   Preferred: **Esc help** on the form, and Help’s first line
   `Esc/Enter return · a allocate · m move · f fire · q quit`.
2. From Help/Normal, Esc and Enter restore the phase-appropriate form
   (`Allocate` / `Movement` / `Fire`) when that phase is still active.
3. `h` and `?` open Help from combat modes (same destination as Esc-to-Normal).
4. Put `q quit` on the Help first line. Optional: add `q` to the form
   footer only if it still fits at 80×24 after Phase 2.

**Tests:**

| Id | Invariant |
|---|---|
| **P10** | From Fire, Esc then 80×24 buffer title/body contains `f fire` or `return`, and Esc or Enter from there restores `Mode::Fire` |
| **P11** | From Allocate, `?` and `h` each set `Mode::Normal` (Help) |
| **P12** | Help buffer at 80×24 contains `q quit` |

**Exit (M3):** P10–P12; rubric T13 still has a visible Esc token on all three
forms (the word may be `help` instead of `back` — update T13 if the copy
changes).

**Commit:** `fix(tui): make Esc/help a round trip`

---

## Phase 4 — Offline, disable, pass (I6, I7, game-over dismiss)

**Goal:** a damaged or empty ship is explained, and the player is not
trapped in empty phases.

**Work:**

1. OFFLINE allocate/fire rows name the cause from snapshot data already on
   the ship: destroyed / not operational after firing. Example:
   `beam_1 (beam) OFFLINE — recharge next allocate` vs
   `beam_1 (beam) OFFLINE — destroyed`.
2. ←/→ on an OFFLINE field already logs; also write the same sentence into
   the selected-row suffix or the pinned budget line so it is on the form.
3. When the focused player ship is disabled (`power_available == 0` and
   engines dead, current CTA path), one Space (or a dedicated `Space skip`)
   submits the remaining empty allocate/path/volley for that ship this turn
   instead of requiring three phase visits. Still send real empty orders;
   do not skip the engine.
4. Game Over: `Enter` and `q` both quit. Keep the confirm-less game-over
   quit (match today for `q`).

**Tests:**

| Id | Invariant |
|---|---|
| **P13** | Fixture with `operational: false` after a prior fire shows `OFFLINE` plus `recharge` or `destroyed` on the allocate row |
| **P14** | ←/→ on that row leaves charge unchanged and the visible allocate panel (not only `app.log`) contains `cannot charge` or the OFFLINE cause |
| **P15** | Disabled ship: one Space from Allocate produces the empty allocate order **and** the client is ready to send empty path/volley without extra mode hunting (assert the helper/auto-pass state machine) |
| **P16** | Game Over 80×24 buffer contains `q` and `Enter` |

**Exit (M4):** P13–P16.

**Commit:** `fix(tui): explain offline and skip a disabled turn`

---

## Phase 5 — Copy that survives 80 columns (I5, I8, I14, I16, I17)

**Goal:** no mid-word clip on the first paint; combat results and stacks
are readable without a codebook.

**Work:**

1. Ship Status identity line: drop `profile=` or put size on the next line
   so `Yard Heavy Cruiser` + size is not clipped mid-token.
2. Hull gloss: `hull 95` is enough (V1 Accept still holds). Do not print
   `(current structure boxes)` if it clips.
3. Map legend: keep the compact glyph line from the rubric pass
   (`A1→ ship · ◇A1→ end · ◇ route`) and **measure** it at the live map
   column width, not 80. If it still clips, drop the lowest-value clause
   rather than a dangling `S`.
4. Header CTA: do not clip `Space passes` to `Space p`. Shorten the
   preceding status chips first.
5. Combat log panel: pin one legend line while history is non-empty:
   `HIT +dmg · sh=shields · int=hull`. Existing event strings may stay.
6. Occupied hex: render `A1+B2` (or `A1+1 enemy`) so `+1` is not a mystery.
7. Movement `w/a/d` line stays in the reserved Movement footer area (same
   trick as `Esc`), or stays in the last scroll-safe body row, so a long
   path cannot hide it.
8. Shield/weapon clamp: one log line per field per “hold”, or only on the
   transition to cap — no per-tick spam into the combat log.

**Tests:**

| Id | Invariant |
|---|---|
| **P17** | 80×24 Allocate+Status: no line contains `structure boxe` or a trailing `profile=` without a digit |
| **P18** | 80×24 map column legend has no lone clipped token `S` / `Ship/faci`; the full chosen legend string is present |
| **P19** | After a fixture combat event containing `HIT`, the log panel contains `shield` or `sh=` **and** `hull` or `int=` on a legend line |
| **P20** | Two ships on one hex render both callsigns or the word `stack` / `enemy` next to `+` |
| **P21** | Movement 80×24 with a 4-step path still contains `w` and `forward` (or `veer`) |
| **P22** | Ten ←/→ on an already-capped field add at most one new clamp line to `app.log` |

**Exit (M5):** P17–P22.

**Commit:** `fix(tui): stop clipping first-play copy`

---

## Phase 6 — Close-out

**Goal:** this file’s catalog is re-scored; TODO points here.

**Work:**

1. Mark each I# Pass or an explicit leftover with a reason.
2. One sentence in `frontend/tui/README.md`: first-play invariants are `P#`
   in `src/tests.rs`, distinct from rubric `T#`.
3. Do not claim Love/REPL first-play compliance.

**Exit (M6):**

| Check | Pass |
|---|---|
| No open Critical/Major from this catalog | yes |
| Leftover Minor/Polish rows have a one-line reason | yes |
| `cargo test --manifest-path frontend/tui/Cargo.toml` | green |
| `cargo clippy --manifest-path frontend/tui/Cargo.toml --all-targets -- -D warnings` | green |
| `cargo test --all` | green |

**Commit:** `docs(tui): close naive playtest pass`

---

## Order

```text
M0 harness
  → M1 illegal fire queue
  → M2 first-screen allocate          (can start after M0; merge after M1)
    → M3 Esc/help round trip
      → M4 offline + disabled skip
        → M5 copy / clip / stack
          → M6 close-out
```

M1 is independent of M2. Ship M1 first if only one PR can land — four of
ten testers hard-failed there.

---

## Out of scope

- Love2D / REPL first-play passes.
- Yard browse Esc (already a documented rubric leftover).
- Emitting `hull_max`.
- Changing PD, bearing, or offline combat rules.
- Teaching `F1` / PTY artifacts.
- A typed `:` command line.
- Medium/Large TUI tiers.

---

## Commands

```bash
cargo test --manifest-path frontend/tui/Cargo.toml
cargo clippy --manifest-path frontend/tui/Cargo.toml --all-targets -- -D warnings
cargo test --all
cargo run --manifest-path frontend/tui/Cargo.toml -- scenarios/battle.toml
```
