# Balance status and design guide

This is the entry point for shipsim balance work. It describes what the
current evidence supports, what remains provisional, and where the underlying
rules and measurements live.

For the tuning procedure, seed discipline, and acceptance gates, read
[`BALANCE-PROTOCOL.md`](BALANCE-PROTOCOL.md). For construction-cost theory, read
[`BALANCE-COST.md`](BALANCE-COST.md).

## Current status

Status as of **2026-07-18**: **pooled tuning candidate; not certified**.

The checked-in catalog passes the reusable A/B/C pooled tuning suite on seeds
1-191. The replacement sign-off seeds 264-327 have intentionally not been run.
Passing reusable seeds is evidence that a candidate is ready for review; it is
not out-of-sample certification.

Current catalog-specific levers include:

- range-aware target-size accuracy ceiling: floor 15, maximum 19;
- fire control only against exact size-2 targets;
- `titan_light`: accuracy bonus 12, five SSD boxes per weapon;
- `titan_heavy`: accuracy bonus 10, three SSD boxes per weapon.

Authoritative values are in [`data/rules/default.toml`](../data/rules/default.toml)
and [`data/ships/`](../data/ships/). The prose above is a readable status
summary, not a second source of truth.

### Current reusable-seed evidence

Run on 2026-07-18 from the checked-in `+12/+10` catalog:

- engine revision: `4634cc0`;
- rules fingerprint: `fnv1a-3fb61002d562620d`;
- suite: `simulation/suites/abc_claims_pooled.toml`, seeds 1-191.

| Engagement | n | Player win | Opponent win | In progress | Gate |
|---|---:|---:|---:|---:|---|
| A: 7 destroyers vs titan light | 764 | 74.9% | 17.5% | 7.6% | 60-80% player: pass |
| B: 10 destroyers vs titan heavy | 764 | 59.4% | 40.6% | 0.0% | 40-60% player: pass |
| C: 8 destroyers vs titan heavy | 764 | 5.2% | 94.2% | 0.5% | player <=10%: pass |
| Destroyer mirror control | 764 | 40.1% | 49.5% | 10.5% | 35-65% player: pass |

Aggregate termination was 95.4%; both `fleet_safety` and `abc_claims` passed.
These are raw engine statuses from `abc_claims_pooled.toml`, not a sign-off
result. The complete generated report is intentionally local and ignored.

## What the claims mean

The claims are fleet-construction experiments, not a promise that every hull is
fair in every scenario.

| Claim | Fleet definition | Intended result |
|---|---|---|
| A | 7 `destroyer_line` vs 1 `titan_light` | Minimum-fill capital is inefficient; swarm wins 60-80% |
| B | 10 `destroyer_line` vs 1 `titan_heavy` | Maximum fill matters; either side can win, player 40-60% |
| C | Fixed 8 `destroyer_line` vs 1 `titan_heavy` | Scenario track; heavy titan wins at least 90% |
| Control | 4 `destroyer_line` vs 4 `destroyer_line` | Detect side/policy drift; player 35-65% |

Claims A and B use deliberately chosen fleet counts around current construction
costs. C is a fixed-count scenario claim, not equal-budget evidence.

## What players should infer

- Ship cost is a fleet-construction design value, not an Elo rating.
- A larger target is easier to hit, but the configured ceiling prevents an
  automatic hit.
- The titan variants have explicit anti-destroyer fire control. It does not
  apply to fighters or size-3-and-larger targets.
- Concentrating value in one hull changes focus-fire, shield, arc, and SSD
  behavior. Equal point totals do not guarantee a 50/50 fight.

The game currently has no general fleet-construction screen, so these costs are
primarily scenario and simulation inputs.

## Scope and known gaps

- The 21-hull size ladder is not certified. `ladder_claims.toml` exists to make
  this gap visible; do not generalize A/B/C to cruisers, battleships, or mixed
  fleets.
- Results depend on the simple baseline policies. They are experimental
  controls, not optimal play.
- Hard-map forced engagement and damage-difference adjudication are experiment
  choices. Open missions, objectives, and human play can produce different
  outcomes.
- Shield values near exact damage breakpoints can change the number of hits to
  penetrate. A one-point edit may be a phase change, not a small adjustment.
- Weapon arcs and localized SSD damage can create disarm or geometry-driven
  stalls. Termination and per-match traces must be checked alongside win rate.

## Sources of truth

| Subject | Authoritative source |
|---|---|
| Combat data | [`data/rules/default.toml`](../data/rules/default.toml) |
| Combat formulas | [`src/combat_tables.rs`](../src/combat_tables.rs) |
| Readable combat tables | [`combat-v2-tables.md`](combat-v2-tables.md) |
| Hull data and costs | [`data/ships/`](../data/ships/), [`data/ship_costs.toml`](../data/ship_costs.toml) |
| Claims and seed sets | [`simulation/suites/`](../simulation/suites/) |
| Numeric acceptance bands | [`simulation/rubrics/abc-claims.toml`](../simulation/rubrics/abc-claims.toml) |
| Simulator operation | [`SIMULATION.md`](SIMULATION.md) |
| Tuning protocol | [`BALANCE-PROTOCOL.md`](BALANCE-PROTOCOL.md) |
| Historical campaign | [`history/BALANCE-CAMPAIGN-2026-07.md`](history/BALANCE-CAMPAIGN-2026-07.md) |

When comparing reports, require both the same engine revision and the same
`rules_fingerprint`. The fingerprint identifies configured rules data; it does
not identify Rust formula changes or ship TOML changes.
