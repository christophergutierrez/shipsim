# Bugfix Plan — Playtest findings (2026-07-13/14)

This plan fixes bugs found by 10 parallel automated playtest sessions (logs in
`tmp/playlogs/results_20260713_*.log` and `tmp/playlogs/results_20260713_181012_9194778.log`).
Every bug below was independently verified by re-reading the actual source and,
where possible, reproducing the failure — this is not just trusting the tester
reports.

**Audience for this document:** written so each phase can be executed by itself,
with exact commands to run and exact pass/fail criteria. Do not skip the
"Verify" step of a phase before moving to the next phase. If a "Verify" step
fails, stop and fix it before continuing — do not proceed to the next phase
with a known-broken build.

## Ground rules for every phase

1. Work on the `master` branch unless told otherwise. Check `git status` before
   starting each phase — if there are unexpected uncommitted changes from a
   previous session, stop and ask before touching anything.
2. After every code change, run the full test suite (both the Rust tests and
   the Python REPL tests) before moving on:
   ```
   cd /mnt/storage/git_home/shipsim
   cargo build -q && cargo test -q
   python3 -m unittest discover -s tests -q
   ```
   All of these must exit 0 / show no failures before you commit anything.
3. Commit each phase separately (do not squash phases together), with a
   commit message describing what was fixed and why. Do not use
   `git commit --amend`. Do not force-push.
4. Do not fix things that are NOT listed in this plan, even if you notice them
   along the way — note them in a comment at the bottom of this file under
   "Noticed but out of scope" instead, so they can be triaged separately.

---

## Phase 0 — Land the already-written `commands` alias fix

**What:** A previous session already edited `frontend/repl/commands.py` to
recognize `commands` as an alias for `help`. This fix is correct and tested;
it just needs to be committed.

**Steps:**
1. Run `git diff frontend/repl/commands.py` and confirm it shows exactly two
   changes:
   - In the alias table (near line 86): adds a line `"commands": "help",`
   - Near line 1443: changes
     `if tokens and tokens[0].lower() in ("help", "?", "h"):`
     to
     `if tokens and tokens[0].lower() in ("help", "?", "h", "commands"):`
   If the diff shows anything else, stop and ask before proceeding — do not
   commit unexpected changes.
2. Run the Ground Rules test suite (see above). All tests must pass.
3. Commit:
   ```
   git add frontend/repl/commands.py
   git commit -m "repl: recognize 'commands' as an alias for help"
   ```

**Verify (must pass before moving to Phase 1):**
```
cd /mnt/storage/git_home/shipsim
printf 'commands\nquit\n' | python3 frontend/repl/repl.py scenarios/combat.toml
```
Confirm the output shows the help text (not "Did you mean 'coast'?" or similar
error).

---

## Phase 1 — Isolated correctness bugs

Four independent bugs. Do them in this order; each one is self-contained and
touches different files, so a failure in one does not block the others.

### 1a. `v2_duel.toml` (and similar no-player scenarios) burn 8192 engine
steps and land at a nonsensical turn number instead of erroring cleanly

**Root cause:** `src/game_state.rs`, function `resolve_v2_npc_actions`
(starts at line 1006). It loops up to `STEP_CAP = 8192` times, auto-playing
every AI-controlled ship's turn. It only breaks out of the loop early when a
*living* ship still needs a human order (see the `None => break` arms for the
`Allocate`/`Movement`/`Firing` phases, and the `v2_has_living_human()` check
at line 1087 for the `TurnEnd` phase). If a scenario has **no player-controlled
ship at all** (every ship's controller is `"ai"`), none of those breaks ever
fire, and the loop runs the full 8192 steps before returning control —
producing whatever turn number the step cap happens to land on (observed:
turn 432 for `scenarios/v2_duel.toml`).

This function is called from `src/bin/shipsim.rs` at lines 49, 66, 84, and 277
— every one of these calls happens immediately after loading a scenario/save,
*before* the first snapshot is emitted to the client. So a human launching the
REPL against a no-player scenario never sees anything until the engine has
already fast-forwarded 432 turns.

Confirmed: `scenarios/v2_duel.toml` is documented in its own file comments as
built for **headless AI-vs-AI self-play** (referenced by the v2 NPC driver
test harness), not for interactive/REPL play. But `docs/AGENT-PLAY.md` line 65
lists `scenarios/v2_duel.toml` as a playable scenario option — that's a doc
bug that needs fixing too (see 1a-iii below).

**Fix — three parts:**

**(i) Add a `has_player_ship()` guard in `src/game_state.rs`.**
Near `v2_has_living_human` (line 995), add a sibling method:
```rust
/// True if this scenario has at least one ship that is not AI-controlled,
/// living or not. Used to detect scenarios that were authored for headless
/// self-play and should not be driven interactively.
pub fn has_player_ship(&self) -> bool {
    self.ships.iter().any(|ship| !self.is_v2_ai(ship.id))
}
```
(Match the exact style/visibility of the surrounding code — read
`v2_has_living_human` just above it for the pattern to copy. `is_v2_ai` is
already used elsewhere in this file, so it exists — grep for it if the exact
signature isn't obvious from context.)

**(ii) Refuse (don't silently fast-forward) when launching interactively
against a no-player scenario, in `src/bin/shipsim.rs`.**
At line 45-51 (the `Mode::Scenario(path) => { ... }` arm), before calling
`game.resolve_v2_npc_actions()`, add a check:
```rust
Mode::Scenario(path) => {
    let game = load_scenario(&path).map_err(|e| e.to_string())?;
    if !game.has_player_ship() {
        return Err(format!(
            "scenario {path:?} has no player-controlled ship; it is meant for \
             headless self-play, not interactive play. Use a scenario with a \
             \"player\" controller, or drive this one through the v2 self-play \
             harness instead."
        ));
    }
    let mut game = game;
    // ... existing code continues unchanged (resolve_v2_npc_actions, emit_snapshot, etc.)
```
Note `load_scenario` returns the game as `let mut game` in the existing code —
you'll need to restructure slightly since you now need an immutable borrow
for the check before making it mutable. The simplest fix: keep `let mut game`
as before, just insert the `if !game.has_player_ship() { return Err(...) }`
check as a new line immediately after the `load_scenario` line and before the
`resolve_v2_npc_actions()` call. `has_player_ship` only needs `&self` so this
works fine on a `mut` binding.

Do **not** add this check to `Mode::Campaign` or `Mode::Resume` — campaigns
and saves are a different, already-validated flow; only guard the raw
`--scenario` entry point that a human might point directly at a
self-play-only file.

**(iii) Fix the doc.** In `docs/AGENT-PLAY.md`, find the line listing
`scenarios/v2_duel.toml` (around line 65, currently described as "Cleaner duel
sibling"). Either remove that row from the playable-scenarios table, or
relabel it clearly as "headless self-play only — not for interactive/REPL
play" so nobody points the REPL at it again.

**New test to add** (put it near existing scenario-loading tests — grep
`src/bin/shipsim.rs` and any `tests/` directory for how existing scenario
load failures are tested, and match that pattern):
- Launching `--scenario scenarios/v2_duel.toml --stdin` with empty stdin must
  exit non-zero and print an error mentioning "no player-controlled ship" (or
  whatever exact wording you used) — NOT silently succeed at turn 432.

**Verify:**
```
cd /mnt/storage/git_home/shipsim
cargo build -q
echo "" | ./target/debug/shipsim --scenario scenarios/v2_duel.toml --stdin
echo "exit code: $?"
```
Must print a clear error and exit non-zero. Must NOT print a snapshot showing
`"turn":432` or similar.

---

### 1b. Concurrent REPL sessions can write into the same session log file

**Root cause:** `frontend/repl/screen.py`, function `default_session_path`
(lines 272-275):
```python
def default_session_path() -> Path:
    """Gitignored session transcript under this client tree."""
    stamp = time.strftime("%Y%m%d-%H%M%S")
    return _LOCAL / f"session-{stamp}.log"
```
This has 1-second resolution and no process-uniqueness. Two REPL processes
launched in the same wall-clock second get the *same* log filename, and
`screen.py` line 55 opens it in append mode (`open(session_path, "a", ...)`)
with no exclusivity check — so both processes' output interleaves into one
file. This was confirmed directly: a real session log
(`frontend/repl/local/session-20260713-220426.log`) was found containing six
different scenarios' banners interleaved from a batch of parallel playtest
runs. This is what caused several playtest agents to mistakenly report "wrong
scenario loaded" — they were reading a log file another concurrent process
had also written into, not a real scenario-loading bug.

**Fix:** In `frontend/repl/screen.py`, change `default_session_path` to
include the process ID so concurrent launches never collide:
```python
import os  # add to the top-level imports if not already imported — check first

def default_session_path() -> Path:
    """Gitignored session transcript under this client tree."""
    stamp = time.strftime("%Y%m%d-%H%M%S")
    return _LOCAL / f"session-{stamp}-{os.getpid()}.log"
```
Check the top of `screen.py` for an existing `import os` before adding a
duplicate import.

**New test to add:** in the REPL test suite (find the existing test file that
covers `screen.py` — search `tests/` for `default_session_path` or
`screen.py` references and match the existing test style), add a test that
calls `default_session_path()` twice in a tight loop (simulating same-second
calls) and asserts the two returned paths are different when `os.getpid()` is
mocked to be the same is not necessary — just assert the filename contains
`str(os.getpid())`.

**Verify:**
```
cd /mnt/storage/git_home/shipsim
python3 -c "from frontend.repl.screen import default_session_path; import os; p = default_session_path(); print(p); assert str(os.getpid()) in p.name, 'pid missing from session path'"
```
Must print a path and not raise.

---

### 1c. `end`/`e` discard-warning may mislabel the target ship

**Root cause:** `frontend/repl/commands.py` lines 1778-1796, specifically the
`queued` string built at lines 1788-1791:
```python
queued = ", ".join(
    f"{s.get('weapon')}→#{s.get('target')}" for s in pending_shots
)
```
`pending_shots` comes from `snap.get("fire_commits")` (line 1783) — this is
server-side snapshot data from the Rust engine, not something the REPL
constructs itself. One playtest log reported this warning showing
`beam_1→#1` (the player's own ship) when the shot was actually queued against
the enemy (`#2`).

**This needs a repro before you touch any code — do not guess-fix it.**

Steps:
1. Reproduce a firing-phase state with a queued shot against the enemy, then
   trigger the `end`/`e` discard warning, and print the raw
   `snap["fire_commits"]` JSON to see exactly what `target` field contains.
   A quick way: add a temporary `print(pending_shots)` right before line 1788,
   run a scripted game (see `tmp/playlogs/tester_prompt.md` or any of the
   `results_2026*.log` "REPRODUCTION" sections for a working command
   sequence), trigger the warning, read the printed raw data, then remove the
   temporary print.
2. If `s.get('target')` in the raw snapshot data already correctly shows the
   enemy's id (e.g. `2`), then the bug is NOT in this display code — it's
   either in how the Rust engine records `fire_commits.target`, or the
   original tester misread the output (own-ship id and target id can look
   similar in a two-ship duel where ids are `1` and `2`). In that case, do
   not change any code. Write up what you found as a comment at the bottom of
   this file under "Investigated, not reproduced" and stop.
3. If the raw data itself is wrong (shows `1` when it should show `2`), the
   bug is upstream in the Rust engine's fire-commit recording, not in this
   REPL display code — trace where `fire_commits` entries are created
   (grep `src/` for `fire_commits` and `FireCommit`) and find where `target`
   is set. Fix it there, not in the REPL.

**Verify:** whatever you find, the verification is: reproduce the same
scripted scenario end-to-end and confirm the discard warning's target id
matches the id you actually issued `fire` against.

---

### 1d. `fire <weapon> <target>` one-liner sometimes drops into an
interactive weapon-selection submenu instead of firing directly

**Root cause (partially isolated, needs a focused repro session):** the
one-liner fire command is supposed to fire immediately without prompting.
Reproduced live: sometimes it instead prints
`Enter weapon number (0), weapon name (b1), or -1 when done:` and consumes
the *next* piped line as the answer to that prompt — which desyncs the rest
of a scripted command sequence (the following `ready` or `fire` commands get
silently eaten as menu answers instead of running). This most likely explains
a separate playtest report of "torpedo never fires" — direct testing showed
torpedo firing itself works fine (`fire t1 2` + `ready` correctly fires and
resolves), so the bug is specifically the one-liner-vs-menu inconsistency,
not torpedo weapons.

**This needs investigation before a fix — do this as a spike:**
1. Find the fire-command dispatch logic in `frontend/repl/commands.py` (grep
   for `interactive_fire` and for where a bare `fire <weapon> <target>`
   one-liner is parsed — it's referenced from around `frontend/repl/repl.py`
   line 220, but the actual submenu logic lives in `commands.py`).
2. Read the code path that decides "one-liner has both weapon and target
   tokens → fire immediately" vs. "fall through to interactive menu". Find
   the condition under which a syntactically-complete one-liner
   (`fire b1 2`) still falls through to the menu. Likely candidates: a phase
   check that doesn't match every valid firing state, or a weapon-name vs.
   weapon-number ambiguity check that rejects a valid one-liner under some
   condition.
3. Write a minimal repro script (a short sequence of piped commands) that
   deterministically reproduces the menu fallback, and add it as a regression
   test in the REPL test suite before fixing anything, so the fix can be
   verified.
4. Fix the dispatch so a syntactically valid `fire <weapon> <target>`
   one-liner never falls through to the interactive menu, regardless of
   phase/queue state, as long as the weapon and target are both resolvable.

**Verify:**
```
cd /mnt/storage/git_home/shipsim
printf 'allocate\nw b1 4\ncommit\nfire b1 2\nready\ne\nyes\n' | python3 frontend/repl/repl.py scenarios/combat.toml
```
(Adjust the exact command sequence to match a real scenario's weapon/target
ids if `b1`/`2` don't apply — check `scenarios/combat.toml` first.) The
output must NOT show `Enter weapon number...` — it must fire directly.

**Phase 1 Verify (all of 1a-1d done):**
Run the Ground Rules test suite. All green. Commit each of 1a, 1b, 1c
(if code changed), 1d separately.

---

## Phase 2 — Missing-feedback cluster (combat visibility)

This is the biggest and most player-visible issue, so it gets its own phase
and should not be rushed. Four related reports all stem from the same root
cause: important events (enemy fire results, weapon destruction, bridge-kill
cause, power-pool halving) are recorded correctly in the game's event/Δ data,
but the REPL's screen-repaint model only shows the *current turn's* compact Δ
summary line, which is truncated, and does not persist the fuller
"FIRE RESOLUTION" style event block across repaints. It only accidentally
worked at game-over because the game-over screen renders once and stays
static (no further repaint clears it).

**Confirmed:** `frontend/repl/repl.py` line 333
(`ui.log(format_combat_events(events, msg, hull_max=ctx.hull_max))`) already
receives and formats the *complete* combat event data — including the
enemy's shots — via `format_combat_events` in `frontend/repl/view.py` (defined
at line 737, builds the `"*** FIRE RESOLUTION ***"` block at line 793). The
data is correct; it's just not retained on screen after the next repaint via
`paint_frame` (`frontend/repl/repl.py` line 139).

Separately, `frontend/repl/view.py` around lines 311-324 renders a
"shots resolved this turn" block that is intentionally scoped to the
currently-focused ship's own weapons (keyed by weapon id) — it will never
show the enemy's shots by design. That's not itself the bug; the bug is that
there's no *other* place in the live UI that shows the enemy's fire.

### Steps

1. **Investigate first — do not write code yet.** Read `paint_frame` in
   `frontend/repl/repl.py` (starts line 139) end-to-end and understand how it
   decides what to draw each repaint, and what happens to anything passed to
   `ui.log(...)` between repaints (does it get cleared, scrolled past, or
   overwritten?).
2. **Design a "recent events" panel** that persists across repaints for at
   least the most recent turn/cycle — something the player can always see,
   not just something that flashes by in `ui.log`. This does not need to be
   fancy: a fixed-height panel showing the last N formatted event lines
   (reusing the existing `format_combat_events` output, not reinventing the
   formatting) is enough. Look at how the existing "RECENT" Δ panel is
   rendered (search `view.py` for the panel that currently shows the
   truncated Δ lines mentioned in the playtest reports) — the new panel
   should sit alongside or replace that one, showing the *untruncated*
   FIRE RESOLUTION-style text instead of the compact Δ line, with wrapping
   instead of truncation.
3. Within the same panel/mechanism, ensure these three specific pieces of
   information always surface in the live UI (not just at game-over or via
   error messages):
   - Weapon destroyed (currently silent — the player only sees a status label
     next render, or hits an error trying to use it)
   - Ship destroyed by bridge=0 specifically (currently the death screen says
     "hull took N" without saying the bridge hit was the kill condition —
     make the death message explicit, e.g. "bridge destroyed — ship lost"
     when that's the actual cause, distinct from a hull=0 kill)
   - Power pool change from system damage (currently only visible in the
     truncated Δ line — e.g. "usable power halved: power_sys damaged")
4. Do not change the underlying event/combat-log data model in the Rust
   engine for this phase — the data is already correct and complete; this is
   purely a REPL-side rendering/retention fix.

**New tests to add:** in the REPL test suite, script a full turn where the
enemy fires and hits the player (reuse a working scripted sequence from one
of the playtest logs' "REPRODUCTION" sections), capture the rendered output
across the repaint that follows the enemy's shot, and assert the rendered
text contains the enemy's weapon name and hit/miss result — not just the
player's own shots. Add a second test for weapon-destruction wording and a
third for bridge-kill wording.

**Verify:**
```
cd /mnt/storage/git_home/shipsim
cargo test -q
python3 -m unittest discover -s tests -q
```
Then manually replay one of the original bug scenarios end-to-end (e.g. the
sequence from `tmp/playlogs/results_20260713_181012_9194778.log`'s
"REPRODUCTION" section) and confirm by eye that enemy fire, weapon
destruction, and bridge-kill cause are all visible during play, not just
after the game ends.

---

## Phase 3 — UX/cosmetic cleanup

Lower priority; do this after Phases 0-2 are verified and committed.

1. **Reduce full-panel repaint noise.** Currently `paint_frame` appears to
   redraw the whole ~80-line frame on every input, even rejected/no-op input,
   which was reported as burying real messages (a tester missed an actual
   printed error — "not enough free power for shield..." in
   `frontend/repl/commands.py` around line 641-657 — because of repaint
   volume). Investigate whether a no-op/rejected command can skip the full
   repaint and just print its message inline instead. This is a judgment call
   on scope — a full diff-only-repaint rewrite is out of scope for this
   phase; just stop repainting the full frame on commands that didn't change
   any game state.
2. **Cosmetic fixes** (each independent, do in any order):
   - `frontend/repl/view.py` line ~776: a MISS result still prints
     `on shield {face}:{lab}` even though no shield was actually involved in
     a miss. Only print the shield-face clause when the shot was a HIT.
   - Map rendering: when ships are far apart on the same row, the row
     sometimes wraps/splits across multiple visual lines. Find the map
     rendering function in `view.py` and check for a fixed-width assumption
     that doesn't account for wide grids.
   - Map legend: the `@` prefix shown on a focused ship's card (e.g.
     `@A1 #1 ...`) is not explained anywhere in the legend text. Add a line
     to the legend, e.g. `@ = currently focused ship`.
3. **Doc fix**: confirm `docs/AGENT-PLAY.md`'s `v2_duel.toml` row was already
   fixed in Phase 1a-iii; if this phase is being done separately, do it now
   instead.

**Verify:** Ground Rules test suite green. Manually eyeball a scripted
playthrough before/after to confirm readability actually improved — this is
a subjective UX check, use judgment.

---

## Phase 4 — Needs-repro item (do last, may be closed without a code change)

**Ship class/size flip on first render**: one playtest report (not
cross-corroborated by any other) claims a single enemy contact rendered as
"Escort / size=1" on the opening frame and then as "Heavy Cruiser / size=2"
on a later frame, for what should be the same ship id.

1. Try to reproduce this directly using the exact scenario and command
   sequence from that report (`results_20260713_220423_429.log`, Bug #1 —
   read the full repro steps from that file).
2. If reproduced: find where the pre-allocate snapshot and post-commit
   snapshot each source ship metadata (class/size) and check for a mismatch
   — likely one path reads from scenario-file defaults and the other from
   live game state, and they can disagree if, e.g., ship ids are reused or
   misassigned.
3. If NOT reproduced after a reasonable attempt (a couple of tries with the
   documented repro steps): close this as "could not reproduce", write a note
   under "Investigated, not reproduced" at the bottom of this file, and do
   not guess-fix it.

---

## Explicitly NOT bugs — do not "fix" these

- **"Wrong scenario loaded" reports from 6 of 8 playtest logs.** This was
  investigated and is a test-harness artifact of Phase 1b (the session-log
  filename collision), not a real scenario-loading bug. Fixing 1b is the
  correct and complete fix for this; do not add any additional
  scenario-loading logic on top of it.
- **`sh <face> <n>` "silently ignored" when there isn't enough free
  power.** `frontend/repl/commands.py` lines 641-657 (`AllocDraft.set_shield`)
  already prints a clear error message
  (`"not enough free power for shield {face}..."`) and returns `False` — this
  was reproduced and confirmed working as intended. The tester missed the
  message due to repaint volume, which Phase 3 step 1 addresses generally.
  Do not add a second/duplicate error message for this specific case.

---

## Noticed but out of scope

*(Fill in here if you notice something while working through this plan that
is real but not covered above — do not fix it inline, just note it here for
follow-up triage.)*

## Investigated, not reproduced

*(Fill in here for Phase 1c / Phase 4 if you could not reproduce the reported
behavior — include what you tried.)*

### Phase 1c — `end`/`e` discard-warning target mislabel — NOT reproduced

**Reported symptom:** the `end`/`e` discard warning showed `beam_1→#1` (the
player's own ship) when the shot was actually queued against the enemy (`#2`).

**What I did:**
1. Read `frontend/repl/commands.py` lines 1778-1796. The warning's `queued`
   string is built at lines 1788-1791 from `pending_shots` (=
   `snap.get("fire_commits")`), reading `s.get('weapon')` and
   `s.get('target')` directly off the server snapshot.
2. The plan's suggested repro (`allocate / w b1 4 / commit / fire b1 2 / end
   / yes`) does NOT reach the firing phase — after `commit` the phase advances
   to `movement`, so `fire b1 2` is rejected with "fire/attack is available
   during firing only (now movement)" and no shot is ever queued. I corrected
   the sequence to actually reach firing: `allocate / w b1 4 / commit / coast
   / fire b1 2 / end / yes` (the `coast` finishes the movement phase and
   advances to firing).
3. Added a temporary `print(pending_shots)` immediately before line 1788, ran
   the corrected scripted game, captured the raw snapshot data, then removed
   the temporary print.

**Raw `fire_commits` data captured (player ship #1, enemy ship #2, fired
`fire b1 2`):**
```
[{'ship': 1, 'weapon': 'beam_1', 'target': 2, 'shield_facing': 0}]
```

**Rendered discard warning:**
```
warning: queued shot(s) not yet resolved will be DISCARDED, not fired: beam_1→#2. Use ready/nofire to resolve them first.
```

**Conclusion:** The raw `fire_commits.target` from the Rust engine is `2`
(correct — the enemy), and the REPL display code renders it as `beam_1→#2`
(correct). The bug is NOT in this display code and NOT in the engine's
fire-commit recording. No code changed for 1c. The original tester's
`beam_1→#1` report was most likely a misread of the output (the player ship
`#1` appears prominently elsewhere in the frame) or occurred against a
different/earlier code state. Closing 1c as not reproduced.
