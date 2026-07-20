# shipsim TUI (ratatui)

Terminal product client for Combat Model v2 over **protocol v4**. **Implemented
(Small tier).** Read **[`ADR.md`](ADR.md)** for why ratatui and how it fits the
architecture, and
**[ADR-0023](../../docs/adr/0023-tui-input-layout-and-verification.md)**
for input model, layout, responsive tiers, and verification strategy.

Turn loop and wire format: `docs/PROTOCOL.md`, ADR-0025. Play types:
`docs/AGENT-PLAY.md`.

This directory is the **entire** TUI client tree (isolation: `frontend/README.md`).
Dropping it must not affect the engine, `frontend/repl/`, or `frontend/love/`.

```
frontend/tui/
  ADR.md           # decision: ratatui + subprocess NDJSON
  README.md        # this file
  Cargo.toml       # standalone crate (ratatui + crossterm + serde_json)
  src/
    main.rs        # binary entry point, crossterm event loop
    app.rs         # app state (Mode, AllocDraft, PathDraft, FireDraft, focus)
    harness.rs     # subprocess wrapper: spawns shipsim, reads/writes NDJSON
    protocol.rs    # NDJSON v4 snapshot + order types
    input.rs       # keyboard → app state + pending orders
    ui.rs          # ratatui rendering (Small-tier layout)
    tests.rs       # TestBackend tests
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
# → scenarios/tutorial_rear_attack.toml (seed 4), step-gated keys
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

## Keys (protocol v4)

Engine phase names are `allocate` / `movement` / `firing` (product language:
allocate / path / volley). There is **no** `end_turn`.

| Key | Action |
|---|---|
| `q` | Request quit; press `y` to confirm (`n`/`Esc` cancels); prints the session-log path after exit |
| `Esc` | Return to Normal mode; in the tutorial, reopen the expected form |
| `Tab` | Cycle focus in free play; blocked during the tutorial |
| `v` | Map focus (pan/zoom); `Esc`/`v`/`Enter` returns |
| `a` / `Enter` | Enter Allocate mode (when phase = allocate) |
| `m` / `Enter` | Enter Movement/path mode (when phase = movement) |
| `f` / `Enter` | Enter Fire/volley mode (when phase = firing) |

**Allocate mode** — `↓`/`↑` (or `j`/`k`) cycles movement / weapons / shield
facings; `←`/`→` decrement/increment the focused field; digits start a fresh
value entry; `Backspace` clears; `Enter` commits the `allocate` order. A ship
with no power can press `Space` to submit its forced zero allocation.

**Movement (path) mode** — draft one ordered path, then submit once:

| Key | Path action |
|---|---|
| `w` / `f` / `↑` | `move_f` |
| `a` | `move_fl` |
| `d` | `move_fr` |
| `←` / `→` | `turn_left` / `turn_right` |
| `0`–`5` | turn toward absolute facing (minimal turns) |
| `Backspace` | undo last action |
| `x` | clear draft |
| `Enter` | `commit_path` with the draft |
| `Space` | hold position (`commit_path` with empty actions) |

**Fire (volley) mode** — queue zero or more shots, then submit once:

| Key | Action |
|---|---|
| `↓`/`↑` or `j`/`k` | cycle weapon |
| `1`–`9` | select target by enemy index |
| `←`/`→` | cycle shield facing |
| `Enter` | queue/unqueue shot for the focused weapon |
| `Backspace` | remove last queued shot |
| `Space` | `commit_volley` (empty shots = hold fire) |

**Tutorial mode** (`--tutorial`) — narration panel + step gate for the aggressive
rear-attack. Wrong keys are blocked. Order-backed steps advance only after
engine acknowledgment.

## Verify

```bash
cargo test   --manifest-path frontend/tui/Cargo.toml   # TUI + live tutorial tests
cargo clippy --manifest-path frontend/tui/Cargo.toml
```

Confirmed quits write a transcript under `frontend/tui/local/` and print its
absolute path after the terminal UI closes.

## Relationship to other clients

| Client | Role |
|---|---|
| **Engine + harness** | Rules + NDJSON (`docs/PROTOCOL.md`) |
| **`frontend/repl/`** | Reference Python client; agent/debug UI play |
| **`frontend/love/`** | Graphical thin client |
| **`frontend/tui/`** | ratatui terminal product UI (this tree; Small tier) |

Port **behavior and vocabulary** from the REPL, not pixel-identical ANSI:

- Ship-centric focus; callsigns by side (`A1` player, `B2` ai, …).
- Facing 0..5 with **board-aligned** arrows (0 = +q → right on q→/r↓ maps). See `frontend/repl/ASCII-UI.md`.
- Allocate / path / volley = **local drafts until commit**.
- After every living ship commits a volley, the next allocate begins automatically.
- Soft errors stay soft; never reimplement hit tables or legality.

## Protocol (v4) — must not reinvent

- Spawn: `shipsim --scenario <path> --stdin` (build with `cargo build -q` in repo root).
- Overrides: `SHIPSIM_BIN`, optionally `SHIPSIM_ROOT`.
- After load and after each **accepted** order, harness runs AI
  (`resolve_v2_npc_actions`); client only drives **player** orders.
- Snapshot fields of interest: `phase`, `turn`, `ships[]`, `combat_log[]`,
  `ships_allocated_this_turn`, `ships_committed_path`, `ships_committed_volley`,
  `path_results`, `protocol_version`.
- Orders: `allocate`, `commit_path`, `commit_volley` — see `docs/PROTOCOL.md`.
  Goldens: `tests/fixtures/v4/`.
- Retired under v4 (rejected): `commit_maneuver`, `commit_fire`, `ready_fire`,
  `end_turn`, `pass_move`.

## Design decisions

**[ADR-0023](../../docs/adr/0023-tui-input-layout-and-verification.md)**
covers keypress-primary input, layout (map + compact status always visible),
responsive tiers (Small shipped; Medium/Large deferred), recoverable pause below
the floor, `TestBackend` verification, and a standalone (non-workspace) Cargo
package. Path/volley turn structure is ADR-0025 (partially supersedes input
assumptions from the inertial era).

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
- REPL remains the non-fullscreen regression path for protocol play and agent UI play.
- TUI smoke: run binary against `scenarios/ai.toml`, quit with `q` → `y`.

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
- ADR-0020 (combat model v2 base), ADR-0025 (simplified simultaneous turns / protocol v4)

## Status checklist

- [x] Package builds under `frontend/tui/`
- [x] Spawns harness, shows live snapshot fields
- [x] Player can finish allocate → path → volley on `scenarios/ai.toml` (auto next turn)
- [x] Soft errors visible; state only changes on accepted orders
- [x] Scratch only under `frontend/tui/local/`
- [x] This README matches protocol v4 orders and keys

## Older design notes in this tree

`PRD.md`, `AGENT-HANDOFF.md`, `PLAY-UX-MILESTONES.md`, and the UX recommendation
docs capture **pre-v4 / build-out** planning. Prefer this README + live
`src/` for current behavior; those files may still describe inertial orders or
`end_turn` as historical context.
