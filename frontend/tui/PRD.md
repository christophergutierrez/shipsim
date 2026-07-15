# PRD — shipsim TUI (ratatui), Small tier

Source: grilling session 2026-07-14 (`killhouse:grill-with-docs`), synthesized against
ADR-0021 (`docs/adr/0021-ratatui-tui-client.md`), ADR-0023
(`docs/adr/0023-tui-input-layout-and-verification.md`), `frontend/tui/README.md`,
`frontend/tui/ADR.md`, `docs/PROTOCOL.md`, and `frontend/repl/GAMEPLAY.md`.

This PRD does not re-litigate anything already decided in the ADRs above — it
operationalizes them into a build-ready spec for the **Small tier only**. Medium
and Large tiers, and their exact breakpoints, are out of scope (see below).

## Problem Statement

Playing shipsim from the terminal today means using `frontend/repl/`, a
line-oriented Python client that clears and redraws a stack of text panels on
every step. On a short terminal, panels below the fold (most critically the
map — the player's only view of ship positions) get pushed above the visible
rows. Because play mode uses the terminal's alternate screen buffer, there is
no scrollback to recover them, and the conventional way to try to scroll back
(the Up arrow) is instead captured by the input line's command history, not
the terminal. The player is left unable to see the board they're fighting on
unless their terminal window happens to be tall enough — and it rarely is. A
measured playthrough of `scenarios/ai.toml` (28 redraws, banner through
footer) produces frames of **71 to 106 lines, median 93** — roughly 4x the
height of a classic 24-row terminal, and taller than most maximized ones. The
map is not marginally off-screen; it is off-screen by default.

Recent REPL-side patches (fixed-height-aware redraw, dropping low-priority
panels with a note) reduced how badly this bites, but they're mitigations on
top of a client that was never designed to be layout- or size-aware — the
REPL prints text; it does not lay out a screen.

There is a second, co-equal failure, and it is not the player's: **what the
player sees is not machine-readable.** The REPL's rendered output is styled
text, so an agent or CI job that wants to verify "does the map actually show
A1 here, does the fire list actually say MISS" has to strip ANSI escapes
(`_ANSI_RE` in `frontend/repl/style.py`) and regex a byte stream back into
meaning. That makes the player-visible surface the *least* verifiable part of
the system, which is backwards. Both drivers — a map that survives a small
terminal, and a rendered surface an agent can assert on directly — are
reasons this project exists; neither is polish on top of the other.

## Solution

Build a real terminal UI client (`frontend/tui/`) using ratatui, which manages
the terminal as a proper 2D grid rather than a stream of printed lines. It
talks to the exact same `shipsim` engine subprocess over the same NDJSON
protocol the REPL already uses (no engine or rules changes). ratatui also
answers the second driver directly: its `TestBackend` hands tests a structured
grid of cells, so "what the player sees" is asserted on as data, not recovered
from a styled byte stream.

**The TUI is an additional client, not a replacement.** `frontend/repl/` stays
supported and unchanged (`frontend/tui/README.md` keeps it as "the
non-fullscreen regression path for protocol play"), and it remains the
fallback for anything Small doesn't cover — the guided tutorial, to-hit
previews. Deprecating the REPL is a separate, later decision, deliberately not
made here. This is what bounds the accepted downgrades in Out of Scope: they
are gaps in *this client*, not capabilities the project loses.

The first deliverable is the **Small tier**: a complete, playable game
(allocate → move → fire → end turn, repeatable to a scenario's conclusion) in
the most constrained on-screen layout, proving the core interaction model —
keypress navigation with a typed-command fallback, and a layout that keeps the
map permanently visible — before any larger/denser tier is attempted.

## User Stories

1. As a player on a small terminal, I want the map to always stay visible, so that I never lose spatial awareness of where my ship and the enemy are.
2. As a player, I want to select my ship, a weapon, or a target using arrow keys or `j`/`k` without typing anything, so that routine actions are fast.
3. As a player, I want number keys `1`-`9` to jump straight to a list item, so that I don't have to arrow through a long list.
4. As a player, I want a typed command line (opened with `:`) as a fallback, so that anything without a dedicated key is still reachable, and so the interaction feels familiar if I already know the REPL's vocabulary.
5. As a player, I want to allocate power to engine, weapons, and shields as a local draft that only takes effect when I commit, so that I don't accidentally waste power with a stray keypress (matches the REPL's existing "local draft until commit" behavior; see `docs/PROTOCOL.md` `allocate`).
6. As a player, I want to issue a movement maneuver (`coast`, `accel`, or `turn` to a facing) for my ship each movement cycle, so that I can move as the current rules allow.
7. As a player, I want to select a weapon, a target, and (when applicable) a shield facing, and queue a shot, so that I can fire during the firing phase.
8. As a player, I want to see HIT/MISS and damage results for shots — mine and the enemy's — immediately and persistently in the UI, so that I'm never surprised by damage I didn't see happen (this was the REPL's own recently-fixed "enemy fire invisible" bug; the TUI must not regress it).
9. As a player, I want to ready/pass on firing when I have no more shots to queue, so that the phase can advance.
10. As a player, I want to end the turn when appropriate, with a confirmation if it would discard an unresolved queued shot, so that I don't lose a shot by accident (matches REPL's existing `end`/`e` discard warning).
11. As a player, I want to see ship detail (hull, shields per facing, weapon charge/status) in a secondary panel I can switch to with `Tab`, so that I have the full picture without it crowding out the map.
12. As a player, I want to see a combat log in a secondary panel, so that I can review what happened over the last several events, not just the most recent one.
13. As a player, I want a help panel reachable the same way (`Tab`-cycled), so that I can look up key bindings without leaving the game.
14. As a player, I want to quit cleanly (`q`), with a confirmation if it would discard unresolved state, so that I don't lose in-progress input by accident.
15. As a player on a terminal that's currently too small to play (below the Small-tier floor), I want a clear "resize to at least W×H" message instead of a broken or unreadable screen, so that I understand what to do.
16. As a player who resizes my terminal back above the floor mid-session, I want the game to resume automatically without restarting, so that a temporary window resize doesn't cost me my game.
17. As a player, I want soft-rejected orders (illegal moves, insufficient power, etc.) to show a clear, immediate message, so that I understand why my input didn't take effect, without the UI inventing its own legality rules.
18. As a player, I want the same callsign/side vocabulary as the REPL (`A#` player, `B#` ai, `C#` scripted) and the same facing convention (0..5, board-aligned arrows), so that switching between REPL and TUI doesn't require relearning the game's language.
19. As an agent/CI process, I want every keypress-to-rendered-output interaction to be verifiable without a real terminal attached, so that the TUI's behavior can be checked the same way the REPL's is today.
20. As a developer extending this later to Medium/Large tiers, I want the Small tier's rendering and input-handling logic decoupled from tier-specific layout constants, so that adding a tier doesn't require rewriting the interaction model.
21. As a player allocating power, I want to adjust each field's value with left/right keys (or type the number directly) and see the free-power total update live, so that filling out the turn's power plan takes seconds, not a chain of menus — this is the single most-repeated interaction in a game.
22. As a player, I want the status bar to always show which movement/firing cycle (1-4) of the turn I'm in and whether I have queued-but-unresolved shots, so that I never lose my place in the turn's inner loop or end a turn that would discard a shot.
23. As a player, I want the game to tell me clearly when I've won or lost the scenario, with the map still on screen, so that the playable loop actually ends instead of silently accepting orders forever.
24. As a player on a terminal larger than the Small-tier floor, I want the layout to use the extra space (a bigger map) rather than sitting in a tiny fixed box, so that the common case — a normal-sized terminal window — is the good case, not an unhandled one.

## Implementation Decisions

### Scope boundary
- Small tier only. No Medium/Large layout, no tier-detection/switching logic beyond the single floor check (see "Below-floor behavior" below). `dimM`/`dimL` are not defined by this PRD.
- **Small is therefore the *only* layout in this deliverable, and it is what renders at every terminal size at or above `dim0` — there is no upper gate.** A terminal comfortably larger than the floor is the common case, not an edge case: above the floor, the layout absorbs the extra space rather than refusing to use it. The map region takes the slack (it is the thing the player most wants more of); the secondary region keeps its minimum height and grows only modestly. `dim0` (the Small-tier floor — an undetermined value, derived by measurement during the build; see Assumptions) is the floor at which this single layout stays readable, **not** the size the layout is pinned to.
- Follows the build order already fixed by ADR-0023 and `frontend/tui/README.md` — **six slices, not five**, each playable/runnable on its own, all targeting Small only. The last slice is load-bearing for this PRD and must not be dropped as "polish":
  1. **Skeleton** — spawn the harness, read the post-load snapshot, draw turn/phase, `q` quits.
  2. **Map + status** — the map region and the status bar. This is the first slice that has a size to check, so it is where `dim0` gets *measured* and where both size behaviors land: the below-floor message (stories 15, 16) and the above-floor slack rule (story 24). It is also where the mandatory status-bar fields are validated against the floor width (see Layout).
  3. **Allocate** — the draft form and the `allocate` order.
  4. **Movement** — the maneuver pick.
  5. **Fire** — weapon/target/facing, ready, combat log, HIT/MISS. This is the first slice in which the loop can actually terminate, so **end-of-scenario handling (`StateSnapshot.status`, WON/LOST — story 23) lands here**, not later.
  6. **Polish** — the Help panel and `Tab` cycling of the reference panels (story 13), plus resize hardening.
  **The first deliverable ships at slice 6, not slice 5 — owner-confirmed.** Slice 5 is the last slice that adds a *rule* to the loop; slice 6 is what makes the result a complete, playable game as this PRD defines one. "Each slice is playable on its own" and "the deliverable is a complete, playable game" are not in tension — they just point at different milestones, and the release is the later one. This is the release boundary any schedule should be drawn against.

### Package
- New standalone Cargo package under `frontend/tui/` (own `Cargo.toml`; ratatui + crossterm + serde_json), per ADR-0021/ADR-0023. Not a member of the root workspace. `shipsim_core` has zero dependency on this package in either direction.
- No rules logic in this package. All legality, hit resolution, and damage computation stays server-side; the TUI only renders `StateSnapshot` and issues orders.

### Process integration (protocol boundary)
- Spawn `shipsim --scenario <path> --stdin` (or `SHIPSIM_BIN` override), matching the REPL/Love integration pattern exactly (`docs/PROTOCOL.md`, `frontend/tui/README.md`).
- Read the post-load snapshot before any input; after every order write, read the next snapshot or soft error line.
- The harness runs `resolve_v2_npc_actions` automatically after each accepted order — the TUI must not invent AI/NPC orders itself.
- Snapshot and order shapes are exactly as defined in `docs/PROTOCOL.md` / `src/snapshot.rs` (`StateSnapshot`, `ShipSnapshot`, `WeaponSnapshot`, `CombatLogEntry`, `FireCommit`) and `src/movement.rs` (`allocate`, `commit_maneuver`, `commit_fire`, `ready_fire`, `end_turn`). No new order or snapshot fields are needed for the Small-tier scope; if a future tier needs a field the protocol doesn't expose, that requires an additive protocol change, not client-side inference.

### Application structure (the seam)
- One `App` type owns: the current `StateSnapshot` (or `None` before first load), local UI state (focused ship, current list selection/cursor, allocate draft, active secondary tab, dialog/mode state, below-floor flag), and nothing else.
- `App` exposes two pure(-ish) entry points that form the primary test seam:
  - a key-handling function: `(KeyEvent, &App state) -> (new App state, Option<Order>)` — returns at most one order to send when a key press completes an action (e.g. `Enter` on a fire-target selection).
  - a render function: `(&App state, &mut ratatui Frame) -> ()` — pure function of state to widgets; immediate-mode, rebuilt from the latest snapshot every frame per ADR-0021's stack notes (no cached "display HP" divorced from the snapshot).
- The subprocess I/O loop described under Process integration, plus the translation of terminal key/resize events into calls into `App`, is a thin adapter around this seam — and it is the only part of the system that isn't exercised by `TestBackend`-driven tests (see Testing Decisions).
- **Terminal events must not be gated on an in-flight order.** Waiting on the engine's reply to an order and reacting to a keypress or a resize are independent concerns; a naive single sequential loop (write order → block on read → *then* look at input) cannot repaint a below-floor message while a read is outstanding, which would quietly falsify the "resumes the instant the terminal grows back" promise below. The adapter must therefore handle terminal input/resize independently of the subprocess read (a separate input thread, a poll/select loop — the mechanism is an implementation choice; the *property* is a requirement). Engine round-trips are fast, so this is a small window in practice, but "small" is not the same as "specified."
- **Layout constants are data, not literals scattered through render logic.** Every Small-tier dimension the layout depends on — `dim0`, region sizes/minimums, map viewport and column widths — is grouped into a single tier-scoped set of constants that the render function *reads*, rather than being inlined at its use sites. The key-handling function does not read them at all: the interaction model is defined over the snapshot and the `App`'s selection state, never over screen geometry. This is what makes story 20 checkable rather than accidental — adding a Medium/Large tier should mean supplying a second set of constants, not editing the render or key-handling logic. (Which module they live in is an implementation detail; that they are separable from the interaction model is a product requirement.)
- This mirrors the REPL's existing split (`commands.py`/`view.py` are pure functions tested directly; `repl.py` wires them to a live session) — same shape, different language.

### Input model
- Keypress-primary, per ADR-0023:
  - `↑`/`↓` and `j`/`k` move the selection cursor within whatever list is active (ship, weapon, target, allocation field).
  - `←`/`→` and `h`/`l` **decrement/increment the value of the selected numeric field** (engine power, a weapon's desired charge, a shield face's power). Typing a digit sequence sets the field directly (e.g. selecting the engine row and typing `8` sets engine=8). Value-setting is the game's most-repeated interaction, and it earns the other half of the vim/arrow cluster rather than a submenu. The reason is structural, not statistical: **every ship must be re-allocated every turn**, across up to three field groups (movement, each weapon, six shield faces), so the volume of value-adjustment keystrokes recurs regardless of scenario. The one quantified transcript available is consistent with that — of the 86 steps in the rear-attack tutorial's command sequence (`_REAR_ATTACK_STEPS` in `frontend/repl/tutorial.py`, a transcript of a verified play session), 31 (~36%) are allocate-phase value-setting (`mov` ×5, `w` ×12, `sh` ×14), the largest single category — but that is one scripted scenario, offered as corroboration, not as a measured distribution of typical play.
  - `1`-`9` jump directly to the Nth item in the active list (value-entry fields consume digits as the value instead, per above — the active-widget kind disambiguates).
  - `Enter` confirms/commits the current step.
  - `Esc`/`Backspace` backs out of the current submenu/draft step without committing.
  - `Tab`/`Shift+Tab` cycles the secondary panel (Ship Detail → Combat Log → Help → back to Ship Detail).
  - `:` opens a typed-command line (fallback path) for anything without a dedicated key. Typed commands reuse the REPL's vocabulary where the concept is shared (e.g. `end`, `commit`).
  - **End turn** has a dedicated binding (and the typed `end`/`e` fallback). If ending the turn would discard an unresolved queued shot, it prompts for confirmation first and only sends `end_turn` on confirm — the same guard as the REPL's `end`/`e` discard warning. Ending the turn and quitting the app are distinct actions that share this guard.
  - `q` quits; if there is an uncommitted allocate draft or an unresolved queued shot, prompt for confirmation first (same guard as above).
- Allocate is a **local draft until commit**, exactly as in the REPL and per `docs/PROTOCOL.md`'s `allocate` order shape (`movement`, `weapons` as desired total charge, `shields` as six face powers) — no partial/implicit application of an uncommitted draft. It is presented as a **form** in the secondary region (one movement field, one field per weapon, six shield-face fields, a free-power readout), navigated with the selection keys and adjusted with the value keys above — not a chain of submenus, and not a full-screen mode.
- Movement phase is a **three-option maneuver pick** (`coast` / `accel` / `turn`), where choosing `turn` opens a facing sub-selection (0..5, shown with the board-aligned arrows). Fits the standard list paradigm; no free-text needed.
- Firing phase is weapon-list → target-list → (when more than one is legal) shield-facing selection, then the shot is queued; `ready` is its own binding or list entry. Sole-option steps auto-select, matching the REPL (see Assumptions).

### Layout (Small tier)
- Two fixed regions, always both present:
  1. **Primary region**: the hex map plus a one-line compact status bar. Always rendered; never replaced by a secondary panel. The status bar must carry, at minimum:
     - **Turn number** and **phase**.
     - **Movement/firing cycle counter (N/4)** — the 4-cycle inner loop is the game's heartbeat (`frontend/repl/GAMEPLAY.md`, "Turn structure"), and the REPL surfaces it only in its movement- and firing-phase headers (`movement/fire cycle 2/4`, `firing cycle 1/4`); promoting it to a permanent status-bar slot means it is visible in every phase, not just the two that print a header.
     - **Focus ship.**
     - **A queued-shots indicator** when fire commits are pending, so "I have an unresolved shot" is visible without tabbing to a panel.
     - **A notice slot.** This carries two kinds of transient line, and it is the *only* always-visible surface for either:
       - the engine's soft-rejection/error line for a rejected order — an illegal move, insufficient power, a shot with no charge — reported verbatim, never a legality rule the UI invented for itself (story 17);
       - **the most recent combat result — HIT/MISS and damage, whoever fired it.** The Combat Log is the *persistent* record, but it is a `Tab`-reachable reference panel, so on its own it satisfies only half of story 8: a player mid-allocate-form would not see incoming enemy fire land. Flashing the latest result in the always-visible notice slot is what makes results **immediate**; the Combat Log is what makes them **persistent**. Both halves are required, and the "enemy fire invisible" bug the REPL just fixed is precisely what a Combat-Log-only design would reintroduce.
     - **Status-bar width is an input to `dim0`, not an independent wish.** A status bar cannot scroll the way the secondary region can, so the floor's *width* must be derived to fit these mandatory fields legibly — that is part of what slice 2 measures, and it is the main reason `dim0` has a width component at all.
     - **Overflow rule — keep it simple:** if the fields still contend for space, the notice slot wins and the fixed fields abbreviate in place (`t3`, `fire 2/4`, `A1`) rather than disappearing. A rejection or a HIT the player cannot read is one that effectively did not happen.
       *Known shortcoming, accepted:* near the floor the fixed fields will be terse to the point of being cryptic to a new player, and a long engine soft-error line may still be cut off. This is judged acceptable because `dim0`'s width is being chosen specifically so it is rare, and because the alternative (dropping a field entirely) hides state instead of merely shortening it. If slice 2's measurement shows it is *not* rare, that is a signal to raise `dim0`'s width, not to invent a cleverer overflow scheme.
  2. **Secondary region**: shows exactly one thing at a time — these never stack, and none of them ever cover the map. Its content is one of:
     - **The current phase's interaction surface**, when the phase has one: the allocate form, the maneuver pick (`coast`/`accel`/`turn` + facing), or the fire selection (weapon → target → shield facing). This is where the player is actually looking for most of a turn, and it is the region's default whenever an interaction is live. (ADR-0023 places the allocate draft here; the maneuver and fire selections are the same kind of surface and live here too.) These surfaces render *inside* this region — nothing in the Small tier ever takes over the whole screen.
     - **A reference panel**: Ship Detail, Combat Log, or Help, cycled with `Tab`/`Shift+Tab`.
  - `Tab` cycles only the reference panels; it never commits, cancels, or discards an in-progress interaction. Tabbing away from a half-made fire selection and back returns to it exactly as it was — in-progress interaction state is held in the `App`, not in the widget. `Esc` (not `Tab`) is the way to back out of an interaction step.
- No panel is silently dropped or truncated by content overflow within the Small tier's own floor — if content doesn't fit even in the single active secondary panel, that panel scrolls internally (not the REPL's "drop the whole panel with a note" strategy, which was a stopgap for a client that couldn't lay out a fixed screen; ratatui can allocate a genuinely fixed, scrollable sub-region instead).
- Presentation vocabulary carries over unchanged from the REPL (ADR-0021, `frontend/repl/ASCII-UI.md`): callsigns `A#`/`B#`/`C#` by controller, facing 0..5 with board-aligned arrows (`0 = +q → right`), HIT/MISS next to fired weapons, shield faces as ship-relative 0..5.

### End of scenario
- "Playable to a scenario's conclusion" needs a conclusion, and the engine already provides one: `StateSnapshot.status` is `in_progress` / `won` / `lost` (`ScenarioStatus`, `src/game_state.rs`), which is exactly what the REPL gates on today. **The TUI reads that field; it does not infer game-over from per-ship `destroyed` flags.** This keeps the "no rules logic in the client" rule intact and needs no protocol change.
- When `status` is no longer `in_progress`: the map and status bar stay on screen (the player wants to see the final board), the secondary region shows a terminal WON/LOST result, and order-issuing input stops being accepted. Quit — and `Tab` to read the combat log one last time — remain available. This is the loop's exit, and it is in scope for Small.

### Below-floor behavior
- On launch and on every `SIGWINCH`/resize event, check the current terminal size against the Small-tier floor (`dim0` — a named placeholder for a value deliberately derived by measurement in slices 1-2, not a constant to be guessed now; see Assumptions).
- Below the floor: render a "resize to at least W×H to keep playing" message in place of the game. No process exit, no state loss — the `shipsim` subprocess and its game state are untouched; the instant the terminal grows back above `dim0`, normal rendering resumes from the current snapshot.

### Accessibility
- Honor the cross-tool `NO_COLOR` standard and a TUI-scoped `SHIPSIM_TUI_COLOR=0` override (not the REPL's `SHIPSIM_REPL_COLOR` — client-specific knobs are named for their client). Monochrome must remain fully playable: glyphs, bars, and labels carry the information, color only reinforces it — same requirement the REPL already holds itself to (`frontend/repl/ASCII-UI.md`).

## Testing Decisions

- **A good test here asserts on rendered cell text and resulting `App` state, not on internal widget tree structure.** A test drives the seam the way a player or an agent would: feed a `KeyEvent` (or a sequence of them) into the key-handling entry point, render into a ratatui `TestBackend`, and assert on the `Buffer`'s visible text (e.g. "the map shows ship A1 at the expected cell," "the fire-target list contains B2 as MISS after this key sequence"). This is the required, primary verification path for every interaction listed in the User Stories above (per ADR-0023) — do not skip a story's test because "it's just a render."
- **Seam count: two**, matching the REPL's existing pattern, not a new shape:
  1. **Primary (required, exhaustive):** `App` key-handling + render, driven purely by synthetic `StateSnapshot`s and `KeyEvent`s against `TestBackend`. This is where the bulk of coverage belongs — every user story above should be traceable to at least one test at this seam.
  2. **Secondary (thin, integration-level only):** the subprocess adapter defined above, run against the real `shipsim` binary. This cannot be collapsed into seam 1 without losing real coverage of the process boundary itself; keep it minimal (smoke-level: does a real allocate→move→fire→end_turn loop against `scenarios/ai.toml` complete without protocol errors), mirroring how `frontend/repl/client.py` is smoke-tested against the real binary today.
- A real-pty smoke test (actual compiled binary, actual pseudo-terminal, real resize/raw-mode/alt-screen behavior) is optional polish per ADR-0023 and does not block any slice.
- Prior art to follow, not reinvent: `frontend/repl/tests/` already has the equivalent split for the Python client (pure `commands.py`/`view.py` tests vs. session-level `test_m3_scripted_driver.py`/`test_m4_recent_events.py`-style tests that replay a snapshot sequence through the wiring layer). The Rust TUI's two seams are the same shape.
- `cargo build`/`cargo test` from the repo root will **not** exercise this package (per ADR-0023's standalone-package decision) — its tests run via `cd frontend/tui && cargo test`. Document this explicitly wherever the project's "how to verify a change" instructions live (e.g. any future equivalent of `docs/BUGFIX-PLAN-20260714.md`'s Ground Rules), so it isn't silently skipped.

## Out of Scope

- Medium and Large tiers, and their exact breakpoints — deferred per ADR-0023, to be designed after Small ships and its real minimal layout can be measured.
- Any change to `frontend/repl/` or `frontend/love/` — this PRD touches `frontend/tui/` only, per the isolation rules in `frontend/README.md`.
- Any change to the engine/protocol (`src/`, `docs/PROTOCOL.md`) — Small tier is buildable entirely against the existing v3 protocol; no new snapshot or order fields are required. This now holds without exception: the to-hit-preview question resolved to "omit from Small" (see Assumptions), so the first deliverable has **no engine dependency and no external blocker**. The additive hit-preview field is a follow-on change, sequenced after Small ships, reviewed on its own per ADR-0021's consequences section.
- **To-hit / expected-damage previews in the fire UI** — deferred with the protocol field that would feed them (see above). A known, accepted downgrade vs. the REPL's fire UI for the Small release.
- Mouse support — not raised during grilling; keypress + typed fallback only.
- Multiplayer/spectator modes, save/resume UI beyond what the CLI already exposes, campaign-mode UI — not part of Small tier's playable loop (allocate/move/fire/end turn on a single scenario).
- **Tutorial mode.** The REPL's guided tutorial (`--tutorial rear-attack`, strict typed-command steps) is a REPL feature, not an engine feature, and its typed-exact step gating doesn't translate to keypress-primary input. A TUI-native tutorial is a future-tier concern; Small tier ships without one — and because the REPL remains supported (see Solution), the tutorial remains available to players there, so this is a gap in the TUI, not a loss to the product.
- A real-pty end-to-end smoke test — optional, non-blocking (see Testing Decisions).
- Root Cargo workspace integration — explicitly rejected for now (ADR-0023); revisit only via a future ADR.

## Further Notes

- This is a genuinely new subsystem, not a modification of existing code — there is no existing `frontend/tui/` implementation to preserve behavior of. The "prior art" references throughout are about *pattern* reuse (protocol integration shape, test-seam shape, presentation vocabulary), not code reuse — `frontend/tui/` must not import from `frontend/repl/` (isolation rule in `frontend/README.md`).
- The motivating problem (map scrolling off-screen) is specific to the REPL's plain-print/ANSI-clear approach and does not carry over structurally to ratatui, which owns the full terminal grid — but the Small-tier floor/below-floor behavior exists precisely so the *same class* of failure (unreadable game state on a too-small screen) can't resurface in a different form.
- `frontend/tui/README.md`'s existing "Suggested first implementation slices" section and its "Status checklist for implementers" remain the right shape for tracking build progress; this PRD does not replace them, it fills in the product decisions they were written before.

## Assumptions and Open Questions

- **`dim0` (Small-tier floor) is deliberately left unset — owner-confirmed.** This is a decision, not an omission: ADR-0023 defers it to be derived empirically once the Small-tier layout exists to measure, and pinning a number on paper first would be a guess dressed as a requirement. It is determined during slices 1-2 (skeleton, map+status), and until then every reference to `dim0` in this document is a named placeholder for "the measured floor," not a constant. Two constraints bound the derivation rather than being free to fail against it: the map must stay readable, and the mandatory status-bar fields must fit on one line (see Layout). The secondary region's *minimum* height — the value the above-floor growth rule grows from — is unset for the same reason and falls out of the same measurement.
- **The `:` typed-command line's exact vocabulary** (which REPL commands it mirrors 1:1 vs. which become dedicated keys instead) is not fully enumerated here. Assumption: it should cover at minimum anything reachable by a dedicated key, for consistency, plus any REPL command a returning player might reflexively type. Exact command list is an implementation-time decision, not a product decision requiring further grilling.
- **Accessibility env var naming — RESOLVED: use TUI-scoped names.** `SHIPSIM_TUI_COLOR=0` (not the REPL's `SHIPSIM_REPL_COLOR`), alongside the cross-tool `NO_COLOR` standard, which is honored by both clients. General rule: where a knob is client-specific, name it for the client it configures; the REPL keeps its own name unchanged.
- **Ratatui/crossterm exact crate versions** are not pinned by this PRD; assumption is "current stable at implementation time," consistent with ADR-0021's original phrasing ("or ratatui's recommended backend pairing at implementation time").
- **Confirmation-prompt UI shape** (a modal dialog widget vs. an inline status-bar message) for the `q`-quit and draft-discard guards is not specified — implementation-time decision, low risk either way since it's cheap to change later (doesn't meet ADR-worthiness per the domain-modeling skill's three-part test: not hard to reverse).
- **Weapon/target/shield-facing selection when a step has only one legal option** (e.g. REPL's existing "sole target: auto-selected" behavior) — assumption: Small tier should carry this REPL convenience over, since it's presentation-layer sugar over already-available snapshot data, not a new rule. Flagged as an assumption rather than confirmed in grilling.
- **To-hit previews — RESOLVED: Small tier ships without them (option (a)); the protocol field is the sequenced follow-on.** The REPL's fire/engagement UI shows advisory to-hit odds and expected damage next to each weapon/target pair (`to-hit≤4 (20%), damage≈5`). In the author's judgment this materially helps target selection, and omitting it is a real, known downgrade against the REPL's fire UI — but that judgment is a design call, not a measured play-test result, and it is not strong enough to put an engine dependency underneath the first deliverable. The data is not in the snapshot: the REPL computes it in a client-side display-geometry mirror (`hexutil.py`), which `frontend/repl/ASCII-UI.md` calls "the ceiling for client-side computation" and pairs with the rule "extend the protocol, don't fork the math." The TUI is Rust but must not depend on `shipsim_core` (ADR-0021). Options were: (a) omit previews from Small; (b) re-mirror the geometry/tables in the TUI crate — a third copy of the math, which the existing docs argue against; (c) add an additive snapshot field via a separate protocol change.
  - **Decision: (a) for Small.** The fire slice is the last and hardest slice of the first deliverable; making it block on a separately-reviewed engine PR would give the TUI's first release an external dependency and would falsify this PRD's own claim that Small is buildable entirely against the existing v3 protocol. Small ships as a pure client with zero engine changes, and the fire UI lists weapon/target pairs without advisory odds.
  - **Follow-on: (c) — the engine WILL change, after Small ships.** Owner-confirmed: this is a committed obligation, not an option left open. An additive per-weapon-per-target hit-preview snapshot field lands as its own small engine change per the ASCII-UI boundary rule and ADR-0021's "engine changes only when the protocol needs a field," and the TUI's fire UI consumes it once it exists. (b) — re-mirroring the math in the TUI crate — remains rejected outright and must not be used as a shortcut if the protocol change slips. The downgrade is accepted, recorded, and time-boxed to the Small release; it is not permanent, and Small must not be designed in a way that makes adding previews later a rewrite (the fire UI should have a place for the odds to appear).
