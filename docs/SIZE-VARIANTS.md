# Size variants (draft fleet catalog)

Three generic hulls per size tier (`data/sizes.toml`), derived from
`tmp/sfb/ships.jsonl` class buckets and intended for **equal-cost simulation**.

**Status:** draft numbers. Combat does not spend `cost` yet. Tune after
`shipsim-sim` matchups under matched budgets.

## Files

| Path | Role |
|---|---|
| `data/ships/{size}_{light\|line\|heavy}.toml` | 21 loadable ship classes |
| `data/ship_costs.toml` | Flat cost index (mirrors `cost` on each ship) |
| `data/sizes.toml` | Size id / name / FASA Class map |

Legacy hulls (`escort`, `heavy_cruiser`, `huge`, `starbase`) are unchanged for
existing scenarios.

## How numbers were shortcut from JSONL

1. **Bucket** ships with STCS `class_number` into size 1–7 (I–III … XIX–XX).
2. Take **p25 / median / p75** of `total_power_units` and superstructure for
   light / line / heavy within each bucket.
3. **Scale into shipsim integers** so destroyer *line* matches the old escort
   power budget:
   - `power ≈ round(stcs_power × 14/34)` (destroyer median power 34 → 14)
   - `structure ≈ round(stcs_ss × 8/14)` (destroyer median SS 14 → 8)
4. **Cost (historical HEAD, superseded as a design target):** Combat Efficiency
   **D** medians with destroyer line = **100**:
   - `cost_line = round(100 × D_median / D_destroyer)`
   - light = 0.85× line, heavy = 1.20× line  
   That forces cost ∝ measured V (η ≈ constant) and **does not** implement
   frame-sunk + flat modules. **Target model and catalog gap:**
   [`docs/BALANCE-COST.md`](BALANCE-COST.md).
5. **Thrust** follows FASA movement-point ratio trend (larger = more power per
   thrust): fighters/destroyers efficient; capitals `power_per_thrust` 2–5.
6. **Weapons** step up by size and variant (light = few mounts; heavy = fuller
   suite). Kinds stay beam / torp / plasma.

**Not used as primary cost:** power×SS product (titan ~89× destroyer). D-based
totals were a fleet-playable stopgap only.

## Line costs (budget unit)

| Size | Class | light | line | heavy | ≈ destroyers (line) |
|---:|---|---:|---:|---:|---:|
| 1 | Fighter | 52 | 61 | 73 | 0.61 |
| 2 | Destroyer | 85 | 100 | 120 | 1.00 |
| 3 | Light Cruiser | 138 | 162 | 194 | 1.62 |
| 4 | Heavy Cruiser | 210 | 247 | 296 | 2.47 |
| 5 | Battleship | 335 | 394 | 473 | 3.94 |
| 6 | Dreadnought | 483 | 568 | 682 | 5.68 |
| 7 | Titan | 678 | 798 | 958 | 7.98 |

### Equal-cost sketch fleets (line hulls)

| Budget ~800 | Example |
|---|---|
| Titan | 1× `titan_line` (798) |
| Dreadnought + screen | 1× `dreadnought_line` + 2× `destroyer_line` |
| Battleship wing | 2× `battleship_line` |
| Heavy pack | 3× `heavy_cruiser_line` |
| Destroyer swarm | 8× `destroyer_line` |
| Mixed | 1× `heavy_cruiser_line` + 2× `light_cruiser_line` + 2× `destroyer_line` |

## Variant roles

| Variant | Intent |
|---|---|
| **light** | Cheaper, thinner, more thrust-efficient; fewer weapons |
| **line** | Median JSONL stats; reference for cost-vs-cost |
| **heavy** | Costlier, thicker, denser weapons; often worse thrust ratio |

## Balance method

Authoritative economics + claims A/B/C + order of ops:
**[`docs/BALANCE-COST.md`](BALANCE-COST.md)**.

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
