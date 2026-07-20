# Product Requirements: shipsim

## Product

shipsim is a deterministic, turn-based starship combat game for players who want meaningful power-allocation and maneuver decisions without the bookkeeping of a full tabletop ruleset. The current product is Combat Model v2 with the protocol-v4 simplified turn loop (ADR-0025), played through thin clients (REPL reference, Love2D, ratatui TUI) or a headless JSON harness.

## Problem

Starship combat should make speed, facing, weapons, and defense compete for the same limited power. Earlier prototypes proved individual systems but left multiple incompatible combat loops and required too much implementation context to understand which one was authoritative.

The product needs one coherent loop that is playable, deterministic under test, and independent of its graphical client.

## Goals

- Make turn-start power allocation a consequential tradeoff among movement, weapons, and shields.
- Reward deliberate path planning while preserving useful turning and facing choices.
- Keep movement and simultaneous fire decisions readable and compact.
- Give beam, plasma, and torpedo weapons distinct range and damage profiles.
- Make facing and powered shields central to survival.
- Support complete battles against a basic AI through at least one thin client
  (REPL reference; Love2D and TUI also supported).
- Keep every rule in the Rust core and every automated run reproducible from a seed.

## Player experience

At the start of a turn, each ship converts its fixed power budget into motion
points, weapon charges, and shields. Every living ship then submits one ordered
path and the paths resolve simultaneously. Ships next submit one complete
volley, which resolves simultaneously before the engine advances to the next
turn.

Players can inspect hull, shields, weapon readiness, motion, facing, phase, and
recent combat results. Illegal input is rejected without losing the session. A
battle ends when its destruction objective is met.

## Functional requirements

### Power and path movement

- A ship allocates no more than its available power among motion, individual weapons, and six shield facings.
- Allocation is locked until the next turn.
- Engine power converts to motion points according to hull efficiency; simultaneous
  commitment means raw motion allocation does not establish initiative.

### Movement

- Each living ship submits exactly one ordered path per turn.
- Every path action costs one motion point from the allocation.
- Turning and occupancy obey hex-grid geometry and board policy.
- An empty path is valid and leaves the ship stationary.

### Combat

- A ship may commit multiple charged, unfired weapons during the volley stage.
- Range, mount arc, target geometry, shield-facing geometry, and charge determine legality.
- Beam damage scales with charge and declines gradually with range; half-up rounding applies and zero-damage shots are illegal.
- Plasma has strong close damage and steep range falloff.
- Torpedoes have flat damage within range.
- To-hit rolls use d20 range tables and the seeded PRNG.
- All commitments in a volley resolve simultaneously, including mutual kills.
- Powered facing shields deplete before overflow damages hull; hull zero destroys the ship.
- Each weapon fires at most once per turn.

### Interface and automation

- Clients support allocation, ordered paths, complete volleys, target-facing selection, and battle outcome display through the protocol-v4 order boundary.
- The CLI accepts NDJSON orders and emits snapshots suitable for clients, fixtures, and agents.
- Invalid orders yield structured soft errors and do not mutate state.
- Generic TOML ship and scenario data can define new battles without source changes.
- A basic AI can allocate, move toward enemies, fire legal weapons, and complete a battle.

## Non-functional requirements

- Runs with identical inputs and seed produce identical results.
- Core rules are testable without Love2D or a graphical environment.
- The client contains no independent combat-rule implementation.
- State mutation preserves phase, resource, controller, and geometry invariants.
- Product content remains generic rather than reproducing licensed source material.

## Current scope

The shipped MVP includes size-ladder and legacy hull data, v2 combat with the
protocol-v4 allocate/path/volley loop, multi-ship scenarios, deterministic AI,
protocol-v4 replay coverage, a reference REPL client, Love2D and ratatui TUI
clients, and in-process simulation (`shipsim-sim`).

## Out of scope

- Multiple selectable combat rulesets.
- Sensors, cloak, crew skills, electronic warfare, and detailed critical-hit charts.
- Full campaign persistence and campaign UI.
- Network multiplayer.
- Exact reproduction of a commercial tabletop ruleset.

## Success criteria

- A player can complete a duel through the REPL (agent reference) or Love2D/TUI without editing files.
- Golden fixtures cover allocate → path → volley through a deterministic outcome (`tests/fixtures/v4/`).
- Over-allocation, illegal movement, illegal fire, and malformed external orders are rejected cleanly.
- Core, REPL, TUI, and Lua headless test suites pass.
- Protocol v4 / ADR-0025 is the active turn loop in current documentation; older impulse/inertial loops are historical only.

## References

- Architecture: `docs/ARCHITECTURE.md`
- Turn loop: `docs/adr/0025-simplified-simultaneous-turns.md`
- Combat model base: `docs/adr/0020-combat-model-v2-momentum-power.md`
- Play guide: `docs/PLAY-V2.md`
- Working combat constants: `docs/combat-v2-tables.md`
- Protocol: `docs/PROTOCOL.md`
- Deferred work: `docs/ROADMAP.md`
