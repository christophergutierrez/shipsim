# Implementation Plan: shipsim Slice 1 (Movement Skeleton)

## Planning Verdict
- verdict: READY
- task_tier: standard
- tier_trigger: Multi-file greenfield Rust feature (a movement-only vertical slice). No migration, no
  externally consumed interface, no persisted-data-format change (all formats brand new), no
  CI/deploy/security/auth/billing surface, no cross-ownership boundary. Blast-Radius gate (sec 16)
  does not fire. Not light: it is multi-file with multiple new modules, a new JSON interface, and a
  falsifiable acceptance gate that warrants a traceability matrix and invariants.
- execution_policy: cost_optimized
- model_routing: current-model-only
- model_tiers: current model for all tiers (fast = standard = reasoning = current runtime model; no
  MODEL_TIER_MAP supplied, so tier labels document routing intent only)
- reason: Repository-grounded, tracer-bullet-first plan with a proven-RED baseline for every gate,
  the headline `status == Won` acceptance gate owned by a dedicated milestone, and the D1/D2/D3
  fidelity hooks captured as invariants/contract constraints rather than built behavior.

## Repository State (Staleness Contract)
- VCS: git monorepo, toplevel `/mnt/storage/git_home` (also mounted at `/home/chris/git_home`).
- HEAD: `00075671b34ba0ecca69a9c2021368d17a628883`
  (`fact <- command: git -C /mnt/storage/git_home rev-parse HEAD -> 0007567...`).
- Working subtree `shipsim/` is UNTRACKED and contains no source code
  (`fact <- command: git status --short -- shipsim/ -> ?? shipsim/`).
- shipsim/ contents at discovery: `CONTEXT.md`, `docs/` (PRD.md, ROADMAP.md, adr/0001-0004),
  `tmp/`, `.fastembed_cache/` only
  (`fact <- command: ls -la /home/chris/git_home/shipsim -> CONTEXT.md docs tmp .fastembed_cache`).
- No Rust anywhere in shipsim/
  (`fact <- command: find /home/chris/git_home/shipsim -name '*.rs' -> (no output)`).
- Discovery timestamp: 2026-07-04T09:09:01-06:00
  (`fact <- command: date -Iseconds -> 2026-07-04T09:09:01-06:00`).
- Existing user changes to preserve: the monorepo has many UNRELATED dirty/untracked files under
  sibling projects (e.g. `question2crux/`, and dozens of other `??` project dirs). This plan touches
  ONLY files under `shipsim/`. The executor MUST NOT stage, revert, or modify anything outside
  `shipsim/`.

Staleness rule for the executor: re-run the citations above before starting. If HEAD moved but no
cited path under `shipsim/` gained Rust source, proceed and note the drift. If any file under
`shipsim/src`, `shipsim/data`, `shipsim/scenarios`, or `shipsim/tests` already exists (someone began
implementation), the greenfield baselines below are stale -- return this plan in `review-existing`
mode before executing.

## Repository Findings

Confirmed facts (each cited):
- Toolchain present: `fact <- command: cargo --version -> cargo 1.91.1`; `rustc 1.91.1`.
- Baseline `cargo test` in shipsim/ FAILS: `fact <- command: cargo test (cwd shipsim) -> error: could
  not find Cargo.toml ...; EXIT=101`. This is the RED baseline for every presence gate below.
- Baseline `cargo run` in shipsim/ FAILS: `fact <- command: cargo run (cwd shipsim) -> error: could
  not find Cargo.toml ...; EXIT=101`. RED baseline for the harness gate.
- NorRust crate manifest (architectural template, ADR-0001):
  `fact <- command: cat norrust_core/Cargo.toml -> [lib] crate-type = ["cdylib","rlib"]; deps
  serde/serde_json/toml/thiserror; [profile.*] panic = "abort"`.
- NorRust hex model: `fact <- command: cat norrust_core/src/hex.rs -> struct Hex { x,y,z } cubic;
  const DIRECTIONS: [Hex;6]; fn distance/neighbors/from_offset/to_offset; #[cfg(test)] mod tests`.
  shipsim will STORE axial (q,r) and convert to cube for distance/neighbor math (PRD "Hex model");
  NorRust stores cube directly -- shipsim differs deliberately (axial storage, orientation-agnostic).
- NorRust TOML schema pattern: `fact <- command: cat norrust_core/src/schema.rs -> #[derive(Deserialize)]
  structs with #[serde(default ...)] fields`. Mirror for ShipDef/ScenarioDef.
- NorRust loader pattern: `fact <- command: cat norrust_core/src/scenario.rs -> load_board(path) reads
  TOML via std::fs + toml::from_str, returns Result<_, String>, validates tiles.len()==w*h`.
- NorRust snapshot pattern: `fact <- command: head norrust_core/src/snapshot.rs -> #[derive(Serialize)]
  flat structs (TileSnapshot/UnitSnapshot/StateSnapshot) with from_game_state(&GameState)`.
- NorRust error pattern: `fact <- command: head norrust_core/src/game_state.rs -> enum ActionError {
  DestinationOutOfBounds, DestinationOccupied, NotAdjacent, ... }; apply_action(...)`. shipsim uses
  `thiserror` (PRD) for its OrderError; NorRust used a bare enum -- shipsim follows the PRD/CONTEXT dep
  list which names thiserror.
- NorRust test pattern: `fact <- command: head norrust_core/tests/simulation.rs -> #[test] fn builds
  Board, drives GameState via apply_action, asserts outcomes`; and
  `fact <- command: head norrust_core/tests/test_ffi.rs -> uses env!("CARGO_MANIFEST_DIR") + .parent()
  to locate data/ and scenarios/`. shipsim tests resolve data via CARGO_MANIFEST_DIR the same way.
- NorRust has NO binary target (`fact <- command: find norrust -name main.rs -> (none)`); its frontend
  is Love2D over FFI. shipsim ADR-0004 REQUIRES a CLI/JSON harness binary -- this is a deliberate
  shipsim addition, not a copy of NorRust.

Baseline status / pre-existing failures: none applicable (no code exists). Every "test" baseline
fails only because the crate is absent, which is the intended RED starting point.

Unsafe/unrun commands: none. All baseline commands are read-only or fail fast; all were run.

Context docs read: `docs/PRD.md`, `CONTEXT.md`, `docs/adr/0001..0004`, `docs/ROADMAP.md` (all read in
full). No context doc missing.

Unknowns requiring spikes: NONE. Every design fact is fixed by the ADRs, the PRD "Implementation
Decisions", the "Assumptions and Resolved Decisions" section (Q1/Q2/Q3 already resolved), and the
NorRust template read above. No milestone needs a discovery spike.

## Requested Outcomes & Non-Goals

Outcomes (from PRD user stories 1-24; grouped, informal per standard tier):
- O1 Scenario load: load map dims, ship placements (pos+facing+class ref), scripted waypoints, and
  objective hex from `scenarios/*.toml` (stories 1,2,17).
- O2 Ship model: each ship occupies exactly one hex, has one of six facings, an id, a generic class,
  a `speed_max`, and a carried-but-unenforced `turn_mode`; stats loaded from `data/ships/*.toml`
  (stories 3,4,16,21,23).
- O3 Orders + validation: apply validated orders (move to adjacent hex, change facing, end turn);
  illegal orders (off-map, beyond `speed_max`, into an occupied hex) rejected with a typed error and
  no state mutation (stories 5,6,7,8).
- O4 Scripted opponent: the second ship follows a deterministic scenario-defined waypoint path
  (story 9).
- O5 Turn structure: discrete turns advance both ships; player-plotted resolves, then scripted
  (sequential) (story 10).
- O6 Terminal condition: status becomes `Won` the instant any ship enters the objective hex; a run
  that never reaches it stays `InProgress` (stories 11; resolved-decision Q1).
- O7 JSON snapshot contract: a serializable snapshot (ships pos/facing, turn number, status) emitted
  after each applied order and at turn boundaries (stories 12,22).
- O8 Harness: a binary that loads scenario+ship data, runs to completion from a scripted orders file
  OR reads orders as JSON on stdin, printing JSON snapshots; deterministic (stories 13,14).
- O9 Headless verifiability: all rules in a pure-Rust core with no render dep, provable via
  `cargo test` in milliseconds (stories 15,18).
- O10 Acceptance gate: a headless integration test plots a legal path to the objective and asserts
  the snapshot reports `status == Won`, demonstrably RED before the logic exists (story 24).
- O11 Fidelity hooks (design-only): turn modeled as an impulse-capable container (D1); movement as
  declare-order -> resolve-order (D2); `turn_mode` field present but unenforced (D3) (stories
  19,20,21).

Explicit non-goals (deferred backlog `docs/ROADMAP.md` D1-D10; reviewers must reject any slice-1
milestone that IMPLEMENTS these, and any shortcut that PRECLUDES the D1/D2/D3 hooks):
- N-D1 full 32-impulse turn / Impulse Movement Chart (only the impulse-capable container hook now).
- N-D2 simultaneous plot-then-resolve movement (only the declare/resolve seam now; slice resolves
  sequentially).
- N-D3 turn-mode ENFORCEMENT (field carried only; free turning in slice 1).
- N-D4 SFB fixed/floating map + off-map subtleties (bounded rectangle, no wraparound).
- N-D5/N-D6 all combat (phasers, disruptors, shields, damage, destruction, SSD systems).
- N-D7 Energy Allocation (speed is a fixed per-ship max).
- N-D8 any graphical frontend.
- N-D9 AI opponent beyond scripted waypoints.
- N-D10 multi-ship / scenario / campaign structure beyond a single 1v1 with one objective.

## Facts, Assumptions, and Decisions

Confirmed facts: see Repository Findings (all cited).

Working assumptions (low-risk; from PRD "Assumptions and Resolved Decisions" and CONTEXT.md; the
plan proceeds under these and none changes architecture/public behavior/persisted data/security/scope
in a way that requires a human):
- A1 Axial (q,r) hex storage with cube conversion for distance/neighbors; core orientation-agnostic.
- A2 Bounded rectangular map from scenario TOML, no wraparound, off-map is an illegal order.
- A3 Harness supports both a scripted orders file and JSON-on-stdin.
- A4 No RNG anywhere in slice 1 (fully deterministic; exact end-state assertions).
- A5 `speed_max` is a fixed per-ship max hexes/turn.
- A6 Single objective hex; first ship to enter resolves `Won`; no draw/timeout terminal; a run that
  never reaches the objective stays `InProgress`.
- A7 One ship per hex; moving into an occupied hex is illegal (rejected, no state change).
- A8 Per-turn order application (no per-impulse ordering) is sufficient for slice 1.
- A9 (staleness) The recorded HEAD/dirty-state holds at execution time; if a `shipsim/src` tree
  already exists, the greenfield baselines are stale (see Staleness Contract).

Decisions requiring human approval before implementation: NONE. The blast-radius gate (sec 16) does
not fire (greenfield, no consumed interface, no persisted-data migration, no CI/security/billing, an
adequate `cargo test` verification surface exists). All previously-open questions (Q1 turn
limit/draw, Q2 occupied hex, Q3 impulse ordering) are already resolved in the PRD.

## Outcome Traceability Matrix

| outcome_id | outcome (explicit) | milestone_id(s) | invariant_id(s) | final_check | baseline_verified |
| --- | --- | --- | --- | --- | --- |
| O1 | scenario load from TOML | M1, M4 | inv-snapshot-contract | scenario loads; slice1 snapshot correct | yes (cargo test EXIT=101) |
| O2 | ship: one hex, 6 facings, class, speed_max, turn_mode field | M1, M2 | inv-turn-mode-field, inv-generic-ship-data | ship fields round-trip to snapshot | yes (no src) |
| O3 | validated orders; illegal rejected, no mutation | M2 | inv-declare-resolve | move-rules test file passes | yes (no src) |
| O4 | scripted ship follows waypoints deterministically | M3 | inv-determinism | opponent path exact-match test | yes (no src) |
| O5 | turns advance both ships (player then scripted) | M3 | inv-turn-container | turn-loop test passes | yes (no src) |
| O6 | Won on objective entry; else InProgress | M3, M4 | inv-acceptance-won | acceptance test status==Won; negative stays InProgress | yes (no src) |
| O7 | JSON snapshot after each order/turn | M1 | inv-snapshot-contract | snapshot fields asserted | yes (no src) |
| O8 | harness: scripted file + JSON stdin | M5 | inv-headless-deps | cargo run harness emits JSON | yes (cargo run EXIT=101) |
| O9 | headless core, no render dep, ms tests | M1 | inv-headless-deps | Cargo.toml deps == {serde,serde_json,toml,thiserror} | yes (no Cargo.toml) |
| O10 | acceptance test asserts Won, RED at baseline | M4 | inv-acceptance-won | cargo test --test acceptance passes | yes (cargo test EXIT=101) |
| O11 | fidelity hooks D1/D2/D3 (design-only) | M1, M2, M3 | inv-turn-container, inv-declare-resolve, inv-turn-mode-field | hook invariants hold | yes (no src) |
| (non-goal) | D1-D10 deferred | (none) | (none) | absent from slice-1 scope | n/a |

No orphan rows: every milestone (M1-M5) maps back to at least one outcome; every outcome maps
forward to at least one milestone, invariant, and final check.

## Final-State Invariants

```yaml
- id: inv-acceptance-won
  statement: The slice-1 acceptance integration test loads the scenario, routes the player ship into
    the objective hex over the turn structure, and asserts the snapshot status == Won.
  category: presence
  check: "cargo test --test acceptance test_player_reaches_objective_wins"
  baseline_polarity: FAIL (no crate; cargo test EXIT=101)
  post_condition: PASS after M4.
  failure_reasoning: Without movement + terminal logic wired end-to-end, the test cannot observe Won.
  scope: final
  cost: cheap
  rationale: O10 (the headline falsifiable acceptance gate).
  evidence: "cargo test (cwd shipsim) -> error: could not find Cargo.toml; EXIT=101"

- id: inv-snapshot-contract
  statement: The JSON snapshot always contains turn (u32), status (InProgress|Won), objective, map
    dims, and a ships array where each ship has id, class, position (q,r), facing (0-5), speed_max,
    and turn_mode.
  category: presence
  check: "cargo test --test tracer test_snapshot_shape (asserts all required JSON keys present)"
  baseline_polarity: FAIL (no crate)
  post_condition: PASS after M1; keys never removed thereafter.
  failure_reasoning: A future frontend renders this contract (ADR-0004); a missing key breaks it.
  scope: phase-end
  cost: cheap
  rationale: O7, O1.
  evidence: "find shipsim -name '*.rs' -> (no output)"

- id: inv-turn-container
  statement: The turn is modeled as an impulse-capable container type (e.g. a Turn struct that
    advances by applying a movement policy), NOT a bare integer counter with inlined single-move
    logic, so the 32-impulse chart (D1) can later be a policy inside it without reworking the loop.
  category: presence
  check: "cargo test --test movement test_turn_container_advances_via_policy AND grep -n 'struct Turn'
    src/game_state.rs (container type exists)"
  baseline_polarity: FAIL (no crate)
  post_condition: PASS after M3; type + policy seam present.
  failure_reasoning: If the turn loop hardcodes 'increment counter, move one hex', D1 needs a rewrite.
  scope: phase-end
  cost: cheap
  rationale: O11 / D1 hook. Reviewers reject any milestone that inlines the turn as a counter.
  evidence: "no src yet (greenfield); enforced once M3 lands"

- id: inv-declare-resolve
  statement: Movement is exposed as a two-phase declare-order -> resolve-order seam (a validated
    DeclaredOrder produced by declaration, then applied by resolution), NOT a single mutate call, so
    simultaneous resolution (D2) can collect declarations before resolving.
  category: presence
  check: "cargo test --test movement test_declare_then_resolve (declaration validates without
    mutating; resolution applies) AND grep -n 'fn declare' src/movement.rs"
  baseline_polarity: FAIL (no crate)
  post_condition: PASS after M2.
  failure_reasoning: A single fused apply-and-mutate call cannot collect declarations for later
    simultaneous resolution; D2 would require inverting control flow.
  scope: phase-end
  cost: cheap
  rationale: O3, O11 / D2 hook.
  evidence: "no src yet (greenfield); enforced once M2 lands"

- id: inv-turn-mode-field
  statement: Every ship carries a turn_mode field, loaded from TOML and present in the snapshot, and
    it is NOT enforced (a facing change is never rejected because of turn_mode in slice 1).
  category: presence
  check: "cargo test --test movement test_turn_mode_carried_and_unenforced (turn_mode round-trips to
    snapshot; a facing change is accepted regardless of turn_mode)"
  baseline_polarity: FAIL (no crate)
  post_condition: PASS after M2.
  failure_reasoning: If turn_mode is absent, enabling D3 later is a data migration, not validation
    logic; if it is enforced, slice-1 free turning breaks.
  scope: phase-end
  cost: cheap
  rationale: O2, O11 / D3 hook.
  evidence: "no src yet (greenfield); enforced once M2 lands"

- id: inv-determinism
  statement: No RNG anywhere in shipsim_core or the harness; the same scenario + same orders yields
    byte-identical snapshots.
  category: absence
  check: "grep -rniE 'rand|rng|thread_rng|SystemTime|Instant' shipsim/src && echo FOUND ; expect no
    FOUND ; AND cargo test --test acceptance test_run_is_reproducible (two identical runs, identical
    final snapshot JSON)"
  baseline_polarity: no-match at baseline because src does not exist yet; this invariant GUARDS
    against introducing RNG once code exists (guard/absence). It becomes non-vacuous the moment src
    exists: seeding any rand crate or nondeterministic clock makes the grep match and the
    reproducibility test can fail.
  post_condition: grep finds nothing AND reproducibility test PASSES at every pass from M1 onward.
  failure_reasoning: Any RNG or wall-clock read makes end-states non-exact and breaks A4.
  scope: every-pass
  cost: cheap
  rationale: A4, O4. NOTE: this is a guard invariant; its non-vacuity is proven by the paired
    reproducibility test, not by the grep alone.
  evidence: "no src yet; guard active from M1"

- id: inv-headless-deps
  statement: shipsim_core dependencies are exactly {serde, serde_json, toml, thiserror}; no rendering,
    windowing, or display crate; crate-type includes cdylib+rlib; panic=abort.
  category: absence
  check: "check Cargo.toml [dependencies] == {serde,serde_json,toml,thiserror}; grep -niE
    'love2d|ggez|bevy|winit|sdl|wgpu|macroquad|piston' shipsim/Cargo.toml -> no match"
  baseline_polarity: no-match at baseline (no Cargo.toml). Guard invariant: adding a render/display
    dependency makes the check fail once Cargo.toml exists.
  post_condition: dependency set unchanged; no render dep; enforced from M1.
  failure_reasoning: A render dep in the core violates ADR-0001's hard boundary and story 15.
  scope: every-pass
  cost: cheap
  rationale: O9, ADR-0001.
  evidence: "cargo test -> could not find Cargo.toml (no manifest exists yet)"

- id: inv-generic-ship-data
  statement: Shipped ship/scenario TOML uses generic SFB-inspired names and shipsim-chosen stats; no
    ADB-trademarked ship names or published stat blocks.
  category: absence
  check: "grep -riE 'federation|klingon|romulan|gorn|tholian|<known ADB ship-name denylist>'
    shipsim/data shipsim/scenarios -> no match (denylist enumerated in the plan, executor may not add
    exceptions)"
  baseline_polarity: no-match at baseline (no data). Guard invariant: introducing a trademarked name
    makes it match.
  post_condition: no denylisted term present; enforced from M1.
  failure_reasoning: Shipping trademarked content violates ADR-0003.
  scope: phase-end
  cost: cheap
  rationale: O2, ADR-0003. Denylist (enumerated, no invariant laundering): federation, klingon,
    romulan, gorn, tholian, orion, kzinti, hydran, lyran, "star fleet battles". Generic names such as
    "Heavy Cruiser", "Escort", "Frigate" are allowed.
  evidence: "find shipsim -name '*.rs' -> (no output); no data dir yet"
```

Cheap per-pass subset: `inv-determinism`, `inv-headless-deps`. Full suite (all eight invariants) runs
at phase-end and at final verification. No invariant is `expensive`; all run at `final` anyway.

## Phased Plan

### Phase: P1 -- Movement Skeleton Core + Harness
- objective: Stand up `shipsim_core` (pure-Rust headless library) plus a thin CLI/JSON harness that
  loads a scenario, applies validated movement orders under a turn structure, resolves a scripted
  opponent deterministically, and reports `Won` when a ship enters the objective hex, all proven by
  `cargo test`.
- rationale: Delivers the whole slice-1 vertical (O1-O11) as the trustworthy foundation the PRD
  demands, with the D1/D2/D3 hooks baked into structure.
- prerequisites: cargo 1.91.1 present (verified). None else.
- files-components: `shipsim/Cargo.toml`; `shipsim/src/{lib.rs,hex.rs,board.rs,ship.rs,schema.rs,
  scenario.rs,movement.rs,game_state.rs,snapshot.rs}`; `shipsim/src/bin/shipsim.rs`;
  `shipsim/data/ships/*.toml`; `shipsim/scenarios/slice1.toml`;
  `shipsim/tests/{tracer.rs,movement.rs,acceptance.rs,harness.rs}`.
- blast_radius: contained entirely within `shipsim/`. No file outside `shipsim/` is created or
  modified. No consumed interface, no persisted-data migration.
- rollback_boundary: `rm -rf shipsim/{Cargo.toml,Cargo.lock,src,data,scenarios,tests,target}` restores
  the pre-slice state; nothing outside `shipsim/` is touched, so unrelated monorepo work is preserved.
- risks: (a) over-abstraction of the D1/D2 hooks beyond slice-1 need -- mitigated by Ponytail check
  and by keeping hooks to the minimum structural seam; (b) hex axial/cube conversion off-by-one --
  mitigated by M2 distance/neighbor unit tests mirroring NorRust hex.rs tests; (c) data-file path
  resolution in tests -- mitigated by the CARGO_MANIFEST_DIR contract (NorRust test_ffi pattern).
- exit_gate: `cargo test` (whole crate) passes AND `cargo run --bin shipsim -- --scenario
  scenarios/slice1.toml --orders <scripted file>` prints a final JSON snapshot with `status: "Won"`,
  AND all eight invariants hold.

#### Milestone: M1-tracer-bullet-e2e
- outcome (observable): From a cold checkout, `cargo test --test tracer` passes: a test loads a
  minimal scenario TOML, applies exactly one validated move order through the public API, and reads a
  JSON snapshot showing the ship's new (q,r) position, its facing, turn number, and
  `status: "InProgress"`. This is the thin end-to-end path through every layer (loader -> ship/board
  -> declare/resolve order -> game_state -> snapshot serialization).
- traces_to: O1, O7, O9, O11 (partial: establishes turn container + declare/resolve seams).
- implementation_scope: create `Cargo.toml`, `lib.rs`, and MINIMAL versions of hex/board/ship/schema/
  scenario/movement/game_state/snapshot sufficient for one move + one snapshot; `data/ships/
  heavy_cruiser.toml`; a minimal `scenarios/tracer.toml`; `tests/tracer.rs`. Full validation rules
  and the win/scripted/turn-loop behaviors are deferred to M2/M3/M4.
- dependencies: none (first milestone).
- tracer_bullet: yes. Rationale: this is the required thin, runnable end-to-end slice (load scenario
  -> apply one order -> emit JSON snapshot) through the highest practical seam (the scenario-run
  surface of `shipsim_core`), before any single layer is fleshed out. Crate scaffolding is folded in
  here rather than in a separate horizontal milestone (Ponytail: no scaffold-only milestone).
- implementation_contracts: see Downstream Handoff file contracts (Cargo.toml, lib.rs, hex.rs,
  board.rs, ship.rs, schema.rs, scenario.rs, movement.rs, game_state.rs, snapshot.rs, tracer.toml,
  heavy_cruiser.toml, tests/tracer.rs). contract_depth: detailed for game_state/movement/snapshot,
  algorithmic for hex, thin for lib.rs/Cargo.toml.
- subagent_work: role=Lead Planner authored contracts; implementation delegated to standard tier
  (cost_optimized: first-pass code is bounded and reversible). policy_rationale: greenfield scaffold
  is mechanical against detailed contracts. escalation_trigger: hex conversion test fails twice, or a
  contract is ambiguous -> escalate to reasoning tier. scope: files listed. inputs: this plan +
  NorRust hex.rs/schema.rs/snapshot.rs as reference. required output: passing `cargo test --test
  tracer` + `cargo build`.
- acceptance_gates:
  - `cargo test --test tracer` -> PASS. baseline_polarity: FAIL (EXIT=101, no Cargo.toml). evidence:
    `cargo test (cwd shipsim) -> could not find Cargo.toml; EXIT=101`. post_condition: PASS.
  - `cargo build` -> succeeds. baseline FAIL (no manifest). post_condition: PASS.
  - snapshot-shape assertion inside tracer test verifies all `inv-snapshot-contract` keys.
- gate_failure_reasoning: absent the crate and end-to-end wiring, neither compiles nor runs.
- invariants_at_risk: inv-snapshot-contract, inv-headless-deps, inv-determinism (all first enforced
  here).
- evidence_to_record: the passing tracer test output; the exact JSON snapshot printed.
- rollback_unit: delete the files created by M1.
- stop_conditions: if a render/display crate seems needed for any slice-1 behavior -> STOP (violates
  ADR-0001; it never is). If TOML shape cannot represent axial (q,r) placements cleanly -> revisit
  schema contract before proceeding.

#### Milestone: M2-movement-rules-and-hex
- outcome (observable): `cargo test --test movement` passes: hex distance/neighbor correctness holds;
  facing stays 0-5; a move beyond `speed_max` is rejected with a typed error and NO state change; an
  off-map move is rejected; a move into an occupied hex is rejected; a facing-change order is accepted
  regardless of `turn_mode` (free turning); declaration validates without mutating and resolution
  applies (declare/resolve seam).
- traces_to: O2, O3, O11 (D2, D3 hooks).
- implementation_scope: flesh out `hex.rs` (cube conversion, distance, neighbors, facing<->direction),
  `movement.rs` (Order, DeclaredOrder, OrderError via thiserror, declare/resolve, per-turn
  hexes-moved tracking), `ship.rs` (turn_mode carried), `game_state.rs` occupancy index; extend
  `snapshot.rs` to include turn_mode; `tests/movement.rs`.
- dependencies: M1.
- tracer_bullet: no (deepens the movement layer behind the already-running tracer path).
- implementation_contracts: hex.rs (algorithmic), movement.rs (algorithmic), game_state.rs (detailed),
  ship.rs (detailed). See Downstream Handoff.
- subagent_work: role=implementer, standard tier. policy_rationale: bounded rule logic with exact,
  falsifiable tests. escalation_trigger: a rejection test cannot be made to fail at baseline (would
  indicate a vacuous gate) -> escalate to reasoning tier for gate redesign. required output: passing
  `cargo test --test movement`.
- acceptance_gates:
  - `cargo test --test movement` -> PASS. baseline_polarity: FAIL (no crate at plan time; after M1 the
    test file does not yet exist / its assertions fail because rules are unimplemented). evidence:
    `cargo test EXIT=101` at plan baseline; executor MUST confirm each rejection test is RED before
    implementing the rule (write test first, observe failure). post_condition: PASS.
  - each of the four rejection tests (over-speed, off-map, occupied, and a passing facing-change)
    individually observed RED before its rule exists (non-vacuity proof).
- gate_failure_reasoning: without the validation rules, illegal orders would mutate state (test sees a
  changed position/facing) or the API would not exist.
- invariants_at_risk: inv-declare-resolve, inv-turn-mode-field.
- evidence_to_record: RED-then-GREEN transcript for each rejection rule.
- rollback_unit: revert M2 edits to the touched files.
- stop_conditions: if enforcing occupancy requires per-impulse ordering -> STOP (A8 says per-turn
  ordering suffices; do not build D1/D2 behavior).

#### Milestone: M3-opponent-and-turn-structure
- outcome (observable): `cargo test --test movement` (turn/opponent cases) passes: the scripted ship
  advances deterministically toward its next waypoint each turn (exact positions asserted); a full
  turn advances both ships with the player-plotted ship resolving before the scripted ship
  (sequential); a run that never reaches the objective stays `InProgress`; the turn is advanced via an
  impulse-capable container type (not a bare counter).
- traces_to: O4, O5, O6 (InProgress branch), O11 (D1 hook).
- implementation_scope: `game_state.rs` turn container (`Turn` struct + `step`/`end_turn` applying a
  movement policy), scripted-waypoint advance logic, sequential player-then-scripted resolution,
  status stays InProgress until objective; extend `scenario.rs`/`schema.rs` for waypoints;
  `tests/movement.rs` turn+opponent cases.
- dependencies: M1, M2.
- tracer_bullet: no.
- implementation_contracts: game_state.rs (detailed/algorithmic for the turn loop), scenario.rs
  (detailed). See Downstream Handoff.
- subagent_work: role=implementer, standard tier. escalation_trigger: sequential-vs-container design
  tension (avoid building D1/D2) -> escalate to reasoning tier for a Ponytail ruling. required output:
  passing turn/opponent tests.
- acceptance_gates:
  - `cargo test --test movement test_scripted_ship_follows_waypoints` -> PASS; asserts EXACT positions
    per turn. baseline: FAIL (logic absent). post_condition: PASS.
  - `cargo test --test movement test_run_without_objective_stays_in_progress` -> PASS. baseline FAIL.
  - `inv-turn-container` check (grep struct Turn + policy-advance test) holds.
- gate_failure_reasoning: without waypoint advance + turn loop, positions never change across turns
  and status logic is absent.
- invariants_at_risk: inv-turn-container, inv-determinism.
- evidence_to_record: per-turn position trace of the scripted ship.
- rollback_unit: revert M3 edits.
- stop_conditions: if determinism cannot be guaranteed (ordering ambiguity) -> STOP and fix ordering
  before M4.

#### Milestone: M4-acceptance-won
- outcome (observable): `cargo test --test acceptance` passes: the test loads `scenarios/slice1.toml`,
  plots the player ship along a legal path into the objective hex across the turn structure, advances
  turns, and asserts the emitted snapshot reports `status == Won`. This is the headline falsifiable
  acceptance gate. A companion negative test confirms a non-winning run stays `InProgress`, and a
  reproducibility test confirms two identical runs produce identical final snapshots.
- traces_to: O6 (Won branch), O10.
- implementation_scope: author `scenarios/slice1.toml` (map, two ship placements+facings+class refs,
  scripted waypoints, objective hex) and a scripted orders sequence for the player ship;
  `tests/acceptance.rs`. Wiring of Won-on-objective-entry lands here if not already complete from M3.
- dependencies: M1, M2, M3.
- tracer_bullet: no (this is the acceptance milestone that owns the headline gate).
- implementation_contracts: slice1.toml (detailed), tests/acceptance.rs (detailed). See Downstream
  Handoff. NOTE: the executor MUST verify with `hex.distance` that the player's start-to-objective
  distance is reachable within the planned turns given `speed_max`; if not, adjust placements/speed in
  slice1.toml (recorded as a permitted data tuning, not a contract change).
- subagent_work: role=implementer, standard tier; contract_review at reasoning tier for this slice
  (it is the acceptance gate -- high-risk). escalation_trigger: acceptance test passes at first write
  without a RED observation -> STOP (vacuous gate) and re-derive.
- acceptance_gates:
  - `cargo test --test acceptance test_player_reaches_objective_wins` -> PASS. baseline_polarity:
    FAIL (EXIT=101 now; and after M1-M3 the test is RED until the win path + scenario exist). evidence:
    `cargo test EXIT=101`. The executor MUST observe this test RED (status==InProgress or compile
    error) BEFORE the win wiring/scenario is complete -- non-vacuity proof. post_condition: PASS.
  - `cargo test --test acceptance test_non_winning_run_stays_in_progress` -> PASS.
  - `cargo test --test acceptance test_run_is_reproducible` -> PASS (paired with inv-determinism).
- gate_failure_reasoning: without terminal-condition wiring and a solvable scenario, the snapshot
  never reports Won.
- invariants_at_risk: inv-acceptance-won, inv-determinism, inv-snapshot-contract.
- evidence_to_record: the RED acceptance run, then the GREEN run with the final `status: "Won"` JSON.
- rollback_unit: revert M4 files (scenario + acceptance test + any win-wiring).
- stop_conditions: if the objective can be reached only by an illegal move -> the scenario is
  mis-designed; fix placements/speed before claiming the gate.

#### Milestone: M5-cli-json-harness
- outcome (observable): `cargo run --bin shipsim -- --scenario scenarios/slice1.toml --orders <file>`
  runs the scenario to completion and prints JSON snapshots (final one `status: "Won"`); AND
  `--stdin` mode reads orders as JSON lines on stdin and prints a snapshot after each. `cargo test
  --test harness` drives the binary path (or the same public run surface the binary uses) and asserts
  the printed JSON parses and matches the snapshot contract. Deterministic.
- traces_to: O8, O7.
- implementation_scope: `src/bin/shipsim.rs` (arg parsing for `--scenario`, `--orders <file>`,
  `--stdin`; loads scenario+ship data; drives `shipsim_core`; serializes snapshots to stdout);
  `tests/harness.rs`; a scripted orders file under `scenarios/` or `tests/fixtures/`.
- dependencies: M1, M2, M3, M4 (reuses the win path + scenario).
- tracer_bullet: no.
- implementation_contracts: src/bin/shipsim.rs (detailed), tests/harness.rs (detailed). See Downstream
  Handoff. The order-JSON-on-stdin schema is specified in the bin contract (one JSON order per line:
  `{"type":"move","ship":<id>,"to":{"q":..,"r":..}}` / `{"type":"face","ship":<id>,"facing":0..5}` /
  `{"type":"end_turn"}`).
- subagent_work: role=implementer, standard tier. escalation_trigger: stdin framing ambiguity ->
  escalate for a contract clarification. required output: passing `cargo test --test harness` +
  working `cargo run`.
- acceptance_gates:
  - `cargo run --bin shipsim -- --scenario scenarios/slice1.toml --orders <scripted file>` -> exits 0,
    final stdout line is JSON with `status: "Won"`. baseline_polarity: FAIL (`cargo run EXIT=101`).
    evidence: `cargo run (cwd shipsim) -> could not find Cargo.toml; EXIT=101`. post_condition: PASS.
  - `cargo test --test harness` -> PASS (parses printed JSON; asserts contract + determinism).
  - stdin mode: piping a JSON order stream produces a snapshot per order (asserted in harness test).
- gate_failure_reasoning: without the binary, `cargo run` cannot resolve a target and exits 101.
- invariants_at_risk: inv-headless-deps (bin must not pull a render dep), inv-determinism.
- evidence_to_record: the harness stdout for both `--orders` and `--stdin` modes.
- rollback_unit: delete `src/bin/shipsim.rs` + `tests/harness.rs` + the scripted orders fixture.
- stop_conditions: if driving the binary in-test is infeasible, drive the identical public run surface
  of `shipsim_core` that the binary calls (PRD: acceptance lives at the core scenario-run surface),
  and keep a thin smoke `cargo run` check for the binary itself.

## Subagent Matrix

| Work item | Role | Tier | Delegate? | Policy rationale | Escalation trigger | Inputs | Required output |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Baseline + repo/NorRust discovery | Repository Scout | fast/standard | inline (done in this loop) | cheap, tightly coupled to planning | n/a | shipsim/, norrust/ | cited baselines (recorded above) |
| Dependency/boundary check | Dependency Analyst | standard | inline | greenfield, trivial (one crate) | new external consumer appears | ADRs, Cargo template | inv-headless-deps + no cross-boundary edits |
| Test/gate design | Test & Gate Analyst | standard | inline | gates are few and derived from PRD story 24 | a gate cannot be made RED | PRD testing decisions | falsifiable gates + baselines (recorded) |
| Contracts authoring | Lead Planner | reasoning | inline | architecture judgment (D1/D2/D3 hooks) | conflict needing arch change | this plan | file contracts (Downstream Handoff) |
| M1-M5 first-pass code | Implementer | standard | delegate | cost_optimized; bounded, reversible, contract-guided | 2x same failure / ambiguous contract -> reasoning | contracts + NorRust refs | passing per-milestone gates |
| M4 acceptance-gate contract review | Contract Reviewer | reasoning | delegate | acceptance gate is the highest-risk slice | test green without a RED observation | M4 contract | non-vacuity confirmed |
| Reviewer lenses + gate audit | Reviewers / Gate Auditor | standard/reasoning | inline (no fresh instance available) | single-model runtime; gate_audit: inline per sec 13 degradation | n/a | draft plan + evidence | findings (Review Record) |

## Consolidated Verification

Run from `shipsim/` after all milestones. Proves: (a) new behavior works; (b) no obsolete
assets/references (none exist -- greenfield); (c) affected tests pass; (d) unrelated behavior not
regressed (nothing outside shipsim/ touched); (e) config/terminology reflect only the intended final
state.

1. `git -C /mnt/storage/git_home status --short | grep -vE '^..(shipsim/)' ` -> no NEW changes
   outside shipsim/ introduced by this work (unrelated pre-existing dirty files remain, untouched).
2. `cargo build` -> succeeds.
3. `cargo test` (whole crate: unit + tracer + movement + acceptance + harness) -> all PASS.
4. `cargo test --test acceptance test_player_reaches_objective_wins` -> PASS (headline gate).
5. `cargo test --test acceptance test_non_winning_run_stays_in_progress` -> PASS.
6. `cargo test --test acceptance test_run_is_reproducible` -> PASS (determinism).
7. Invariant sweep:
   - inv-determinism: `grep -rniE 'rand|rng|thread_rng|SystemTime|Instant' shipsim/src` -> no match.
   - inv-headless-deps: Cargo.toml `[dependencies]` == {serde, serde_json, toml, thiserror};
     `grep -niE 'love2d|ggez|bevy|winit|sdl|wgpu|macroquad|piston' Cargo.toml` -> no match.
   - inv-generic-ship-data: `grep -riE 'federation|klingon|romulan|gorn|tholian|orion|kzinti|hydran|
     lyran|star fleet battles' shipsim/data shipsim/scenarios` -> no match.
   - inv-turn-container / inv-declare-resolve / inv-turn-mode-field: their named tests pass.
   - inv-snapshot-contract: tracer snapshot-shape test passes.
8. `cargo run --bin shipsim -- --scenario scenarios/slice1.toml --orders <scripted file>` -> final
   stdout JSON has `status: "Won"`.
9. Confirm `docs/ROADMAP.md` D1-D10 remain deferred (no combat/energy/frontend/turn-mode-enforcement/
   impulse-chart code shipped): `grep -riE 'phaser|disruptor|shield|energy_alloc|impulse_chart'
   shipsim/src` -> no match.

## Replan Triggers
- A `shipsim/src` tree already exists at execution start (greenfield baselines stale) -> return to
  this loop in `review-existing` mode.
- Any acceptance/rejection test passes on first write without an observed RED baseline -> gate is
  vacuous; STOP and re-derive (do not proceed).
- The slice-1 scenario is unsolvable within the planned turns given `speed_max` (verify via
  `hex.distance`) -> retune slice1.toml placements/speed (data tuning) before claiming M4.
- A milestone appears to need combat/energy/impulse-chart/turn-mode-enforcement/AI to pass a gate ->
  the gate or milestone is mis-scoped; STOP (those are non-goals N-D1..N-D10).
- A render/display or RNG dependency seems required -> STOP (violates ADR-0001 / A4).
- HEAD moved and a cited path under shipsim/ changed -> re-run that citation and re-validate.

## Downstream Handoff

Executor MUST first run the Staleness Contract check (Repository State section): confirm HEAD, confirm
`shipsim/` has no pre-existing Rust source, echo the model routing (`current-model-only`). Then
execute milestones in order M1 -> M2 -> M3 -> M4 -> M5 (dependencies are linear and acyclic). No
milestone is a spike; none authorizes editing anything outside `shipsim/`.

Human-confirmation points: none required (blast-radius gate does not fire). M4 (acceptance gate) gets
reasoning-tier contract review but not a human gate.

File contracts (standard tier: per production file the executor may create/modify). Depth noted per
file; the executor may add private helpers within a file's stated responsibility and test helpers
scoped to one test file without a contract update. A new public/exported symbol, a new production
file, a new dependency, or any edit outside `shipsim/` requires a contract update or replan.

- path: shipsim/Cargo.toml  (contract_depth: thin)
  responsibility: package `shipsim_core`, edition 2021, `[lib] crate-type=["cdylib","rlib"]`,
  `[[bin]] name="shipsim" path="src/bin/shipsim.rs"`, deps EXACTLY serde(+derive)/serde_json/toml/
  thiserror, `[profile.dev]` and `[profile.release]` `panic="abort"`. forbidden: any render/display
  crate; any RNG crate. gate: inv-headless-deps.
- path: shipsim/src/lib.rs  (thin)
  responsibility: `pub mod hex; board; ship; schema; scenario; movement; game_state; snapshot;`.
  forbidden: business logic in lib.rs.
- path: shipsim/src/hex.rs  (algorithmic)
  responsibility: axial `Hex { q: i32, r: i32 }` stored; cube conversion for math; `distance`,
  `neighbors() -> [Hex;6]`, direction<->facing (facing 0-5 indexes the 6 neighbor directions), add of
  a direction step. Mirror NorRust hex.rs test coverage (distance, neighbor count/uniqueness,
  add/sub inverse) but for AXIAL storage. invariants: facing always in 0..=5; neighbor of a hex is
  distance 1. gate: movement hex tests. forbidden: offset/orientation assumptions leaking into the
  core (orientation is a frontend concern).
- path: shipsim/src/board.rs  (detailed)
  responsibility: bounded rectangle `Board { width: u32, height: u32 }`; `contains(Hex) -> bool` (A2,
  no wraparound). forbidden: terrain/movement-cost modeling (deferred, not slice 1).
- path: shipsim/src/ship.rs  (detailed)
  responsibility: runtime `Ship { id, class: String, pos: Hex, facing: u8 (0-5), speed_max: u32,
  turn_mode: u32 }`; `turn_mode` carried, NEVER read by validation in slice 1. invariants:
  inv-turn-mode-field. forbidden: enforcing turn_mode; any combat/energy field behavior.
- path: shipsim/src/schema.rs  (detailed)
  responsibility: `#[derive(Deserialize)]` TOML defs: `ShipDef { id/class/name, speed_max, turn_mode
  (with serde default) }`; `ScenarioDef { width, height, objective:{q,r}, [[ships]] placement
  (class ref, q, r, facing, controller: player|scripted), scripted waypoints [{q,r}] }`. Mirror
  NorRust schema.rs `#[serde(default)]` idiom. forbidden: combat/energy fields.
- path: shipsim/src/scenario.rs  (detailed)
  responsibility: `load_scenario(&Path) -> Result<Scenario, LoadError>` and ship-data loading from
  `data/ships/*.toml`; validate objective/placements are on-board; build initial `GameState`. Mirror
  NorRust scenario.rs (std::fs + toml::from_str, validating result). forbidden: RNG; silent
  clamping of off-board placements (return an error).
- path: shipsim/src/movement.rs  (algorithmic)
  responsibility: `enum Order { Move{ship,to:Hex}, Face{ship,facing:u8}, EndTurn }`; a validated
  `DeclaredOrder`; `declare(&GameState, Order) -> Result<DeclaredOrder, OrderError>` (validation ONLY,
  no mutation) and `resolve(&mut GameState, DeclaredOrder)` (apply). `OrderError` via `thiserror`:
  OffMap, BeyondSpeed, HexOccupied, NotAdjacent, ShipNotFound, NotSixFacing. Track per-turn
  hexes-moved per ship; reject Move that would exceed `speed_max`; reject Move into an occupied hex or
  off-board or non-adjacent; Face always accepted for 0-5 (free turning). invariants:
  inv-declare-resolve, inv-turn-mode-field. forbidden: fusing declare+resolve into one mutate call;
  per-impulse ordering (A8); simultaneous resolution (N-D2).
- path: shipsim/src/game_state.rs  (detailed/algorithmic for the turn loop)
  responsibility: `GameState { board, ships, positions/occupancy index, turn: Turn, status:
  ScenarioStatus }`; `struct Turn` = impulse-capable container (number + a movement-policy application
  seam, NOT a bare counter); `apply_order` = declare-then-resolve convenience; `step`/`end_turn`
  advances the turn by applying the slice-1 movement policy: resolve the player-plotted ship, THEN the
  scripted ship toward its next waypoint (sequential); recompute `status` (Won if any ship pos ==
  objective, else InProgress). `enum ScenarioStatus { InProgress, Won }` (Serialize as string).
  invariants: inv-turn-container, inv-determinism. forbidden: inlining the turn as "increment counter
  + one hardcoded move"; RNG; wall-clock reads; multi-objective or draw/timeout terminal (N-D10, A6).
- path: shipsim/src/snapshot.rs  (detailed)
  responsibility: `#[derive(Serialize)] StateSnapshot { turn: u32, status: ScenarioStatus,
  map:{width,height}, objective:{q,r}, ships: Vec<ShipSnapshot{ id, class, q, r, facing, speed_max,
  turn_mode }> }`; `from_game_state(&GameState) -> StateSnapshot`; JSON via serde_json. Mirror NorRust
  snapshot.rs flat-Serialize-struct + from_game_state idiom. invariants: inv-snapshot-contract.
  forbidden: dropping any contracted key; shield/damage/energy fields (N-D5..N-D7).
- path: shipsim/src/bin/shipsim.rs  (detailed)
  responsibility: CLI harness. Args: `--scenario <path>` (required), `--orders <path>` (scripted
  orders file, one JSON order per line) XOR `--stdin` (read JSON orders from stdin). Load scenario +
  ship data, drive `shipsim_core`, print a `StateSnapshot` JSON to stdout after each applied order and
  at turn boundaries; exit 0 on completion. Order JSON: `{"type":"move","ship":<id>,"to":{"q":_,"r":_}}`,
  `{"type":"face","ship":<id>,"facing":0..5}`, `{"type":"end_turn"}`. invariants: inv-headless-deps
  (no render dep), inv-determinism. forbidden: embedding game rules in the binary (rules live in the
  core, ADR-0001); RNG.
- path: shipsim/data/ships/heavy_cruiser.toml (+ one more generic ship, e.g. escort.toml)  (thin)
  responsibility: generic class name ("Heavy Cruiser"/"Escort"), shipsim-chosen `speed_max`,
  `turn_mode`. invariants: inv-generic-ship-data. forbidden: any denylisted trademarked term.
- path: shipsim/scenarios/tracer.toml  (thin)
  responsibility: minimal 1-ship-move scenario for M1 (small map, one player ship, an objective the
  test does NOT reach in one move so status stays InProgress).
- path: shipsim/scenarios/slice1.toml  (detailed)
  responsibility: the acceptance scenario -- bounded map, a player "Heavy Cruiser" and a scripted
  "Escort" with placements+facings+class refs, scripted waypoints, and an objective hex REACHABLE by
  the player within the planned turns given `speed_max` (executor verifies via `hex.distance`).
  invariants: inv-generic-ship-data. forbidden: unreachable/illegal-only objective.
- path: shipsim/tests/tracer.rs  (detailed)
  responsibility: M1 end-to-end + snapshot-shape assertions.
- path: shipsim/tests/movement.rs  (detailed)
  responsibility: M2 rejection/hex tests + M3 turn/opponent/InProgress tests + D1/D2/D3 hook tests.
- path: shipsim/tests/acceptance.rs  (detailed)
  responsibility: headline `status == Won` test + non-winning-InProgress test + reproducibility test.
  Resolve data paths via `env!("CARGO_MANIFEST_DIR")` (NorRust test_ffi idiom).
- path: shipsim/tests/harness.rs  (detailed)
  responsibility: drive the binary (or the identical core run surface) and assert printed JSON parses,
  matches the contract, and is reproducible.

Reasoning-tier code-writing exception: allowed ONLY for M4 if the acceptance wiring fails the same
contract twice under standard tier; record the exception and why review-only feedback was
insufficient. Contract review cadence: standard tier per milestone; reasoning tier for M4.

## Review Record

Reviewer lenses and the gate audit ran as labeled inline passes (single-model runtime; no fresh
instance -> `gate_audit: inline` per sec 13 degradation). Findings, all keyed to stable ids:

Accepted / adapted:
- fold-scaffold-into-tracer (Simplification/Ponytail, Minor -> accepted): do NOT create a
  scaffold-only milestone; crate creation folds into M1 so the first non-spike milestone is a real
  tracer bullet, not a horizontal "build the crate" layer. Applied.
- turnmode-must-be-observable (Completeness, Material -> adapted): D3 "field present" is not
  observable unless it reaches the snapshot; snapshot now includes `turn_mode` and
  inv-turn-mode-field asserts round-trip + non-enforcement. Applied.
- determinism-gate-vacuity (GateAudit, Material -> adapted): a pure absence grep for RNG is vacuous on
  greenfield (nothing to match); reframed inv-determinism as a guard paired with a positive
  reproducibility test (two identical runs -> identical JSON), which is non-vacuous once code exists.
  Applied.
- deps-lockdown (RepositoryAlignment, Material -> accepted): added inv-headless-deps pinning the exact
  ADR-0001 dependency set and forbidding render deps in both lib and bin. Applied.
- data-path-resolution (ColdStart, Material -> accepted): tests resolve `data/` and `scenarios/` via
  `env!("CARGO_MANIFEST_DIR")` (NorRust test_ffi pattern), removing a cold-start guess. Applied.
- stdin-schema-ambiguity (PreMortem, Material -> accepted): specified the exact per-line order JSON
  schema in the bin contract. Applied.
- acceptance-nonvacuity (GateAudit, Blocking-if-unaddressed -> accepted): M4 must observe the
  acceptance test RED (compile error or status==InProgress) before wiring the win path; added as an
  explicit gate step, a replan trigger, and a stop condition. Applied.
- scenario-solvability (PreMortem, Material -> accepted): added a contract note + replan trigger that
  the executor verify start-to-objective reachability via `hex.distance` and retune slice1.toml if
  needed. Applied.

Conflict-triage decisions (sec 11.1):
- id: scaffold-milestone-vs-tracer -> winner_lens: GateFalsifiability/Sequencing (tracer bullet must
  run a real end-to-end gate), loser_lens: Simplification. Resolution satisfies BOTH: fold scaffold
  into M1 (simpler) while keeping M1's gate a genuine end-to-end test (falsifiable). No safety loss.

Rejected:
- add-turn-limit-terminal (proposed defensive draw/timeout) -> REJECTED: PRD resolved-decision Q1
  fixes status to exactly {InProgress, Won} with no timeout; adding one is scope creep (N-D10-adjacent).
- add-pathfinding-module (mirror NorRust) -> REJECTED (Ponytail): slice-1 player path is plotted by
  the test/harness and the scripted ship follows explicit waypoints; no pathfinder is needed.

Characterization-before-deletion: N/A -- greenfield, no behavior removed (no removal in this plan).

Falsification (sec 11):
- Cold-start walk over M1..M5: no remaining point requires an un-specified guess after the data-path,
  stdin-schema, and scenario-solvability fixes. No cold-start gap on a REQUEST outcome.
- Pre-mortem top failure modes and their guards: (a) vacuous acceptance gate -> RED-before-green rule;
  (b) hex conversion off-by-one -> M2 mirrors NorRust hex tests; (c) accidental non-determinism ->
  inv-determinism + reproducibility test; (d) stale repo state -> Staleness Contract check; (e)
  touching files outside shipsim/ -> Consolidated Verification step 1.

Gate audit (inline, per sec 13): every gate names a concrete failure it catches; every baseline
polarity is proven by a cited command or an explicit "observe RED before implementing" instruction;
no gate can pass while its traced outcome is unachieved (acceptance gate asserts the exact
`status == Won` snapshot the outcome demands). No open Blocking or Material finding remains.

Convergence ledger: Pass 1 draft -> Pass 2 applied the eight accepted/adapted findings above and the
two rejections. No finding id regressed or oscillated; Blocking/Material count went to 0. No thrash.
passes_used: 2.

```json
{
  "verdict": "READY",
  "task_tier": "standard",
  "tier_trigger": "multi-file greenfield Rust feature; new modules + new JSON interface + falsifiable acceptance gate; no migration, no consumed interface, no persisted-data change, no CI/security/billing surface; blast-radius gate does not fire",
  "execution_policy": "cost_optimized",
  "model_routing": "current-model-only",
  "model_tiers": { "fast": "current-runtime-model", "standard": "current-runtime-model", "reasoning": "current-runtime-model" },
  "passes": 2,
  "open_blocking_findings": 0,
  "open_material_findings": 0,
  "vacuous_gates_found": 0,
  "cold_start_gaps": 0,
  "uncited_facts": 0,
  "gate_audit": "inline",
  "staleness": { "head": "00075671b34ba0ecca69a9c2021368d17a628883", "dirty_files": ["shipsim/ (untracked; no Rust source)", "unrelated sibling projects untouched"], "discovered_at": "2026-07-04T09:09:01-06:00" },
  "traceability_complete": true,
  "orphan_milestones": [],
  "characterization_gaps": [],
  "conflicts_resolved": [ { "id": "scaffold-milestone-vs-tracer", "winner_lens": "Sequencing", "loser_lens": "Simplification" } ],
  "invariants": [
    { "id": "inv-acceptance-won", "category": "presence", "scope": "final", "cost": "cheap", "check": "cargo test --test acceptance test_player_reaches_objective_wins", "baseline_polarity": "FAIL (cargo test EXIT=101, no crate)", "evidence": "cargo test (cwd shipsim) -> could not find Cargo.toml; EXIT=101" },
    { "id": "inv-snapshot-contract", "category": "presence", "scope": "phase-end", "cost": "cheap", "check": "cargo test --test tracer test_snapshot_shape", "baseline_polarity": "FAIL (no crate)", "evidence": "find shipsim -name '*.rs' -> (no output)" },
    { "id": "inv-turn-container", "category": "presence", "scope": "phase-end", "cost": "cheap", "check": "cargo test --test movement test_turn_container_advances_via_policy", "baseline_polarity": "FAIL (no crate)", "evidence": "no src yet" },
    { "id": "inv-declare-resolve", "category": "presence", "scope": "phase-end", "cost": "cheap", "check": "cargo test --test movement test_declare_then_resolve", "baseline_polarity": "FAIL (no crate)", "evidence": "no src yet" },
    { "id": "inv-turn-mode-field", "category": "presence", "scope": "phase-end", "cost": "cheap", "check": "cargo test --test movement test_turn_mode_carried_and_unenforced", "baseline_polarity": "FAIL (no crate)", "evidence": "no src yet" },
    { "id": "inv-determinism", "category": "absence", "scope": "every-pass", "cost": "cheap", "check": "grep -rniE 'rand|rng|thread_rng|SystemTime|Instant' shipsim/src (no match) AND cargo test --test acceptance test_run_is_reproducible", "baseline_polarity": "no-match (guard); non-vacuous via paired reproducibility test once src exists", "evidence": "no src yet; guard active from M1" },
    { "id": "inv-headless-deps", "category": "absence", "scope": "every-pass", "cost": "cheap", "check": "Cargo.toml deps == {serde,serde_json,toml,thiserror}; grep render crates -> no match", "baseline_polarity": "no-match (guard; no Cargo.toml)", "evidence": "cargo test -> could not find Cargo.toml" },
    { "id": "inv-generic-ship-data", "category": "absence", "scope": "phase-end", "cost": "cheap", "check": "grep -riE '<ADB denylist>' shipsim/data shipsim/scenarios -> no match", "baseline_polarity": "no-match (guard; no data)", "evidence": "no data dir yet" }
  ],
  "cheap_every_pass_invariants": ["inv-determinism", "inv-headless-deps"],
  "blast_radius_decisions": [],
  "human_decisions_required": [],
  "plan_location": "/home/chris/git_home/shipsim/implementation-plan.md",
  "summary": "Standard-tier, tracer-bullet-first plan for shipsim slice-1 movement skeleton: 5 linear milestones (tracer e2e, movement+hex rules, opponent+turn structure, Won acceptance gate, CLI/JSON harness). Every gate proven RED at the greenfield baseline (cargo EXIT=101). D1/D2/D3 fidelity hooks captured as invariants/contract constraints, not built behavior. Blast-radius gate clear. READY."
}
```
