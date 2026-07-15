# Handoff — TUI tutorial playtest + code review (2026-07-14)

Session ran out of context mid-review wrap-up. Findings are complete and verified;
**no fixes have been applied** — the working tree is exactly the uncommitted diff
on top of `114f41d "Tutorial draft 1 for ratatui"`.

## What this session did

1. **Played `--tutorial` end-to-end** (tmux-driven, 110×40): Won on turn 3 as
   scripted. Then probed edge cases and reported bugs (Esc soft-lock, Tab gate
   leak → enemy-ship orders + lesson desync, invisible shield rows, one-line
   Recent Events, missing below-floor message, unguarded `e`/`q`).
2. **Reviewed the uncommitted diff** that responds to those bugs
   (`frontend/tui/`: app.rs, input.rs, ui.rs, tutorial.rs, tests.rs, README).
   8 finder agents + manual/empirical verification. `cargo test` (68) passes;
   clippy: 3 nits (identical `if` blocks ui.rs:412, `filter().next()`
   main.rs:30, dead `name` field tutorial.rs:50).

## Verified findings (ranked; also filed via review tooling)

1. **CONFIRMED input.rs:132** — `reopen_tutorial_mode` (tutorial Esc) rebuilds
   the draft via `AllocDraft::from_ship`, wiping completed ReachValue values
   without rewinding the lesson. Repro: set Movement=10, Esc → draft shows
   0 cost, step unchanged. Later commit sends movement=0 → Accel steps rejected
   → unfixable lesson desync. Fix: preserve the draft on Esc, or rewind the
   tutorial to the first ReachValue step of the current form / re-seed the
   draft from completed steps' targets.
2. **CONFIRMED ui.rs:429** — `allocate_scroll` counts pre-wrap logical lines,
   but the Paragraph wraps; at 80×24 (declared floor) the 58%-wide tutorial
   panel wraps Power+Movement lines and the shield cursor is off-screen again.
   Repro: 80×24 tmux, tutorial to step 9 (shield F). Fix: compute against
   rendered rows (account for wrap or disable Wrap for the form), and derive
   line-of-cursor from the same code that builds the lines.
3. **CONFIRMED input.rs:526** — `digit_entry` never cleared on Enter commit or
   new snapshot: type 5, commit, next turn type 3 → 53. Clear it in the Enter
   arm and in `update_snapshot`.
4. **CONFIRMED ui.rs:658** — tutorial layout gives Combat Log 42%-width halved
   again with Log (~15 cols, List, no wrap): the new longer event strings
   ("HIT for 4 (shield -2, hull -2) face=F roll=5") truncate at the arrow.
   Give the events panel real width (merge with Log or stack vertically).
5. **CONFIRMED ui.rs:33** — below-floor screen claims "game is paused" but
   input is not gated; keys (e→y etc.) still send orders invisibly. Gate
   handle_key on below-floor (except quit), or change the message.
6. **PLAUSIBLE input.rs:312** — `tutorial_order_pending` armed at keypress,
   pre-confirmation; 'n' cancel leaves it set → any orderless snapshot
   advances the lesson. Set the flag where SendOrder is actually returned.
7. **PLAUSIBLE input.rs:45** — tutorial Esc branch no longer clears
   `last_error`; a stale red ERROR banner persists after recovery.
8. **PLAUSIBLE input.rs:36 (altitude)** — Esc/Tab fixed as special cases but
   global-keys-before-gate ordering remains fail-open; future global keys leak
   into lessons by default. Prefer gate-first, whitelist-per-lesson.

Refuted (do not re-raise): combat_history zip-prefix dedup dropping repeated
lines — engine `combat_log` is append-only within a turn and cleared at
end-turn (src/game_state.rs:239), so the prefix diff is sound.

Cleanup worth batching (from finders, unverified individually but low-risk):
dead identical `if` ui.rs:412 (clippy agrees); `validate_action`/`check_action`
duplication → make check_action call validate_action; `order_backed` matches!
list → `ExpectedAction::is_order_backed()` next to the enum; synthetic
KeyEvent('e') replay in handle_confirmation → store the approved intent
instead; `reopen_tutorial_mode` duplicates handle_normal's Enter branch →
shared `open_phase_form`; per-frame `narration()` markdown stripping and full
Snapshot clone per Esc; layout literals (MIN 80×24, 58/42, Length/Min) belong
in the tier-scoped constants table the PRD requires (PRD.md "Layout constants
are data, not literals"); movement panel "(next accel: N→N-1)" hardcodes the
1-thrust accel cost (README anti-goal: no rules reimplementation); "q swallowed
while a confirmation prompt is open" (minor UX regression vs q-always-quits).

## How to verify fixes

- `cargo test --manifest-path frontend/tui/Cargo.toml` (68 tests; add cases
  for: Esc preserves draft values mid-lesson, scroll keeps cursor visible at
  80×24 tutorial width, digit_entry cleared on commit, pending flag cleared on
  confirmation cancel).
- Live repro: `tmux new-session -d -s s -x 80 -y 24 'cargo run -q --manifest-path
  frontend/tui/Cargo.toml -- --tutorial'`, drive with `tmux send-keys`, read
  with `tmux capture-pane -p`. Full tutorial key path: →×10 ↓ →×4 ↓ → ↓ → ↓
  →×6 Enter, then t/Space cycles, 3, Space, e(+y); see tests.rs
  `tutorial_rear_attack_wins_against_engine` for the canonical sequence.
- Playtest UX report (tutorial improvements + controls critique) was delivered
  in-conversation on 2026-07-14; the actionable parts overlap the findings
  above plus: no Tab-cycled Ship Detail/Combat Log/Help panels at game over,
  56-step counter should group by turn, `**` markdown now stripped (fixed in
  this diff), weapon ordering differs between allocate (alphabetical) and fire
  (snapshot order) panels.
