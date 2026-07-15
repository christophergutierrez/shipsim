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
4. **Cost** uses Combat Efficiency **D** (more fleet-playable than raw
   power×SS mass). Destroyer line = **100**:
   - `cost_line = round(100 × D_median / D_destroyer)`
   - light = 0.85× line, heavy = 1.20× line  
   JSONL median D ratios vs destroyer:  
   Fighter 0.61 · Destroyer 1.00 · LC 1.62 · HC 2.47 · BB 3.94 · DN 5.68 · Titan 7.98  
5. **Thrust** follows FASA movement-point ratio trend (larger = more power per
   thrust): fighters/destroyers efficient; capitals `power_per_thrust` 2–5.
6. **Weapons** step up by size and variant (light = few mounts; heavy = fuller
   suite). Kinds stay beam / torp / plasma.

**Not used as primary cost:** power×SS product (titan ~89× destroyer). That is a
useful *mass* signal but makes equal-cost fleets unwieldy; D-based cost keeps
“~8 destroyers vs 1 titan” as the top equal-budget sketch.

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

## Balance method (start with power)

1. **Sweep power** on one hull in a fixed equal-cost (or fixed-count) matchup
   (`[[power_sweeps]]` in a suite — see `docs/SIMULATION.md`).
2. Expect a curve: **not enough** → dies / never works; **enough** → viable band;
   **too much** → surplus (WR/turns plateau; energy left idle).
3. Read **how** the underdog loses before touching other levers:
   - **Loses quickly** → defense not cutting it (shields max/ratio, structure, power
     into faces) — not primarily weapon count.
   - **Loses after many turns** → survives but cannot finish (weapons, charge,
     damage tables, or power left after defense).
4. Only after the power band is sane, move to shields / weapons / structure.

```bash
cargo run --release --bin shipsim-sim -- \
  --suite simulation/suites/titan_power_sweep.toml \
  --output tmp/simulation/reports/titan_power_sweep.json
```

## Simulation

Reusable suite (not one-off scenarios):

```bash
cargo run --release --bin shipsim-sim -- \
  --suite simulation/suites/cost_matched.toml \
  --output tmp/simulation/reports/cost_matched.json
```

- Suite format: `[[engagements]]` + optional `budget` / `cost_tolerance` (`docs/SIMULATION.md`).
- Engine builds fleets in-memory; win condition is **annihilation** (all enemy ships).
- Blocking gate: `simulation/rubrics/fleet-safety.toml` (legal orders, bounded turns).
- Advisory: `simulation/rubrics/cost-matched.toml` (termination / win-rate band).

Tune power / structure / shields / cost from report breakdowns by `engagement`
(see `docs/TODO.md`). Early runs often show high stalemate on some pairings and
lopsided WR on others — that is expected draft signal, not a harness bug.

Plating, shield boosters, and batteries are **not** in these drafts; they remain
future construction modules that should also carry cost/space so large hulls
pay for anti-magnet tools.
