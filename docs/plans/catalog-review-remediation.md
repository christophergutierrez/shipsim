# Plan: Catalog review remediation and evidence rebuild

## Purpose

Resolve the merge blockers and process gaps identified by:

- `tmp/gpt_code_review.md`
- `tmp/grok_code_review.md`
- `tmp/ops_code_review.md`
- `tmp/grok_review_summary.md`
- `tmp/gpt_review_summary.md`
- `tmp/ops_review_summary.md`

This plan replaces review-time ambiguity with explicit roles, exact catalog
numbers, non-writing drift checks, runnable simulation suites, and a fresh
evidence ladder. It does not authorize tuning against held-out seed pools
before the corresponding phase.

## Adjudicated findings

| Review issue | Decision |
|---|---|
| `yard_baseline` gained a torpedo and stopped being a controlled SKU fixture | Fix. `yard_baseline` remains a one-beam control. |
| `yard_baseline` and `yard_destroyer` became mechanically identical | Fix through role separation: baseline stays one beam; standard destroyer keeps beam + torpedo. |
| Size-7 battery has three beams but the locked table requires four | Fix. Add a fourth beam on `aft_port`. |
| `design_matched.toml` aborts on swarm/capital | Fix as a pre-existing blocker; split catalog evidence from weapon-quality evidence. |
| Quality rows no longer meet tolerance after baseline torpedo | Fix by preserving the control and using exact per-pair counts. |
| Active `skip_cost_validation` suites can emit misleading “equal-budget” reports | Audit, archive stale claims, and prohibit the combination in active suites. |
| Generated ships can drift from design/compiler output | Add a pure render path and non-writing `check` / `check-all` gate; regenerate all yard outputs. |
| `docs/TODO.md` lists shipped M1–M9 work as future work | Rewrite against current `HEAD`. |
| `docs/SHIPYARD.md` omits systems and drift-check workflow | Document the complete current design surface. |
| Love default adds a second weapon without a regression test | Add exact default and round-trip assertions. |
| Clippy acceptance gate is red | Fix in a separate mechanical phase; do not mix with balance tuning. |
| Existing balance evidence predates the batteries/catalog identity | Mark stale, then rebuild with fast, pooled, eval, and sign-off tiers. |
| Cost ladder has a shallow size-3→4 step | Measure in adjacent-size evidence; do not change costs merely to smooth the curve. |
| Love yard was out of scope in the old plan | No defect. Keep it aligned because the file is already an authoring client; do not expand its rules authority. |

## Locked roles and numbers

These choices are prerequisites, not tuning levers.

### Catalog roles

| Class | Role | Required battery |
|---|---|---|
| `yard_swarm` | Standard size-1 ship | 2× `beam_compact` |
| `yard_destroyer` | Standard size-2 playable ship | `beam` + `torpedo` |
| `yard_light_cruiser` | Standard size-3 ship | Existing locked battery |
| `yard_heavy_cruiser` | Standard size-4 ship | Existing locked battery |
| `yard_battleship` | Standard size-5 ship | Existing locked battery |
| `yard_dreadnought` | Standard size-6 ship | Existing locked battery |
| `yard_capital` | Standard size-7 ship | 4× `beam` + 2× `torpedo` + 2× `plasma` |
| `yard_baseline` | Weapon-quality control, not an eighth standard | 1× ordinary `beam` |
| `yard_compact` | Weapon-quality fixture | 1× `beam_compact` |
| `yard_potent` | Weapon-quality fixture | 1× `beam_potent` |
| `yard_precise` | Weapon-quality fixture | 1× `beam_precise` |

The fourth capital beam uses `aft_port`. This fills the uncovered rear-port
mount while retaining the existing forward, forward-starboard, and
forward-port beam mounts.

### Expected costs after source-design correction

| Class | Expected cost |
|---|---:|
| `yard_swarm` | 74 |
| `yard_destroyer` | 98 |
| `yard_light_cruiser` | 216 |
| `yard_heavy_cruiser` | 309 |
| `yard_battleship` | 546 |
| `yard_dreadnought` | 1322 |
| `yard_capital` | 3470 |
| `yard_baseline` | 86 |
| `yard_compact` | 96 |
| `yard_potent` | 96 |
| `yard_precise` | 92 |

The capital value is the reviewed 3454 design plus one ordinary beam at cost
16 (verified: `[weapons.beam] cost = 16` in `data/components.toml`). Any
different result blocks **Phase 2** and must be explained before proceeding.

### Evidence rules

- `yard_baseline` is never used as the standard destroyer.
- A suite named or documented as equal-cost may not set
  `skip_cost_validation = true`.
- Quality comparisons vary one beam SKU only. No torpedo or second weapon may
  appear on the control or quality fixtures.
- Old reports involving changed standard batteries are historical, not current
  evidence.
- Seeds 1–191 are reusable tuning data.
- Seeds 328–391 are evaluation data and are run only after pooled acceptance.
- Seeds 264–327 remain virgin sign-off data until the final frozen candidate.

---

## Phase 0 — Capture and freeze the remediation baseline

### Goal

Preserve the exact starting state and prevent review artifacts or stale reports
from being mistaken for implementation evidence.

### Work

1. Record `git rev-parse HEAD`, `git status --short`, and the complete working
   diff in a dated file under `tmp/catalog-remediation/`.
2. Record current rules fingerprint and the output of `shipsim-yard cost` for
   all 11 yard designs.
3. Run the current non-mutating checks and save output:
   - `git diff --check`
   - `cargo test --all`
   - REPL unittest discovery
   - TUI tests
   - `shipsim-yard validate` for all yard designs
4. Reproduce and record the current `design_matched.toml` failure.
5. Inventory active simulation suites, their seed ranges, cost-validation
   setting, rubrics, and referenced classes.
6. Do not run seeds 264–327 or 328–391.

### Exit criteria

| Check | Pass condition |
|---|---|
| Baseline provenance | HEAD, dirty paths, rules fingerprint, and diff recorded |
| Current costs | 11-class table recorded |
| Broken-suite reproduction | Exact failing engagement and totals recorded |
| Suite inventory | Every `simulation/suites/*.toml` classified active, historical, or superseded |
| Held-out pools | No report contains seeds 264–327 or 328–391 |

### Commit

No source commit. Baseline files under `tmp/` remain local evidence.

---

## Phase 1 — Add deterministic compiler drift checking

> **Ordering note (changed from an earlier draft).** Drift tooling now comes
> *before* the catalog correction. The reverse order left a commit in which
> `data/designs/yard_baseline.toml` had no torpedo while
> `data/ships/yard_baseline.toml` still did — and every consumer
> (`fleet_cost`, all suites, the running game) reads the *compiled* ship, so
> that commit would ship the exact defect this plan exists to fix, with no tool
> able to detect it. Building the check first also means the catalog fix is
> verified by `check-all` from the moment it lands.

### Goal

Make `data/designs/*.toml` the authoring source and prove every committed
generated ship is byte-identical to current compiler output without writing
during checks.

### Work

1. Extract a pure Shipyard pipeline:
   - parse design;
   - validate/project `ShipDef`;
   - render canonical TOML bytes including the generated marker;
   - return bytes without filesystem mutation.
2. Keep `compile` as the only writing path. It consumes the pure rendered bytes
   and preserves atomic overwrite protection.
3. Add CLI commands:
   - `shipsim-yard check <design.toml>`: compare expected bytes with the target
     `data/ships/<id>.toml`, exit 0 on equality and nonzero with a concise diff
     summary on drift;
   - `shipsim-yard check-all`: discover all committed yard designs in stable
     path order and check each;
   - neither command creates, renames, or rewrites a file.
4. **Canonical serialization for empty additive fields — decided:** annotate
   `ShipDef.systems` with `#[serde(default, skip_serializing_if = "Vec::is_empty")]`
   so an empty vector is *omitted*. Two reasons, both concrete:
   - it matches existing repository precedent — `src/snapshot.rs:54` already
     uses `skip_serializing_if = "Vec::is_empty"` for the same shape;
   - it keeps the Phase 2 regeneration diff scoped to the real catalog change.
     Today `src/schema.rs:43` declares `pub systems: Vec<SystemDef>` with no
     skip, so a compile emits `systems = []` and dirties **all 11** ships for a
     field this work never touched. Omitting empty vectors makes the 9
     unchanged ships already byte-correct.

   Lock the chosen form with a test. Do not tolerate both forms. If a future
   change makes emitting preferable, flip it deliberately and regenerate in a
   dedicated commit.
5. Add tests for missing output, stale cost, stale weapon, extra field,
   deterministic ordering, and no-write behavior.
6. Run `check-all` against the **current** tree and require 11/11 green. With
   the serialization decision above this should pass without regenerating
   anything — that is the point of doing tooling first, and any failure here is
   a real pre-existing drift to investigate before Phase 2.
7. Run explicit `compile` twice for one design and require a byte-empty diff.

### Exit criteria

| Check | Pass condition |
|---|---|
| Pure render | Unit test produces canonical bytes without filesystem writes |
| Single check | Detects equal, missing, and stale targets |
| Check all | 11/11 outputs match current compiler, no regeneration required |
| No-write guarantee | File mtimes/content unchanged after `check` and `check-all` |
| Determinism | Two explicit compiles are byte-identical |
| Generated marker | Present on 11/11 ship files |
| Source/target identity | IDs, costs, systems, weapons, flags, arcs, and mounts match projection |

### Commit

`feat(shipyard): add non-writing generated catalog checks`

---

## Phase 2 — Correct catalog source roles and batteries

### Goal

Make source designs agree with the locked role table, and regenerate their
compiled output in the same commit so the two never disagree in committed
history.

### Work

1. Remove the torpedo from `data/designs/yard_baseline.toml`.
2. Keep beam + forward torpedo on `data/designs/yard_destroyer.toml`.
3. Keep two compact beams on `data/designs/yard_swarm.toml`.
4. Add an ordinary beam at `aft_port` to
   `data/designs/yard_capital.toml`, yielding 4/2/2. Capacity is not a concern:
   the size-7 hull carries `space = 20000` and a beam costs 10.
5. Audit the unchanged size-3 through size-6 designs against the locked battery
   table in `catalog-combat-squads.md`.
6. Keep the Love and documentation default as a playable standard destroyer:
   beam + torpedo. Do not point those defaults at `yard_baseline`.
7. Add a small catalog-role table in the authoritative Shipyard documentation
   distinguishing standards from quality fixtures.
8. Regenerate the affected compiled ships with `shipsim-yard compile` — never by
   hand — and include them in this commit. `check-all` must be green at the end
   of the phase.

### Tests

Run read-only validation/cost commands for all 11 designs. Add unit assertions
that inspect compiled `ShipDef` values in memory:

- standards have exactly the required weapon counts and mount identities;
- baseline/Compact/Potent/Precise each have exactly one beam-kind weapon;
- the quality fixtures differ only in the expected SKU-derived quality fields;
- `yard_baseline != yard_destroyer` mechanically;
- all expected costs match the locked table above.

**Re-verify the two existing cost gates in `src/simulation/fleet.rs`.** They read
*compiled* ships and are the only automated cost assertions in the repository, so
the capital change moves one of them:

| Test | Effect of 3454 → 3470 |
|---|---|
| `yard_destroyer_cost_near_100` | unaffected (98, gate `95..=105`) |
| `capital_cost_is_a_large_yard_multiple` | ratio 35.24 → **35.41**, gate `30.0..=40.0` — still passes |

Neither needs editing, but both must be run and seen green; a surprise here means
the cost projection changed in a way this plan did not intend.

### Exit criteria

| Check | Pass condition |
|---|---|
| Design validity | 11/11 `validate` successfully |
| Standard battery audit | 7/7 exact counts and mounts |
| Capital | exactly 4 beams, 2 torpedoes, 2 plasma; cost 3470 |
| Quality control | baseline has one ordinary beam and cost 86 |
| Role separation | baseline and destroyer differ by exactly the destroyer's torpedo |
| Generated consistency | `check-all` 11/11 green; no generated ship hand-edited |
| Fleet cost gates | Both `fleet.rs` cost tests pass with the new capital cost |

### Commit

`fix(catalog): restore quality control and complete standard batteries`

Source designs, their regenerated compiled ships, and role tests together.

---

## Phase 3 — Separate weapon-quality and standard-catalog suites

### Goal

Ensure every active suite has one claim, valid fleet arithmetic, and a cost
gate that runs before simulation.

### Work

#### 3A. Weapon-quality suite

Replace the mixed-purpose `design_matched.toml` with
`weapon_quality_matched.toml` containing only the rows below.

*Disposition of the old file:* `design_matched.toml` has exactly **one**
reference outside this plan — `docs/TODO.md:60`, which names it as the standard
catalog suite. Delete the file and fix that line in Phase 4; there is no test or
rubric to migrate. Do not leave it in place as a superseded copy, or the
duplicate will be re-run by someone reading the old TODO.

| Engagement | Fleet A | Cost A | Fleet B | Cost B | Delta |
|---|---|---:|---|---:|---:|
| Baseline mirror | 8× baseline | 688 | 8× baseline | 688 | 0 |
| Compact vs baseline | 7× compact | 672 | 8× baseline | 688 | 16 |
| Baseline vs Compact | reverse | 688 | reverse | 672 | 16 |
| Potent vs baseline | 7× potent | 672 | 8× baseline | 688 | 16 |
| Baseline vs Potent | reverse | 688 | reverse | 672 | 16 |
| Precise vs baseline | 12× precise | 1104 | 13× baseline | 1118 | 14 |
| Baseline vs Precise | reverse | 1118 | reverse | 1104 | 14 |

Remove the global `budget = 800`; retain `cost_tolerance = 16`. Different
quality pairings may use different absolute budgets, but each named engagement
must be equal-cost within tolerance.

This is legal and does exactly what is wanted: `SuiteSpec.budget` is
`Option<u32>` with `#[serde(default)]`, and `validate_engagement_costs` checks
each side against the budget **only when one is present**, while the
player-vs-opponent parity check always runs. Omitting `budget` therefore enforces
"the two fleets cost the same" without also demanding they hit a fixed total —
which is the correct semantics for a controlled A/B comparison.

Note the tolerance comparison is `delta > tolerance`, so the 16-point rows above
pass exactly at the boundary. That is intentional but has zero slack: any future
cost change to `yard_baseline`, `yard_compact`, or `yard_potent` breaks these
rows immediately. That is the desired failure mode — it is what caught the
original defect — but it means these counts must be recomputed, not nudged,
whenever a fixture's cost moves.

#### 3B. Standard-catalog suite

Create a separate active suite for standard classes. It contains:

- mirrors for all seven standards, forward and reversed policy assignments;
- adjacent-size equal-budget engagements using deterministic integer fleet
  counts selected by a checked helper;
- a swarm/capital equal-cost probe only if map and order caps support the exact
  fleet size. With the locked costs, 47× swarm = 3478 versus one capital = 3470
  (delta 8). If this fleet is too large for useful simulation, label a smaller
  engagement as fixed-count rather than equal-cost.

Do not place swarm/capital in the weapon-quality suite.

#### 3C. Structural suite tests

Add tests that parse every active suite and assert:

- all referenced classes load;
- all counts are nonzero;
- equal-cost suites have `skip_cost_validation = false` or omit it;
- every engagement passes cost validation before a match starts;
- reverse pairs exist where required;
- seed lists contain no held-out seed before the evidence phases;
- suite names/comments do not claim equal cost when validation is skipped.

### Exit criteria

| Check | Pass condition |
|---|---|
| Quality suite load | All seven engagements pass cost validation |
| Controlled comparison | Every ship in each quality pair differs only by beam SKU |
| Standard suite load | All standard engagements pass declared cost semantics |
| Suite test coverage | Every active suite is parsed and composition-validated by `cargo test` |
| No silent bypass | Zero active equal-cost suites use `skip_cost_validation = true` |
| Smoke execution | Both new suites complete seeds 1–5 with zero rejected orders |

### Commit

`test(sim): separate quality controls from standard catalog evidence`

---

## Phase 4 — Retire stale claims and repair authoritative documentation

### Goal

Stop old suite names, comments, rubrics, and status documents from presenting
pre-battery evidence as current.

### Work

1. Audit all active suites referencing `yard_swarm`, `yard_destroyer`,
   `yard_baseline`, or `yard_capital`.
2. For each old ABC, ladder, titan, velocity, and long-turn suite:
   - migrate it to current costs/batteries and a current claim; or
   - move it under an explicitly historical location and remove it from active
     command tables/rubric gates.
3. Preserve historical reports, but stamp them with catalog commit/fingerprint
   and “not current evidence.”
4. Rewrite `docs/TODO.md`:
   - M1–M9 are shipped implementation, not open work;
   - remaining work is compiler drift protection, active-suite repair,
     evidence/tuning/evaluation/sign-off, and any deliberately deferred SKU
     cleanup such as `torpedo_potent`;
   - remove contradictory candidate sequences from the active section or move
     them to history.
5. Update `docs/plans/catalog-combat-squads.md` header with an execution-status
   note. Do not leave “Not coding yet” as the current status.
6. Update `docs/SHIPYARD.md` with:
   - `[systems.*]` catalog and `[[systems]]` design examples;
   - duplicate/incompatible system validation rules;
   - `validate`, `cost`, `compile`, `check`, and `check-all` semantics;
   - standard-versus-quality-fixture roles.
7. Update `docs/SIZE-VARIANTS.md` to cost 3470 for the capital.
8. Update `docs/BALANCE.md`, `docs/BALANCE-COST.md`,
   `docs/BALANCE-PROTOCOL.md`, and `docs/SIMULATION.md` so only active suites
   and current costs are named.
9. Remove stale statements that describe the one retired titan class as two
   different variants under the same `yard_capital` ID.

### Documentation checks

Use repository searches as executable gates:

```text
rg for “Not coding yet” in the active plan
rg for M1–M9 features under TODO “Still open”
rg for old capital costs 3370/3454 where current catalog is intended
rg for equal-budget comments adjacent to skip_cost_validation=true
rg for duplicated yard_capital bullets that formerly described light/heavy variants
```

Each result must be either zero or located in a clearly marked historical
document.

### Exit criteria

| Check | Pass condition |
|---|---|
| TODO truthfulness | No shipped feature listed as future implementation |
| Plan status | Executed phases and remaining evidence phases clearly marked |
| Shipyard surface | Weapons and all four systems documented |
| Cost consistency | All authoritative docs report capital 3470 |
| Suite command table | Only active, runnable suites shown |
| Historical claims | Retained only with explicit stale provenance |
| Search gates | No unexplained stale status/cost/equal-budget matches |

### Commit

`docs: align catalog status and balance claims with current rules`

---

## Phase 5 — Lock client defaults and restore static-quality gates

### Goal

Cover the touched Love authoring behavior and make the repository's declared
static checks executable before expensive simulation.

### Work

1. Extend Love Shipyard tests to assert the exact default weapon array:
   - weapon 1 = ordinary beam, forward;
   - weapon 2 = torpedo, forward;
   - TOML serialization and parse round-trip preserve both in order.
2. Run Love tests in LuaJIT/Love's environment, not stock Lua without `ffi`.
   Document the exact command/dependency.
3. Ensure Rust and Love new-design defaults match the playable standard role,
   not `yard_baseline`.
4. Resolve clippy blockers separately from gameplay changes:
   - fix mechanical `needless_borrow` and `collapsible_if` findings;
   - rename used `_material` binding;
   - remove or route through dead `new_with_options`;
   - replace growing combat/allocation argument lists with typed parameter
     structs where that preserves API clarity, with parity tests before/after.
5. Do not alter combat formulas while restructuring signatures. Snapshot and
   seeded combat tests must remain byte-/result-identical.

### Exit criteria

| Check | Pass condition |
|---|---|
| Love default | Exact two-weapon default and round trip asserted |
| Love execution | Full Love test script passes under documented runtime |
| Client parity | Rust/Love default TOML projects to same `ShipDef` |
| Clippy | `cargo clippy --all-targets -- -D warnings` exits 0 |
| Behavior preservation | Existing seeded combat, snapshot, and protocol tests unchanged |

### Commit

Use two commits:

1. `test(love): lock standard shipyard defaults`
2. `refactor(core): restore clippy-clean combat interfaces`

---

## Phase 6 — Define and run the fast evidence gate (seeds 1–32)

### Goal

Establish current-rule evidence and reject obviously broken catalog/suite
combinations without spending pooled or held-out seeds.

### Work

1. Freeze numeric gates before running the suite:

| Signal | Starting gate |
|---|---|
| Same-class mirror player win rate | 35–65% |
| Equal-budget adjacent sizes | Neither side >75% |
| Largest vs spread Swarm | Neither side >80% |
| In-progress/stalemate | <15% |
| Size-7 zero-translation among scheduled paths | <40% |
| Rejected/illegal orders | 0 |

2. Run each standard mirror, adjacent-size engagement, and budget-policy pair
   forward and reversed on seeds 1–32.
3. Add a deterministic mechanism fixture: size-7 graviton versus one declared
   12-fighter squad. It is a correctness gate, not a win-rate claim.
4. Capture engine commit, dirty status, rules fingerprint, catalog digest,
   suite digest, and per-engagement raw outcomes in the report.
5. Inspect representative traces for wins, losses, and in-progress outcomes.
6. If a gate fails, classify it as correctness, policy, map/termination,
   composition, or balance before changing anything.

### Exit criteria

| Check | Pass condition |
|---|---|
| Fast matrix | Seeds 1–32, forward/reverse, complete |
| Blocking rubric | All predeclared gates pass |
| Mechanism fixture | All 12 fighters receive the expected graviton effect |
| Provenance | Commit, fingerprints, and catalog/suite digests recorded |
| Trace audit | At least one win/loss/in-progress trace inspected where available |
| Held-out discipline | No seed ≥264 used |

### Commit

`test(balance): establish current catalog fast evidence`

Only suite/rubric/report metadata is committed. Raw large reports may remain in
the documented report location if repository policy excludes them.

---

## Phase 7 — Pooled tuning gate (seeds 1–191)

### Goal

Produce one coherent sign-off candidate without fitting to evaluation or
sign-off pools.

### Work

1. Follow `docs/BALANCE-PROTOCOL.md` one lever per cycle.
2. For every cycle, state a falsifiable mechanism hypothesis and rejection
   condition before editing.
3. Re-run safety, quality-control, standard-catalog fast suites, and trace
   inspection after each lever.
4. Run seeds 1–191 only after the fast gate passes.
5. Preserve the symmetric mirrors and quality controls unchanged during tuning.
6. Record `REJECT`, `KEEP FOR NEXT CYCLE`, or `SIGN-OFF CANDIDATE` for each
   cycle.
7. Freeze the exact candidate commit once all pooled gates pass.

### Exit criteria

| Check | Pass condition |
|---|---|
| Pooled matrix | Seeds 1–191 complete, forward/reverse |
| Rubrics | All blocking gates pass with reported boundary distance |
| Controls | Mirrors remain 35–65%; quality suite remains structurally valid |
| Safety | Zero rejected orders and no correctness/instrumentation blocker |
| Candidate freeze | Exact commit and rules/catalog/suite fingerprints recorded |
| Held-out discipline | No seed 264–391 inspected |

### Commit

One commit per accepted semantic lever, followed by:

`docs(balance): record pooled catalog candidate`

---

## Phase 8 — Evaluation (seeds 328–391)

### Goal

Test the frozen pooled candidate out of sample without tuning on the result.

### Work

1. Verify candidate commit and all fingerprints match the Phase 7 freeze.
2. Run the identical matrix and gates on seeds 328–391 once.
3. Record pass/fail and raw outcomes.
4. If evaluation fails, return to Phase 7 using seeds 1–191 only. Mark the
   evaluation range burned for that candidate; do not adjust gates from the
   observed result.
5. Do not run sign-off seeds in this phase.

### Exit criteria

| Check | Pass condition |
|---|---|
| Frozen identity | Commit/fingerprints exactly match Phase 7 candidate |
| Eval run | Seeds 328–391 run once |
| Gates | Same predeclared gates pass without edits |
| Ledger | Range, timestamp, candidate, and verdict recorded |
| Sign-off pool | Seeds 264–327 remain unused |

### Commit

`docs(balance): record catalog evaluation verdict`

---

## Phase 9 — Sign-off (seeds 264–327)

### Goal

Certify only the named catalog, quality, squad, and budget-policy claims for the
unchanged candidate.

### Work

1. Confirm the Phase 8 candidate is unchanged and seeds 264–327 remain virgin.
2. Run the identical matrix once on seeds 264–327.
3. Record raw and adjudicated outcomes, confidence intervals, boundary
   distances, catalog/rules/suite fingerprints, and report digests.
4. If any blocking gate fails, mark the range burned, allocate a new tracked
   sign-off range, and return to pooled tuning. Do not tune and continue calling
   264–327 held out.
5. Update `docs/BALANCE.md` with narrowly scoped claims only; do not generalize
   to all maps, policies, mixed fleets, or human play.

### Exit criteria

| Check | Pass condition |
|---|---|
| Sign-off identity | Candidate exactly matches evaluated candidate |
| Sign-off run | Seeds 264–327 run once |
| Gates | All named blocking gates pass |
| Ledger | Virgin-range consumption recorded permanently |
| Claims | `BALANCE.md` names fleet, policy, map, seeds, and limitations |
| Final verification | Full tests, clippy, yard check-all, and active-suite validation pass |

### Commit

`docs(balance): certify named standard catalog claims`

---

## Required full verification commands

Run at the end of Phases 1–5 and before each evidence tier (`check-all` exists
from the end of Phase 1):

```bash
git diff --check
cargo test --all
cargo clippy --all-targets -- -D warnings
(cd frontend/repl && python3 -m unittest discover -s tests)
cargo test --manifest-path frontend/tui/Cargo.toml
# Run with the repository's documented LuaJIT/Love test command:
luajit frontend/love/tests/run_all.lua
cargo run --release --bin shipsim-yard -- check-all
cargo run --release --bin shipsim-sim -- \
  --suite simulation/suites/smoke.toml \
  --output tmp/simulation/reports/catalog-remediation-smoke.json
```

If the Love test command needs a repository-relative working directory, encode
that exact invocation in `frontend/love/README.md` and use it here.

## Commit sequence

1. `feat(shipyard): add non-writing generated catalog checks`
2. `fix(catalog): restore quality control and complete standard batteries`
3. `test(sim): separate quality controls from standard catalog evidence`
4. `docs: align catalog status and balance claims with current rules`
5. `test(love): lock standard shipyard defaults`
6. `refactor(core): restore clippy-clean combat interfaces`
7. Evidence/tuning commits from Phases 6–9, only as their gates are reached.

Never mix suite arithmetic, balance levers, and held-out evidence in one commit.

**Generated output is the one deliberate exception.** A regenerated
`data/ships/*.toml` ships *with* the source design that caused it (commit 2), so
no commit ever contains a design and its stale compiled output. Separating them
was the earlier draft's `chore(catalog): regenerate …` step; it has been removed
because it guaranteed one commit where `check-all` fails by construction.

## Final definition of done

- All 11 yard designs validate and all 11 generated ships pass non-writing
  byte-drift checks.
- Seven standards meet the locked batteries; capital is 4/2/2 at cost 3470.
- Baseline/Compact/Potent/Precise remain one-weapon controlled fixtures.
- No active equal-cost suite bypasses cost validation.
- Weapon-quality and standard-catalog suites both load and run independently.
- Active documentation reflects current implementation and current costs.
- Rust, REPL, TUI, Love, clippy, yard drift, and smoke gates are green.
- Old reports are explicitly historical.
- Fast and pooled evidence pass before evaluation; evaluation passes before
  sign-off; virgin ranges are consumed and recorded exactly once.
- Balance claims are limited to the named matrices and include full provenance.

## Non-goals

- Reintroducing light/line/heavy stock catalogs.
- Smoothing costs without evidence of a failed named claim.
- Expanding Love into a rules authority.
- Adding new combat systems or changing the v4 turn loop.
- Loosening cost tolerances to make invalid equal-cost claims pass.
- Using evaluation/sign-off seeds as tuning data.
- Claiming universal balance from the named simulation matrices.
