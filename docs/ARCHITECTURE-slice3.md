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

## Residual RFC -- REALIZED (state encapsulation)

Approved by maintainer: encapsulate `GameState` now. Landed as a dedicated plan
(`implementation-plan-state-encapsulation.md`) and commit series after this review.

### Delivered
1. `GameState` fields private; public reads via accessors (`status`, `impulse`, `turn_number`,
   `seed`, `board`, `objective`, `ship`, `ships`).
2. Explicit setup mutators (`set_ship_pos`, `set_ship_facing`, `set_ship_shields`,
   `set_ship_structure`, weapon configure helpers) replace public `ship_mut` / field poking.
3. `combat::resolve_fire(attacker, weapon_id, target, prng)` — no `GameState`.
4. `Ship::apply_hit` owns shield/structure/destroyed application.
5. `GameState::apply_fire` is the only bridge (PRNG + ship lookup) for turn resolution.

### Still deferred (optional)
- Full `Ship` field privacy / newtypes (`ImcSpeed`, `Facing`)
- First-class `Plot` type with methods
- `DeclaredOrder` as validated intents rather than wire-isomorphic shapes

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
