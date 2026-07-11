# Product Requirements: shipsim

## Product

shipsim is a deterministic, turn-based starship combat game for players who want meaningful power-allocation and maneuver decisions without the bookkeeping of a full tabletop ruleset. The current product is Combat Model v2, a single supported ruleset played through a Love2D client or a headless JSON harness.

## Problem

Starship combat should make speed, facing, weapons, and defense compete for the same limited power. Earlier prototypes proved individual systems but left multiple incompatible combat loops and required too much implementation context to understand which one was authoritative.

The product needs one coherent loop that is playable, deterministic under test, and independent of its graphical client.

## Goals

- Make turn-start power allocation a consequential tradeoff among movement, weapons, and shields.
- Reward maintaining momentum while preserving useful turning and reversal choices.
- Alternate small movement decisions with opportunities for simultaneous fire.
- Give beam, plasma, and torpedo weapons distinct range and damage profiles.
- Make facing and powered shields central to survival.
- Support complete battles against a basic AI in the Love2D client.
- Keep every rule in the Rust core and every automated run reproducible from a seed.

## Player experience

At the start of a turn, each ship allocates its fixed power budget. Movement allocation also establishes initiative. During each movement phase, eligible ships act once in stable order, spending power according to their momentum choice. A firing phase follows, where ships commit one or more charged weapons against geometry-legal targets and shield facings. Fire resolves simultaneously, then the move/fire cycle repeats until ships exhaust useful actions or end the turn.

Players can inspect hull, shields, weapon readiness, movement power, phase, initiative, and recent combat results. Illegal input is rejected without losing the session. A battle ends when its destruction objective is met.

## Functional requirements

### Power and initiative

- A ship allocates no more than its available power among movement, individual weapons, and six shield facings.
- Allocation is locked until the next turn.
- Higher movement allocation acts earlier; seeded random tie-breaking is resolved once and remains stable for the turn.

### Movement

- Each eligible ship takes at most one movement decision per movement phase.
- Continuing with momentum is cheaper than reversing it.
- Turning and occupancy obey hex-grid geometry and board policy.
- Passing is a valid decision and cannot strand the phase.

### Combat

- A ship may commit multiple charged, unfired weapons during a firing phase.
- Range, mount arc, target geometry, shield-facing geometry, and charge determine legality.
- Beam damage scales with charge and declines gradually with range; half-up rounding applies and zero-damage shots are illegal.
- Plasma has strong close damage and steep range falloff.
- Torpedoes have flat damage within range.
- To-hit rolls use d20 range tables and the seeded PRNG.
- All commitments in a firing phase resolve simultaneously, including mutual kills.
- Powered facing shields deplete before overflow damages hull; hull zero destroys the ship.
- Each weapon fires at most once per turn.

### Interface and automation

- The Love2D client supports allocation, movement, passing, multi-weapon fire commitment, shield-facing selection, turn ending, and battle outcome display.
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

The shipped MVP includes generic escort and heavy-cruiser data, v2 power/momentum combat, multi-ship scenarios, deterministic AI, a Love2D thin client, campaigns at the core/CLI level, and golden replay coverage.

## Out of scope

- Multiple selectable combat rulesets.
- Sensors, cloak, crew skills, electronic warfare, and detailed critical-hit charts.
- Full campaign persistence and campaign UI.
- Network multiplayer.
- A stable, versioned save-game format.
- Exact reproduction of a commercial tabletop ruleset.

## Success criteria

- A player can complete the v2 duel through Love2D without editing files.
- The acceptance replay covers allocation and at least two move/fire cycles through a deterministic outcome.
- Over-allocation, illegal movement, illegal fire, and malformed external orders are rejected cleanly.
- Core and Lua headless test suites pass.
- Only v2 is presented as the active product loop in current documentation.

## References

- Architecture: `docs/ARCHITECTURE.md`
- Product decisions: `docs/adr/0020-combat-model-v2-momentum-power.md`
- Play guide: `docs/PLAY-V2.md`
- Working combat constants: `docs/combat-v2-tables.md`
- Deferred work: `docs/ROADMAP.md`
