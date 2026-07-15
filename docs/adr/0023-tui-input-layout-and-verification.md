# ADR-0023 — TUI input model, responsive layout, and verification strategy

Status: Accepted
Date: 2026-07-14
Scope: `frontend/tui/` only
Related: ADR-0021 (ratatui terminal client — the parent decision this refines)

## Context

ADR-0021 accepted ratatui for `frontend/tui/` but deliberately left implementation
details open ("non-binding sketch"). Before writing code, a grilling session
(`killhouse:grill-with-docs`) resolved the product-shape questions ADR-0021 didn't
answer: how input works, how the screen is laid out, how it behaves on small
terminals, how an agent verifies it, and how the crate is packaged.

The REPL's plain-print + ANSI-clear + alternate-screen approach (see recent
history: `docs/BUGFIX-PLAN-20260714.md` and the follow-up fixed-frame-height
work) hit a hard ceiling — no real scrollback, no per-panel scrolling, no
terminal-size-aware layout — which is the immediate motivation for building
the TUI now rather than continuing to patch the REPL.

## Decisions

### 1. Input model: keypress-primary, typed fallback

Routine actions (select ship/weapon/target, adjust allocation, confirm) are
driven by keypresses, not typed commands — this is what lets the UI stay
compact instead of needing to print a menu of typed options. A typed command
line (opened with `:`) remains available as a fallback for anything without a
dedicated key, mirroring the REPL's vocabulary for players/agents who prefer
it.

Bindings: `↑↓`/`j k` move selection in the current list; `1-9` jump directly
to a list item; `Enter` confirms; `Esc`/`Backspace` backs out of a submenu
without committing; `Tab`/`Shift+Tab` cycles the secondary panel; `q` quits
(with a confirmation guard if it would discard an uncommitted/unresolved
draft, matching the REPL's existing `end`/`e` discard warning); `:` opens the
typed-command fallback. Both arrow keys and `hjkl` are supported
simultaneously, not one or the other.

### 2. Layout: map always visible, secondary content tabbed

The map plus a compact one-line status bar are always on screen. Everything
else — ship detail, combat log, allocate draft, help — lives in a secondary
region below it, switched with `Tab`. This directly targets the REPL's
failure mode (the map got scrolled off-screen behind stacked panels); losing
sight of "where is everything" was the actual complaint, so it's the one
thing the layout guarantees never happens.

**Rejected alternatives:**
- *Fixed multi-region split* (every panel simultaneously visible, REPL-style)
  — degrades gradually into unreadable slivers on a small terminal instead
  of failing cleanly; same underlying problem via a slower path.
- *Fully tabbed, including the map* — loses spatial context while reading
  the log, which is the thing being fixed.

### 3. Responsive tiers: Small / Medium / Large

Layout density scales with terminal size across three tiers, gated on
**independent width and height thresholds per tier** (not one symmetric
`dim` applied to both axes — terminals are typically much wider than tall,
so a symmetric gate would make width almost never the binding constraint in
practice).

- **Small** — minimal information, compact structure; "just works."
- **Medium** — between Small and Large.
- **Large** — full detail (lit charge bars, inline explanations, larger map),
  continues to grow with the terminal above the Large floor.
- **Below the Small floor (`dim0`)** — refuse to render the game and show a
  "resize to at least W×H" message instead, checked at launch and on every
  resize (`SIGWINCH`). This is a **live, recoverable pause**, not an
  exit: game state lives in the `shipsim` subprocess, not the TUI, so
  rendering resumes the instant the terminal grows back above `dim0` — no
  restart, no lost state.

Exact `dim0`/`dimM`/`dimL` numbers are **not fixed by this ADR**. `dim0` will
be derived empirically from what the Small tier actually needs once it's
built and measured; `dimM`/`dimL` stay provisional until Medium/Large are
designed in a later session.

### 4. Build order: Small tier, fully playable, before Medium/Large exist

First deliverable is a complete playable loop (allocate → move → fire → end
turn) built entirely against the Small tier's layout, following the README's
existing slice order (skeleton → map/status → allocate → movement → fire)
with every slice targeting Small only. No tier-switching code, no
Medium/Large layout, until that loop works end to end.

### 5. Verification: ratatui `TestBackend` is the required primary path

Every keypress → state-change → rendered-text interaction gets a test driven
against ratatui's `TestBackend` — an in-memory backend that the same render
function draws into as a grid of cells (plain-text `symbol()` + style as
structured data, not ANSI bytes an agent has to parse). This is *more*
reliable for agent verification than the REPL's current approach, which
requires stripping ANSI escapes with a regex (`style.py`'s `_ANSI_RE`) to
read rendered output at all.

A real-pty smoke test (spawn the actual compiled binary, drive it over a
pseudo-terminal, exercise real resize/alt-screen/raw-mode behavior) is
optional polish and never blocks a slice — consistent with the README's
existing anti-goal that "fullscreen UI is not the agent primary path."

### 6. Packaging: standalone Cargo package, not a workspace member

`frontend/tui/` keeps its own independent `Cargo.toml`, per ADR-0021's
literal default, rather than joining the root package as a Cargo workspace
member. Verifying the TUI requires `cd frontend/tui && cargo build/test`
separately from the root suite's `cargo build -q && cargo test -q` — the two
are not covered by one command.

## Consequences

- A future contributor should not be surprised that pressing `f` to fire
  doesn't require typing `fire b1 2` — that's deliberate, not an incomplete
  port of the REPL.
- `frontend/tui/README.md`'s "Suggested first implementation slices" should
  be read together with this ADR: build order is unchanged, but every slice
  now specifically targets the Small tier.
- Root `cargo build`/`cargo test` will **not** exercise `frontend/tui/` once
  it exists; anyone verifying a change there needs the separate command
  above until/unless a future ADR revisits packaging.
- Exact breakpoint numbers are an open follow-up, not a gap in this decision
  — they're deferred by design until Small's real layout exists to measure.
