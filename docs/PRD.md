# PRD: shipsim Slice 1 (Movement Skeleton)

Source alignment: `CONTEXT.md`, `docs/adr/0001` through `0004`, `docs/ROADMAP.md`.
Pipeline stage: post-grill -> to-prd. Autonomy: Checkpoint. Execution policy: cost_optimized.

## Problem Statement

There is no starship tactical game to play. The maintainer wants a hex-grid tactical game in the
spirit of Star Fleet Battles, but SFB's full ruleset is enormous and cannot be built in one pass.
Before any combat or energy rules can exist, there must be a trustworthy foundation: ships that
occupy hexes, face directions, and move across a map under a turn structure, provable to be correct
without a graphical frontend. Today none of that exists (`shipsim/` is empty).

## Solution

Build `shipsim_core`, a pure-Rust headless simulation library (mirroring the NorRust architecture),
and drive it with a CLI/JSON harness. The first slice models **movement only**: two ships on a hex
map, a turn structure, 6-direction facing, simplified per-turn movement, one **player-plotted** ship
and one **scripted-waypoint** ship, and a scenario that resolves as `Won` when a ship reaches an
objective hex. Correctness is proven by headless `cargo test`. The engine is shaped so the deferred
high-fidelity SFB mechanics (32-impulse chart, simultaneous movement, turn-mode, combat, energy) can
be added as later slices without rearchitecting (ADR-0002).

## User Stories

1. As a player, I want to load a scenario from a file, so that a defined map, ships, and objective
   are set up without hand-coding state.
2. As a player, I want two ships placed on a hex map at scenario-defined positions and facings, so
   that the battle starts from a known configuration.
3. As a player, I want each ship to occupy exactly one hex, so that position is unambiguous.
4. As a player, I want each ship to have one of six facings, so that orientation is represented
   faithfully to a hex game.
5. As a player, I want to issue movement orders to my ship (advance, change facing), so that I can
   maneuver toward the objective.
6. As a player, I want my ship limited to at most its speed in hexes per turn, so that movement
   obeys the ship's capability.
7. As a player, I want to change my ship's facing between moves, so that I can steer (free turning in
   this slice; turn-mode enforcement is deferred).
8. As a player, I want illegal orders (off-map, beyond speed, into an occupied hex) rejected with a
   clear reason, so that I never reach an invalid game state.
9. As a player, I want the opposing ship to follow a scripted waypoint path defined in the scenario,
   so that the encounter is deterministic and repeatable.
10. As a player, I want a turn structure that advances both ships, so that play proceeds in discrete
    turns.
11. As a player, I want the scenario to resolve as `Won` the moment a ship enters the objective hex,
    so that there is a clear terminal condition.
12. As a player, I want to see the full game state as JSON after each order/turn (ship positions,
    facings, turn number, status), so that I can follow the game or render it later.
13. As a player, I want to run a scenario to completion from a scripted orders file, so that a whole
    game can be replayed deterministically.
14. As a player, I want to feed orders as JSON on stdin, so that an external driver (or future
    frontend/AI) can play through the same interface.
15. As a developer, I want all game rules to live in a headless core with no rendering dependency, so
    that rules are testable in milliseconds (ADR-0001).
16. As a developer, I want ship stats loaded from TOML, so that content is data-driven and editable
    without recompiling (ADR-0001).
17. As a developer, I want scenarios (map size, placements, waypoints, objective) loaded from TOML,
    so that new setups need no code.
18. As a developer, I want a deterministic engine (no RNG in this slice), so that tests assert exact
    end-states.
19. As a developer, I want the turn model expressed as an impulse-capable container, so that the
    32-impulse chart can be added later without reworking the turn loop (ADR-0002, D1).
20. As a developer, I want movement modeled as declare-order -> resolve-order, so that simultaneous
    resolution can be added later without inverting control flow (ADR-0002, D2).
21. As a developer, I want a `turn_mode` field present in the ship schema now (unenforced), so that
    enabling turn-mode later is validation logic, not a data migration (ADR-0002, D3).
22. As a developer, I want a stable JSON snapshot contract exercised by the harness and tests, so
    that a future graphical frontend has a proven contract to render (ADR-0004).
23. As a developer, I want ship data to use generic, SFB-inspired names and our own stat values, so
    that no trademarked content is shipped (ADR-0003).
24. As a developer, I want a headless integration test that plots a path to the objective and asserts
    the scenario resolves `Won`, so that the slice's acceptance gate is falsifiable and automated.
25. As a maintainer, I want deferred mechanics tracked in `docs/ROADMAP.md` with resume cues, so that
    nothing cut from slice 1 is lost.

## Implementation Decisions

- **Crate layout.** A `shipsim_core` library crate (Rust 2021, `crate-type = ["cdylib", "rlib"]`,
  `panic = "abort"`, deps `serde`/`serde_json`/`toml`/`thiserror`) plus a thin binary target for the
  CLI/JSON harness. Mirrors NorRust (ADR-0001).
- **Hex model.** Axial coordinate storage with cube conversion for distance/neighbor math; core is
  render-orientation-agnostic (flat/pointy-top is a frontend concern). Six neighbor directions
  index facing 0-5.
- **Facing.** Represented as a direction 0-5 aligned to the six hex neighbors. Facing changes are
  free in this slice (turn-mode deferred).
- **Ship model.** A ship has an id, a generic class/name, a position (hex), a facing, and a
  `speed_max` (max hexes/turn), plus a `turn_mode` field carried but unenforced. Loaded from
  `data/ships/*.toml`.
- **Movement.** Simplified: a ship may move up to `speed_max` hexes per turn. Movement is expressed
  as declare-order -> resolve-order (D2 hook). Sequential resolution in this slice: the
  player-plotted ship resolves, then the scripted ship.
- **Turn structure.** A turn is modeled as an impulse-capable container (D1 hook) even though this
  slice applies a single simplified movement policy per turn rather than a 32-impulse chart.
- **Orders / API.** The core exposes an order-application surface (apply a validated order; advance a
  turn/step). An order is one of: move to an adjacent hex, change facing, end turn. Orders are
  validated; invalid orders are rejected with a typed error (`thiserror`) and do not mutate state.
- **Opponent.** The scripted ship's per-turn intent is a waypoint list in the scenario TOML; the
  engine advances it toward its next waypoint deterministically.
- **Scenario model.** `scenarios/*.toml` defines map dimensions, ship placements (position + facing +
  ship class ref), the scripted ship's waypoints, and the objective hex.
- **Map.** Bounded rectangle sized by the scenario; no wraparound; off-map is an illegal order.
- **Terminal condition.** Scenario status is one of `InProgress` or `Won`. Status becomes `Won` the
  instant any ship's position equals the objective hex (single objective, first ship to reach it).
  Slice 1 has no distinct win/lose state, so `Won` means only that the objective hex was reached; the
  acceptance scenario routes the player ship to the objective.
- **Snapshot / JSON contract.** A serializable snapshot (ships with position/facing, turn number,
  scenario status) is emitted as JSON after each applied order and at turn boundaries. This is the
  contract the harness prints and tests assert against, and the future frontend renders (ADR-0004).
- **CLI/JSON harness.** A binary that: loads a scenario + ship data; runs to completion from a
  scripted orders file, or reads orders as JSON on stdin; prints JSON snapshots. Deterministic.
- **Determinism.** No RNG anywhere in this slice.
- **Content.** Generic ship TOML (e.g. "Heavy Cruiser"), shipsim-chosen stats (ADR-0003).

## Testing Decisions

- **Test the external behavior, not internals.** Tests drive the public order/snapshot surface (load
  a scenario, apply orders, read the snapshot), never private movement internals. This keeps the
  declare/resolve and impulse-container refactors (D1/D2) free to evolve.
- **Primary seam (single, highest possible): the scenario-run surface of `shipsim_core`.** Load a
  scenario, apply a sequence of orders (or run a scripted orders file), observe the JSON snapshot and
  scenario status. All acceptance is expressed here. This is the ideal one-seam target.
- **Acceptance gate (falsifiable).** An integration test loads the slice-1 scenario, plots the
  player ship along a legal path into the objective hex, advances turns, and asserts the snapshot
  reports `status == Won`. It must be demonstrably red before the movement/terminal logic exists.
- **Unit-level behavioral tests** at the same seam where cheaper: hex distance/neighbor correctness;
  facing values stay 0-5; move beyond `speed_max` rejected; off-map move rejected; move into an
  occupied hex rejected; scripted ship follows its waypoints deterministically; a run that never
  reaches the objective stays `InProgress`.
- **Prior art:** NorRust's headless `tests/` (`simulation.rs`, `scenario_validation.rs`,
  `test_ffi.rs`), the same pattern: construct/load state, drive it headlessly, assert outcomes.
  Mirror that structure.
- **Determinism enables exact assertions:** with no RNG, tests assert exact end positions/facings and
  exact terminal status.

## Out of Scope

Everything in `docs/ROADMAP.md` D1-D10, specifically: the full 32-impulse turn / Impulse Movement
Chart (D1); simultaneous plot-then-resolve movement (D2); turn-mode enforcement (D3); SFB
fixed/floating map and off-map subtleties (D4); all combat (phasers, disruptors, shields, damage
allocation, ship destruction) (D5-D6); Energy Allocation (D7); any graphical frontend (D8); AI
opponent beyond scripted waypoints (D9); multi-ship/scenario/campaign structure beyond a single 1v1
with one objective (D10). These are deferred-but-designed-for (D1/D2/D3 have slice-1 hooks) per
ADR-0002.

## Further Notes

- The whole point of slice 1 is a *trustworthy foundation*: reviewers should reject any slice-1
  shortcut that would preclude the deferred mechanics (ADR-0002), while still keeping slice-1
  behavior minimal.
- The JSON snapshot contract is load-bearing for the eventual frontend decision (ADR-0004); treat
  its shape as an early, deliberately-designed interface, not an afterthought.

## Assumptions and Resolved Decisions

Assumptions (from CONTEXT.md, low-risk; carried into slice 1):
1. Axial hex coords with cube conversion; core orientation-agnostic.
2. Bounded rectangular map from scenario TOML, no wraparound, off-map illegal.
3. Harness supports both a scripted orders file and JSON-on-stdin.
4. No RNG in slice 1 (fully deterministic).
5. `speed_max` is a fixed per-ship max hexes/turn (energy-driven speed arrives with D7).
6. Single objective hex, first ship to enter resolves the scenario; no draw/timeout terminal in
   slice 1 (a run that never reaches the objective stays `InProgress`).

Resolved decisions (previously open questions, resolved from the captured context in CONTEXT.md and
the ADRs):
- Turn limit / draw terminal (was Q1): Slice 1 has no turn limit and no draw or timeout terminal.
  The status enum is exactly `InProgress` and `Won`; a run that never reaches the objective stays
  `InProgress`, and tests do not depend on a timeout. Rationale: the sole acceptance gate is that
  reaching the objective yields `Won`, and terminal complexity is deferred with the rest of the
  ruleset. A draw or turn-limit terminal is a candidate for a later slice and can be tracked in
  `docs/ROADMAP.md` when it is needed.
- Occupied hex (was Q2): One ship per hex. Moving into an occupied hex is an illegal order, rejected
  with a typed error and no state change. Rationale: user stories 3 and 8 and the "Orders / API" and
  "Movement" implementation decisions already require unambiguous single-hex occupancy; allowing
  stacking would contradict them.
- Impulse ordering (was Q3): Per-turn order application is sufficient for slice 1; per-impulse
  ordering is not implemented now. Rationale: the impulse-capable turn container (the D1 hook) exists
  so the 32-impulse chart can be added later (ADR-0002, D1) without reworking the turn loop, so no
  per-impulse ordering is required until that slice lands.
