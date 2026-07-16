# Construction cost, efficiency, and balance claims

Status: **design target**. The HEAD catalog does **not** implement this model yet
(see [Catalog gap](#catalog-gap-head)). Simulator win matrices certify balance;
η only generates candidate catalogs.

## Intended economic shape

Frame + fill:

\[
\mathrm{Cost}(s,L) = C_{\mathrm{frame}}(s) + c\cdot L
\]

\[
0 \le L \le L_{\max}\big(S(s), P(s)\big)
\]

- \(C_{\mathrm{frame}}(s)\): **large positive** sunk cost (structure / hull / yard),
  growing in size \(s\) (e.g. \(a\cdot s^{\alpha}\), \(\alpha \ge 1\)).
- \(c\): **flat** (or only mildly size-dependent) module prices — weapons, shield
  banks, etc. FASA construction SS/MCr tables may set *relative* \(c\), not absolute
  shipsim points.
- \(S(s), P(s)\): space and design power **capacity**; grow with size so capitals
  can fill more modules than destroyers.
- **Do not** price \(C_{\mathrm{engine}} \propto P(s)\) as pure sunk cost on top of
  power-as-capability: in shipsim, design power already drives thrust and weapons.
  Clean split: **structure/frame = fixed intercept**; **power and modules = marginal**
  (power may still have a per-point cost inside \(c\cdot L\) or a separate linear term).

Rough combat proxy (catalog generation only):

\[
V \approx V_0(s) + k\cdot L,\qquad
\eta(L) = \frac{V_0 + k L}{C_{\mathrm{frame}} + c L}
\]

Story this encodes:

| Fit | Efficiency story |
|-----|------------------|
| Titan **min** | High \(C_{\mathrm{frame}}\), low \(L\) → low \(\eta\) vs maxed destroyers |
| Titan **max** | Same frame, large \(L\) → \(\eta\) rises (hull amortized) |
| Destroyer | Small \(C_{\mathrm{frame}}\), tiny \(L_{\max}\) → almost always “full”; cannot climb |

### Monotone η (explicit)

\[
\frac{d\eta}{dL} > 0 \iff k\,C_{\mathrm{frame}} > c\,V_0
\]

The inequality **does not depend on \(L\)**. So under scalar \(k,c\):

- η is **monotone** on \([0, L_{\max}]\) for each hull.
- Within a hull, only **min or max** is η-optimal; **line** is budget granularity,
  not an efficiency choice.

If mid fits must be genuine decisions, add concavity somewhere:

- declining \(k(L)\) (stacking / fire-control / arc limits), or
- convex \(c(L)\) (integration cost of each extra battery).

### η does not certify battles

Fleet value is **not** linear:

- Lanchester / focus fire: \(V(N\times D) \neq N\cdot V(D)\).
- Overkill and target-switching cut the other way.
- **Regenerating per-facing shields** create a **hard breakpoint**: if incoming DPS
  per face stays below shield re-power per turn, net hull damage is ~0 — \(V\)
  collapses discontinuously. “Max titan cannot be beaten” is a **dominant-strategy
  failure to detect**, not a design target.
- Aggregation is **policy-dependent** (focus vs spread). Certify claims against a
  policy set that includes asymmetric best-response attempts, not only greedy/greedy
  mirrors.

Use η to **propose** catalogs; use the **win matrix** (and stalemate / damage
scores) to **accept** them.

Niche / mid hulls likely need **matrix** module value \(k_{ij}\) (anti-fighter vs
capital, tracking vs size), not a single scalar \(k\).

## Falsifiable budget claims (A / B / C)

Three distinct definitions — do not mix them in one suite row.

| Claim | Budget definition | Desired outcome |
|-------|-------------------|-----------------|
| **A** | Equal budget \(B\); titan at **min** legal combat fit vs as many destroyers as \(B\) buys at **their** practical max | Swarm wins more often than not |
| **B** | Equal budget \(B\); titan at **max** fill vs as many destroyers as \(B\) still buys | Titan competitive or favored — **but** not unkillable; mid hulls still relevant |
| **C** | Fixed **count** (e.g. 8 DD) vs titan max (titan cost may exceed \(8\times C_D\)) | Scenario design claim, not pure cost-vs-cost |

Claim **B is ill-posed without forced engagement**. Mobility can refuse battle on
open maps (empirically: high non-termination under annihilation-only). Either:

- anchor objectives (hold point, convoy, timer), or  
- run balance suites on **bounded** arenas / closing incentives.

## Catalog gap (HEAD)

`data/ship_costs.toml` / size variants were generated as:

```text
cost_line = round(100 × D_median / D_destroyer)   # Combat D from JSONL
cost_light = 0.85 × cost_line
cost_heavy = 1.20 × cost_line
```

That is **cost ∝ measured combat efficiency**, which forces \(\eta \approx\) constant
across tiers by construction. Light→heavy within a size often shows **rising**
cost-per-power and **near-zero or negative** fixed intercepts when regressing
cost on power — the **opposite** of large positive \(C_{\mathrm{frame}}\) and flat
marginal \(c\). Titan variants share structure 120 while heavy mainly buys power,
so “fill” does not buy hull capacity.

**Consequence:** claims A and B are not expressible as fill-level choices on the
current catalog; equal-cost fleets only compare D-scaled package deals.

**Fix when regenerating:** `tools/generate_size_variants.py` must emit

- frame intercept \(C_{\mathrm{frame}}(s)\) from size (and structure capacity),
- flat (or FASA-ratio) module prices,
- light/line/heavy as **fill levels** \(L\), not D-multipliers,

not Combat-D medians as total cost.

## Order of operations

0. **Diagnose mechanism from match records before picking a lever.** Win rates
   alone mislead (e.g. B “titan too weak” was power_sys=2 → reactor dead after
   two DAC Power hits). Read traces / B2 geometry metrics / per-class
   `power_utilization`. Instrument: `min_class_power_utilization` on
   `fleet-safety` (blocking).
1. **Engagement / termination instrumentation** — policy velocity governor
   (close when far); `stalemate_scoring = "damage_diff"` on suites.
2. **Forced engagement** — fleet scenarios use **hard** maps and ~9-hex start
   gap (`build_engagement_scenario`).
3. **Catalog regeneration** — `tools/generate_size_variants.py` emits
   frame-sunk + flat module costs (normalized `destroyer_line = 100`) and
   scaled `power_sys` / `engine_boxes` (not a global literal 2).
4. **Certify** — `simulation/suites/abc_claims.toml` (A/B/C + asymmetric policies).
   Prefer **≥100 matches per claim cell** and a **held-out seed set** for final
   sign-off (binomial noise at n=32 is ~±17pp).

```bash
cargo run --release --bin shipsim-sim -- \
  --suite simulation/suites/abc_claims.toml \
  --output tmp/simulation/reports/abc_claims.json
```

Power / shield / structure **sweeps** remain useful for local levers once games
decide; they do not replace catalog form or engagement rules.

## Related docs

- Size ladder: `data/sizes.toml`, `docs/SIZE-VARIANTS.md` (historical D-cost note)
- Suites: `docs/SIMULATION.md`, `simulation/suites/cost_matched.toml`
- FASA component ratios (mass/SS/MCr): `tmp/sfb/Master-Construction-Book.xlsx`
  (Beam / Torpedo / shields / Weight sheets) — input for relative \(c\), not HEAD costs
