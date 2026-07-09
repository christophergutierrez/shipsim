# Architecture Design Loop -- Slice 3 (movement fidelity)

Mode: converge. MAX_PASSES: 3. TEST_COMMAND: `cargo test` and
`cargo clippy --all-targets -- -D warnings`.

Baseline HEAD before loop: `75693b8` (slice 3 feature). Workspace clean; 51 tests green.

## Scope (top modules)

`game_state`, `movement`, `turn` (new), `impulse`, `combat`, `hex`, `ship`, `snapshot`,
`scenario`, CLI harness.

## Pass 0 -- Parallel review (Depth / Seams / Domain / Tests)

### Critical / High (deduped)

| id | severity | lens | summary |
|----|----------|------|---------|
| SEAM-01 | Critical | Seams | Cycles game_state <-> movement / combat; orchestration on state bag |
| SEAM-02 / declare-resolve-hollow | High | Seams/Depth | RunTurn/plot/fire driver lived on GameState; movement was facade |
| SEAM-03 / fire-legality-dup | High | Seams/Domain | Fire legality copied in declare + turn-end resolve |
| plot-lifecycle / ship-anemic | High | Depth | Plot/Ship as bags; deferred as larger type design |
| imc-plot-length-split | High | Domain/Seams | path max used raw speed; IMC used clamp 31 |
| tests-fire-deferred | High | Tests | No Fire-without-RunTurn characterization |
| tests-post-move-fire | High | Tests | No post-movement range skip test |
| tests-path-facing | High | Tests | Facing from path unasserted |
| tests-imc-runturn-bind | High | Tests | Headline gate weak on IMC integration (deferred) |

## Pass 1 -- Characterization tests (low blast)

**Target:** test safety net for fire timing, facing, collision residual plot.

**Findings addressed:** tests-fire-deferred, tests-post-move-fire, tests-path-facing,
tests-collision-residual, tests-pass-through-misnamed.

**Changes:**
- `test_fire_without_run_turn_no_damage`
- `test_fire_skipped_when_target_moves_out_of_range`
- `test_plot_implies_facing_after_run_turn`
- `test_collision_clears_remaining_plot`
- rename misleading pass-through test to `test_plot_rejects_currently_occupied_hex`

**Verify:** cargo test green.

## Pass 2 -- Shared domain rules (low/medium blast)

**Target:** single fire legality predicate + single plot length from IMC.

**Findings addressed:** SEAM-03, imc-plot-length-split (High domain/seams).

**Changes:**
- `combat::fire_legality` + `FireIllegal` used by declare and turn-end resolve
- `impulse::max_plot_steps` used by `validate_plot` and scripted generation

**Verify:** cargo test + clippy green.

## Pass 3 -- Turn orchestration module (medium blast)

**Target:** deepen movement seam; move 32-impulse driver off GameState.

**Findings addressed:** SEAM-02 (partial), movement-module-is-order-facade (partial),
SEAM-01 (partial -- game_state no longer imports movement/combat).

**Intended shape:**
- `GameState` = passive aggregate + mutators (`store_plot`, plot cursor, pending fire)
- `movement` = Order DTO, declare/validate, `apply_order`, resolve dispatch
- `turn` = RunTurn policy (scripted plot, impulses, collision, deferred fire)
- `impulse` / pure combat helpers remain leaves

**Changes:**
- new `src/turn.rs`
- `GameState::apply_order` / `run_turn` body removed from state bag
- call sites use `movement::apply_order(&mut game, order)`
- dead `fire_attacker_index` removed with index cleanup as part of surface shrink (index helpers that remain are still used by combat)

**Blast radius:** Medium (API method move `GameState::apply_order` -> free function; no wire/JSON change).

**Verify:** cargo test (55) + clippy green.

## Residual -- RFC (High blast; not applied)

### RFC: break combat <-> GameState cycle and deepen Ship/Plot

#### Files
- `src/combat.rs`, `src/game_state.rs`, `src/ship.rs`, `src/turn.rs`, `src/movement.rs`

#### Problem
`combat::resolve_fire` still takes `&mut GameState` (PRNG + ships vec). `Ship` remains an
anemic public bag. `DeclaredOrder` still mirrors wire `Order` rather than validated intents.
Full encapsulation of `ships`/`board` would change the public Rust surface used by tests.

#### Proposed Change (future slice / follow-up)
1. `resolve_fire(attacker, weapon, target, prng) -> FireOutcome` pure-ish; apply damage via
   `Ship::apply_damage`.
2. Optional `Plot` domain type with cursor methods.
3. Optional `ImcSpeed` / `Facing` newtypes at schema boundary.
4. Narrow `GameState` public fields behind snapshot + accessors (High blast for tests).

#### Benefits
True DAG: primitives -> entities -> pure rules -> orchestration -> IO.

#### Blast Radius
High: combat API, ship encapsulation, many test setup sites that poke `ship_mut` fields.

#### Risks
Large mechanical rewrite; risk of silent combat drift without seeded characterization suite.

#### Test Plan
Existing combat seed-pinned tests + fire deferral / post-move skip tests from this loop.

#### Alternatives
Keep status quo for combat until D6 SSD forces Ship depth.

#### Stop Conditions
Human approval required before encapsulating `Ship` public fields or changing combat signatures.

## Final verdict

- **verdict:** PASS (converge stopped: no remaining Critical that is safe to fix without RFC;
  highest-value Medium work landed; residual High encapsulated in RFC)
- **passes used:** 3
- **refactored modules:** `combat`, `impulse`, `movement`, `game_state`, new `turn`; tests + CLI
- **resolved:** SEAM-03, imc-plot-length-split, tests High (most), SEAM-02 (partial), SEAM-01 (partial)
- **persisting (RFC):** full cycle elimination via combat purity; Ship/Plot domain types; public
  field encapsulation; stronger IMC-vs-RunTurn schedule integration test
- **regressed:** none
- **tests:** `cargo test` green (55); `cargo clippy --all-targets -- -D warnings` clean
