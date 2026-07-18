# Size variants (fleet catalog)

Three generic hulls per size tier (`data/sizes.toml`). Initial power and
structure shapes came from local STCS-derived class buckets; current costs use
the frame-plus-modules model in [`BALANCE-COST.md`](BALANCE-COST.md).

**Status:** loadable scenario and simulation catalog. `cost` is a construction
and scenario-design value; gameplay does not spend points during combat. The
destroyer/titan A/B/C candidate passes pooled tuning seeds, but the full size
ladder is not certified. See [`BALANCE.md`](BALANCE.md).

## Files

| Path | Role |
|---|---|
| `data/ships/{size}_{light\|line\|heavy}.toml` | 21 loadable ship classes |
| `data/ship_costs.toml` | Flat cost index (mirrors `cost` on each ship) |
| `data/sizes.toml` | Size id / name / FASA Class map |

Legacy hulls (`escort`, `heavy_cruiser`, `huge`, `starbase`) are unchanged for
existing scenarios.

## How the catalog is generated

1. **Bucket** ships with STCS `class_number` into size 1–7 (I–III … XIX–XX).
2. Take **p25 / median / p75** of `total_power_units` and superstructure for
   light / line / heavy within each bucket.
3. **Scale into shipsim integers** so destroyer *line* matches the old escort
   power budget:
   - `power ≈ round(stcs_power × 14/34)` (destroyer median power 34 → 14)
   - `structure ≈ round(stcs_ss × 8/14)` (destroyer median SS 14 → 8)
4. **Cost:** positive size-dependent frame intercept plus linear power,
   shield-cap, and flat weapon-kind prices, normalized so `destroyer_line=100`.
   Combat-D-scaled total costs were an early draft and are no longer emitted.
5. **Thrust** follows FASA movement-point ratio trend (larger = more power per
   thrust): fighters/destroyers efficient; capitals `power_per_thrust` 2–5.
6. **Weapons** step up by size and variant (light = few mounts; heavy = fuller
   suite). Kinds stay beam / torp / plasma.

**Not used as primary cost:** power×SS product or historical Combat-D total.
Neither expresses the intended frame-plus-fill economics.

## Line costs (budget unit)

| Size | Class | light | line | heavy | ≈ destroyers (line) |
|---:|---|---:|---:|---:|---:|
| 1 | Fighter | 43 | 47 | 73 | 0.47 |
| 2 | Destroyer | 93 | 100 | 126 | 1.00 |
| 3 | Light Cruiser | 162 | 190 | 213 | 1.90 |
| 4 | Heavy Cruiser | 279 | 304 | 327 | 3.04 |
| 5 | Battleship | 413 | 441 | 501 | 4.41 |
| 6 | Dreadnought | 597 | 625 | 682 | 6.25 |
| 7 | Titan | 762 | 822 | 910 | 8.22 |

### Equal-cost sketch fleets (line hulls)

| Budget ~800 | Example |
|---|---|
| Titan | 1× `titan_line` (822) |
| Dreadnought + screen | 1× `dreadnought_line` + 2× `destroyer_line` (825) |
| Battleship wing | 2× `battleship_line` (882) |
| Heavy pack | 3× `heavy_cruiser_line` (912) |
| Destroyer swarm | 8× `destroyer_line` |

## Variant roles

| Variant | Intent |
|---|---|
| **light** | Cheaper, thinner, more thrust-efficient; fewer weapons |
| **line** | Median JSONL stats; reference for cost-vs-cost |
| **heavy** | Costlier, thicker, denser weapons; often worse thrust ratio |

## Balance method

Authoritative economics + claims A/B/C + order of ops:
**[`docs/BALANCE-COST.md`](BALANCE-COST.md)**.

Current evidence and limitations: **[`docs/BALANCE.md`](BALANCE.md)**. Agent
tuning procedure: **[`docs/BALANCE-PROTOCOL.md`](BALANCE-PROTOCOL.md)**.

Local lever sweeps (power, shields, structure) remain useful **after** games
decide and the catalog form is frame/module — see that doc’s order of operations
(engagement instrumentation first; do not retune from Combat-D costs).

```bash
cargo run --release --bin shipsim-sim -- \
  --suite simulation/suites/cost_matched.toml \
  --output tmp/simulation/reports/cost_matched.json
```

- Suite format: `[[engagements]]` + optional `budget` / `cost_tolerance` (`docs/SIMULATION.md`).
- Engine builds fleets in-memory; win condition is **annihilation** (all enemy ships).
- Blocking gate: `simulation/rubrics/fleet-safety.toml` (legal orders, bounded turns).
- Advisory: `simulation/rubrics/cost-matched.toml` (termination / win-rate band).

High stalemate rates block catalog tuning; treat as instrument debt, not a green
balance signal.

Plating, multi-bank shields, boosters, and batteries remain future modules with
their own space/cost under the frame/module split.
