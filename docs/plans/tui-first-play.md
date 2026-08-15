# Plan: TUI first-play remediation

**This is the single execution plan.** A lesser agent implements it
phase by phase. Do not invent extra work. Do not reopen
`docs/plans/ui-rubric-tui.md` except where a named test in this file
says to update copy.

**Not coding in this file.** Implement in the repo following each
milestone. One phase per commit. Do not start the next phase until
that phase’s exit commands are green.

**TUI combat only.** Do not change Love2D, REPL, yard, combat rules,
or balance.

## Sources (already decided — do not re-litigate)

Three independent docs-blind playtests of

```bash
cargo run --manifest-path frontend/tui/Cargo.toml -- scenarios/battle.toml
```

| Cohort | Where | Headline |
|---|---|---|
| Grok 4.5 ×10 | `docs/plans/tui-naive-playtest.md` | 10/10 confused; 4/10 hard-failed on illegal volley; first allocate paint hides weapons/shields |
| Luna ×10 | `tmp/luna/playtest-findings.md` | Mean 4.7/10; 0 finished the battle; engine preview exists but TUI still queues illegal shots |
| Haiku ×9 | `tmp/haiku/FINDINGS.md` | **F1** is Critical: Status vs Allocate show the same quantity with different numbers; three “bugs” were false |

Those three files plus `tmp/luna/implementation-plan.md` and
`tmp/haiku/REMEDIATION-PLAN.md` are **inputs**. This file is the
**work order**. If they disagree, the **Locked decisions** below win.

## How to execute

1. Read **Locked decisions** and **Do not implement**.
2. Do **M0**, then **M1** … **M8** in order. M2 may be coded in
   parallel with M1 after M0, but **merge M1 first**.
3. Each phase names files, exact copy, test function names, and a
   commit message. Use those names.
4. New tests live in `frontend/tui/src/tests.rs`, prefix
   `playtest_pNN_`. Each assertion names the exact row or copy.
5. After every phase:

```bash
cargo test --manifest-path frontend/tui/Cargo.toml
cargo clippy --manifest-path frontend/tui/Cargo.toml --all-targets -- -D warnings
```

6. After M8, also run `cargo test --all` and
   `cargo clippy --all-targets --all-features -- -D warnings`.
7. **Stop and ask** only if a locked decision is physically
   impossible at 80×24. Do not invent a fourth panel layout.

---

## Locked decisions

These are not options. Implement them as written.

1. **Engine owns rules.** Queue legality comes from snapshot fields
   (`operational`, `charge`, `ammo_remaining`) plus a **matching**
   `FireDecisionPreview` (`legal`, `reason`, `legal_shield_facings`).
   Do not copy hex-arc or range math into the TUI. Do not add a
   protocol version.
2. **80×24 is the combat floor.** Every new paint/copy test renders
   at exactly 80×24.
3. **One truth per quantity (Haiku F1).** During Allocate, Ship
   Status shows the **pending draft**, not the last committed
   snapshot, for shields and weapon charge. Denominators are always
   the real cap (`ship.shield_cap(i)`, `weapon.max_charge`). Mark
   uncommitted projections with a trailing `*` (shape, not colour
   only). Example: Status `F:3*/12` agrees with Allocate
   `F  forward (nose): 3/12`.
4. **One word for a dead gun.** `operational == false` is
   `DESTROYED` in **every** panel. Never print `OFFLINE` or
   `[DAMAGED]` for that state. Operational + `charge == 0` is
   `UNCHARGED`. Point defense stays `auto` / PD. Do **not** invent
   “OFFLINE — recharge next allocate”; that misread damage as a
   cooldown.
5. **Esc contract.** Combat form Esc opens Overview & Help and
   keeps the draft. Help Esc or Enter restores the form for the
   **current engine phase**. Footer word is `Esc help`, never
   `Esc back`. `?` and `h` also open Help. Title the Normal panel
   `Overview & Help`.
6. **First Allocate paint teaches three spends.** At 80×24, cursor
   0, without Down, the Allocate **region** shows Movement `n/cap`,
   at least one weapon id with `charge n/m` or `DESTROYED`, and a
   shield fraction. This is density/copy, not a new architecture.
   Do not collapse faces or delete SKUs.
7. **Zero spend is not success.** An all-zero draft must not say
   `ok`. It must name `no motion`, `no charge`, and `no shields`.
8. **Love2D is a reference, not a target.**
   `frontend/love/main.lua` `do_commit_fire` already refuses
   uncharged / non-operational / dry ammo. TUI must refuse that
   **and** `preview.legal == false` / invalid face. Do not edit Love.
9. **Do not invent `hull N/N`.** Snapshot still has no `hull_max`.
10. **Preserve** callsigns, `◇` path family, phase title, confirmed
    `q` then `y` quit, and existing rubric `T#` / `rubric_tNN_` tests.
    If M4 changes `Esc back` copy, update `rubric_t13_*` to the new
    token `Esc help`.

## Do not implement

These were reported and **falsified** (`tmp/haiku/FINDINGS.md`).
They are symptoms of F1 or of a colour-stripped harness. Fixing
them is wasted work and will fight M1.

| Id | False claim | Reality |
|---|---|---|
| F-A | Budget header stuck at `0/36` | Budget already updates (`Budget 3/36 (33 free)`) |
| F-B | Arrows do nothing; must type digits | Arrows increment; caps already log |
| F-C | Power bar saturates at 11/36 | Bar is proportional to panel width |
| F-D | No selection feedback | `▶` plus yellow; yellow was stripped in one harness |

Also out of scope: yard, REPL, Love2D edits, tutorial-as-primary-fix,
typed `:` command line, Medium/Large TUI, changing PD / bearing /
offline **rules**, a fresh 10-agent playtest (that is operator-only
**M9**, not your merge gate).

---

## Issue index (merged)

Use these ids in test comments. Do not open a new id.

| Id | Sev | Source | Defect |
|---|---|---|---|
| **X1** | Critical | Haiku F1 | Status vs Allocate disagree; Status shield denom is powered (often 0), not cap |
| **X2** | Major | all | Enter queues uncharged / out-of-arc / invalid-face shots; Space → `ENGINE: order_illegal` |
| **X3** | Major | Grok I2, Luna L02 | 80×24 first Allocate paint hides weapons and shields |
| **X4** | Major | Grok I3, Luna L02 | Zero draft reads `ok` |
| **X5** | Major | Grok I4, Luna L05 | `Esc back` opens Help; no return |
| **X6** | Major | Haiku F2, Luna L03, Grok I6 | `[DAMAGED]` vs `OFFLINE` vs unexplained dead gun |
| **X7** | Major | Haiku F4, Grok I9 | Movement cap drops with engines and `→ N path` looks purchased |
| **X8** | Major | Luna L07, Grok I7 | Disabled ship still walks three empty phases |
| **X9** | Major | Haiku F6 | Arc failure not on the weapon row; not tied to map |
| **X10** | Minor | all | Clip (`profile=`, `structure boxe`, `Space p`), jargon (`sh`/`int`), `Map @(q,r)`, `A1+1`, silent unknown keys, clamp spam |
| **X11** | Minor | Haiku F8 | No per-turn “what happened to you” line |
| **X12** | Minor | Haiku F9 | No-TTY start prints raw `Os { code: 6 }` |
| **X13** | Polish | Luna L13, Grok I12–I13 | `h`/`?` silent; `q` only after Help |

## Close-out scoreboard

| Issue | Status | Evidence / reason |
|---|---|---|
| X1 | Pass | M1 status and Allocate use the same draft numerator and real cap denominator; P01–P04 |
| X2 | Pass | Matching preview and weapon-state checks gate queue and volley submission; P05–P11 |
| X3 | Pass | First Allocate paint exposes movement, weapon, and shield spends; P13 |
| X4 | Pass | Zero spend names no motion, no charge, and no shields; P14 |
| X5 | Pass | Esc opens Overview & Help and round-trips drafts; P17–P18 |
| X6 | Pass | DESTROYED and UNCHARGED are distinct and destroyed rows are skipped; P23–P25 |
| X7 | Pass | Movement conversion cap and engine loss are named; P15, P26 |
| X8 | Pass | Disabled body/footer and explicit empty-order autopass are implemented; P28–P32 |
| X9 | Pass | Illegal preview reason is shown on the selected weapon row; P06, P12 |
| X10 | Pass | Copy, map identity, stack labels, legends, unknown-key and clamp behavior are covered; P33–P40 |
| X11 | Leftover — event detail only | Combat logs retain real hit/shield/hull details and a pinned legend; no synthetic per-turn delta is claimed because the plan does not add a protocol history contract |
| X12 | Pass | Non-TTY startup reports the PTY requirement; P41 |
| X13 | Pass | h/? help entry and recoverable Help/phase controls are implemented; P17–P22 |

---

## M0 — Invariant index

**Goal:** a reviewer can find every `P#` before behavior changes.

**Files:** `frontend/tui/src/tests.rs` only.

**Work:**

1. Directly under the existing `T#` comment block, add a `P01…`
   index that lists every test name in M1–M8 (you may stub
   `#[ignore]` only if the function exists and the ignore message
   names the owning milestone). Prefer **writing the real test in
   the same commit as the fix** — do not leave a graveyard of
   ignored tests.
2. Do not rewrite rubric tests.

**Exit:**

| Check | Pass |
|---|---|
| Comment index lists P01–P41 and the planned `playtest_pNN_` names | yes |
| `cargo test --manifest-path frontend/tui/Cargo.toml` | green |

**Commit:** `test(tui): index first-play invariants P01+`

---

## M1 — One truth per quantity (X1)

**Goal:** two panels never show the same shield or weapon number
with different values, unlabeled. Status never prints denom `0`
when the cap is nonzero.

**Files:** `frontend/tui/src/ui.rs` (Ship Status + Allocate),
tests in `tests.rs`. Read `Ship::shield_cap` in
`frontend/tui/src/protocol.rs`. Do not change the engine.

**Work:**

1. Ship Status shields today: `remaining/powered`
   (`ui.rs` ~871–876). That is why testers saw `F:0/0` with cap 12.
   Change Status, **during Allocate**, to
   `{label}:{draft}/{cap}` using `alloc_draft.shields[i]` and
   `ship.shield_cap(i)`. Append `*` when draft ≠ committed
   remaining. After allocate is committed (or not in Allocate),
   show `{label}:{remaining}/{cap}` (denom is still cap).
2. Ship Status weapons: during Allocate, `chg={draft}/{max}*`
   when draft ≠ snapshot charge. If `!operational`, print
   `DESTROYED` and **do not** print a plain `chg=N/M` that looks
   ready (Haiku F3).
3. Add a one-line Status gloss: `* = not yet committed`.
4. Allocate panel already uses draft/cap. Leave that model. Make
   sure a reviewer comparing the two panels sees the same
   numerator and the same denominator.

**Tests:**

| Id | Function | Invariant |
|---|---|---|
| **P01** | `playtest_p01_status_and_allocate_shields_agree` | After Right on face F to 3, 80×24 buffer’s Status and Allocate both contain `3/12` (or the fixture cap) for F; Status does not contain `F:0/0` |
| **P02** | `playtest_p02_status_weapon_draft_matches_allocate` | After charging `beam_1` to max, Status and Allocate both show that max; Status charge is not stuck at `0/max` |
| **P03** | `playtest_p03_uncommitted_marked_without_color` | Drafted values include `*` somewhere in Status |
| **P04** | `playtest_p04_destroyed_weapon_has_no_ready_charge` | `operational: false` Status row has `DESTROYED` and no unadorned `chg=4/4` |

**Exit:** P01–P04; existing allocate cap tests (`rubric_t07`, `t15`,
`t16`) green.

**Commit:** `fix(tui): show one consistent value per quantity during allocate`

---

## M2 — Illegal fire is unrepresentable (X2, X9)

**Goal:** Enter cannot queue a shot the engine will reject. Space
never submits a known-illegal volley. The reason is on the Fire
row **before** commit.

**Files:** `frontend/tui/src/input.rs` (`handle_fire`,
`commit_volley`), `frontend/tui/src/app.rs` (preview cache),
`frontend/tui/src/ui.rs` (Fire rows + footer). Reference only:
`frontend/love/main.lua` `do_commit_fire`.

**Work:**

1. Add a helper that decides queue readiness from:
   - selected weapon `operational`, `charge`, `ammo_remaining`, PD;
   - selected target + shield face;
   - a `FireDecisionPreview` whose `(ship, weapon, target)` match.
2. Cache accepted previews by
   `(snapshot generation or turn+phase, ship, weapon, target)`.
   Invalidate on new snapshot / phase. Face legality is only
   `legal_shield_facings` from that cached preview.
3. Enter:
   - `!operational` → do not queue; show `DESTROYED — cannot queue`
   - PD → existing auto message
   - `charge == 0` → do not queue; `UNCHARGED — charge in Allocate`
   - ammo 0 → do not queue; `out of ammo`
   - missing/stale preview → request preview; do not queue;
     `checking shot…`
   - `legal == false` → do not queue; show engine `reason`
     (player-facing; `cannot bear` stays those words)
   - face not in `legal_shield_facings` → do not queue; list
     valid face labels from the preview
   - else toggle queue (legal shots only)
4. `commit_volley`: re-check every queued shot. If any is illegal
   or stale, send **no** order, focus the first bad shot, keep the
   draft. Empty queue is still a legal hold-fire pass.
5. Fire footer (dynamic):
   - unqueued legal: `Enter queue`
   - queued: `Enter remove`
   - illegal selected: `Enter unavailable`
   - non-empty legal queue: `Space fire`
   - else: `Space pass`
6. Each Fire weapon row shows `BEARS` or `NO ARC` from the
   matching preview (`legal` / reason contains bear, or empty
   `legal_shield_facings`). Do not recompute hex math.
7. Clear `last_error` when the player unqueues or queues a legal
   shot.

**Tests:**

| Id | Function | Invariant |
|---|---|---|
| **P05** | `playtest_p05_uncharged_enter_never_queues` | `shots` unchanged; no `SendOrder`; buffer has `UNCHARGED` |
| **P06** | `playtest_p06_illegal_preview_enter_never_queues` | Injected `legal:false` reason `cannot bear`; no queue; buffer has `cannot bear` |
| **P07** | `playtest_p07_invalid_face_enter_names_valid_faces` | Refuse; at least one label from `legal_shield_facings` is visible |
| **P08** | `playtest_p08_stale_preview_cannot_queue_other_weapon` | Preview for `beam_1` does not authorize `torp_1` |
| **P09** | `playtest_p09_legal_preview_queues_and_removes` | Enter toggles; footer `queue` ↔ `remove` |
| **P10** | `playtest_p10_space_never_sends_known_illegal_volley` | Seeded illegal draft + Space → no `commit_volley` order; `last_error` is not `order_illegal` |
| **P11** | `playtest_p11_empty_space_is_explicit_pass` | Empty queue Space sends empty volley; footer/CTA contains `pass` |
| **P12** | `playtest_p12_fire_row_marks_bear_or_no_arc` | Illegal-bear preview ⇒ selected or listed row contains `NO ARC` |

**Exit:** P05–P12; tutorial fire tests and existing PD/offline fire
tests green. A scripted uncharged Enter never produces
`ENGINE: order_illegal`.

**Commit:** `fix(tui): refuse illegal fire queues`

---

## M3 — First Allocate paint (X3, X4, X7 path wording)

**Goal:** before any key, a new player sees movement, a weapon, and
shields. Zero spend does not look like success.

**Files:** `frontend/tui/src/ui.rs` `render_allocate_panel` and the
pinned budget line. Keep the reserved footer row.

**Work:**

1. First 80×24 Allocate paint (cursor 0) must include, in the
   Allocate region:
   - Movement `n/cap`
   - at least one weapon id + `charge` or `DESTROYED`
   - a shield signpost with a `n/n` fraction (compact
     `Shields F:0/12 …` or the face-map line is fine)
2. Drop or shorten, in this order, until that fits:
   `(structure boxes)` / `(current structure boxes)`,
   `Weapons (ship order — same as fire list):`,
   `engine→thrust`, the happy `power 0/36 ok` line.
3. All-zero draft copy (pinned budget or the line that currently
   says `ok`):
   `Budget 0/36 (36 free) · no motion · no charge · no shields`
   (use the fixture’s pool). **Must not contain ` ok`.**
4. Movement row: do not print `→ 6 path` as if path were already
   bought at 0 power. Print
   `Movement: {pwr}/{pwr_cap} pwr (max path {motion_cap})`
   using `movement_power_cap()` / `motion_cap()`. No new formula.
5. Keep full weapon list + six faces scrollable. Up/Down still
   reaches every field. Preserve `rubric_t15` / `t16` (pinned
   budget + selected movement/shield).

**Tests:**

| Id | Function | Invariant |
|---|---|---|
| **P13** | `playtest_p13_first_allocate_shows_three_spends` | 80×24, cursor 0: Allocate region has Movement, a weapon id with charge/`DESTROYED`, and a shield `n/n` |
| **P14** | `playtest_p14_zero_draft_names_consequences` | Contains `no motion`, `no charge`, `no shields`; does not contain ` ok` |
| **P15** | `playtest_p15_movement_row_not_fake_purchased_path` | At 0 pwr the row does not use `→ N path` with N = unused cap |
| **P16** | `playtest_p16_all_alloc_fields_reachable` | Down from 0 reaches last shield face |

**Exit:** P13–P16; `rubric_t15`, `rubric_t16` green.

**Commit:** `fix(tui): expose all allocation choices at first paint`

---

## M4 — Help and phase commits are a round trip (X5, X13)

**Goal:** Esc means one labeled thing. Help cannot trap the player.
Commits name the next phase.

**Files:** `frontend/tui/src/input.rs` (global Esc, Normal, `h`/`?`,
`a`/`m`/`f`), `frontend/tui/src/ui.rs` (Help lines + combat
footers), `frontend/tui/src/app.rs` if you need a return-mode.

**Work:**

1. Form Esc → `Mode::Normal` (Help). Store enough to restore
   Allocate / Movement / Fire **and** the draft.
2. From Help, Esc and Enter restore the phase form when that
   engine phase is still active.
3. Footer on Allocate / Movement / Fire: `Esc help` (update
   `rubric_t13_*` if it looks for `Esc back`).
4. Help title: `Overview & Help`. First line, not scrollable:
   `Esc/Enter return · q quit · a allocate · m move · f fire`
5. `h` and `?` from combat forms open Help.
6. Inactive `a`/`m`/`f` (wrong phase) must log
   `{phase} is not active; current phase is {phase}` — never silent.
7. Commit footers:
   - Allocate: `Enter commit power → Movement`
   - Movement with path: `Enter commit path → Fire`
   - Movement empty: `Space hold → Fire` (Enter on empty path
     does **not** send; say `Path empty — Space holds`)
   - Fire with legal queue: `Space fire → Resolve`
   - Fire empty: `Space pass → Resolve`
8. After the engine acknowledges a phase commit, pin one line for
   the next paint: `Power committed — plot movement` /
   `Path committed — choose shots` (keep it short).

**Tests:**

| Id | Function | Invariant |
|---|---|---|
| **P17** | `playtest_p17_escape_help_round_trip` | From each of Allocate, Movement, Fire: Esc → Help; Esc or Enter returns to the same `Mode` with draft equal |
| **P18** | `playtest_p18_help_first_line_return_and_quit` | 80×24 Help contains `return` and `q quit` |
| **P19** | `playtest_p19_question_and_h_open_help` | Both keys from Allocate set Help |
| **P20** | `playtest_p20_inactive_hotkey_explains` | From Allocate, `f` does not silently no-op |
| **P21** | `playtest_p21_commit_footer_names_next_phase` | Allocate footer contains `Movement`; Movement footer contains `Fire` |
| **P22** | `playtest_p22_enter_does_not_commit_empty_path` | Empty path Enter → no order; Space → empty path order |

**Exit:** P17–P22; quit-confirm, map-mode Esc, tutorial Esc tests
green. No footer contains `Esc back`.

**Commit:** `fix(tui): make help and phase transitions recoverable`

---

## M5 — Name broken systems (X6, X7, Haiku F3/F4)

**Goal:** a player can tell DESTROYED from UNCHARGED, and why the
movement cap shrank.

**Files:** `frontend/tui/src/ui.rs` (Status, Allocate, Fire rows),
`frontend/tui/src/input.rs` (skip destroyed rows on Up/Down in
Allocate). Snapshot already has `engine`, `operational`,
`motion_cap()`.

**Work:**

1. Grep the TUI for `OFFLINE` and `[DAMAGED]` and replace display
   uses with `DESTROYED` / `UNCHARGED` per Locked decision 4.
   Update tests that search for `OFFLINE` (there are several in
   `tests.rs`) to the new word in **this** commit so the suite
   stays green.
2. Allocate Up/Down skips `!operational` weapons. Selecting a
   destroyed row via leftover cursor jumps to the first operational
   weapon and pins `{old} destroyed; selected {new}`.
3. Selected summary on Allocate and Fire:
   `Selected weapon i/N: beam_2 (beam)`.
4. Carried charge inline:
   `carried 2; add 2 for ready 4/4` (use the real numbers).
5. When the movement cap is below the undamaged design cap
   (`motion_cap() < max_maneuver_actions` or engines < starting
   boxes), annotate:
   `Movement: 0/4 pwr (max path 4; engines 2/3)`.
   Use snapshot `engine` and whatever max is already on the ship
   (do not invent `engine_max` if the snapshot lacks it — then
   write `engines {n}` without a fake denom).
6. If all weapons are unusable, Fire shows
   `No usable weapons — Space passes` and no `Enter queue`.

**Tests:**

| Id | Function | Invariant |
|---|---|---|
| **P23** | `playtest_p23_destroyed_and_uncharged_are_distinct` | Same 80×24 buffer never has both `OFFLINE` and `[DAMAGED]`; dead = `DESTROYED`, live-empty = `UNCHARGED` |
| **P24** | `playtest_p24_allocate_skips_destroyed` | Down from Movement lands on an operational weapon if one exists |
| **P25** | `playtest_p25_carried_charge_names_power_to_ready` | Fixture with carried 2 / max 4 contains `carried` and `2` |
| **P26** | `playtest_p26_movement_row_names_engine_loss` | Fixture engines damaged below design: Movement row contains `engine` |
| **P27** | `playtest_p27_no_usable_weapons_is_pass_only` | All destroyed: Fire has `pass` and no `Enter queue` |

**Exit:** P23–P27; any test that still greps `OFFLINE` is updated
and green.

**Commit:** `fix(tui): explain weapon state and engine loss`

---

## M6 — Disabled ships and game over (X8)

**Goal:** a disabled ship is explained and cheap to pass. Game over
is dismissible.

**Files:** `frontend/tui/src/ui.rs` (banner, game over),
`frontend/tui/src/input.rs` / `app.rs` (auto-pass state machine).

**Work:**

1. When the focused player ship matches the existing CTA
   (`power_available == 0` and cannot act — same condition as
   `A1 disabled; Space passes`), replace the form **body** with:
   `DISABLED — no power; cannot move or fire`
   Footer: `Space pass disabled ship`.
2. That Space may start a **client** auto-pass: wait for each
   authoritative snapshot, then send the normal **empty**
   allocate / path / volley for **this ship only**. Do not invent
   engine orders. Do not act for another living player ship.
3. Cancel auto-pass on game over, focus change, engine error,
   unexpected phase, or return of power/weapons.
4. If `repair` is present and `repair_cap` is usable, **do not**
   auto-pass; keep Allocate so repair can be bought.
5. Game Over: `Enter` and `q` both quit. Buffer contains both
   words.

**Tests:**

| Id | Function | Invariant |
|---|---|---|
| **P28** | `playtest_p28_disabled_banner_names_cause` | 80×24 contains `DISABLED` and `Space` |
| **P29** | `playtest_p29_disabled_space_emits_empty_allocate` | Space from disabled Allocate → `SendOrder` empty allocate (or the project’s existing empty-allocate shape) |
| **P30** | `playtest_p30_autopass_stops_on_focus_change` | After Tab to another ship, no further auto orders |
| **P31** | `playtest_p31_repairable_not_forced_autopass` | Ship with repair + `repair_cap` still shows a repair field |
| **P32** | `playtest_p32_game_over_names_enter_and_q` | Game Over 80×24 contains `Enter` and `q` |

**Exit:** P28–P32; scripted pump / full-cycle tests green. No new
protocol messages.

**Commit:** `fix(tui): explain and pass disabled ships`

---

## M7 — Copy, map identity, cold start (X10, X11, X12)

**Goal:** first-play words survive 80 columns. No label pretends to
be ship position. A non-TTY launch is actionable.

**Files:** `frontend/tui/src/ui.rs`, `frontend/tui/src/main.rs`
(TTY check), `frontend/tui/src/input.rs` (unknown-key /
clamp rate-limit).

**Work:**

1. Identity line: do not clip `profile=` mid-token. Put size on
   its own short token (`size 4`) or drop `profile=`.
2. Drop `(current structure boxes)` if it clips; `hull 95` is
   enough.
3. Header CTA must not become `Space p`. Shorten chips before
   the CTA.
4. Map title: `View origin (q,r)` — never `Map @(q,r)` as if it
   were the ship.
5. Stacked hex: `A1+B2` or `A1+1 enemy`, never bare unexplained
   `A1+1`.
6. Combat log: while history is non-empty, pin
   `HIT +dmg · sh=shields · int=hull`
   **or** rewrite the event line to
   `HIT 7 · shields 0 · hull 7`. Pick the pin-legend if rewriting
   event strings would break existing tests; then update those
   tests only if you rewrite.
7. After a turn with damage, one summary line:
   `Turn N: hull 95→92 · engines 3→2` (use real deltas you can
   compute from last snapshot vs current; if you cannot do this
   without new protocol, pin the legend only and skip the delta
   line — do not invent hull_max).
8. Movement `w/a/d` stays on the reserved Movement footer so a
   long path cannot hide it.
9. Unknown key: one transient `Unknown key — use the controls
   below` per burst; rate-limit.
10. Clamp log: at most one new line per field while the value
    stays at cap.
11. `main.rs`: if stdin/stdout is not a TTY, print
    `shipsim TUI needs an interactive terminal (a PTY).`
    and exit 1. Do not print the raw `Os { code: 6 }` as the only
    message.

**Tests:**

| Id | Function | Invariant |
|---|---|---|
| **P33** | `playtest_p33_no_midword_status_clip` | 80×24 has no `structure boxe` and no trailing `profile=` without a digit |
| **P34** | `playtest_p34_cta_not_space_p` | No line equals or ends with `Space p` |
| **P35** | `playtest_p35_map_origin_not_ship_coords` | Map title has `View origin` or has no `@(` viewport pair |
| **P36** | `playtest_p36_stack_names_occupants` | Two ships one hex: both callsigns or `enemy` next to `+` |
| **P37** | `playtest_p37_combat_legend_or_plain_words` | After a `HIT` fixture, log panel has `shield` or `sh=` and `hull` or `int=` |
| **P38** | `playtest_p38_movement_keys_survive_long_path` | 4-step path 80×24 still has `w` and `forward` or `veer` |
| **P39** | `playtest_p39_unknown_key_rate_limited` | Five `r` keypresses add ≤2 unknown-key log lines |
| **P40** | `playtest_p40_clamp_notice_rate_limited` | Ten Right at cap add ≤1 new clamp line |
| **P41** | `playtest_p41_no_tty_message` | Unit-test the helper that formats the no-TTY error (do not require a real `/dev/null` run if that is awkward); string contains `PTY` or `interactive terminal` |

**Exit:** P33–P41; map geometry / zoom tests green.

**Commit:** `fix(tui): clarify first-play copy and no-TTY start`

---

## M8 — Close-out

**Goal:** this plan’s catalog is scored; docs point here.

**Files:** this file’s scoreboard (add a **Status** column Pass /
leftover), `docs/TODO.md`, `frontend/tui/README.md` (one
sentence: first-play invariants are `playtest_pNN_`, distinct from
rubric `T#`).

**Work:**

1. Mark X1–X13 Pass or leftover-with-reason. No open Critical or
   Major without a reason that cites a failed test you could not
   write honestly.
2. Do not claim Love/REPL/yard first-play compliance.
3. Leave **M9** as operator-only (below). You do not run it.

**Exit:**

| Check | Pass |
|---|---|
| X1–X9 Pass or explicit leftover | yes |
| `cargo test --manifest-path frontend/tui/Cargo.toml` | green |
| `cargo clippy --manifest-path frontend/tui/Cargo.toml --all-targets -- -D warnings` | green |
| `cargo test --all` | green |
| `cargo clippy --all-targets --all-features -- -D warnings` | green |

**Commit:** `docs(tui): close first-play remediation`

---

## M9 — Fresh blind cohort (operator only — not your job)

Do **not** run this as the implementing agent.

A human or a later playtest pass repeats the original method
(10 new docs-blind agents, 80×24, same launch command) **after**
M8. Gates: 0/10 `ENGINE: order_illegal`; 0/10 trapped in Help;
≥8/10 fund a shield or can say they chose zero; ≥8/10 queue a
legal shot or correctly pass; no tester calls a correct number
“broken” (F1 regression). Wins are recorded, not required.

Fix harness confounds first if you re-use `tmp/haiku/play.py`:
keep colour or declare monochrome; allow session continuation.

---

## Order

```text
M0 index
  ├─ M1 one truth per quantity     ← merge first (stops false bugs)
  └─ M2 illegal fire               ← may code in parallel after M0
       → M3 first Allocate paint   (needs M1 numbers)
         → M4 Help / phase copy
           → M5 DESTROYED vs UNCHARGED + engine cap
             → M6 disabled + game over
               → M7 copy / map / no-TTY
                 → M8 docs close-out
                   → M9 operator playtest
```

## Recommended commits

1. `test(tui): index first-play invariants P01+`
2. `fix(tui): show one consistent value per quantity during allocate`
3. `fix(tui): refuse illegal fire queues`
4. `fix(tui): expose all allocation choices at first paint`
5. `fix(tui): make help and phase transitions recoverable`
6. `fix(tui): explain weapon state and engine loss`
7. `fix(tui): explain and pass disabled ships`
8. `fix(tui): clarify first-play copy and no-TTY start`
9. `docs(tui): close first-play remediation`

Do not combine M1 or M2 with copy cleanup.

## Commands

```bash
cargo test --manifest-path frontend/tui/Cargo.toml
cargo clippy --manifest-path frontend/tui/Cargo.toml --all-targets -- -D warnings
cargo test --all
cargo clippy --all-targets --all-features -- -D warnings
cargo run --manifest-path frontend/tui/Cargo.toml -- scenarios/battle.toml
```
