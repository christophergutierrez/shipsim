# PRD - shipsim Slice 3: Movement Fidelity (D1, D2, D3)

Source alignment: `docs/CONTEXT-slice3.md`, `docs/adr/0007` (impulse movement), `docs/adr/0008`
(simultaneous resolution). Builds on slices 1-2 (committed). Pipeline stage: post-grill to-prd.
Autonomy: Checkpoint. Execution policy: cost_optimized.

## Problem Statement

Slices 1-2 gave us ships that move and fight, but the movement is a simplified "up to N hexes per
turn" policy resolved one ship at a time. It does not feel like Star Fleet Battles. The maintainer's
stated end-state requirements -- a 32-impulse turn driven by the Impulse Movement Chart, simultaneous
pre-plotted movement, and turn-mode enforcement -- are the signature mechanics that make the game
*feel* like SFB, and they are the biggest complexity driver still outstanding. Without them the
maneuver system has no real timing, no facing discipline, and no simultaneous-resolution tension;
the product is a toy, not a simulation.

## Solution

Replace the simplified movement policy with the authentic SFB movement heartbeat, inside the
headless `shipsim_core`. A turn becomes **32 impulses** driven by the canonical **Impulse Movement
Chart** (IMC): a ship at a given speed moves on exactly the impulses the chart says it does. The
player submits a pre-plotted **path** per ship; the system advances each ship one step along its
path on each of its move-impulses, resolving all ships' steps **simultaneously** per impulse.
**Turn-mode** is enforced at plot submission: a ship must move N hexes straight between facing
changes. A single `RunTurn` order plays out all 32 impulses atomically, then resolves any declared
fire at turn end. Combat mechanics are unchanged from slice 2. Correctness is proven by
seed-pinned headless `cargo test` asserting exact end positions and exact move-impulses.

## User Stories

1. As a player, I want my ship's speed to drive which impulses it moves on, via the Impulse Movement
   Chart, so that movement timing is authentic SFB.
2. As a player, I want to submit a pre-plotted path of adjacent hexes for my ship, so that I plan a
   whole turn of movement at once the way SFB is played.
3. As a player, I want my ship's facing to be implied by the direction of each step in its path, so
   that I do not issue facing separately.
4. As a player, I want turn-mode enforced: my ship must move N hexes straight between turns, so that
   facing changes cost maneuver and heavy ships turn sluggishly.
5. As a player, I want an illegal plot (non-adjacent steps, turn-mode violation, path through an
   occupied hex) rejected with a clear typed reason and no state change, so that state stays valid.
6. As a player, I want all ships' movement on a given impulse resolved simultaneously, so that no
   ship gains a positional advantage from being resolved first.
7. As a player, I want two ships moving into the same hex on the same impulse to both stop (collision),
   so that simultaneous movement is deterministic and fair.
8. As a player, I want a single `RunTurn` order to play out the entire turn (all 32 impulses), so
   that I drive a turn with one command.
9. As a player, I want my declared fire to resolve after all movement completes, at turn end, so
   that firing uses post-movement positions.
10. As a player, I want combat (to-hit, shields, destruction) to behave exactly as in slice 2, so
    that this slice changes only movement.
11. As a player, I want the scenario to check win conditions once at turn end, so that a mid-turn
    collision or position never produces a premature win.
12. As a player, I want the scripted ship to move under the same impulse rules as mine, so that the
    AI opponent is a real participant in simultaneous movement.
13. As a player, I want the JSON snapshot to show each ship's speed and turn_mode and the current
    impulse, so that I can see the movement situation (and render it later).
14. As a player, I want an impulse scenario loadable from TOML, so that a movement-fidelity duel is
    data-defined.
15. As a developer, I want the canonical SFB IMC encoded as a static lookup, so that "when does
    speed S move" has one source of truth.
16. As a developer, I want the IMC testable as a pure function (`moves_on_impulse(speed, impulse)`),
    so that the chart is verified independently of game state.
17. As a developer, I want the `Order` API to break cleanly (`Move`/`Face` -> `Plot`, `EndTurn` ->
    `RunTurn`) with no compatibility shim, so that there is one movement code path, not two.
18. As a developer, I want the declare/resolve seam preserved (`Plot` declares, `RunTurn` resolves),
    so that ADR-0002's design-for-fidelity invariant holds.
19. As a developer, I want speed to remain a fixed per-ship TOML field this slice, so that Energy
    Allocation (D7) stays cleanly deferred.
20. As a developer, I want the scripted ship to auto-generate a turn-mode-valid plot from its
    waypoints each turn, so that the AI needs no player input.
21. As a developer, I want the collision rule to be deterministic with no ship-order tie-break, so
    that runs are reproducible.
22. As a developer, I want movement rules to live entirely in `shipsim_core`, so that the CLI harness
    stays a thin driver (ADR-0001).
23. As a developer, I want the headline acceptance test to pin a scenario, run a turn, and assert
    exact end positions and exact move-impulses, so that the slice gate is falsifiable and exact.
24. As a developer, I want ship data to stay generic (no ADB-trademarked names/values), so that no
    trademarked content is shipped (ADR-0003).
25. As a maintainer, I want movement fidelity's deferred pieces (energy-driven speed D7,
    impulse-gated fire D1-fire, simultaneous fire D2-fire, pass-through rules) tracked in ROADMAP, so
    nothing is lost.

## Implementation Decisions

- **Modules.** Add an `impulse` module (the IMC static lookup) to `shipsim_core`; rewrite the
  movement order types and resolution in `movement`; rewrite the turn driver in `game_state`; extend
  `ship` with a stored per-turn plot; extend `schema`/loader for the `speed` field and the new
  scenario; extend `snapshot` with `impulse` and rename `speed_max` -> `speed`; update the CLI `bin`
  for the new order shapes; add an `impulse` scenario.
- **Impulse Movement Chart.** The canonical SFB 32-impulse x 32-speed table as a static lookup:
  `moves_on_impulse(speed: u8, impulse: u8) -> bool`. Speeds 0-31; speed 0 never moves. Impulses are
  1-indexed (1..=32). This is the single source of truth for when a ship moves. It is a movement
  schedule, not trademarked content (ADR-0003).
- **Speed.** A ship's `speed` (renamed from `speed_max`) is a fixed per-ship TOML field and *is* its
  IMC speed. The player does not choose speed per turn this slice (D7 deferred). A ship at speed S
  moves on exactly the impulses `moves_on_impulse(S, _)` returns true for.
- **Plot order.** New order variant `Plot { ship, path: Vec<Hex> }` replaces `Move` and `Face`. The
  path is a sequence of mutually adjacent hexes; the first step must be adjacent to the ship's
  current hex. Facing is implied by each step's direction (the hex direction from the previous
  position to the next). The old `Move` and `Face` orders are removed.
- **Plot validation (at submission).** `Plot` is validated when declared: (a) every step is adjacent
  to its predecessor and the first to the ship's hex; (b) no step passes through a hex occupied at
  submission time; (c) turn-mode holds -- between any two steps whose directions differ, there must be
  at least `turn_mode` straight steps since the last facing change. A violation rejects the whole
  plot with a typed `OrderError` and no mutation. Speed-0 ships and turn-free plots trivially pass.
- **RunTurn order.** New order variant `RunTurn` replaces `EndTurn`. It resolves the entire turn:
  for impulse 1..=32, collect every ship's next path step (only ships that move on that impulse per
  the IMC), apply all steps atomically, then handle collisions. After all 32 impulses, resolve
  declared `Fire` orders in declaration order, then check terminal conditions once. The old `EndTurn`
  order is removed.
- **Simultaneous per-impulse resolution.** On each impulse, the system gathers each moving ship's
  next step (the next hex along its plot). All steps are applied atomically: no ship is resolved
  before another within an impulse.
- **Collision rule.** If two ships end up in the same hex after an impulse's step, both stop in
  their pre-step hexes (no movement that impulse for either) and the scenario continues. Deterministic;
  no ship-order tie-break. Pass-through (two ships swapping adjacent hexes) is allowed this slice.
- **Fire timing.** `Fire` orders declared during a turn are stored and resolved at turn end, after
  all 32 movement impulses, in declaration order. Combat mechanics (to-hit, shields, bleed-through,
  destruction) are unchanged from slice 2 (ADR-0006). Bearing and shield-facing use post-movement
  positions. Impulse-gating fire is deferred.
- **Scripted ship.** When `RunTurn` is called, any scripted ship with no player-submitted plot
  auto-generates one: greedy step toward the next waypoint, up to the number of hexes its speed
  entitles it to this turn (count of move-impulses for its speed), validated against turn-mode
  (shortened to the longest turn-mode-valid prefix if needed). Reuses the existing waypoint logic.
- **Terminal check.** Win conditions (objective-hex from slice 1, destruction from slice 2) are
  checked once, at turn end, after all movement and fire. No mid-impulse wins.
- **Snapshot.** `StateSnapshot` gains `impulse: u8` (0 between turns, 1..=32 mid-turn -- though
  `RunTurn` is atomic so tests observe 0 after a turn). `ShipSnapshot` renames `speed_max` -> `speed`.
  `turn_mode` is already present.
- **Declare/resolve seam.** `Plot` declares (validates + stores); `RunTurn` resolves. The ADR-0002
  seam is preserved; only the order shapes change.
- **Determinism.** No new RNG this slice. The IMC is a pure static table; collision resolution is
  order-independent; scripted-ship plot generation is deterministic. The reproducibility invariant
  (ADR-0005) holds: same seed + same orders -> identical snapshot.

## Testing Decisions

- **Same single seam as slices 1-2:** the scenario-run surface of `shipsim_core` (load scenario,
  apply orders incl. `Plot`/`RunTurn`/`Fire`, read JSON snapshot and status). Tests assert external
  behavior, never movement internals.
- **One new sub-seam, at the highest point:** the IMC pure function `moves_on_impulse(speed, impulse)`
  is tested directly -- it is a pure lookup, the highest possible seam for that logic. No other new
  seams.
- **Headline acceptance gate (falsifiable, exact):** load an impulse scenario with two ships at known
  positions and speeds, submit plots, `RunTurn`, assert the exact end positions of both ships and
  that each ship moved on exactly the impulses the IMC prescribes for its speed. Must be demonstrably
  RED before the impulse system exists (baseline: `Plot`/`RunTurn` orders absent).
- **Behavioral unit tests at the seam:** IMC pure-function tests (speed 0 never moves; a known speed
  moves on the canonical impulses; out-of-range speed/impulse rejected); plot adjacency rejection;
  turn-mode violation rejection; occupied-hex-in-plot rejection; no-mutation-on-rejection; collision
  (both stop, no move that impulse); simultaneous resolution (no first-mover advantage -- assert
  symmetric outcome regardless of declaration order); fire resolves at turn end with post-movement
  positions; scripted ship auto-plots toward waypoint; **reproducibility**: same seed + same orders ->
  identical final snapshot.
- **Prior art:** slice-1/2 `tests/{acceptance,movement,combat,harness,tracer}.rs` -- same
  construct/drive/assert headless pattern; impulse tests mirror it. Existing tests are updated to the
  new `Plot`/`RunTurn` orders (clean break, no shim).

## Out of Scope

Energy Allocation and energy-driven speed (D7); impulse-gated fire windows / fire on specific
impulses (D1-fire); simultaneous fire resolution (D2-fire -- only movement is simultaneous this
slice; fire stays sequential in declaration order); itemized damage allocation / destroyable systems
(D6); seeking weapons; pass-through / overlap movement rules (collisions only consider destination
hexes this slice); reinforcement and advanced arcs. All tracked in ROADMAP.

## Further Notes

- This is the slice that makes movement feel like SFB. Keep the authentic *shape* (32 impulses, IMC,
  pre-plotted paths, turn-mode, simultaneous resolution) while holding the line on the
  fixed-speed and fire-at-turn-end simplifications.
- The `Order` API break is deliberate and cheap (greenfield, one consumer). A shim would mean two
  movement code paths coexisting -- the rewrite ADR-0002 says to avoid.
- The snapshot contract change is small (`impulse` field, `speed` rename) but deliberate: it is the
  interface for the future frontend and AI.

## Assumptions and Open Questions

Assumptions (from CONTEXT-slice3, low-risk):
1. The IMC is the canonical SFB table (speeds 0-31, impulses 1-32). It is a movement schedule, not
   trademarked content.
2. A plot's path is a sequence of mutually adjacent hexes; the first step must be adjacent to the
   ship's current hex.
3. A ship moves at most one hex per move-impulse (one step along the path), regardless of speed.
4. Turn-mode counts hexes moved straight since the last facing change; the ship's initial facing at
   turn start is its current facing, and the first path step's direction is the first facing.
5. A collision only considers the destination hex of a step; pass-through (swapping adjacent hexes)
   is allowed this slice.
6. Fire is resolved in declaration order at turn end; simultaneous fire (D2-fire) is deferred.
7. The scripted ship's auto-plot is greedy toward the next waypoint and never intentionally collides;
   if its greedy plot would collide, the collision rule handles it at impulse time.

Open questions for the spec audit to surface if material: none -- all 14 design branches were
resolved during grilling and recorded in ADR-0007/0008 and CONTEXT-slice3.
