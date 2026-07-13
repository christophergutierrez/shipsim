# Gameplay Simulation (sim play)

`shipsim-sim` is **sim play**: seeded policy matchups through the same
`GameState` and `apply_order` path used by tests, the CLI harness, and frontends
— but **in-process**, without NDJSON or a UI. Use it for hundreds–thousands of
matches, balance, and rubrics.

Play-type overview (UI / API / sim): `docs/AGENT-PLAY.md`. This file is the
sim-play reference.

`shipsim-sim` exists to make gameplay claims measurable and reproducible.

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

The report contains aggregate metrics plus each match's final snapshot and complete order trace. Generated reports belong under ignored `tmp/simulation/`.

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

Every policy receives a read-only snapshot, the acting ship, and orders already validated against a cloned production `GameState`. Allocation and inertial maneuver orders are applied through the normal validator. A rejected policy order fails the simulation; the rejected trace event and `rejected_orders` metric are retained by the match runner rather than silently replacing it with Coast.

The five baseline identities are stable: `random` selects deterministically from
legal alternatives using its seeded stream; `greedy` closes and fires; `aggressive`
prioritizes closing and weapon opportunity; `defensive` favors defensive geometry;
and `mobility`/`mobility_first` prioritizes velocity and course control. None of
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

- The core has controller labels but no explicit teams. `simulation_duel.toml` therefore uses one `player` ship and one externally controlled `scripted` opponent so `Won`/`Lost` remain meaningful.
- Policies operate one ship at a time and share no fleet memory.
- Motion metrics are sampled after movement-phase resolution. `velocity_distribution`
  counts living-ship velocity observations at that boundary; `thrust_spent` is the
  sum of accepted maneuver costs; `coasting_distance` counts translated hexes for
  Coast commitments only; `course_changes` and `facing_rotations` count their
  distinct maneuver variants; `scheduled_translations` counts eligible schedule
  entries; `blocked_translations` consumes the core's authoritative eligible
  translation outcome, so floating-map recentering cannot create a false block;
  and `reversals` counts a legal decelerate-to-zero then opposite-course
  accelerate transition. Raw numerators and denominators are retained.
- Hull-efficiency metrics separately track allocation resources and zero-velocity /
  zero-translation observations by serialized hull class. The broad-resource
  counter records nonzero engine, weapon, and shield buckets; it is explicitly a
  breadth signal rather than a normalized effectiveness claim. Missing configured
  hull classes fail their checks rather than becoming zero-valued passes. Advisory rubric failures
  remain visible in reports and do not become green silently.
- Movement power is reported only as engine allocation. There is no initiative
  metric or initiative ordering in M7; simultaneous commitment removes that
  obsolete correlation.
- Reports retain full traces and can become large.
- Statistical confidence intervals, paired mirrored scenarios, parameter sweeps, and visual screenshot evaluation are next-layer capabilities.

## Interpretation

A green rubric means its explicit numeric bounds held for the configured scenario, policies, and seeds. Advisory red results are recorded without failing the CLI exit status; blocking red results fail it. It does not prove that the game is fun. Red results are design evidence to investigate, not automatically software defects.
