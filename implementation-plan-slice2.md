# Implementation Plan: shipsim Slice 2 -- Direct-fire Combat (D5)

## Planning Verdict
- verdict: READY
- task_tier: standard
- tier_trigger: Multi-file feature extending an existing crate (combat module, ship/schema/snapshot/game_state/movement edits) with limited, additive contract growth and no removal of used behavior. The JSON snapshot is an internal, unreleased wire format with no external consumer, so the sec-16 blast-radius trigger for "wire format" does NOT fire (see Blast-Radius note). No security/auth/billing/CI/persisted-data surface.
- execution_policy: cost_optimized
- model_routing: current-model-only
- model_tiers: current model (claude-opus-4-8[1m]) serves fast / standard / reasoning; tier labels document intent only.
- reason: Additive combat capability over a green slice-1 baseline; every combat gate is proven RED at HEAD ea3a39c because the fire order, combat scenario, shields, structure, and seed do not exist yet. Tracer-bullet-first sequencing with a seed-pinned destruction-win headline gate.

## Repository State (Staleness Contract)
- VCS HEAD: `ea3a39c8006dc0a518e23264047605384c999432`
  <- command: `git -C /mnt/storage/git_home rev-parse HEAD` -> `ea3a39c8006dc0a518e23264047605384c999432`
- Git toplevel is `/mnt/storage/git_home` (a multi-project working tree). shipsim lives at `/mnt/storage/git_home/shipsim` (symlinked as `/home/chris/git_home/shipsim`). Only shipsim paths are in scope; other projects' dirty entries (question2crux/*, untracked sibling dirs) are unrelated and MUST be preserved untouched.
- Dirty files in scope (shipsim):
  - `shipsim/docs/ROADMAP.md` -- modified (D5 marked IN PROGRESS). Preserve; Milestone M7 edits it further.
  - Untracked (preserve; these are this slice's source docs): `shipsim/docs/CONTEXT-slice2.md`, `shipsim/docs/PRD-slice2.md`, `shipsim/docs/adr/0005-seeded-prng-for-combat.md`, `shipsim/docs/adr/0006-combat-model-slice2.md`.
  <- command: `git -C /mnt/storage/git_home status --short` -> ` M shipsim/docs/ROADMAP.md` plus `??` untracked shipsim docs.
- Discovery timestamp: 2026-07-04.
- Existing user changes to preserve: the modified ROADMAP.md and the four untracked slice-2 docs above; all sibling-project dirty state outside `shipsim/`.

## Repository Findings
Confirmed facts. Citations are symbol names + re-derivable commands (no line numbers -- the files are short and evolve; the executor re-runs the grep to confirm the seam, per the Staleness Contract):
- Baseline is green: 18 tests pass across 6 suites. <- command: `cargo test` -> `18 passed (6 suites, 0.20s)`. Test names enumerated via `cargo test -- --list` (18 entries: acceptance/movement/harness/tracer suites).
- The order seam is `GameState::apply_order` -> `movement::declare` (validate, no mutation) -> `movement::resolve` (mutate, then `refresh_status`). <- command: `grep -n "fn apply_order\|fn declare\|fn resolve\|refresh_status" src/game_state.rs src/movement.rs`.
- `Order` enum (serde `tag="type", rename_all="snake_case"`) has only `Move`, `Face`, `EndTurn`; NO `Fire`. <- command: `grep -n "enum Order\|Move\|Face\|EndTurn\|Fire" src/movement.rs` -> Move/Face/EndTurn present, Fire absent. RED baseline for the fire order.
- `Ship` has `id, class, pos, facing, speed_max, turn_mode` only -- NO shields/structure/weapons. <- command: `grep -n "pub " src/ship.rs` (six fields); `grep -rn "shield\|structure\|weapon" src/ship.rs src/snapshot.rs` -> none. RED baseline for combat state.
- `ScenarioStatus` is `{ InProgress, Won }`; `GameState::refresh_status` sets `Won` iff a ship sits on `objective` (objective-hex terminal only). <- command: `grep -n "enum ScenarioStatus\|fn refresh_status\|== self.objective" src/game_state.rs`. RED baseline for the destruction terminal.
- No seed / PRNG / rng anywhere. <- command: `grep -rn "seed\|prng\|rng" src/` -> none. RED baseline for ADR-0005 seed serialization.
- No combat scenario file. <- command: `ls scenarios/` -> `movement.toml, slice1.toml, slice1_orders.jsonl, tracer.toml`. RED baseline for a data-defined duel.
- Determinism guard: `thread_rng`/`SystemTime`/`Instant` appear nowhere; `rand` is NOT a dependency. <- command: `grep -rn "thread_rng\|SystemTime\|Instant" src/ tests/` -> none; `grep -n rand Cargo.toml` -> none. This is the amended ADR-0005 guard's current (passing) baseline.
- Dependencies are serde, serde_json, toml, thiserror only; `crate-type=["cdylib","rlib"]`, `panic="abort"` (dev/release), `panic="unwind"` (test). <- `Cargo.toml`.
- Hex math: `Hex::distance` (cube), `Hex::direction(facing:u8)->Option<Hex>` maps a facing index 0..=5 to a unit delta via `DIRECTIONS`. There is NO inverse (delta -> facing / bearing) function yet; the arc check and shield-facing selection both need one, AND both must fold in the ship's `facing` (see DD5). <- command: `grep -n "fn distance\|fn direction\|const DIRECTIONS" src/hex.rs`. This is the one non-trivial new algorithm (see M2).
- Scenario schema requires `objective: HexDef` and `ships: Vec<ShipPlacementDef>`; the loader unconditionally validates `objective` is on-board; a scripted ship is one with `controller=="scripted"` or non-empty `waypoints`. <- command: `grep -n "struct ScenarioDef\|objective\|struct ShipPlacementDef\|controller\|waypoints\|validate_on_board" src/schema.rs src/scenario.rs`.
- Ship-class TOML lives at `<root>/data/ships/<class>.toml`, resolved relative to the scenario file's grandparent dir. <- command: `grep -n "fn load_ship_def\|data\|ships" src/scenario.rs`.
- Slice-1 snapshot tests assert `json["objective"]["q"|"r"]` and per-ship fields, so `StateSnapshot.objective` and existing `ShipSnapshot` fields are load-bearing and must not be renamed/removed. <- command: `grep -rn "objective\|ships\[0\]" tests/acceptance.rs tests/tracer.rs`.

Baseline status / pre-existing failures: none. Full suite green.
Unsafe/unrun commands: none; all discovery commands are read-only and were run.
Context docs read: PRD-slice2.md, CONTEXT-slice2.md, ADR-0001..0006, ROADMAP.md. None missing.
Unknowns requiring spikes: none. The only non-trivial algorithm (hex bearing delta->facing) is knowable and directly testable, so it is a normal milestone (M2), not a spike.

## Requested Outcomes & Non-Goals (standard: informal enumeration; outcomes keyed for traceability)

Outcomes (from PRD user stories US1-US22, condensed to verifiable end states):
- O1 data-driven weapons: ships load weapons (type, arc, max range, resolution params) from TOML.
- O2 fire order: `fire(weapon_id, target_ship_id)` order parses and resolves via declare/resolve.
- O3 arc gate: fire rejected when target not in the weapon's firing arc.
- O4 range gate: fire rejected when target beyond the weapon's max range.
- O5 phaser damage: phaser rolls damage from a range-based table (seeded PRNG), exact for a pinned seed.
- O6 disruptor to-hit: disruptor rolls to-hit vs a by-range threshold (seeded PRNG); can miss/hit, exact for a pinned seed.
- O7 shield facing: incoming damage hits the shield facing on the bearing from defender to attacker.
- O8 bleed-through: damage beyond a facing's remaining strength reduces the single structure pool in the same step; a non-overflowing hit leaves structure unchanged; a depleted facing stays down for the scenario.
- O9 destruction: structure <= 0 marks the ship destroyed; a destroyed ship resolves no further fire order.
- O10 destruction-win terminal: scenario status becomes `Won` when the scenario's designated enemy ship is destroyed; slice-1 objective-hex terminal still works for movement scenarios.
- O11 illegal-fire rejection: out-of-arc, out-of-range, unknown weapon, unknown/self target, and refire-this-turn are rejected with a typed `OrderError` and zero state change (snapshot byte-identical).
- O12 snapshot growth: snapshot shows each ship's 6 shield facings, structure, destroyed flag, and weapon state; state shows the seed.
- O13 combat scenario: a two-armed-ships duel loads from TOML with the destruction terminal.
- O14 seeded PRNG: all dice come from a seeded generator owned by game state (ADR-0005).
- O15 seed serialized: the seed is in the snapshot/state.
- O16 declare/resolve reuse: fire resolves through the existing declare->resolve seam (D2 hook), deterministic order by ship id.
- O17 fire-freely: weapons fire with no energy/arming cost this slice.
- O18 single structure pool: internals are one pool, not itemized systems.
- O19 rules in core: all combat logic lives in `shipsim_core`; the CLI stays a thin driver.
- O20 headline gate: a seed-pinned test fires until the enemy is destroyed and asserts `status == Won` and enemy destroyed.
- O21 generic data: ship/weapon content uses generic names/values (ADR-0003).
- O22 roadmap: D5 marked realized; D6/D5a/D7/D1/D2/D10 remain explicitly deferred.

Explicit non-goals (deferred, tracked in ROADMAP -- reviewers must reject any milestone that BUILDS these):
- NG-D6 itemized damage allocation / destroyable SSD systems (single structure pool only).
- NG-D5a seeking weapons / other weapon classes / reinforcement / richer arcs.
- NG-D7 energy arming / Energy Allocation Form (weapons fire freely).
- NG-D1 32-impulse firing windows (this slice uses one-shot-per-weapon-per-turn as the simplified stand-in; it must NOT implement the impulse chart).
- NG-D2 simultaneous fire resolution (only the declare/resolve SEAM is reused; resolution stays sequential/deterministic).
- NG-D10 multi-ship / fleet combat beyond 1v1.

## Facts, Assumptions, and Decisions

Confirmed facts: see Repository Findings (all cited).

Working assumptions (low-risk; from CONTEXT-slice2 / PRD, adopted without a human gate):
- A1 one shot per weapon per turn, no refire within a turn (simplified stand-in for D1 impulses). This slice ENFORCES it by tracking fired weapons per turn and resetting on `end_turn`, mirroring the existing `moves_this_turn` reset pattern (`grep -n "moves_this_turn|fn end_turn" src/game_state.rs`). Refire is an O11 rejection.
- A2 range = hex distance between firing and target ship (`Hex::distance`).
- A3 arc check and shield-facing selection share the same hex-direction math (facings 0..=5).
- A4 overflow past a facing bleeds to internals in the same step; no partial carry-over.
- A5 1v1 duel; one target per fire order; no friendly fire.
- A6 weapons fire freely (no energy cost); revisited at D7.
- A7 seed comes from the scenario TOML with a fixed default; tests pin it.

Design decisions (made here, follow from ADRs; none require a human gate):
- DD1 PRNG: implement a small, explicit, seedable, serializable generator (a SplitMix64-style `u64`-state stepper) INSIDE `shipsim_core`, rather than adding the `rand` crate. ADR-0005 requires "a small, explicit, seedable generator ... advanced deterministically"; a hand-rolled stepper satisfies it exactly, keeps the dependency set minimal (ADR-0001), and makes the serialized PRNG position a single `u64`. ADR-0005 permits `rand`; this decision prefers no new dependency (Ponytail: stdlib/native over a new crate). The determinism guard therefore forbids `thread_rng`/`SystemTime`/`Instant`; `rand` simply never enters `Cargo.toml`.
- DD2 terminal selection: `ScenarioDef` gains an optional `[terminal]` (default = objective-hex, preserving slice 1). A combat scenario declares `type = "destruction"` and a `target` ship id; `Won` fires when that ship's structure <= 0. `objective` becomes `Option` in the schema/state, required (validated on-board) ONLY for the objective-hex terminal. For a combat (destruction) scenario there is NO objective hex -- `refresh_status` must return `InProgress` whenever `objective` is `None` and the destruction terminal has not been reached (this is the pre-M5 behavior too, see DD6). The dummy-objective fallback is explicitly forbidden for combat scenarios (a live dummy hex could spuriously fire `Won`). This is data-driven (ADR-0006 "a scenario declares which terminal applies") and additive.
- DD3 destroyed ships are MARKED (`destroyed: bool`), not removed from the ship list, so the snapshot ship array stays stable for the future frontend; a destroyed ship is skipped as an actor during resolution and its fire orders are rejected/no-op.
- DD5 facing-relative bearing (CORRECTS the naive absolute-bearing model): both the arc check and the shield-facing selection MUST fold in the ship's own `facing`, or rotating a ship (a `Face` order) would change neither offense nor defense and slice 1's maneuver payoff would be inert (ADR-0006: "Facing now matters for both defense ... and offense"; PRD US3, US7). Define a pure geometry helper `bearing_to(from, to) -> u8` (world hex direction 0..=5). Then: (a) target is in arc iff `arc_contains(weapon.arc, (bearing_to(attacker.pos, target.pos) + 6 - attacker.facing) % 6)`; (b) the shield facing hit is `(bearing_to(defender.pos, attacker.pos) + 6 - defender.facing) % 6`. A gate MUST prove a `Face` order changes which shield is hit / whether a target is in arc, so the facing input cannot be silently dropped.
- DD6 terminal sequencing: because `resolve` calls `refresh_status` on every order, the combat scenario is evaluated by the terminal logic from M1 onward, but the destruction branch does not land until M5. Therefore M1 already implements the `objective: None -> InProgress` rule (DD2) and asserts the freshly loaded combat scenario reports `InProgress`; M5 adds the destruction branch on top. No milestone leaves the combat scenario able to spuriously win.
- DD4 deterministic resolution order: within a turn, declared fire orders resolve one at a time ordered by acting-ship id (D2 deferred). This fixes the PRNG draw sequence for reproducibility (ADR-0005). This slice applies fire orders one per `apply_order` call (the harness/tests drive them sequentially), so the ordering rule is realized as "resolve on receipt, deterministically"; no simultaneous batching is built (NG-D2).

Decisions needing human approval: none. (No architecture/public-behavior/persisted-data/security/scope change beyond the agreed slice.)

## Outcome Traceability Matrix

| outcome_id | outcome (explicit/implied) | milestone_id(s) | invariant_id(s) | final_check | baseline_verified |
| --- | --- | --- | --- | --- | --- |
| O1 | data-driven weapons (explicit) | M1, M2 | inv-weapons-loaded | `cargo test test_combat_scenario_loads_weapons` | yes: no weapon fields (grep src/ship.rs) |
| O2 | fire order via declare/resolve (explicit) | M1 | inv-fire-order-exists | `cargo test test_tracer_fire_damages_shield` | yes: no Fire variant (grep src/) |
| O3 | out-of-arc rejected, facing-relative (explicit) | M2 | inv-illegal-fire-rejected, inv-facing-correct | `cargo test test_out_of_arc_rejected test_face_order_changes_arc_eligibility` | yes: no fire path, facing inert |
| O4 | out-of-range rejected (explicit) | M2 | inv-illegal-fire-rejected | `cargo test test_out_of_range_rejected` | yes: no fire path |
| O5 | phaser range-table damage (explicit) | M4 | inv-seeded-exact | `cargo test test_phaser_damage_by_range_pinned_seed` | yes: no combat resolution |
| O6 | disruptor to-hit by range (explicit) | M4 | inv-seeded-exact | `cargo test test_disruptor_miss_then_hit_pinned_seed` | yes: no combat resolution |
| O7 | correct (facing-relative) shield facing hit (explicit) | M2(bearing), M3 | inv-facing-correct | `cargo test test_damage_hits_bearing_facing test_face_order_changes_hit_shield` | yes: no shields, facing inert |
| O8 | bleed-through to structure (explicit) | M3 | inv-bleed-through | `cargo test test_overflow_bleeds_then_stops` | yes: no structure |
| O9 | destruction at 0 structure (explicit) | M5 | inv-destruction | headline `cargo test test_fire_until_destroyed_wins` | yes: no destroyed state |
| O10 | destruction-win terminal; objective terminal preserved (explicit) | M5 | inv-destruction, inv-objective-terminal-regress | headline test + `cargo test test_player_reaches_objective_wins` | yes: refresh_status objective-only |
| O11 | illegal fire rejected, no mutation (explicit) | M2 | inv-illegal-fire-rejected, inv-no-mutation | `cargo test test_illegal_fire_no_mutation` | yes: no fire path |
| O12 | snapshot shows shields/structure/destroyed/weapons (explicit) | M1 | inv-snapshot-shape | `cargo test test_combat_snapshot_shape` | yes: ShipSnapshot lacks fields |
| O13 | combat scenario TOML (explicit) | M1 | inv-weapons-loaded | `cargo test test_combat_scenario_loads_weapons` | yes: no combat scenario |
| O14 | seeded PRNG owned by game state (explicit) | M1, M6 | inv-reproducible, inv-determinism-guard | `cargo test test_same_seed_same_orders_identical` | yes: no prng |
| O15 | seed serialized in snapshot (explicit) | M1 | inv-snapshot-shape, inv-reproducible | `cargo test test_combat_snapshot_shape` | yes: no seed |
| O16 | declare/resolve reuse, deterministic order (explicit) | M1, M5 | inv-reproducible | `cargo test test_same_seed_same_orders_identical` | yes: seam exists, fire absent |
| O17 | fire freely, no energy cost (implied non-goal boundary) | M2 | inv-no-energy-field | `grep -rn "energy" src/` returns nothing | yes: no energy concept |
| O18 | single structure pool (explicit constraint) | M1, M3 | inv-single-structure | `grep`-checked: one `structure` field, no per-system pools | yes: no structure |
| O19 | combat rules in core (explicit) | M1..M6 | inv-thin-cli | `git diff --stat src/bin/shipsim.rs` shows only order-plumbing lines | yes: CLI is generic order driver |
| O20 | seed-pinned destruction headline gate (explicit) | M5 | inv-destruction | headline `cargo test test_fire_until_destroyed_wins` | yes: fire/terminal absent |
| O21 | generic ship/weapon data (explicit) | M1 | inv-generic-data | `grep` combat TOML for trademarked names returns nothing | yes: existing data generic |
| O22 | roadmap D5 realized, others deferred (implied) | M7 | inv-roadmap-updated | `grep` ROADMAP for D5 landed + D6/D5a/D7/D1/D2/D10 present | yes: D5 marked IN PROGRESS |
| -- | NG-D1/D2/D6/D5a/D7/D10 non-goals | (none) | inv-no-energy-field, inv-single-structure | absence greps | n/a (non-goal guards) |

No orphan rows: every outcome maps to >=1 milestone; every milestone (M1-M7) maps back to outcomes. NG rows are guarded by absence invariants, not built by milestones.

## Final-State Invariants
Cheap per-pass subset: inv-fire-order-exists, inv-illegal-fire-rejected, inv-no-mutation, inv-facing-correct, inv-bleed-through, inv-seeded-exact, inv-destruction, inv-reproducible (from M6), inv-determinism-guard, inv-snapshot-shape, inv-no-energy-field, inv-single-structure, inv-generic-data (all are fast `cargo test` / `grep`). Full suite (adds inv-objective-terminal-regress, inv-thin-cli git-diff check, inv-roadmap-updated) at phase-end / final.

```yaml
- id: inv-fire-order-exists
  statement: A fire order parses (type="fire") and resolves through declare/resolve, landing damage.
  category: presence
  check: cargo test test_tracer_fire_damages_shield -- --exact
  baseline_polarity: FAIL (does not compile / test absent; no Fire variant)
  post_condition: PASS after M1
  failure_reasoning: Order::Fire absent (grep -rn "Fire" src/ -> none).
  scope: every-pass
  cost: cheap
  rationale: O2, O16
  evidence: grep -rn "Fire" src/ -> none

- id: inv-weapons-loaded
  statement: A combat scenario loads two armed ships with weapons parsed from TOML.
  category: presence
  check: cargo test test_combat_scenario_loads_weapons -- --exact
  baseline_polarity: FAIL (no combat scenario, no weapon schema)
  post_condition: PASS after M1
  failure_reasoning: scenarios/ has no combat file; ShipDef has no weapons.
  scope: phase-end
  cost: cheap
  rationale: O1, O13
  evidence: ls scenarios/ -> no combat file; grep weapon src/schema.rs -> none

- id: inv-snapshot-shape
  statement: Snapshot exposes per-ship shields[6], structure, destroyed, weapons; state exposes seed. Existing objective-hex scenarios keep `objective` present/unchanged; combat scenarios serialize `objective` as null/omitted. No existing ShipSnapshot/StateSnapshot field is renamed or removed.
  category: presence
  check: cargo test test_combat_snapshot_shape -- --exact  (asserts shields len 6, structure, destroyed, weapons with a concrete weapon field, top-level seed, and combat objective null/omitted); slice-1 snapshot tests still assert objective present.
  baseline_polarity: FAIL (ShipSnapshot lacks these fields; StateSnapshot lacks seed)
  post_condition: PASS after M1
  failure_reasoning: ShipSnapshot has only id/class/q/r/facing/speed_max/turn_mode; StateSnapshot has no seed.
  scope: every-pass
  cost: cheap
  rationale: O12, O15
  evidence: `grep -n "struct ShipSnapshot\|struct StateSnapshot\|objective" src/snapshot.rs` (no shields/structure/destroyed/weapons/seed; objective is a required HexSnapshot)

- id: inv-illegal-fire-rejected
  statement: out-of-arc, out-of-range, unknown weapon, unknown/self target, and refire all return a typed OrderError.
  category: presence
  check: cargo test test_out_of_arc_rejected test_out_of_range_rejected test_fire_at_self_rejected test_unknown_weapon_rejected test_unknown_target_rejected test_refire_rejected
  baseline_polarity: FAIL (no fire validation exists)
  post_condition: PASS after M2
  failure_reasoning: declare() has no Fire arm.
  scope: every-pass
  cost: cheap
  rationale: O3, O4, O11
  evidence: `grep -n "fn declare\|Fire" src/movement.rs` (declare has Move/Face/EndTurn arms only, no Fire)

- id: inv-no-mutation
  statement: A rejected fire order leaves the snapshot byte-identical.
  category: presence
  check: cargo test test_illegal_fire_no_mutation -- --exact
  baseline_polarity: FAIL (fire path absent)
  post_condition: PASS after M2
  failure_reasoning: no fire order to reject; mirrors slice-1 no-mutation tests (tests/movement.rs).
  scope: every-pass
  cost: cheap
  rationale: O11
  evidence: `grep -n "rejected_without_mutation|to_value(before)" tests/movement.rs` (slice-1 no-mutation pattern)

- id: inv-facing-correct
  statement: Damage depletes the shield facing on the defender-relative bearing to the attacker -- `(bearing_to(defender.pos, attacker.pos) + 6 - defender.facing) % 6` -- so the defender's own facing changes which shield is hit; and arc eligibility uses the attacker-relative bearing to the target (DD5). Ship facing is a live input to both.
  category: presence
  check: cargo test test_damage_hits_bearing_facing test_bearing_delta_to_facing test_face_order_changes_hit_shield test_face_order_changes_arc_eligibility
  baseline_polarity: FAIL (no shields, no bearing function, facing inert)
  post_condition: PASS after M2 (bearing + relative arc) + M3 (relative shield application)
  failure_reasoning: Hex has direction(facing)->delta but no delta->facing inverse; Ship has no shields; nothing folds facing into hit/arc selection.
  scope: every-pass
  cost: cheap
  rationale: O7, O3 (arc), ADR-0006 facing-matters
  evidence: `grep -n "fn direction\|DIRECTIONS" src/hex.rs` (only facing->delta exists, no inverse); `grep -rn "shield" src/ship.rs` -> none

- id: inv-bleed-through
  statement: Overflow past a facing reduces structure in the same step; a non-overflowing hit leaves structure unchanged; a depleted facing stays down.
  category: presence
  check: cargo test test_overflow_bleeds_then_stops test_underflow_leaves_structure test_depleted_facing_stays_down
  baseline_polarity: FAIL (no structure, no shields)
  post_condition: PASS after M3
  failure_reasoning: no structure pool exists.
  scope: every-pass
  cost: cheap
  rationale: O8, O18
  evidence: `grep -rn "structure" src/` -> none

- id: inv-seeded-exact
  statement: Phaser damage-by-range and disruptor to-hit-by-range are exact for a pinned seed AND genuinely range-dependent -- the test pins recorded values (no-drift) and additionally asserts a structural range relationship, so a constant table cannot pass: phaser damage at range 1 differs from damage at max range, and a disruptor miss yields zero damage while a hit yields nonzero.
  category: presence
  check: cargo test test_phaser_damage_by_range_pinned_seed test_disruptor_miss_then_hit_pinned_seed
  baseline_polarity: FAIL (no resolution tables, no prng)
  post_condition: PASS after M4
  failure_reasoning: no combat resolution or prng exists; a constant-return table would satisfy an equality-only gate, so range-dependence is pinned separately.
  scope: every-pass
  cost: cheap
  rationale: O5, O6, O14
  evidence: `grep -rn "prng\|phaser\|disruptor" src/` -> none

- id: inv-destruction
  statement: Under the destruction terminal, the combat scenario stays InProgress until the destroying shot, then firing until the enemy's structure reaches 0 marks it destroyed and sets status == Won, seed pinned. The Won is isolated to the destruction terminal -- the combat scenario has no objective hex (objective None), so a passing gate cannot be a coincidental objective-hex match.
  category: presence
  check: cargo test test_fire_until_destroyed_wins -- --exact   (HEADLINE: asserts status InProgress before the fatal shot, status == Won and enemy destroyed == true after, and no ship occupies an objective hex / objective is None)
  baseline_polarity: FAIL (no destroyed state, refresh_status objective-only)
  post_condition: PASS after M5
  failure_reasoning: ScenarioStatus/refresh_status have no destruction terminal; refresh_status wins only on `ship.pos == self.objective`.
  scope: every-pass
  cost: cheap
  rationale: O9, O10, O20
  evidence: `grep -n "fn refresh_status\|== self.objective\|enum ScenarioStatus" src/game_state.rs`

- id: inv-objective-terminal-regress
  statement: The slice-1 objective-hex win still passes unchanged.
  category: regression
  check: cargo test test_player_reaches_objective_wins test_non_winning_run_stays_in_progress -- --exact
  baseline_polarity: PASS (works at HEAD)
  post_condition: PASS at final (unchanged)
  failure_reasoning: adding the destruction terminal must not alter objective-hex behavior.
  scope: final
  cost: cheap
  rationale: O10 (preserve slice 1)
  evidence: cargo test -> test_player_reaches_objective_wins passes at ea3a39c

- id: inv-reproducible
  statement: Same seed + same order sequence -> byte-identical final snapshot (behavioral proof of ADR-0005 reproducibility).
  category: presence
  check: cargo test test_same_seed_same_orders_identical -- --exact
  baseline_polarity: FAIL (no prng/seed/combat, so the test cannot exist at baseline)
  post_condition: PASS after M6
  failure_reasoning: reproducibility of seeded combat cannot be demonstrated until the seeded resolution exists.
  scope: every-pass (from M6 onward)
  cost: cheap
  rationale: O14, O16
  evidence: `grep -rn "seed\|prng" src/` -> none

- id: inv-determinism-guard
  statement: thread_rng / SystemTime / Instant never appear in src or tests, and `rand` never enters Cargo.toml (the amended ADR-0005 guard: seeded randomness allowed, ambient randomness/clock forbidden).
  category: regression
  check: grep -rn "thread_rng\|SystemTime\|Instant" src/ tests/  (expect no output)  AND  grep -n "^rand" Cargo.toml  (expect no output)
  baseline_polarity: PASS (determinism holds now: no forbidden APIs, no rand dep)
  post_condition: PASS at final (still no ambient nondeterminism after seeded combat added)
  failure_reasoning: introducing ambient randomness or a clock would break reproducibility (ADR-0005).
  scope: every-pass
  cost: cheap
  rationale: O14, O16, DD1
  evidence: `grep -rn "thread_rng\|SystemTime\|Instant" src/ tests/` -> none; `grep -n rand Cargo.toml` -> none

- id: inv-no-energy-field
  statement: No energy/arming cost concept enters this slice (NG-D7 boundary).
  category: absence
  check: grep -rniE "energy|arming|power_alloc|capacitor|charge" src/   (expect no combat-cost usage)
  baseline_polarity: PASS-absent now; this guards against building D7. Recorded as a non-goal guard (naming heuristic, not an exhaustive proof), not a removal gate.
  post_condition: still absent at final
  failure_reasoning: an energy budget field would mean D7 was built (scope creep).
  scope: phase-end
  cost: cheap
  rationale: O17, NG-D7
  evidence: grep -rni energy src/ -> none

- id: inv-single-structure
  statement: Internals are a single structure pool; no itemized/per-system damage pools (NG-D6 boundary). Encoded structurally: the Ship struct has exactly one `structure` field and no per-system collection.
  category: absence
  check: `grep -c "structure" src/ship.rs` -> the Ship struct declares exactly one `structure: u32` field; `grep -rniE "system_damage|allocation_chart|systems\[|Vec<.*System" src/` -> none. (The keyword grep is a naming heuristic against the naive D6 implementation; the "exactly one structure field" check on Ship is the load-bearing structural guard.)
  baseline_polarity: PASS-absent now; guards against building D6.
  post_condition: still absent at final
  failure_reasoning: itemized SSD systems would mean D6 was built.
  scope: phase-end
  cost: cheap
  rationale: O18, NG-D6
  evidence: `grep -rniE "system_damage|allocation" src/` -> none; Ship currently has no structure field at all

- id: inv-generic-data
  statement: Shipped combat TOML uses generic names/values (no ADB-trademarked content).
  category: absence
  check: grep -riE "federation|klingon|romulan|gorn|tholian|kzinti|lyran|star fleet|adb" data/ scenarios/ (expect none)
  baseline_polarity: PASS-absent now (existing data generic; naming heuristic, not exhaustive); guard holds through new files.
  post_condition: still absent at final
  failure_reasoning: trademarked content would violate ADR-0003.
  scope: phase-end
  cost: cheap
  rationale: O21
  evidence: data/ships/*.toml use "Heavy Cruiser"/"Escort"

- id: inv-thin-cli
  statement: src/bin/shipsim.rs stays a thin driver -- it deserializes orders and prints snapshots, holding no combat rules.
  category: regression
  check: git diff ea3a39c -- src/bin/shipsim.rs shows no rule logic (only generic Order plumbing, if any); combat logic lives under src/.
  baseline_polarity: PASS (CLI is a generic order/snapshot driver now)
  post_condition: PASS at final
  failure_reasoning: combat rules leaking into the binary would violate ADR-0001/0019.
  scope: final
  cost: cheap
  rationale: O19
  evidence: `grep -n "apply_order|from_str|StateSnapshot" src/bin/shipsim.rs` (generic serde order plumbing, no rules)

- id: inv-roadmap-updated
  statement: ROADMAP marks D5 realized and still lists D6, D5a, D7, D1, D2, D10 as deferred.
  category: presence
  check: grep -nE "D5|D6|D5a|D7|D1\.|D2\.|D10" docs/ROADMAP.md and confirm D5 no longer "IN PROGRESS".
  baseline_polarity: FAIL (D5 currently "IN PROGRESS", not realized)
  post_condition: PASS after M7
  failure_reasoning: docs/ROADMAP.md:42 reads "D5 ... IN PROGRESS".
  scope: final
  cost: cheap
  rationale: O22
  evidence: docs/ROADMAP.md:42
```

## Phased Plan

### Phase: Combat Foundation and Slice
- objective: turn the movement skeleton into a killable duel behind the existing seam, tracer-first.
- rationale: prove the full vertical path (combat TOML -> fire order -> seeded damage -> snapshot) before hardening any single layer, then flesh out validation, shields, weapon tables, and the win terminal.
- prerequisites: green slice-1 baseline at ea3a39c (confirmed).
- files-components: `src/combat.rs` (new), `src/prng.rs` (new), `src/ship.rs`, `src/schema.rs`, `src/scenario.rs`, `src/movement.rs`, `src/game_state.rs`, `src/snapshot.rs`, `src/lib.rs`, `data/ships/*.toml` (add weapons), `scenarios/combat.toml` (new), `tests/combat.rs` (new), `docs/ROADMAP.md`.
- blast_radius: internal only. Snapshot JSON shape grows (additive fields + seed); no external consumer (ADR-0004 harness/tests are the only readers). sec-16 does not fire.
- rollback_boundary: revert the phase by `git checkout -- <listed files>` and deleting the new files (`src/combat.rs`, `src/prng.rs`, `scenarios/combat.toml`, `tests/combat.rs`); slice-1 files are edited only additively, so reverting restores the green baseline. Preserve the untracked slice-2 docs and sibling projects.
- risks: (a) hex bearing delta->facing math is the one non-trivial algorithm -- mitigated by an isolated, exhaustively tested function in M2; (b) PRNG draw-order coupling to reproducibility -- mitigated by DD4 deterministic order + inv-reproducible; (c) snapshot shape churn -- mitigated by fixing the full shape in M1.
- exit_gate: full `cargo test` green including the headline `test_fire_until_destroyed_wins`; all every-pass invariants pass; inv-reproducible grep clean; ROADMAP updated.

#### Milestone: M1-tracer-combat-slice
- outcome: loading `scenarios/combat.toml` and applying ONE fire order reduces the target's toward-attacker shield facing, observable in the JSON snapshot, which now also carries structure, destroyed, weapons, and the seed.
- traces_to: O1, O2, O12, O13, O14, O15, O16, O18, O19, O21
- tracer_bullet: yes -- thinnest end-to-end path through every new layer (schema/loader -> combat state -> fire order -> seeded resolution -> snapshot). Geometry is chosen trivial (attacker one hex away along the defender's facing-0 bearing) so the tracer does not depend on the general bearing algorithm (that is hardened in M2); the gate asserts the toward-attacker facing strictly decreased, not exact damage.
- implementation_scope: add `prng.rs` (SplitMix64-style, `new(seed)`, `next_u64`, serializable `u64` state); add `combat.rs` (Weapon struct: kind, arc, max_range, params; a first-cut `resolve_fire` that computes the hit facing using the defender-relative bearing per DD5, rolls damage via prng, depletes facing then bleeds to structure); extend `Ship` with `weapons: Vec<Weapon>`, `shields: [u32;6]`, `structure: u32`, `destroyed: bool`; extend `ShipDef`/schema with weapons, shield strengths, structure; extend `ScenarioDef` with optional `[terminal]` and make `objective` an `Option` (DD2); make `GameState.objective` an `Option<Hex>` and change `refresh_status` so it returns `InProgress` when `objective` is `None` (the destruction branch is added in M5) -- this preserves the objective-hex terminal exactly when `objective` is `Some`; add seed to `GameState` (+ prng) and to `StateSnapshot`; make `StateSnapshot.objective` serialize the `Option` (present for objective-hex scenarios, null/omitted for combat) WITHOUT renaming the field; extend `ShipSnapshot` with shields[6], structure, destroyed, weapons; add `Order::Fire { weapon, target }` + `DeclaredOrder::Fire` + minimal declare/resolve arms; author `scenarios/combat.toml` (two armed ships, adjacent along the defender's facing-0 bearing, destruction terminal, no objective) and add weapon blocks to a generic ship class.
- dependencies: none (first milestone).
- implementation_contracts:
  - path `src/prng.rs`, responsibility: deterministic seedable PRNG. contract_depth: algorithmic. public: `pub struct Prng { state: u64 }`, `pub fn new(seed: u64) -> Self`, `pub fn next_u64(&mut self) -> u64`, `pub fn roll(&mut self, sides: u32) -> u32` (1..=sides). invariants: identical seed -> identical sequence; state is a single u64 serialized in the snapshot. forbidden: thread_rng, SystemTime, Instant, any `rand` crate use. gates: inv-reproducible, inv-seeded-exact. pseudocode: SplitMix64 (state = state.wrapping_add(0x9E3779B97F4A7C15); z = state; z = (z ^ (z>>30)).wrapping_mul(0xBF58476D1CE4E5B9); z = (z ^ (z>>27)).wrapping_mul(0x94D049BB133111EB); z ^ (z>>31)).
  - path `src/combat.rs`, responsibility: weapon definitions + fire resolution + shield/bleed math + hex bearing. contract_depth: algorithmic (bearing + bleed are the hard parts; see M2/M3). public: `pub struct Weapon { pub id: String, pub kind: WeaponKind, pub arc: Arc, pub max_range: u32, ... }`, `pub enum WeaponKind { Phaser, Disruptor }`, `pub fn bearing_to(from: Hex, to: Hex) -> u8` (0..=5), `pub fn resolve_fire(...) -> FireOutcome`. invariants: damage applies to `shields[bearing_to(defender,attacker)]` then bleeds remainder to structure; a phaser/disruptor draws from prng in a fixed order. gates: inv-facing-correct, inv-bleed-through, inv-seeded-exact.
  - path `src/movement.rs`, responsibility: add `Fire` order arms. contract_depth: detailed. public: extend `Order` and `DeclaredOrder` with `Fire { weapon: String, target: u32 }`; `declare` Fire arm returns typed errors (full set in M2); `resolve` Fire arm calls combat resolution then `refresh_status`. forbidden: no combat math inside movement.rs beyond dispatch (rules live in combat.rs, ADR-0001).
  - path `src/snapshot.rs`, responsibility: additive fields + objective becomes Option-serialized. contract_depth: detailed. public: `ShipSnapshot` gains `shields: [u32;6]`, `structure: u32`, `destroyed: bool`, `weapons: Vec<WeaponSnapshot>`; `StateSnapshot` gains `seed: u64` and its `objective` reflects `Option<Hex>` (present unchanged for objective-hex scenarios; `null` or `skip_serializing_if` omission for combat). forbidden: renaming/removing existing fields or changing the objective JSON shape for objective-hex scenarios (slice-1 tests assert `objective.q/.r`).
  - path `src/game_state.rs`, responsibility: own the prng+seed, the Option objective, and (M5) the destruction terminal. contract_depth: detailed. public: `GameState` gains `seed: u64` and a `Prng`; `objective` becomes `Option<Hex>`; constructors take a seed (default fixed). `refresh_status`: `Some(obj)` -> existing objective-hex rule; `None` -> `InProgress` in M1 (M5 adds the destruction branch keyed off the scenario terminal). allowed private helpers: `fired_this_turn` tracker mirroring `moves_this_turn`.
  - path `src/schema.rs` / `src/scenario.rs`, responsibility: parse weapons, shields, structure, seed, `[terminal]`; make `objective` an `Option` validated on-board only when present; a destruction-terminal scenario needs no objective (dummy objective forbidden, DD2). contract_depth: detailed. gates: inv-weapons-loaded.
  - allowed private/test helpers: test constructors in `tests/combat.rs`; private roll/bearing helpers in combat.rs. forbidden edits: no changes to slice-1 test assertions; no removal of the objective terminal.
  - contract_review cadence: standard-tier review at milestone end (M1 is the highest-risk slice -> reasoning-tier contract review of prng.rs and combat.rs bearing/bleed).
- subagent_work: implementer (fast/standard tier) writes M1 from the contracts; reasoning-tier review of prng.rs + combat.rs. policy_rationale: cost_optimized -- mechanical struct/serde/loader work is cheap-tier; only the algorithmic PRNG/bearing gets reasoning review. escalation_trigger: bearing/bleed test fails twice, or snapshot shape breaks slice-1 tests.
- acceptance_gates:
  - `cargo test test_tracer_fire_damages_shield -- --exact` -> PASS; expected: after one fire order the defender's toward-attacker shield facing < its starting value. baseline_polarity: FAIL (no Fire variant; does not compile). post_condition: PASS. evidence: grep -rn "Fire" src/ -> none.
  - `cargo test test_combat_snapshot_shape -- --exact` -> PASS; snapshot has ships[].shields (len 6), structure, destroyed, weapons, and top-level seed. baseline_polarity: FAIL. post_condition: PASS. evidence: `grep -n "struct ShipSnapshot\|struct StateSnapshot" src/snapshot.rs`.
  - `cargo test test_combat_scenario_loads_weapons -- --exact` -> PASS; asserts a concrete parsed weapon field (kind AND arc AND max_range) on a loaded ship, not merely ships.len()==2. baseline_polarity: FAIL (no combat scenario). post_condition: PASS.
  - `cargo test test_combat_scenario_starts_in_progress -- --exact` -> PASS; a freshly loaded combat scenario (objective None) reports `status == "InProgress"`, proving the destruction terminal does not spuriously win before M5. baseline_polarity: FAIL (does not compile; no combat scenario). post_condition: PASS.
  - regression: `cargo test` full suite still green (18 slice-1 tests unchanged, incl. objective-hex Won). baseline_polarity: PASS.
- gate_failure_reasoning: fire order / combat scenario / shield state do not exist at ea3a39c; the tracer test cannot compile until Order::Fire and the combat fields land.
- invariants_at_risk: inv-fire-order-exists, inv-snapshot-shape, inv-weapons-loaded, inv-single-structure, inv-generic-data, inv-objective-terminal-regress.
- evidence_to_record: the tracer test's before/after shield value; the serialized snapshot JSON line; `cargo test` summary.
- rollback_unit: delete new files + `git checkout -- src/ship.rs src/schema.rs src/scenario.rs src/movement.rs src/game_state.rs src/snapshot.rs src/lib.rs data/ships/*.toml`.
- stop_conditions: if the additive snapshot fields break a slice-1 test, STOP -- the field was not additive; fix without altering slice-1 assertions.

#### Milestone: M2-fire-validation
- outcome: illegal fire orders (out-of-arc, out-of-range, unknown weapon, unknown target, fire-at-self, refire-this-turn) are each rejected with a distinct typed `OrderError` and leave the snapshot byte-identical; general hex bearing delta->facing is correct for arbitrary geometry.
- traces_to: O3, O4, O7 (bearing), O11, O17 (fire-freely boundary), A1
- tracer_bullet: no (hardening layer on top of the M1 tracer).
- implementation_scope: implement `bearing_to(from, to)` for arbitrary hex deltas (map delta to nearest of 6 directions; ties resolved deterministically); compute the attacker-relative bearing `(bearing_to(attacker.pos, target.pos) + 6 - attacker.facing) % 6` and test `arc_contains(weapon.arc, relative_bearing)` (DD5 -- facing is a live input, so a `Face` order changes arc eligibility); range check via `Hex::distance`; per-turn `fired_this_turn` set reset in `end_turn`; new `OrderError` variants: `WeaponNotFound`, `TargetNotFound`, `FireAtSelf`, `OutOfArc`, `OutOfRange`, `WeaponAlreadyFired`. All checks in `declare` (no mutation).
- dependencies: M1.
- implementation_contracts:
  - `src/combat.rs`: `pub fn bearing_to(from: Hex, to: Hex) -> u8` returns the world hex direction 0..=5; `pub fn arc_contains(arc: &Arc, relative_bearing: u8) -> bool` takes the FACING-RELATIVE bearing (caller subtracts `attacker.facing` mod 6). contract_depth: algorithmic -- include the delta->direction derivation (normalize the hex vector, pick the direction with max dot / min angular distance; document the tie rule as a fixed lowest-index winner) and the facing subtraction. counterexamples: straight-line vs off-axis deltas; same geometry with two different attacker facings must yield different relative bearings.
  - `src/movement.rs`: Fire declare arm returns the six typed errors; order of checks: weapon exists -> target exists -> not self -> in range -> in arc (relative) -> not already fired. contract_depth: detailed.
  - contract_review cadence: reasoning-tier review of `bearing_to` + the facing-relative arc (the algorithm); standard for the rest.
- subagent_work: implementer standard tier; reasoning-tier review of bearing_to. escalation_trigger: any bearing test fails.
- acceptance_gates:
  - `cargo test test_out_of_arc_rejected test_out_of_range_rejected test_fire_at_self_rejected test_unknown_weapon_rejected test_unknown_target_rejected test_refire_rejected` -> all PASS, each asserting the specific `OrderError`. baseline_polarity: FAIL (declare has no Fire arm). post_condition: PASS.
  - `cargo test test_illegal_fire_no_mutation -- --exact` -> PASS; snapshot byte-identical before/after a rejected order (mirrors the slice-1 no-mutation tests, `grep -n "rejected_without_mutation" tests/movement.rs`). baseline_polarity: FAIL. post_condition: PASS.
  - `cargo test test_bearing_delta_to_facing -- --exact` -> PASS across all six directions + off-axis samples. baseline_polarity: FAIL (no bearing fn). post_condition: PASS.
  - `cargo test test_face_order_changes_arc_eligibility -- --exact` -> PASS: with fixed ship positions, a `Face` order flips a target from in-arc to out-of-arc (or vice versa), proving `facing` is a live input to the arc check (DD5). baseline_polarity: FAIL (no arc path). post_condition: PASS.
- gate_failure_reasoning: no fire declare path exists at baseline; bearing inverse is absent; facing is not yet an arc input.
- invariants_at_risk: inv-illegal-fire-rejected, inv-no-mutation, inv-facing-correct.
- evidence_to_record: each typed-error assertion; the no-mutation byte comparison.
- rollback_unit: `git checkout -- src/movement.rs src/combat.rs src/game_state.rs`.
- stop_conditions: if a "rejected" order mutates state, STOP -- validation leaked into resolve; move it into declare.

#### Milestone: M3-shield-facing-and-bleedthrough
- outcome: damage depletes the correct facing (general geometry), overflow bleeds to structure in the same step, a non-overflowing hit leaves structure unchanged, and a depleted facing stays down for the scenario.
- traces_to: O7, O8, O18, A3, A4
- tracer_bullet: no.
- implementation_scope: finalize `resolve_fire` shield/bleed logic using `bearing_to` from M2; ensure depleted facings persist (no regen -- CONTEXT decision); structure decremented only by overflow.
- dependencies: M1, M2.
- implementation_contracts: `src/combat.rs` resolve path. contract_depth: algorithmic -- pseudocode: `let f = (bearing_to(defender.pos, attacker.pos) + 6 - defender.facing) % 6; let absorbed = min(dmg, shields[f]); shields[f] -= absorbed; let overflow = dmg - absorbed; structure = structure.saturating_sub(overflow);` (DD5: the shield index is defender-facing-relative, so rotating the defender changes which shield absorbs). invariants: structure changes iff overflow>0; facing never negative. gates: inv-bleed-through, inv-facing-correct.
- subagent_work: implementer standard; standard-tier review (batched with M4). escalation_trigger: bleed test fails.
- acceptance_gates:
  - `cargo test test_overflow_bleeds_then_stops -- --exact` -> PASS: a hit exceeding facing strength reduces structure by exactly the overflow.
  - `cargo test test_underflow_leaves_structure -- --exact` -> PASS: a hit within facing strength leaves structure unchanged, facing reduced.
  - `cargo test test_depleted_facing_stays_down -- --exact` -> PASS: a second hit to a depleted facing goes straight to structure.
  - `cargo test test_damage_hits_bearing_facing -- --exact` -> PASS: for an attacker off-axis, the depleted facing is `(bearing_to(defender.pos, attacker.pos) + 6 - defender.facing) % 6`.
  - `cargo test test_face_order_changes_hit_shield -- --exact` -> PASS: with fixed positions, applying a `Face` order to the defender before an identical shot depletes a DIFFERENT shield index, proving defender facing is a live input (DD5). This is the gate that prevents enshrining the facing-independent model.
  - all baseline_polarity: FAIL (no structure/shields at baseline). post_condition: PASS.
- gate_failure_reasoning: no shield/structure model exists at baseline.
- invariants_at_risk: inv-bleed-through, inv-facing-correct, inv-single-structure.
- evidence_to_record: shield/structure values before/after each scenario.
- rollback_unit: `git checkout -- src/combat.rs`.
- stop_conditions: if structure drops on a non-overflowing hit, STOP -- bleed math is wrong.

#### Milestone: M4-weapon-resolution-tables
- outcome: phaser damage follows a range-based table and disruptor to-hit follows a by-range threshold, both drawn from the seeded PRNG and exact for a pinned seed (miss and hit both demonstrated).
- traces_to: O5, O6, O14
- tracer_bullet: no.
- implementation_scope: phaser range->dice table and disruptor by-range to-hit thresholds in `combat.rs`, parameterized from weapon TOML (generic values, ADR-0003); fixed prng draw order per weapon kind.
- dependencies: M1, M2, M3.
- implementation_contracts: `src/combat.rs` weapon resolution. contract_depth: algorithmic -- document the exact prng draw order (phaser: draw N dice for damage; disruptor: draw 1 to-hit then, on hit, damage) so pinned-seed values are stable. gates: inv-seeded-exact.
- subagent_work: implementer standard; standard-tier review batched with M3. escalation_trigger: pinned-seed value drifts between runs.
- acceptance_gates:
  - `cargo test test_phaser_damage_by_range_pinned_seed -- --exact` -> PASS: for a fixed seed, phaser damage at ranges {1, mid, max} equals the recorded expected values AND damage at range 1 differs from damage at max range (range-dependence pinned, so a constant table fails).
  - `cargo test test_disruptor_miss_then_hit_pinned_seed -- --exact` -> PASS: for a fixed seed the disruptor misses at one range and hits at another, matching recorded outcomes AND the miss applies zero damage while the hit applies nonzero (to-hit is real, not a constant).
  - baseline_polarity: FAIL (no resolution/prng). post_condition: PASS.
- gate_failure_reasoning: no weapon resolution or prng exists at baseline.
- invariants_at_risk: inv-seeded-exact, inv-reproducible.
- evidence_to_record: the pinned-seed expected values (recorded in the test as the contract).
- rollback_unit: `git checkout -- src/combat.rs data/ships/*.toml`.
- stop_conditions: if pinned-seed values differ run-to-run, STOP -- an ambient source leaked in; check inv-reproducible grep.

#### Milestone: M5-destruction-win-terminal
- outcome (HEADLINE OWNER): under the destruction terminal, firing until the enemy's structure reaches 0 marks it destroyed (it resolves no further fire), and scenario status becomes `Won`; the slice-1 objective-hex terminal still works.
- traces_to: O9, O10, O20, DD2, DD3, DD4
- tracer_bullet: no (it completes the vertical the M1 tracer opened).
- implementation_scope: `structure <= 0` sets `destroyed = true`; a destroyed ship is skipped as an actor and its fire orders are rejected/no-op; `GameState::refresh_status` gains a terminal branch (objective-hex vs destruction target from `[terminal]`); combat scenario declares `type="destruction", target=<enemy id>`.
- dependencies: M1, M2, M3, M4.
- implementation_contracts: `src/game_state.rs` `refresh_status` + terminal field; `src/combat.rs` destruction marking. contract_depth: detailed. invariants: destruction terminal never affects objective-hex scenarios; objective-hex terminal unchanged. gates: inv-destruction, inv-objective-terminal-regress.
- subagent_work: implementer standard; reasoning-tier review (win-terminal is the headline surface). escalation_trigger: objective-hex regression.
- acceptance_gates:
  - HEADLINE: `cargo test test_fire_until_destroyed_wins -- --exact` -> PASS: loads `scenarios/combat.toml` (seed pinned, objective None), asserts `status == "InProgress"` before the fatal shot, then issues fire orders (with `end_turn` between volleys per A1) until the enemy's structure is 0, and asserts snapshot `status == "Won"` AND the enemy ship `destroyed == true`. Because the combat scenario has no objective hex, the Won is isolated to the destruction terminal (no coincidental objective-hex match). baseline_polarity: FAIL (no destruction terminal; refresh_status wins only on `ship.pos == self.objective`). post_condition: PASS.
  - regression: `cargo test test_player_reaches_objective_wins test_non_winning_run_stays_in_progress -- --exact` -> PASS unchanged. baseline_polarity: PASS.
- gate_failure_reasoning: `ScenarioStatus`/`refresh_status` have no destruction path at baseline.
- invariants_at_risk: inv-destruction, inv-objective-terminal-regress.
- evidence_to_record: the winning snapshot JSON (status Won + enemy destroyed); the objective-hex test still green.
- rollback_unit: `git checkout -- src/game_state.rs src/combat.rs scenarios/combat.toml`.
- stop_conditions: if adding the destruction terminal changes objective-hex behavior, STOP -- terminals are not isolated.

#### Milestone: M6-reproducibility-and-determinism-guard
- outcome: two runs with the same seed and the same orders produce a byte-identical final snapshot, and the forbidden-API guard (no thread_rng/SystemTime/Instant, no rand dep) holds.
- traces_to: O14, O15, O16, DD1, DD4
- tracer_bullet: no.
- implementation_scope: a reproducibility test that runs the combat sequence twice and compares serialized snapshots; confirm seed serialized; keep the determinism grep clean.
- dependencies: M1, M4, M5.
- implementation_contracts: `tests/combat.rs` reproducibility test. contract_depth: detailed. gates: inv-reproducible, inv-determinism-guard.
- subagent_work: implementer standard; standard review. escalation_trigger: snapshots differ between identical runs.
- acceptance_gates:
  - `cargo test test_same_seed_same_orders_identical -- --exact` -> PASS: identical serialized final snapshots (mirrors the slice-1 reproducibility pattern in `tests/acceptance.rs`, `grep -n "reproducible\|assert_eq" tests/acceptance.rs`). baseline_polarity: FAIL (combat/prng absent). post_condition: PASS.
  - `grep -rn "thread_rng\|SystemTime\|Instant" src/ tests/` -> no output; `grep -n "^rand" Cargo.toml` -> no output. baseline_polarity: PASS (clean now). post_condition: PASS (still clean).
- gate_failure_reasoning: the reproducibility behavior does not exist until combat + prng land; the grep guards against a determinism regression while adding randomness.
- invariants_at_risk: inv-reproducible, inv-determinism-guard.
- evidence_to_record: the two identical snapshot strings; the empty grep output.
- rollback_unit: `git checkout -- tests/combat.rs`.
- stop_conditions: if identical inputs diverge, STOP -- a HashMap iteration or ambient source is in the draw path (see ROADMAP T1); order by ship id.

#### Milestone: M7-roadmap-and-nongoals
- outcome: `docs/ROADMAP.md` records D5 as realized (with commit ref) and still lists D6, D5a, D7, D1, D2, D10 as deferred with their hooks.
- traces_to: O22, all NG-*
- tracer_bullet: no (docs closure).
- implementation_scope: move D5 under its realized slice per the ROADMAP maintenance note; leave deferred items intact.
- dependencies: M1..M6.
- implementation_contracts: docs only; no code. contract_depth: thin.
- subagent_work: inline (trivial doc edit; handoff overhead exceeds the work). policy_rationale: tightly coupled to final state, trivial.
- acceptance_gates:
  - `grep -nE "D6|D5a|D7|D1\.|D2\.|D10" docs/ROADMAP.md` -> all still present; `grep -n "IN PROGRESS" docs/ROADMAP.md` -> D5 line no longer flagged in progress. baseline_polarity: FAIL (D5 "IN PROGRESS" now, docs/ROADMAP.md:42). post_condition: PASS.
- gate_failure_reasoning: ROADMAP.md:42 currently marks D5 IN PROGRESS.
- invariants_at_risk: inv-roadmap-updated.
- evidence_to_record: the ROADMAP diff.
- rollback_unit: `git checkout -- docs/ROADMAP.md` (re-applying the user's prior IN-PROGRESS edit is preserved in history).
- stop_conditions: if a deferred item would be removed rather than kept, STOP -- non-goals must remain tracked.

## Subagent Matrix
| Work item | Role | Tier | Delegate? | Policy rationale | Escalation trigger | Inputs | Required output |
| --- | --- | --- | --- | --- | --- | --- |
| Baseline + discovery | Repository Scout | standard | done inline (this loop) | grounding is tightly coupled to planning | n/a | repo, docs | cited findings (above) |
| M1 tracer build | Implementer | standard | yes | mechanical struct/serde/loader + thin fire path | slice-1 test breaks | M1 contracts | code + passing tracer gate |
| prng.rs / combat.rs bearing+bleed review | Contract reviewer | reasoning | yes | algorithmic, reproducibility-critical | 2x gate fail | prng.rs, combat.rs | review verdict |
| M2 validation build | Implementer | standard | yes | typed errors + bearing inverse | bearing test fail | M2 contracts | code + passing gates |
| M3/M4 shields+tables build | Implementer | standard | yes | bounded, seeded, table-driven | pinned-seed drift | M3/M4 contracts | code + passing gates |
| M5 win terminal build | Implementer | standard | yes | headline surface | objective regression | M5 contracts | code + headline gate green |
| M6 reproducibility | Implementer | standard | yes | one test + grep | runs diverge | M6 contract | code + passing gate |
| M7 roadmap | Lead (inline) | standard | no | trivial doc edit | non-goal dropped | ROADMAP | updated doc |
| Gate audit | Gate Auditor | reasoning | yes (fresh instance) | independence over the gates | vacuous gate found | this plan + repo | audit findings |

## Consolidated Verification
Run in order at final:
1. `cargo test` -> full suite green: 18 slice-1 tests unchanged PLUS the new combat tests (tracer, snapshot shape, scenario-loads-weapons, six rejection tests, no-mutation, bearing, bleed x3, phaser/disruptor pinned-seed, headline destruction-win, reproducibility). Proves new behavior works and slice 1 is not regressed.
2. Headline: `cargo test test_fire_until_destroyed_wins -- --exact` -> PASS (status Won + enemy destroyed, seed pinned).
3. Reproducibility: `cargo test test_same_seed_same_orders_identical -- --exact` -> PASS.
4. Determinism guard: `grep -rn "thread_rng\|SystemTime\|Instant" src/ tests/` and `grep -n rand Cargo.toml` -> no output.
5. Non-goal guards: `grep -rniE "energy|arming" src/` -> none; `grep -rniE "system_damage|allocation_chart" src/` -> none; `grep -riE "federation|klingon|romulan|star fleet|adb" data/ scenarios/` -> none.
6. Thin CLI: `git diff ea3a39c -- src/bin/shipsim.rs` -> only generic order/snapshot plumbing (no combat rules).
7. ROADMAP: D5 realized; D6/D5a/D7/D1/D2/D10 still deferred.
8. `cargo build` (release + dev, panic=abort) succeeds; `grep -n "^rand" Cargo.toml` empty (DD1 no new dep).

## Replan Triggers
- The hex bearing delta->facing algorithm cannot be made exact/deterministic for arbitrary deltas (tie ambiguity) -> pause, record the counterexample, decide a tie rule before M3.
- A pinned-seed value cannot be stabilized (runs diverge) -> inv-reproducible failing; audit the prng draw order and any HashMap iteration (ROADMAP T1) before continuing.
- Making `objective` optional breaks a slice-1 scenario load -> revisit DD2 (keep objective required + dummy) rather than change the loader contract.
- The additive snapshot fields force a change to an existing slice-1 field name/type -> STOP; the change is not additive; re-scope.
- Any milestone would require an energy field, itemized systems, simultaneous batching, or impulse chart -> that is a non-goal (D7/D6/D2/D1); reject and keep the simplified stand-in.

## Downstream Handoff (for the execute loop)
- Staleness check first: verify `git -C /mnt/storage/git_home rev-parse HEAD` == `ea3a39c8...` and that shipsim dirty state is limited to `docs/ROADMAP.md` + the four untracked slice-2 docs. If HEAD moved but no cited file (src/*, Cargo.toml, scenarios/*, data/ships/*) changed, proceed and note drift. If any cited file changed, re-run that citation and re-validate the affected milestone before executing.
- Milestone order: M1 -> M2 -> M3 -> M4 -> M5 -> M6 -> M7 (linear; each depends on all prior). No spikes.
- Invariant suite: run the cheap every-pass subset after every milestone; run inv-thin-cli and inv-roadmap-updated + full grep guards at final.
- Human-confirmation points: none (no blast-radius trigger fired). If execution discovers an external consumer of the snapshot JSON, halt and escalate (that would fire sec-16).
- File contracts: as specified per milestone above. Allowed flexibility: private helpers within combat.rs/game_state.rs responsibilities and test helpers in tests/combat.rs may be added by the executor. A new public symbol, new production file beyond those listed, new dependency, or edit outside `implementation_scope` requires a contract update or replan. Reasoning-tier code-writing exception: allowed only for the prng.rs/combat.rs bearing+bleed algorithm if the cheaper tier fails the same gate twice; record the exception.

## Review Record
- Reviewer lenses (Gate Quality, Completeness, Migration/Removal, Sequencing, Risk & Rollback, Repository Alignment, Simplification) ran as an independent adversarial subagent, and the Gate Audit (sec 13) ran as a separate fresh-instance subagent -- both with repository access and no author-conversation context (gate_audit: independent). Findings and dispositions recorded in "Pass N changes" below.
- Conflict-triage decisions: DD1 (hand-rolled PRNG over the `rand` crate) is a Simplification/Alignment win that does NOT weaken falsifiability (inv-seeded-exact + inv-reproducible remain non-vacuous), so it is retained under sec 11.1 priority 4-5 with priorities 1-3 already satisfied.
- Characterization-before-deletion: N/A -- this slice removes no behavior; the objective-hex terminal is preserved and guarded by inv-objective-terminal-regress (regression, baseline PASS).

### Pass 1 changes (reviewer + independent gate-audit synthesis)
One adversarial Reviewer (all lenses) and one independent fresh-instance Gate Auditor ran over the drafted plan with repo access and no author context (gate_audit: independent). Eleven distinct findings; all accepted or adapted, none rejected. No conflicts required sec-11.1 triage (the DD1 hand-rolled-PRNG choice was independently confirmed well-justified by both agents).

Accepted -- Blocking:
- facing-dropped-from-arc-and-shield-math (Reviewer F1): the naive `bearing_to(defender,attacker)` shield index and `arc_contains(arc,bearing)` dropped ship `facing`, violating ADR-0006 ("facing matters for offense and defense") and PRD US3/US7 -- and worse, the M3 gate would have ENSHRINED the facing-independent model. FIX: added DD5 (facing-relative bearing for both arc and shield); corrected the M1/M2/M3 contracts and the M3 pseudocode to `(bearing_to(..) + 6 - facing) % 6`; added `test_face_order_changes_hit_shield` (M3) and `test_face_order_changes_arc_eligibility` (M2) so a `Face` order MUST change offense/defense, making facing a proven-live input; updated inv-facing-correct and traceability O3/O7.

Accepted -- Material:
- citation-drift-nonrederivable / fabricated-line-citations (Auditor + Reviewer F2): every `path:line` citation was fabricated (files are far shorter than cited). Structural claims were all correct and independently re-verified, but the line anchors broke the Staleness Contract. FIX: replaced every code line-number citation with a re-derivable `grep -n <symbols> <file>` command; kept only the real `docs/ROADMAP.md:42` (verified). Verified zero residual `.rs`/`.toml` line citations remain.
- inv-reproducible-polarity-mislabel (Auditor): the invariant bundled a behavioral test (baseline FAIL) with a grep guard (baseline PASS) under one "regression/PASS" label. FIX: split into inv-reproducible (presence, FAIL->PASS after M6) and inv-determinism-guard (regression, PASS baseline); updated the cheap subset, M6, and traceability O14.
- inv-seeded-exact-not-range-proving (Auditor): equality-to-recorded-values could pass with a constant (range-independent) table. FIX: added structural range-dependence assertions to inv-seeded-exact and the M4 gates (phaser dmg range-1 != max; disruptor miss=0, hit>0).
- combat-scenario-has-no-valid-terminal-until-M5 (Reviewer F3): `resolve` calls `refresh_status` every order, but the destruction branch only lands in M5, leaving the combat scenario evaluated by objective-only logic (or a spurious dummy-objective Won) across M1-M4. FIX: added DD6 + DD2 rule that M1 makes `objective` an `Option` and `refresh_status` returns `InProgress` when `None`; added the M1 gate `test_combat_scenario_starts_in_progress`; forbade the dummy-objective fallback.

Accepted -- Minor:
- inv-weapons-loaded-vacuity-risk (Auditor): FIX -- M1 gate now asserts concrete parsed weapon fields (kind/arc/max_range), not ships.len().
- inv-destruction-win-source (Auditor): FIX -- headline gate now asserts InProgress before the fatal shot and that the combat scenario has no objective, isolating Won to the destruction terminal.
- absence-guards-keyword-fragile (Auditor): FIX -- encoded inv-single-structure's "exactly one structure field" as a concrete Ship-struct check; broadened the energy/generic-data keyword sets; labeled all three greps as naming heuristics, not exhaustive proofs.
- invariant-check-omits-unknown-target-test (Reviewer F4): FIX -- added `test_unknown_target_rejected` to the inv-illegal-fire-rejected every-pass check.
- optional-objective-snapshot-shape-unspecified (Reviewer F5): FIX -- inv-snapshot-shape and the M1 snapshot contract now specify objective stays present/unchanged for objective-hex scenarios and serializes null/omitted for combat, with no field rename.

Non-findings independently confirmed by both agents: no deferred non-goal is built (declare/resolve seam reused not D2; single structure pool not D6; no energy not D7; one-shot-per-turn stand-in not D1; 1v1 not D10); DD3 mark-not-remove preserves the D2/D10 hooks; the hand-rolled SplitMix64 PRNG over `rand` is well-justified (ADR-0001 minimal deps, ADR-0005 explicit seedable generator; seed=0 safe for SplitMix64); rollback boundaries concrete; sibling-project and untracked slice-2 docs preservation explicit; all RED baselines genuinely RED.

Convergence: all 11 findings resolved in one pass; 0 Blocking / 0 Material remain open; no thrash. Exit check satisfied at pass 1.
```

Now let me spawn a fresh independent Gate Auditor and an adversarial Reviewer in parallel over the drafted plan.
</content>
</invoke>

## Machine-Readable Verdict

```json
{
  "verdict": "READY",
  "task_tier": "standard",
  "tier_trigger": "Multi-file additive feature over a green committed baseline; internal unreleased snapshot wire format with no external consumer, so sec-16 does not fire; no removal of used behavior, no security/auth/billing/CI/persisted-data surface.",
  "execution_policy": "cost_optimized",
  "model_routing": "current-model-only",
  "model_tiers": { "fast": "claude-opus-4-8[1m]", "standard": "claude-opus-4-8[1m]", "reasoning": "claude-opus-4-8[1m]" },
  "passes": 1,
  "open_blocking_findings": 0,
  "open_material_findings": 0,
  "vacuous_gates_found": 0,
  "cold_start_gaps": 0,
  "uncited_facts": 0,
  "gate_audit": "independent",
  "staleness": { "head": "ea3a39c8006dc0a518e23264047605384c999432", "dirty_files": ["shipsim/docs/ROADMAP.md", "shipsim/docs/CONTEXT-slice2.md (untracked)", "shipsim/docs/PRD-slice2.md (untracked)", "shipsim/docs/adr/0005-seeded-prng-for-combat.md (untracked)", "shipsim/docs/adr/0006-combat-model-slice2.md (untracked)"], "discovered_at": "2026-07-04" },
  "traceability_complete": true,
  "orphan_milestones": [],
  "characterization_gaps": [],
  "conflicts_resolved": [],
  "invariants": [
    { "id": "inv-fire-order-exists", "category": "presence", "scope": "every-pass", "cost": "cheap", "check": "cargo test test_tracer_fire_damages_shield", "baseline_polarity": "FAIL", "evidence": "grep -rn Fire src/ -> none" },
    { "id": "inv-weapons-loaded", "category": "presence", "scope": "phase-end", "cost": "cheap", "check": "cargo test test_combat_scenario_loads_weapons", "baseline_polarity": "FAIL", "evidence": "ls scenarios/ no combat file" },
    { "id": "inv-snapshot-shape", "category": "presence", "scope": "every-pass", "cost": "cheap", "check": "cargo test test_combat_snapshot_shape", "baseline_polarity": "FAIL", "evidence": "grep struct ShipSnapshot/StateSnapshot src/snapshot.rs" },
    { "id": "inv-illegal-fire-rejected", "category": "presence", "scope": "every-pass", "cost": "cheap", "check": "cargo test test_out_of_arc_rejected test_out_of_range_rejected test_fire_at_self_rejected test_unknown_weapon_rejected test_unknown_target_rejected test_refire_rejected", "baseline_polarity": "FAIL", "evidence": "declare has no Fire arm" },
    { "id": "inv-no-mutation", "category": "presence", "scope": "every-pass", "cost": "cheap", "check": "cargo test test_illegal_fire_no_mutation", "baseline_polarity": "FAIL", "evidence": "no fire path" },
    { "id": "inv-facing-correct", "category": "presence", "scope": "every-pass", "cost": "cheap", "check": "cargo test test_damage_hits_bearing_facing test_bearing_delta_to_facing test_face_order_changes_hit_shield test_face_order_changes_arc_eligibility", "baseline_polarity": "FAIL", "evidence": "no delta->facing inverse; no shields; facing inert" },
    { "id": "inv-bleed-through", "category": "presence", "scope": "every-pass", "cost": "cheap", "check": "cargo test test_overflow_bleeds_then_stops test_underflow_leaves_structure test_depleted_facing_stays_down", "baseline_polarity": "FAIL", "evidence": "no structure pool" },
    { "id": "inv-seeded-exact", "category": "presence", "scope": "every-pass", "cost": "cheap", "check": "cargo test test_phaser_damage_by_range_pinned_seed test_disruptor_miss_then_hit_pinned_seed", "baseline_polarity": "FAIL", "evidence": "no prng/resolution; range-dependence pinned separately" },
    { "id": "inv-destruction", "category": "presence", "scope": "every-pass", "cost": "cheap", "check": "cargo test test_fire_until_destroyed_wins", "baseline_polarity": "FAIL", "evidence": "refresh_status objective-only" },
    { "id": "inv-objective-terminal-regress", "category": "regression", "scope": "final", "cost": "cheap", "check": "cargo test test_player_reaches_objective_wins test_non_winning_run_stays_in_progress", "baseline_polarity": "PASS", "evidence": "green at ea3a39c" },
    { "id": "inv-reproducible", "category": "presence", "scope": "every-pass", "cost": "cheap", "check": "cargo test test_same_seed_same_orders_identical", "baseline_polarity": "FAIL", "evidence": "no prng/seed" },
    { "id": "inv-determinism-guard", "category": "regression", "scope": "every-pass", "cost": "cheap", "check": "grep -rn thread_rng|SystemTime|Instant src/ tests/ (none); grep ^rand Cargo.toml (none)", "baseline_polarity": "PASS", "evidence": "no forbidden APIs, no rand dep" },
    { "id": "inv-no-energy-field", "category": "absence", "scope": "phase-end", "cost": "cheap", "check": "grep -rniE energy|arming|power_alloc|capacitor|charge src/ (none)", "baseline_polarity": "PASS-absent (non-goal guard)", "evidence": "no energy concept" },
    { "id": "inv-single-structure", "category": "absence", "scope": "phase-end", "cost": "cheap", "check": "Ship has exactly one structure field; grep -rniE system_damage|allocation_chart|systems[|Vec<.*System src/ (none)", "baseline_polarity": "PASS-absent (non-goal guard)", "evidence": "no structure field yet" },
    { "id": "inv-generic-data", "category": "absence", "scope": "phase-end", "cost": "cheap", "check": "grep -riE federation|klingon|romulan|gorn|tholian|kzinti|lyran|star fleet|adb data/ scenarios/ (none)", "baseline_polarity": "PASS-absent (non-goal guard)", "evidence": "existing data generic" },
    { "id": "inv-thin-cli", "category": "regression", "scope": "final", "cost": "cheap", "check": "git diff ea3a39c -- src/bin/shipsim.rs shows no rule logic", "baseline_polarity": "PASS", "evidence": "CLI is generic order/snapshot driver" },
    { "id": "inv-roadmap-updated", "category": "presence", "scope": "final", "cost": "cheap", "check": "grep D5 realized + D6/D5a/D7/D1/D2/D10 present in docs/ROADMAP.md", "baseline_polarity": "FAIL", "evidence": "docs/ROADMAP.md:42 IN PROGRESS" }
  ],
  "cheap_every_pass_invariants": ["inv-fire-order-exists", "inv-illegal-fire-rejected", "inv-no-mutation", "inv-facing-correct", "inv-bleed-through", "inv-seeded-exact", "inv-destruction", "inv-reproducible", "inv-determinism-guard", "inv-snapshot-shape", "inv-no-energy-field", "inv-single-structure", "inv-generic-data"],
  "blast_radius_decisions": [],
  "human_decisions_required": [],
  "plan_location": "implementation-plan-slice2.md",
  "summary": "Standard-tier, tracer-first plan for shipsim slice-2 direct-fire combat: 7 linear milestones (tracer fire -> validation -> shields/bleed -> weapon tables -> destruction-win headline -> reproducibility -> roadmap). All combat gates proven RED at ea3a39c. One Blocking review finding (ship facing dropped from arc/shield math) and ten Material/Minor findings fixed in a single pass with an independent gate audit. READY."
}
```
