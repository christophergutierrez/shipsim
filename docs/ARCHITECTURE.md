# shipsim Architecture

## Purpose

shipsim is a deterministic, turn-based hex-grid starship combat simulator. The Rust crate owns all game rules and exposes a JSON-friendly state and order boundary. A Love2D application renders that state and submits orders without reimplementing combat logic.

The current product rules are Combat Model v2 as accepted in ADR-0020. Earlier impulse and FASA loops are historical context only.

## System boundaries

### Rust core

`shipsim_core` is both an `rlib` and `cdylib`. It owns:

- scenario and ship-data loading from TOML;
- board, hex, facing, arc, and movement validation;
- turn phases, power allocation, momentum, and initiative;
- weapon legality, deterministic hit resolution, shields, and hull damage;
- AI decisions, victory state, campaigns, and snapshots.

`GameState` is the aggregate root. Mutations pass through order application and focused state methods so callers cannot bypass phase and resource invariants.

### CLI harness

The `shipsim` binary loads a scenario, accepts newline-delimited JSON orders, and emits JSON snapshots or soft errors. This is the primary automation and integration boundary. Given the same scenario, order stream, and seed, its output must be deterministic.

### Love2D client

`frontend/love/` is a thin client over the harness contract. It owns display, input, order construction, and presentation state. It does not calculate legal movement, hit chance, damage, or phase transitions.

## Core model

The v2 turn progresses through:

1. `Allocation`: each ship assigns its fixed power budget to movement, weapons, and six shield facings.
2. `Movement`: ships act in stable initiative order and spend movement power one decision at a time.
3. `Firing`: ships commit legal weapon shots or declare readiness; committed fire resolves simultaneously.
4. Movement and firing repeat while useful actions remain.
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
| Adapters | `src/bin/shipsim.rs`, `frontend/love/` | NDJSON harness and graphical client |

## Data and control flow

1. A scenario references generic ship definitions in `data/ships/`.
2. The loader validates those definitions and creates `GameState`.
3. A human, AI, test, or client submits an order.
4. `apply_order` validates the order against phase, controller, geometry, and remaining resources.
5. The aggregate mutates atomically or returns a soft protocol error.
6. `StateSnapshot::from_game_state` produces the client-safe serialized view.

## Invariants

- Rust is the sole rules authority; clients may project but not decide legality.
- Power allocation cannot exceed ship power (and per-facing shield max) and is locked for the turn.
- **Movement allocation is power units**, not hex count; reverse after forward costs 2 units.
- Initiative is sorted **once per turn** by movement allocation (high first); ties broken once with PRNG; order frozen.
- Movement phase is **sequential**: one ACTIVE mover at a time (`active_mover`); each ship at most one Move/Pass per phase.
- Ships with `move_remaining == 0` are skipped; when every living ship has decided or has no move power, phase advances to firing.
- Firing phase: `CommitFire` then `ReadyFire` per ship; when **all living ships** are ready, resolve simultaneously. AI must ReadyFire (core `resolve_v2_npc_actions` does).
- **Miss still consumes charge** and marks the weapon fired this turn.
- Turn continues after fire only if `can_any_move()` (legal **hex-changing** forward/reverse) or `can_any_legal_fire()`; turn-in-place alone does not keep the turn open.
- `end_turn_warning` is true when useful move/fire remains; EndTurn still always advances after allocation.
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

The external NDJSON contract is now versioned; see `docs/PROTOCOL.md`. Save files use deterministic scenario-plus-order replay with a verified PRNG checkpoint; see `docs/SAVE-FORMAT.md`.

## Verification

- `cargo test` covers unit, integration, acceptance, AI, and deterministic fixture behavior.
- `luajit frontend/love/tests/run_all.lua` covers pure Lua order and UI-state behavior.
- `tests/fixtures/v2/duel.jsonl` is the golden end-to-end v2 replay.

Architecture decisions and supersession history live in `docs/adr/`.

Combat constants: `docs/combat-v2-tables.md`. Play guide: `docs/PLAY-V2.md`.
