# Gameplay Simulation (sim play)

`shipsim-sim` is **sim play**: seeded policy matchups through the same
`GameState` and `apply_order` path used by tests, the CLI harness, and frontends
— but **in-process**, without NDJSON or a UI. Use it for hundreds–thousands of
matches, balance, and rubrics.

Play-type overview (UI / API / sim): `docs/AGENT-PLAY.md`. This file is the
sim-play reference.

`shipsim-sim` exists to make gameplay claims measurable and reproducible.
Policy boundary decision: ADR-0026.

For balance work, read [`BALANCE.md`](BALANCE.md) for current claim status and
[`BALANCE-PROTOCOL.md`](BALANCE-PROTOCOL.md) before choosing seeds or changing a
lever. In particular, do not use a sign-off suite for tuning.

## Quick start

```bash
cargo run --release --bin shipsim-sim -- \
  --suite simulation/suites/smoke.toml \
  --output tmp/simulation/reports/smoke.json
```

`--output` is optional; without it the full JSON report is printed to stdout.

Exit codes:

- `0`: simulation completed and every rubric passed;
- `1`: configuration, loading, policy, or runtime failure;
- `2`: simulation completed but at least one rubric failed.

The report contains aggregate metrics plus each match's final snapshot and complete order trace. Each match also records `rules_fingerprint`, identifying the exact semantic rules data used. Generated reports belong under ignored `tmp/simulation/`.

## Components

- `src/simulation/runner.rs`: match/suite orchestration and authoritative legal-order generation.
- `src/simulation/policy.rs`: policy boundary.
- `src/simulation/policies/`: deterministic baseline policy implementations.
- `src/simulation/trace.rs`: reproducible per-order decision record.
- `src/simulation/metrics.rs`: match and aggregate measurements.
- `src/simulation/rubric.rs`: structured threshold evaluation.
- `simulation/suites/`: tracked scenario, seed, policy, and rubric matrices.
- `simulation/rubrics/`: tracked pass/fail expectations.

## Baseline policies

The built-in policy names are `random`, `greedy`, `aggressive`, `defensive`, and `mobility` (the alias `mobility_first` is also accepted). They are deliberately simple experimental controls, not claims of strong play.

Every policy receives a read-only snapshot, the acting ship, and orders already
validated against a cloned production `GameState`. Policies emit protocol-v4
orders only: `allocate`, one complete `commit_path`, and one complete
`commit_volley` per ship per turn. A rejected policy order fails the simulation;
the rejected trace event and `rejected_orders` metric are retained by the match
runner.

The five baseline identities are stable: `random` selects deterministically from
legal alternatives using its seeded stream; `greedy` closes and fires; `aggressive`
prioritizes closing and weapon opportunity; `defensive` favors defensive geometry;
and `mobility`/`mobility_first` prioritizes path length toward the target. None of
these policies mutates `GameState` directly.

## Suite format

```toml
name = "simulation smoke"
scenario = "scenarios/simulation_duel.toml"
seeds = [1, 2, 3, 4]
max_turns = 50
max_orders = 20000
rubrics = ["simulation/rubrics/safety.toml"]

[[matchups]]
player = "aggressive"
opponent = "mobility"
```

### Cost-matched fleet engagements

For equal-construction-budget fleet tests, omit a fixed `scenario` and declare
`[[engagements]]` instead. The runner builds in-memory scenarios (annihilation
terminal: all enemy ships destroyed) from ship class TOMLs and validates costs.

```toml
name = "cost-matched size fleets"
seeds = [1, 2, 3, 4]
max_turns = 80
budget = 800
cost_tolerance = 60
rubrics = ["simulation/rubrics/safety.toml"]

[map]
width = 36
height = 28

[[matchups]]
player = "greedy"
opponent = "greedy"

[[engagements]]
name = "swarm_vs_titan"
player = [{ class = "destroyer_line", count = 8 }]
opponent = [{ class = "titan_line", count = 1 }]
```

Ship `cost` comes from `data/ships/{class}.toml` (see `docs/SIZE-VARIANTS.md`).
Both fleets must be within `cost_tolerance` of each other and of `budget` when set
(unless `skip_cost_validation = true`).

### Power sweeps

Vary design `power` on one class without editing ship TOMLs. Placement overrides
also support `structure` and `max_shield_per_facing` on fleet lines / scenarios.

```toml
skip_cost_validation = true

[[power_sweeps]]
name = "titan_vs_swarm"
class = "titan_line"
side = "opponent"
values = [60, 90, 120, 150, 180]
player = [{ class = "destroyer_line", count = 8 }]
opponent = [{ class = "titan_line", count = 1 }]
```

Generated engagements are named `{name}_p{value}`. Method notes:
`docs/SIZE-VARIANTS.md` (balance method). Example suite:
`simulation/suites/titan_power_sweep.toml`.

```bash
cargo run --release --bin shipsim-sim -- \
  --suite simulation/suites/cost_matched.toml \
  --output tmp/simulation/reports/cost_matched.json
```

Run suites from the repository root. The CLI also resolves paths relative to the suite file when a root-relative path does not exist.

## Test tiers

Fast deterministic gates run under normal Rust tests:

```bash
cargo test --test simulation_invariants
cargo test --test simulation_determinism
cargo test --test simulation_rubrics
```

The smoke suite is a blocking safety gate. The larger balance suite may intentionally be red while it exposes dominant strategies or scenario asymmetry:

```bash
cargo run --release --bin shipsim-sim -- \
  --suite simulation/suites/balance.toml \
  --output tmp/simulation/reports/balance.json
```

## Current limitations

- The core has controller labels but no explicit teams. `simulation_duel.toml`
  therefore uses one `player` ship and one externally controlled `scripted`
  opponent so `Won`/`Lost` remain meaningful.
- Policies operate one ship at a time and share no fleet memory.
- Path metrics are sampled after path-stage resolution from `path_results`:
  `path_orders`, `path_cost_total` / `path_cost_distribution`, `path_translated_steps`
  (actual travel after fallback, not submitted length), `blocked_paths`,
  `scheduled_paths`, and `zero_translation_observations`.
- Volley metrics: `volley_orders`, `volley_shots`, `volley_size_distribution`,
  plus hit/miss/damage from `combat_log` (retained across auto turn advance
  until the next volley).
- Hull-efficiency metrics track allocation resources using **charge increases
  only** (not requested totals). `power_spent_total` must not exceed
  `power_available_total`. The broad-resource counter records nonzero engine,
  weapon-increase, and shield buckets as a breadth signal, not effectiveness.
- Reports retain full traces and can become large.
- The canonical rules data is loaded with each scenario; clients and policies do
  not load it or decide legality. Compare `rules_fingerprint` before comparing
  balance reports from different runs. Protocol-v3 balance reports are not
  comparable to v4 (see `tmp/V4-BASELINE-NOTE.md`).
- Statistical confidence intervals, paired mirrored scenarios, parameter sweeps,
  and visual screenshot evaluation are next-layer capabilities.

## Interpretation

A green rubric means its explicit numeric bounds held for the configured scenario, policies, and seeds. Advisory red results are recorded without failing the CLI exit status; blocking red results fail it. It does not prove that the game is fun. Red results are design evidence to investigate, not automatically software defects.

Balance evidence also requires provenance beyond the rubric result. Record the
engine revision, dirty ship/catalog changes, suite and seed range, and
`rules_fingerprint`. The fingerprint covers configured rules data, not Rust
formula changes or ship TOML, so it is not sufficient by itself.
