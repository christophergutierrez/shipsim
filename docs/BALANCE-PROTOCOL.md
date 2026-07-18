# Balance tuning protocol

This is the agent-facing procedure for changing combat, hull, cost, policy, or
balance-suite data. It prevents mechanism bugs, seed overfitting, and report
provenance mistakes from being mistaken for balance.

Current balance status and claim definitions: [`BALANCE.md`](BALANCE.md).
Simulator reference: [`SIMULATION.md`](SIMULATION.md).

## Non-negotiable rules

1. Diagnose from traces and metrics before choosing a lever. Win rate alone is
   not a mechanism.
2. Fix correctness, legality, termination, and instrumentation bugs before
   tuning. A bug fix invalidates measurements that depended on the old behavior.
3. Change one balance lever per cycle. If fields must move together as one
   semantic lever, name the bundle and justify why it cannot be separated.
4. Keep a symmetric control. If the control moves, suspect policy, side,
   scenario, or simulator bias before hull stats.
5. Use reusable seeds for tuning and a virgin range once for sign-off. Never
   tune after seeing sign-off outcomes and still call those seeds held out.
6. Compare only reports with known engine revision, ship catalog, suite, and
   `rules_fingerprint`.
7. Record raw `Won`/`Lost`/`InProgress` separately from any adjudicated status.
8. Treat rubrics as explicit contracts. Do not silently make a blocking rubric
   advisory merely to get a green process.
9. Do not infer UI playability or human strategy from simulation policies.
10. Never claim that equal cost implies universal fairness. Certify named fleet,
    policy, map, and seed matrices only.

## Seed tiers

| Tier | Suite | Seeds | Purpose | Reusable? |
|---|---|---|---|---|
| Fast | `abc_claims_fast.toml` | 1-32 | Reject weak ideas quickly | Yes |
| Pooled | `abc_claims_pooled.toml` | 1-191 | Accept/reject a tuning candidate | Yes |
| Sign-off | `abc_claims_signoff.toml` | 264-327 | One out-of-sample acceptance run | No |

Seeds 1-191 are known tuning data. Seeds 200-263 were used repeatedly in the
2026-07 campaign and are burned. As of 2026-07-18, seeds 264-327 remain virgin.
If anyone runs them, update the suite comments and this document immediately.
If the candidate then changes, allocate a new tracked sign-off range.

The older `abc_claims.toml` and `abc_claims_holdout.toml` suites are subsets of
the pooled tuning range and no longer provide independent holdout evidence.

## Baseline capture

Before an experimental edit:

```bash
git status --short
git rev-parse HEAD
cargo test
cargo run --release --bin shipsim-sim -- \
  --suite simulation/suites/abc_claims_fast.toml \
  --output tmp/simulation/reports/abc-baseline-fast.json
python3 tools/claims_report.py \
  tmp/simulation/reports/abc-baseline-fast.json
```

Record:

- commit and dirty files;
- suite path and seed tier;
- engine/rules provenance;
- per-engagement raw status counts and rates;
- termination, capped matches, average turns, and rejected orders;
- control result;
- mechanism metrics relevant to the proposed lever.

Do not overwrite the baseline report during the cycle.

## Cycle procedure

### 1. State a falsifiable hypothesis

Bad: "buff the titan."

Good: "The titan loses B because rear-arc effective shots disappear after the
pass; increasing weapon survival should raise useful late-turn fires without
changing the destroyer mirror."

Name the expected metric movement and a rejection condition before editing.

### 2. Make one lever change

Typical levers include one of:

- configured accuracy rule;
- per-hull fire control;
- SSD subsystem depth;
- weapon charge, count, range, or arc;
- shield, structure, power, or thrust;
- construction cost or engagement count;
- map, objective, policy, or adjudication rule.

Changing a simulator mechanism is not a balance lever. Re-establish a new
baseline after such a fix.

### 3. Run deterministic safety checks

```bash
cargo test
cargo run --release --bin shipsim-sim -- \
  --suite simulation/suites/smoke.toml \
  --output tmp/simulation/reports/smoke.json
```

Catalog generator edits must regenerate byte-identically when no intentional
catalog change is requested:

```bash
python3 tools/generate_size_variants.py
git diff -- data/ships data/ship_costs.toml
```

### 4. Run the fast gate

```bash
cargo run --release --bin shipsim-sim -- \
  --suite simulation/suites/abc_claims_fast.toml \
  --output tmp/simulation/reports/abc-candidate-fast.json
python3 tools/claims_report.py \
  tmp/simulation/reports/abc-candidate-fast.json
```

Reject a candidate that moves the named mechanism in the wrong direction,
breaks safety, creates a control regression, or clearly misses a claim band.
Do not spend pooled compute to rescue a mechanically weak candidate.

### 5. Inspect traces, not just aggregates

At minimum, inspect representative wins, losses, and in-progress matches from
the affected engagement. Determine:

- actual hit rate by range and weapon, not a kills-per-shot proxy;
- turns and shots in legal arcs;
- shield absorption and internal damage;
- weapon, power, and engine subsystem loss;
- closest approach and time in range;
- which side leads any capped or mutually disarmed result;
- whether policies attack enemies rather than allies;
- whether a threshold has saturated at 1 or the die ceiling.

For shield or beam-charge edits, calculate hits-to-penetrate before running.
Exact divisibility is a breakpoint and must be called out in the cycle record.

### 6. Run the pooled gate

Only a fast candidate with a coherent mechanism proceeds:

```bash
cargo run --release --bin shipsim-sim -- \
  --suite simulation/suites/abc_claims_pooled.toml \
  --output tmp/simulation/reports/abc-candidate-pooled.json
python3 tools/claims_report.py \
  tmp/simulation/reports/abc-candidate-pooled.json
```

The configured bands in `simulation/rubrics/abc-claims.toml` are authoritative.
Also report confidence half-widths and distance from each boundary. A result on
an edge is a risk, not extra confidence.

### 7. Decide explicitly

Write one verdict:

- `REJECT`: revert only the experiment and record why;
- `KEEP FOR NEXT CYCLE`: strict improvement, but another named issue remains;
- `SIGN-OFF CANDIDATE`: all pooled gates pass and no unresolved mechanism
  concern invalidates the evidence.

Do not combine the next lever before recording the current verdict.

## Sign-off procedure

Sign-off is a release decision, not another tuning cycle.

Before running it:

1. Freeze the candidate commit or exact dirty diff.
2. Confirm all normal tests and pooled gates pass.
3. Confirm the sign-off range has not appeared in any prior tuning report.
4. Record the engine revision, ship-data diff, rules fingerprint, suite, and
   rubric revision.
5. Run `abc_claims_signoff.toml` once.

If it passes, write a concise tracked result in
`docs/history/` and update [`BALANCE.md`](BALANCE.md) to `certified`, including
scope and date. If it fails, label the candidate rejected, mark the range burned,
choose a new sign-off range, and return to reusable seeds. Do not adjust a value
and rerun the same sign-off range.

## Report lifecycle

Generated JSON reports belong under ignored `tmp/simulation/reports/`. They can
be very large and are not the documentation layer.

For every result cited in checked-in docs, retain a compact tracked summary with:

- status: `candidate`, `accepted`, `rejected`, or `historical`;
- date, commit, dirty diff identity, suite, seeds, and match count;
- engine revision and rules fingerprint;
- per-engagement counts/rates and confidence half-widths;
- raw versus adjudicated status semantics;
- rubric results;
- conclusion and superseding result, if any.

Compress local raw reports when they are still useful. Never call a report
"latest" without stating whether its code and data match the current tree.

## Cycle record template

```markdown
## Cycle N: <lever>

Status: REJECT | KEEP FOR NEXT CYCLE | SIGN-OFF CANDIDATE
Commit/diff: <sha and dirty files>
Rules fingerprint: <value>
Suite/seeds: <path and range>

Hypothesis: <mechanism and expected metric movement>
Change: <one semantic lever>

| Engagement | n | Won | Lost | InProgress | Target |
|---|---:|---:|---:|---:|---|
| ... | ... | ... | ... | ... | ... |

Safety/control: <results>
Trace findings: <mechanism evidence>
Breakpoint/saturation checks: <results>
Verdict: <why this status follows from the evidence>
Next step: <one action>
```
