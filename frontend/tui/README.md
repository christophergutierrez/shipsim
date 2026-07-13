# shipsim TUI (ratatui) — not implemented yet

Terminal product client for Combat Model v2. **Decision is accepted; code is not
started.** Read **[`ADR.md`](ADR.md)** for why ratatui and how it fits the
architecture.

This directory is the **entire** TUI client tree (isolation: `frontend/README.md`).
Dropping it must not affect the engine, `frontend/repl/`, or `frontend/love/`.

```
frontend/tui/
  ADR.md           # decision: ratatui + subprocess NDJSON
  README.md        # this handoff
  .gitignore       # local/ scratch
  (future) Cargo.toml, src/, …
  local/           # gitignored session junk only
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
- HIT/MISS next to fired weapons (combat log includes `weapon`).
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

## Suggested first implementation slices

Keep PRs small; each should stay playable or at least runnable.

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

- [ ] Package builds under `frontend/tui/`
- [ ] Spawns harness, shows live snapshot fields
- [ ] Player can finish allocate → move → fire → end turn on `scenarios/ai.toml`
- [ ] Soft errors visible; state only changes on accepted orders
- [ ] Scratch only under `frontend/tui/local/`
- [ ] This README updated with real run commands once the binary exists
