# ADR-0026: Run gameplay policies through validated production orders

Status: Accepted

> Formerly numbered ADR-0021 (shared number with the ratatui TUI decision).
> Renumbered to ADR-0026 for uniqueness. Ratatui client remains ADR-0021.

## Context

The project needs repeatable gameplay and balance evaluation across policies, scenarios, and seeds. A simulator that mutates `GameState` directly would be faster to write but could produce results that the actual CLI and Love2D game cannot reproduce.

## Decision

Implement simulation as a Rust module and `shipsim-sim` binary. Policies receive read-only snapshots and an authoritative set of legal move/fire orders. All selected orders, including allocation, pass through `movement::apply_order`. Every run records scenario, seed, policy names, order trace, PRNG checkpoints, final snapshot, and aggregate metrics.

Keep tracked suites and rubrics as TOML data. Store generated traces and reports under ignored `tmp/simulation/`.

Use a `player` versus `scripted` scenario convention until the domain model gains explicit teams. Do not use the automatic greedy NPC driver inside policy-versus-policy simulations.

## Consequences

- Simulation behavior remains reproducible through production rules.
- Invalid policy behavior is visible and fails fast.
- Legal-order generation currently clones state and favors correctness over throughput.
- Baseline policies can be replaced or extended without changing the runner.
- Explicit teams, confidence intervals, parameter sweeps, external agents, and visual evaluation remain additive future work.
