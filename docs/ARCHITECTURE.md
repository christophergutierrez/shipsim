# shipsim Architecture

## Purpose

shipsim is a deterministic, turn-based hex-grid starship combat simulator. The Rust crate owns all game rules and exposes a JSON-friendly state and order boundary. A Love2D application renders that state and submits orders without reimplementing combat logic.

The current product rules are Combat Model v2 as accepted in ADR-0020. Earlier impulse and FASA loops are historical context only.

## System boundaries

### Rust core

`shipsim_core` is both an `rlib` and `cdylib`. It owns:

- scenario and ship-data loading from TOML;
- board, hex, facing, arc, and movement validation;
- turn phases, power allocation, inertial movement, and maneuver resolution;
- weapon legality, deterministic hit resolution, shields, and hull damage;
- AI decisions, victory state, campaigns, and snapshots.

`GameState` is the aggregate root. Mutations pass through order application and focused state methods so callers cannot bypass phase and resource invariants.

### CLI harness

The `shipsim` binary loads a scenario, accepts newline-delimited JSON orders, and emits JSON snapshots or soft errors. This is the primary automation and integration boundary. Given the same scenario, order stream, and seed, its output must be deterministic.

### Frontends

All clients live under `frontend/`. Policy is in `frontend/README.md`:

- one directory tree per client (`frontend/<name>/`);
- each client's code, tests, and untracked scratch stay under that tree
  (`frontend/<name>/local/` is gitignored);
- clients must not depend on each other or on engine internals beyond the public
  NDJSON harness (`docs/PROTOCOL.md`);
- the core and its `tests/` must not depend on anything under `frontend/`;
- adding or deleting a client must not require engine changes beyond optional
  docs pointers.

Current clients:

- `frontend/repl/` — interactive Python dev client (`--stdin` NDJSON).
- `frontend/love/` — Love2D graphical thin client (display, input, order
  construction only).

Neither client reimplements rules or AI. After load and after each accepted
order, the harness runs `GameState::resolve_v2_npc_actions` so
`controller = "ai"` ships act without the client inventing NPC orders.

## Core model

The v2 turn progresses through:

1. `Allocation`: each ship converts fixed power into thrust, weapon charges, and six shield facings.
2. `Movement`: every living ship commits one maneuver; maneuvers and scheduled translations resolve simultaneously.
3. `Firing`: ships commit legal weapon shots or declare readiness; committed fire resolves simultaneously.
4. Exactly four movement/firing windows occur per turn.
5. `EndTurn` resets turn-scoped resources and begins the next allocation phase.

Movement cost depends on momentum. Weapon charge and firing are limited per turn. Powered shields absorb damage by legal facing before overflow reaches hull. Destroyed ships remain eligible to deal already-committed simultaneous damage.

## Module map

| Area | Modules | Responsibility |
|---|---|---|
| Aggregate and protocol | `game_state`, `movement`, `snapshot` | State machine, orders, validation, serialized views |
| Geometry | `hex`, `board`, `arc`, `momentum` | Coordinates, occupancy, facings, firing arcs, movement cost |
| Combat | `combat`, `combat_tables`, `ssd`, `prng` | Weapon data, hit/damage rules, damage application, determinism |
| Content | `schema`, `scenario`, `ship`, `campaign` | TOML schemas, loading, ship instances, campaign setup |
| Orchestration | `turn`, `ai` | Turn counter and NPC actions |
| Simulation | `simulation` | Policies, match runner, traces, metrics, and rubric evaluation |
| Adapters | `src/bin/shipsim.rs`, `frontend/repl/`, `frontend/love/` | NDJSON harness, REPL dev client, Love client |

## Data and control flow

1. A scenario references generic ship definitions in `data/ships/`.
2. The loader validates those definitions and creates `GameState`.
3. A human, AI, test, or client submits an order.
4. `apply_order` validates the order against phase, controller, geometry, and remaining resources.
5. The aggregate mutates atomically or returns a soft protocol error.
6. `StateSnapshot::from_game_state` produces the client-safe serialized view.

### Content data

| Path | Role |
|---|---|
| `data/ships/*.toml` | Shipped ship classes (`ShipDef`); loaded by the scenario loader |
| `data/ships/*_{light,line,heavy}.toml` | Draft size variants (21 hulls); see `docs/SIZE-VARIANTS.md` |
| `data/ship_costs.toml` | Fleet-budget index (`cost`; destroyer_line = 100) |
| `data/sizes.toml` | Canonical hull-size ladder (1..=7: Fighter … Titan) and FASA Class I–XX map; **catalog only** (not loaded by the engine yet) |

Ship `size` is the to-hit silhouette and the future construction frame key.
See `docs/combat-v2-tables.md` and `docs/TODO.md` (hull size / construction).

## Invariants

- Rust is the sole rules authority; clients may project but not decide legality.
- Power allocation cannot exceed ship power (and per-facing shield max) and is locked for the turn.
- **Movement allocation is power units**, converted to a turn-scoped thrust reserve;
  thrust is spent on inertial maneuvers and does not directly buy distance.
- There is no movement initiative queue; all living ships commit once per phase
  and resolve simultaneously.
- Movement phase is **simultaneous**: every living ship commits one maneuver per
  phase, then maneuvers and translation resolve as a batch. Legacy Move/Pass
  variants are deserialization-only compatibility cases and are rejected.
- Firing phase: `CommitFire` then `ReadyFire` per ship; when **all living ships** are ready, resolve simultaneously. AI must ReadyFire (core `resolve_v2_npc_actions` does).
- **Miss still consumes charge** and marks the weapon fired this turn.
- The turn always follows four movement/fire cycles; coasting ships can therefore
  translate on their schedule without an open-ended heuristic.
- `end_turn_warning` reflects remaining legal fire actions; EndTurn still always
  advances after allocation.
- Fire resolution uses pre-resolution ship snapshot; mutual destruction possible.
- Random outcomes use seeded PRNG (`roll(20)` is 1..=20 for to-hit).
- Content remains generic and does not copy protected game data.
- Invalid external orders soft-fail without partial mutation (ADR-0018 / protocol v1).

## Local agent state (not in git)

Ephemeral session notes live under `/tmp` (gitignored) and optional root `AGENT-LOCAL.md` (gitignored).
Checked-in docs must not depend on those paths. Agents: if `AGENT-LOCAL.md` exists, read it for
pointers to live scratch (handoffs, killhouse artifacts, PDFs under `tmp/`).

## Persistence and compatibility

Scenarios, ship definitions, campaigns, and test order streams are version-controlled inputs. Snapshots are the integration representation, while save files are deterministic replay documents. Breaking wire changes require a new protocol version, and fixtures and clients must be updated together.

The external NDJSON contract is now versioned; see `docs/PROTOCOL.md` (the public
API). Save files use deterministic scenario-plus-order replay with a verified
PRNG checkpoint; see `docs/SAVE-FORMAT.md`.

**Agents / humans playing the game** — three types (**UI play**, **API play**,
**sim play**): `docs/AGENT-PLAY.md` and root `AGENTS.md`.

## Verification

- `cargo test` covers unit, integration, acceptance, AI, and deterministic fixture behavior.
- `luajit frontend/love/tests/run_all.lua` covers pure Lua order and UI-state behavior.
- `tests/fixtures/v2/duel.jsonl` is the golden end-to-end v2 replay.
- REPL automated suite: `(cd frontend/repl && python3 -m unittest discover -s tests)`.
- REPL live play: `python3 frontend/repl/repl.py scenarios/ai.toml`.

Architecture decisions and supersession history live in `docs/adr/`.

Gameplay simulation uses validated production orders as defined by ADR-0021. See `docs/SIMULATION.md` and `docs/GAMEPLAY-RUBRICS.md`.

Combat constants: `docs/combat-v2-tables.md`. Play guide: `docs/PLAY-V2.md`.
Play types (UI / API / sim): `docs/AGENT-PLAY.md`.
