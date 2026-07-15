# shipsim TUI (ratatui)

Terminal product client for Combat Model v2. **Implemented (Small tier).** Read
**[`ADR.md`](ADR.md)** for why ratatui and how it fits the architecture, and
**[ADR-0023](../../docs/adr/0023-tui-input-layout-and-verification.md)**
for input model, layout, responsive tiers, and verification strategy.

This directory is the **entire** TUI client tree (isolation: `frontend/README.md`).
Dropping it must not affect the engine, `frontend/repl/`, or `frontend/love/`.

```
frontend/tui/
  ADR.md           # decision: ratatui + subprocess NDJSON
  README.md        # this file
  Cargo.toml       # standalone crate (ratatui + crossterm + serde_json)
  src/
    main.rs        # binary entry point, crossterm event loop
    app.rs         # app state (Mode, AllocDraft, FireDraft, focus)
    harness.rs     # subprocess wrapper: spawns shipsim, reads/writes NDJSON
    protocol.rs    # NDJSON v3 snapshot + order types
    input.rs       # keyboard → app state + pending orders
    ui.rs          # ratatui rendering (Small-tier layout)
    tests.rs       # TestBackend tests (all 6 slices)
  local/           # gitignored session junk only
```

## Run

The TUI is a standalone Cargo package that spawns the `shipsim` engine binary as
a subprocess. **Run from the repo root** so the default engine path
(`target/debug/shipsim`) resolves.

```bash
# 1. Build the engine binary the TUI spawns (one-time).
cargo build

# 2. Build + run the TUI. Defaults to scenarios/ai.toml.
cargo run --manifest-path frontend/tui/Cargo.toml
```

**Guided rear-attack tutorial** (same fight as REPL `--tutorial rear-attack`):

```bash
cargo run --manifest-path frontend/tui/Cargo.toml -- --tutorial
# → scenarios/tutorial_rear_attack.toml (seed 4), step-gated keys, ~3 turns to Won
```

Pick a different scenario (first positional arg):

```bash
cargo run --manifest-path frontend/tui/Cargo.toml -- scenarios/fleet.toml
cargo run --manifest-path frontend/tui/Cargo.toml -- scenarios/combat.toml
```

If the engine binary is not at `target/debug/shipsim` relative to the repo root,
point at it explicitly:

```bash
SHIPSIM_BIN=/path/to/shipsim cargo run --manifest-path frontend/tui/Cargo.toml
```

Release build (smoother rendering):

```bash
cargo build --release
cargo run --release --manifest-path frontend/tui/Cargo.toml
```

## Keys

| Key | Action |
|---|---|
| `q` | Request quit; press `y` to confirm (`n`/`Esc` cancels) |
| `Esc` | Return to Normal mode; in the tutorial, reopen the expected form |
| `Tab` | Cycle focus in free play; blocked during the tutorial |
| `e` | Request end turn; press `y` to confirm (`n`/`Esc` cancels) |
| `a` / `Enter` | Enter Allocate mode (when phase = allocate) |
| `m` / `Enter` | Enter Movement mode (when phase = movement) |
| `f` / `Enter` | Enter Fire mode (when phase = fire) |

**Allocate mode** — `↓`/`↑` cycles movement / weapons / shield facings;
`←`/`→` decrement/increment the focused field; digits start a fresh value entry;
`Backspace` clears; `Enter` commits the `allocate` order.

**Movement mode** — `c` coast, `t` accel along facing, `0`–`5` turn to absolute
facing, `r` turn +1 facing. Each sends one `commit_maneuver`.

**Tutorial mode** (`--tutorial`) — narration panel + step gate for the aggressive
rear-attack (race past → reverse-thrust brake → point-blank beam/torp/plasma).
Wrong keys are blocked; `↓`/`→` fill the allocate form, `t`/`3`/`Space`/`e` drive
motion and the kill volley. Order-backed steps advance only after engine
acknowledgment, so a rejected order cannot desynchronize the lesson.

**Fire mode** — `↓`/`↑` cycles weapon; `1`–`9` select target by enemy index;
`←`/`→` cycle shield facing; `Enter` commits `commit_fire`. Target auto-selects
the first enemy if none is chosen.

## Verify

```bash
cargo test   --manifest-path frontend/tui/Cargo.toml   # 56 TestBackend tests
cargo clippy --manifest-path frontend/tui/Cargo.toml
```

## Relationship to other clients

| Client | Role |
|---|---|
| **Engine + harness** | Rules + NDJSON (`docs/PROTOCOL.md`) |
| **`frontend/repl/`** | Playable Python client; agent/debug; fixed play-frame prototype of “TUI shape” |
| **`frontend/love/`** | Graphical thin client |
| **`frontend/tui/`** | Future ratatui terminal product UI |

Port **behavior and vocabulary** from the REPL, not pixel-identical ANSI:

- Ship-centric focus; callsigns by side (`A1` player, `B2` ai, …) until scenarios carry fleet/side ids.
- Facing 0..5 with **board-aligned** arrows (0 = +q → right on q→/r↓ maps). See `frontend/repl/ASCII-UI.md`.
- Allocate = **local draft until commit**; bare numbers must not wipe drafts.
- Fire: optional shots; `ready` / nofire leaves fire phase; `end` is whole turn.
- HIT/MISS, damage, shield absorption, and hull damage persist in the Combat Log,
  including after game over.
- Soft errors stay soft; never reimplement hit tables or legality.

## Protocol (v1) — must not reinvent

- Spawn: `shipsim --scenario <path> --stdin` (build with `cargo build -q` in repo root).
- Overrides: `SHIPSIM_BIN`, optionally `SHIPSIM_ROOT`.
- After load and after each **accepted** order, harness runs AI
  (`resolve_v2_npc_actions`); client only drives **player** orders.
- Snapshot fields of interest: `phase`, `turn`, `active_ship`, `ships[]`
  (weapons charge/fired, shields rem/powered, structure, controller),
  `combat_log[]` (attacker, target, **weapon**, kind, damage, shield),
  `ships_allocated_this_turn`, `ships_ready_fire`, `end_turn_warning`,
  `move_order`, `protocol_version`.
- Orders: `allocate`, `move`, `pass_move`, `commit_fire`, `ready_fire`, `end_turn`
  — shapes in `src/movement.rs` and `docs/PROTOCOL.md`. Fixtures:
  `tests/fixtures/v2/duel_orders.jsonl`.

## Design decisions (grilled 2026-07-14)

**[ADR-0023](../../docs/adr/0023-tui-input-layout-and-verification.md)**
resolved the product-shape questions this handoff originally left open:
keypress-primary input (vim + arrows, `:` for a typed fallback), a layout
where the map + a compact status bar are always visible and everything else
is tabbed, three responsive tiers (Small/Medium/Large, exact breakpoints
deferred until Small is built and measured), a live recoverable pause below
the floor rather than a crash, `TestBackend`-driven tests as the required
verification path, and a standalone (non-workspace) Cargo package. Read it
before starting slice 1 below — every slice targets the **Small tier only**;
Medium/Large don't exist yet.

## Suggested first implementation slices

Keep PRs small; each should stay playable or at least runnable. Every slice
below targets the Small tier's layout (see ADR-0023) — no tier-switching
code until slice 5 is done and fully playable.

1. **Skeleton** — `Cargo.toml` (ratatui + crossterm + serde_json), binary that
   spawns harness, reads post-load snapshot, draws turn/phase + ship names, `q` quits.
2. **Map + status** — hex/occupancy panel; callsigns; facing; hull/shield gauges.
3. **Allocate UX** — focused ship draft (mov/weapons/shields) → `allocate` order;
   show engine-accepted values after commit.
4. **Movement** — active ship; forward/reverse/turns (and optional absolute dir helper).
5. **Fire** — weapon/target/facing; ready; combat log region; FIRED HIT/MISS.
6. **Polish** — layout regions (map | ship | log | help), resize, key help overlay,
   optional transcript flag (mirror REPL `--debug` idea; prefer `frontend/tui/local/`
   for product logs, `/tmp` only if explicitly requested).

## Stack notes (ratatui)

- Immediate-mode: each frame rebuild UI from **current snapshot** + local UI state
  (focus ship, draft, selected menu). Do not cache “display HP” separate from
  snapshot without invalidating on every message.
- Use crossterm (or current ratatui default backend) for input and terminal lifecycle.
- Prefer a **separate Cargo package** under this directory so `shipsim_core` does
  not depend on TUI crates. Linking `shipsim_core` as a library later is optional
  and must not become a back-door for UI-side rules.

## Verification

- Engine: `cargo test` (unchanged).
- REPL remains the non-fullscreen regression path for protocol play.
- When TUI exists: document a one-command smoke (e.g. run binary against
  `scenarios/ai.toml`, exit). Fullscreen UI is not the agent primary path;
  keep subprocess protocol tests on the harness.

## Anti-goals

- No rules reimplementation in the TUI.
- No writes of TUI scratch into repo root or sibling frontends.
- No requirement that Love or REPL import this crate.
- No blocking core releases on TUI polish.

## Pointers

- `docs/PROTOCOL.md` (API), `docs/AGENT-PLAY.md` (UI / API / sim play), root `AGENTS.md`
- `docs/PLAY-V2.md`, `docs/ARCHITECTURE.md`
- `frontend/README.md` (isolation)
- `frontend/repl/README.md`, `frontend/repl/ASCII-UI.md` (play vocabulary + ASCII lessons)
- ADR-0017 (Love subprocess pattern — same integration style)
- ADR-0020 (combat model v2)

## Status checklist for implementers

- [x] Package builds under `frontend/tui/`
- [x] Spawns harness, shows live snapshot fields
- [x] Player can finish allocate → move → fire → end turn on `scenarios/ai.toml`
- [x] Soft errors visible; state only changes on accepted orders
- [x] Scratch only under `frontend/tui/local/`
- [x] This README updated with real run commands once the binary exists
