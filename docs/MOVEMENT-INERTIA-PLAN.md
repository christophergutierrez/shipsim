# Phased Plan: Persistent Velocity and Inertial Movement

Status: Accepted (ADR-0022). M0–M2 complete; M3 in progress.

Related design TODO: `docs/TODO.md`

## Goal

Replace movement-power-as-distance with persistent inertial movement:

- Ships have a velocity from 0 through 4 and a course direction.
- Velocity and course persist between turns.
- Ships coast without spending power.
- Turn-start engine power becomes a finite thrust reserve.
- Hull engine efficiency converts power to thrust at different rates.
- Thrust changes velocity, course, or hull facing; it does not directly buy distance.
- Each turn contains four movement/fire cycles.
- Every movement phase permits a maneuver, then scheduled ships translate simultaneously.
- Fire remains simultaneous after every movement phase.

## Frozen Rules for Initial Implementation

These rules are the baseline for implementation. Changes require updating this plan and its tests before code changes.

### Turn sequence

```text
Allocate power

For phase 1 through 4:
    Commit one maneuver or coast decision per living ship
    Resolve maneuvers simultaneously
    Translate scheduled ships simultaneously
    Resolve destination conflicts
    Commit and resolve fire simultaneously

End turn
Preserve velocity and course
Clear unused thrust and other turn-scoped allocation
```

### Movement schedule

| Velocity | Translation phases |
|---:|---|
| 0 | none |
| 1 | 4 |
| 2 | 2, 4 |
| 3 | 1, 2, 4 |
| 4 | 1, 2, 3, 4 |

Maneuvers occur before translation. The resulting velocity determines eligibility in the current phase.

### Motion state

```text
velocity: 0..=4
course: hex direction 0..=5 when velocity > 0
facing: independent hex direction 0..=5
thrust_remaining: turn-scoped integer
movement_phase: 1..=4
```

When velocity is zero, course retains the last course for display. A stopped ship may select any course as part of its next acceleration order.

### Relationship to existing `speed` field

The existing ship-data `speed` field (used today as the movement-power cap in `allocate_v2`) is retired and replaced by `max_velocity`. During M2, `speed` is kept as a load-time fallback: if a ship definition omits `max_velocity`, it is derived as `speed` so existing data loads without edits. M9 removes the `speed` field entirely.

### Engine efficiency

Ship data gains an explicit rational conversion:

```toml
thrust_per_power = 2
power_per_thrust = 1
max_velocity = 4
```

Exactly one conversion direction may exceed one:

```text
thrust = engine_power * thrust_per_power / power_per_thrust
```

Division is integer truncation performed last: multiply `engine_power` by `thrust_per_power` first, then divide by `power_per_thrust`. For example, a capital hull with `thrust_per_power = 1`, `power_per_thrust = 2`, and 1 engine power yields `(1 * 1) / 2 = 0` thrust (unusable remainder); 2 engine power yields `(2 * 1) / 2 = 1` thrust.

Initial examples:

| Hull behavior | Conversion |
|---|---|
| Tiny | 1 power -> 4 thrust |
| Escort | 1 power -> 2 thrust |
| Cruiser | 1 power -> 1 thrust |
| Capital | 2 power -> 1 thrust |
| Huge | 4 power -> 1 thrust |
| Starbase | max velocity 0; no thrust generation |

Every mobile ship definition must be capable of buying at least one thrust with its design power.

### Maneuvers

Each ship submits exactly one maneuver decision per movement phase:

- `Coast`: no thrust cost.
- `Accelerate`: velocity +1; cost 1 thrust. When stopped, the order includes the desired course and establishes it without an additional course-change cost.
- `Decelerate`: velocity -1; cost 1 thrust.
- `TurnCoursePort`: course -60 degrees; cost equal to current velocity, minimum 1.
- `TurnCourseStarboard`: course +60 degrees; same cost.
- `RotatePort`: facing -60 degrees; cost 1 thrust.
- `RotateStarboard`: facing +60 degrees; cost 1 thrust.

Velocity remains within `0..=max_velocity`. A course maneuver changes course by at most 60 degrees in one movement phase.

Reversal is compositional, not a separate cheap maneuver. A ship must decelerate to zero, select the opposite course while stopped, then accelerate. Reversing speed `v` into opposite speed `n` therefore costs at least `v + n` thrust and takes multiple movement phases.

### Simultaneous translation

- Maneuver decisions are committed before any are resolved.
- After maneuver resolution, scheduled ships attempt one-hex translation along course.
- Ships may cross paths or exchange origin hexes.
- Multiple ships attempting the same empty destination are all blocked and remain at their origins.
- A ship attempting to enter a hex occupied by a non-departing ship is blocked.
- Blocked movement does not erase velocity or refund thrust.
- Collision damage is deferred.
- Hard-map exits are illegal maneuver outcomes: the ship remains in place and retains velocity. Floating maps recenter after the simultaneous batch.

## Cross-Cutting Changes

- The active NDJSON protocol must increment from version 1 because order and snapshot contracts change.
- Replay saves must increment with the protocol and reject incompatible movement orders.
- Golden fixtures must be regenerated only after the new contract is accepted.
- Existing momentum `Keel` state becomes obsolete after migration.
- Initiative based on allocated movement power becomes obsolete. Maneuver commitment is simultaneous; deterministic resolution order is only an implementation detail.
- The simulation runner must control both sides through the new maneuver orders before Love2D migration begins.

### Legacy bridge strategy (M2–M5)

The current movement path is hard-wired to the exact fields being replaced. `apply_v2_move` (`src/movement.rs:185`) reads `ship.keel` and `ship.move_remaining`; `build_v2_move_order` (`src/game_state.rs:319`) sorts by `movement_allocated`; `active_v2_mover` (`src/game_state.rs:359`) filters on `move_remaining > 0`; `can_any_move` / `ship_has_useful_hex_move` (`src/game_state.rs:650`) read `keel` and `move_remaining`; and `v2_move_decision` (`src/ai.rs:96`) reads `keel` and `move_remaining`. These functions will not compile once the underlying fields change.

To keep each milestone independently deliverable, a **legacy bridge** is maintained from M2 through M5:

- During M2–M3, `move_remaining` and `keel` are kept as derived fields. `allocate_v2` writes `thrust_remaining` and also writes a synthetic `move_remaining = thrust_remaining` so the legacy `Phase::Movement` path continues to compile and function. `keel` is left at its current default.
- During M3, `build_v2_move_order` and `active_v2_mover` are bridged to sort/filter on `thrust_remaining` instead of `movement_allocated`/`move_remaining`, but the single-active-mover `Phase::Movement` structure is preserved until M4 replaces it.
- During M4, the legacy `apply_v2_move` / `can_any_move` / `ship_has_useful_hex_move` path is deleted and replaced by the simultaneous maneuver/translation core. The bridge is no longer needed.
- The AI layer (`src/ai.rs:96` `v2_move_decision`) is bridged with a stub that returns `None` (coast) during M3–M6, then replaced with real maneuver logic in M7.

This means M2 and M3 remain individually deliverable: the bridge keeps the legacy path compiling and the existing test suite green while the new fields are added alongside the old ones.

### Turn-loop termination under inertia

The current turn loop does not end after one movement/fire cycle. `resolve_fire_phase_v2` (`src/game_state.rs:632`) re-enters movement whenever `can_any_move() || can_any_legal_fire()`. Under inertia, a coasting ship with velocity > 0 always "can move" by translation, so the old `can_any_move` heuristic would loop forever.

Under the new model, the turn is bounded by the four-phase schedule. The turn ends after phase 4's fire window completes — there is no `can_any_move` re-entry heuristic. The `can_any_move` / `ship_has_useful_hex_move` predicate (`src/game_state.rs:650`) is deleted in M4 and replaced by the fixed four-phase counter. A red test in M4 covers a coasting ship (velocity > 0, thrust = 0) to confirm the turn terminates after exactly four phases.

## Milestone Rules

Every milestone follows red/green discipline:

1. Add the named failing tests and demonstrate failure for the intended missing behavior.
2. Implement only the milestone scope.
3. Run the focused green gate.
4. Run all previously completed milestone gates.
5. Do not continue while tests are skipped, vacuous, flaky, or passing for the wrong reason.

Generated reports and traces stay under ignored `tmp/`.

## M0: Decision Freeze and Characterization

### Scope

- Accept an ADR for persistent velocity, independent facing/course, four cycles, and simultaneous maneuver resolution.
- Characterize current allocation, movement, fire alternation, save replay, protocol errors, and Love order-building behavior.
- Record the intentional breaking-contract boundary.

### Red tests

No production behavior changes in M0. Add ignored or compile-failing test sketches only if needed to establish test names; do not merge permanently ignored gates.

### Green gate

```bash
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
luajit frontend/love/tests/run_all.lua
```

### Exit criteria

- ADR accepted.
- No unresolved rule questions remain for M1-M4.
- Existing suites are green before behavior changes.

## M1: Pure Motion and Thrust Rules

### Scope

- Add pure `Velocity`, course, movement schedule, maneuver, and thrust-cost types/functions.
- Add rational engine conversion validation.
- Do not integrate `GameState`, orders, scenarios, protocol, or UI.

### Red tests

- `velocity 0..4` maps to the exact four-phase schedule.
- Acceleration and deceleration clamp or reject outside hull limits.
- Course changes wrap across directions 0 and 5.
- Course-change cost equals current velocity with minimum cost 1.
- Facing rotation costs 1 without changing course.
- Tiny/escort/cruiser/capital/huge conversion examples produce 4/2/1/0-with-remainder/0-with-remainder thrust as specified.
- Invalid zero denominators and immobile/mobile contradictions reject.

### Green gate

```bash
cargo test motion::
cargo test thrust::
```

### Exit criteria

- All motion math is deterministic and independent of aggregate state.
- No floating-point arithmetic exists in thrust conversion.

## M2: Ship Data and Persistent Motion State

### Scope

- Add thrust conversion and `max_velocity` to ship schema/data.
- Add persistent velocity/course and turn-scoped thrust reserve to ships/snapshots.
- Load existing mobile hulls with explicit values.
- Add an immobile generic starbase definition and scenario-load coverage.
- Preserve old movement behavior temporarily behind the existing order path via the legacy bridge (see Cross-Cutting Changes): `move_remaining` and `keel` remain as derived fields so `apply_v2_move`, `build_v2_move_order`, `active_v2_mover`, `can_any_move`, and `v2_move_decision` continue to compile and function.

### Red tests

- Mobile ship data unable to buy one thrust rejects at load.
- Starbase accepts `max_velocity = 0` and rejects nonzero initial velocity.
- Scenario initial velocity above hull maximum rejects.
- Snapshot exposes velocity, course, facing, thrust conversion, and thrust remaining.
- Turn reset preserves velocity/course but clears thrust remaining.

### Green gate

```bash
cargo test --test motion_schema
cargo test --test scenario_motion
cargo test --test snapshot_motion
```

### Exit criteria

- Every tracked ship and scenario loads with explicit motion data.
- Persistence semantics are covered before state-machine integration.

## M3: Engine Allocation Becomes Thrust

### Scope

- Replace movement-point allocation with engine-power allocation converted into thrust reserve.
- Bridge initiative onto `thrust_remaining`: `build_v2_move_order` (`src/game_state.rs:319`) sorts by `thrust_remaining` instead of `movement_allocated`; `active_v2_mover` (`src/game_state.rs:359`) filters on `thrust_remaining > 0`. The single-active-mover `Phase::Movement` structure is preserved until M4.
- Keep the current phase entry temporarily, but prevent legacy movement orders from consuming the new reserve. The legacy bridge (see Cross-Cutting Changes) keeps `move_remaining` as a derived mirror of `thrust_remaining` so `apply_v2_move` and `can_any_move` still compile.
- Bridge the AI layer: `v2_move_decision` (`src/ai.rs:96`) returns `None` (coast) during M3–M6 until M7 replaces it with real maneuver logic.

### Red tests

- Tiny ship allocating 1 engine power receives 4 thrust.
- Huge ship allocating 4 engine power receives 1 thrust.
- Cruiser allocating 1 receives 1.
- Allocation above total ship power rejects without mutation.
- Fractional conversion truncation and unusable remainder are deterministic.
- Engine allocation remains locked for the turn.
- Coasting with zero engine allocation remains possible for a ship already moving.

### Green gate

```bash
cargo test --test thrust_allocation
cargo test --test v2_allocate
```

### Exit criteria

- Power tradeoffs work for all conversion ratios.
- Initiative no longer depends on raw reactor expenditure.

## M4: Four-Phase Maneuver and Translation Core

### Scope

- Add four movement phases per turn.
- Add maneuver commitment/readiness orders.
- Resolve maneuvers and translations simultaneously.
- Implement dynamic schedule eligibility after maneuver resolution.
- Implement destination conflicts and hard/floating map behavior.
- Delete the legacy bridge: remove `apply_v2_move`, `can_any_move`, `ship_has_useful_hex_move`, and the `move_remaining`/`keel` derived fields. The turn loop terminates after phase 4 instead of re-entering via `can_any_move`.
- No firing integration yet; use a test-only phase advance after translation.

### Red tests

- Speed 0/1/2/3/4 ships translate on exact phases.
- Accelerating 2 -> 3 in phase 1 causes immediate phase-1 translation.
- Decelerating 3 -> 2 in phase 1 prevents phase-1 translation.
- Coast consumes no thrust and preserves motion.
- Turn course, rotate facing, accelerate, and decelerate consume exact thrust.
- Insufficient-thrust maneuver rejects without mutation.
- Every living ship commits exactly once before resolution.
- Resolution result is independent of commitment order.
- Same-destination attempts block all contenders.
- Hex swaps and path crossings succeed.
- Entry into a non-departing occupied hex blocks.
- Hard-map exit blocks without changing velocity.
- Floating-map recenter runs once after the simultaneous batch.
- Course can change by at most 60 degrees per phase.
- A speed-4 reversal cannot occur in one phase.
- A coasting ship (velocity > 0, thrust = 0) terminates the turn after exactly four phases (no infinite loop).

### Green gate

```bash
cargo test --test inertial_movement
cargo test --test simultaneous_translation
cargo test --test movement_boundaries
```

### Exit criteria

- Four complete movement phases terminate without manual state mutation.
- Results are deterministic across ship insertion and commitment order.

## M5: Fire Interleaving and Complete Turn Loop

### Scope

- Interleave one simultaneous fire phase after each resolved translation phase.
- Return from fire to the next movement phase until phase 4 completes.
- Preserve velocity/course and clear thrust/allocation at turn end.
- Ensure destruction during fire removes ships from later maneuver commitments without corrupting already-committed simultaneous effects.

### Red tests

- Exactly four movement/fire windows occur in a full nonterminal turn.
- Range and shield facing use post-translation positions and current hull facing.
- Rotating facing without changing course changes weapon arc and shield geometry.
- Mutual destruction still resolves from pre-fire state.
- A destroyed ship is excluded from later movement phases.
- End turn preserves velocity/course and clears thrust.
- A coasting ship repeats its schedule next turn without engine allocation.
- The loop terminates when no meaningful actions remain or the scenario ends.

### Green gate

```bash
cargo test --test inertial_turn_loop
cargo test --test v2_fire
cargo test --test v2_turn_loop
```

### Exit criteria

- A two-turn headless duel demonstrates persistent inertia and interleaved fire.
- No legacy movement phase remains reachable through production orders.

## M6: Protocol v2, Saves, and Golden Replay

### Scope

- Increment NDJSON protocol version.
- Replace legacy move/pass orders with maneuver commit/ready semantics.
- Add motion fields and movement-phase index to snapshots.
- Increment save document version and retain deterministic replay verification.
- Regenerate golden fixtures after tests prove the new contract.
- Keep `Order::Move` and `Order::PassMove` variants in the enum (as deserialization-only) until M9 so that old saves can be parsed. They are rejected at the `apply_order` level as unsupported, not at deserialization. Version rejection happens at `SaveDocument::read` (`src/save.rs:66`) via the existing `protocol_version` check — v1 saves are rejected by version, not by order shape.

### Red tests

- Protocol-v1 movement orders soft-reject as unsupported without mutation.
- Missing maneuver commitments cannot advance the movement phase.
- Protocol-v2 snapshots contain velocity, course, facing, thrust, phase index, and commitments.
- Protocol-v1 saves reject by version at `SaveDocument::read` (`src/save.rs:66`), not by order-shape errors during replay.
- Save/resume during each of the four movement/fire windows matches uninterrupted replay byte-for-byte at the final snapshot.
- Malformed or illegal maneuver orders remain soft errors.

### Green gate

```bash
cargo test --test harness
cargo test --test save_resume
cargo test --test inertial_protocol
cargo test --test acceptance
```

### Exit criteria

- Only protocol v2 is emitted or accepted externally.
- Golden fixtures are regenerated and byte-locked.

## M7: Simulation Policies and Gameplay Rubrics

### Scope

- Teach all five baseline policies to allocate engine power and choose maneuvers.
- Replace the AI bridge stub (`v2_move_decision` returning `None` since M3) with real maneuver-selection logic.
- Add motion metrics and inertial red conditions.
- Add hull-efficiency comparison suites without introducing ship construction yet.

### Red tests

- Same scenario, policies, and seed produce identical inertial traces.
- Every policy completes maneuver readiness without illegal orders.
- Simulation reports velocity distribution, thrust spent, coasting distance, course changes, facing rotations, blocked moves, and reversals.
- Large ships moving zero hexes too frequently can fail a rubric.
- Tiny ships maximizing movement, weapons, and shields simultaneously can fail a rubric.
- Initiative metrics no longer correlate with raw engine power by construction.
- Mirrored scenario results expose side bias.

### Green gate

```bash
cargo test --test simulation_invariants
cargo test --test simulation_determinism
cargo test --test simulation_rubrics
cargo run --release --bin shipsim-sim -- \
  --suite simulation/suites/inertial-smoke.toml \
  --output tmp/simulation/reports/inertial-smoke.json
```

### Exit criteria

- Blocking simulation safety rubrics pass.
- Balance results are recorded even when advisory rubrics remain red.

## M8: Love2D Maneuver UI

### Scope

- Replace legacy movement controls with velocity/course/facing display and maneuver controls.
- Show four movement/fire phases, scheduled translation, thrust remaining, and projected maneuver cost.
- Submit simultaneous maneuver commitments through protocol v2.
- Keep rule validation in Rust.

### Red tests

- Lua builders emit every protocol-v2 maneuver shape.
- UI state distinguishes course from facing.
- Controls disable or surface core errors for insufficient thrust and velocity bounds.
- Schedule display matches velocity 0 through 4.
- Maneuver readiness waits for every living controlled ship.
- Defeat/victory flow survives all four cycles.
- Layout rubric detects clipped maneuver controls at tracked viewports.

### Green gate

```bash
luajit frontend/love/tests/run_all.lua
cargo test --test harness
```

Manual gate:

- Play two turns while coasting, accelerating, decelerating, changing course, and rotating independently.
- Verify the board, HUD, click targets, and phase indicators at every tracked viewport.

### Exit criteria

- A complete inertial duel is playable without editing orders by hand.
- No movement rule is duplicated in Lua.

## M9: Legacy Removal and Acceptance Package

### Scope

- Remove `Keel`, momentum move costs, old movement orders, old snapshot fields, obsolete fixtures, and superseded documentation.
- Add an end-to-end acceptance replay covering inertia across at least two turns.
- Update PRD, architecture, play guide, protocol, save format, roadmap, and handoff.

### Red tests

- Acceptance scenario begins with one stationary and one already-moving ship.
- Replay covers acceleration, coast, course change, independent facing rotation, deceleration, four fire windows, and turn persistence.
- Reversal test proves stop-then-opposite acceleration cost and phase duration.
- Repository search gate finds no production references to `Keel`, `move_remaining`, `movement_allocated`, old move/pass protocol shapes, or movement-power-as-distance terminology. The `Order::Move` and `Order::PassMove` deserialization-only variants are removed.
- Full simulator and Love suites use only inertial movement.

### Green gate

```bash
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
luajit frontend/love/tests/run_all.lua
git diff --check
```

Run and archive under `tmp/`:

```bash
cargo run --release --bin shipsim-sim -- \
  --suite simulation/suites/inertial-balance.toml \
  --output tmp/simulation/reports/inertial-balance.json
```

### Exit criteria

- All blocking gates pass.
- Protocol/save migration is documented.
- Legacy movement code is deleted rather than left dormant.
- Residual red balance or aesthetic rubrics are recorded as gameplay findings, not hidden.

## Deferred Beyond This Plan

- Ship-construction money and configurable loadouts.
- System-capacity enforcement beyond the data hooks in `docs/TODO.md`.
- Collision damage, ramming, tractor effects, and formation movement.
- Continuous vectors or movement between hex directions.
- More than velocity 4.
- Hidden full-turn plotting.
- Networked simultaneous commitment.
- LLM policies and qualitative trace judges.

## Open Questions

- **Weapon refire across fire windows:** `fired_weapons_this_turn` currently persists across the whole turn. Under four fire windows, can a weapon fire in phase 1 and again in phase 3? The current model says no (one fire per weapon per turn). This plan preserves that semantics; if per-window refire is desired, it requires a separate design change.
- **Love2D test bridge:** Love2D tests exercise legacy `Order::Move` building. They will stay green through M0–M7 (the bridge keeps `Order::Move` in the enum) then flip at M8 when the UI switches to maneuver orders. M8 must update Love2D test builders in the same milestone.
