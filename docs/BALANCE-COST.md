# Construction cost, efficiency, and balance claims

Status: **implemented catalog model; pooled balance candidate, not certified**.
See [`BALANCE.md`](BALANCE.md) for current measurements and scope. Simulator win
matrices certify named claims; efficiency only generates candidate catalogs.

### Status (2026-07-18)

- The frame-plus-modules cost model is implemented by
  `tools/generate_size_variants.py` and projected into `data/ship_costs.toml`.
- A/B/C pass reusable pooled seeds 1-191 for the current catalog.
- The virgin 264-327 sign-off range has not been run; do not call the catalog
  certified.
- The cross-size ladder remains uncertified. Do not extend destroyer/titan
  evidence to cruiser, battleship, dreadnought, or mixed-fleet fairness.
- The early 2026-07 "lock" and holdout reports are rejected historical evidence
  because tuning exposed overfit and then a friendly-fire simulation bug.

Campaign history: [`history/BALANCE-CAMPAIGN-2026-07.md`](history/BALANCE-CAMPAIGN-2026-07.md).

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

## Falsifiable claims (A / B / C)

The economic design starts from equal-budget comparisons, but the current A/B
suite uses calibrated near-cost counts and sets `skip_cost_validation = true`.
Do not describe these rows as exact equal-budget certification.

| Claim | Current operational definition | Desired outcome |
|-------|-------------------------------|-----------------|
| **A** | 7 destroyers (700) vs `titan_light` (762), minimum fill | Swarm wins 60-80% |
| **B** | 10 destroyers (1000) vs `titan_heavy` (910), maximum fill | Either side can win; player 40-60% |
| **C** | Fixed 8 destroyers (800) vs `titan_heavy` (910) | Titan wins at least 90%; scenario claim, not cost parity |
| **Control** | 4 destroyers vs 4 destroyers | Player 35-65%; detect side/policy drift |

Claim **B is ill-posed without forced engagement**. Mobility can refuse battle on
open maps (empirically: high non-termination under annihilation-only). Either:

- anchor objectives (hold point, convoy, timer), or  
- run balance suites on **bounded** arenas / closing incentives.

## Implemented catalog model

`tools/generate_size_variants.py` now computes:

```text
raw cost = C_frame(size) + 1.2 * power + 3 * shield-face-cap
           + sum(flat weapon-kind prices)
catalog cost = normalize(raw cost so destroyer_line = 100)
```

`C_frame(size)` grows approximately with `size^1.85`. Light, line, and heavy
variants are different fill packages on that frame, not total-cost multiples of
a historical Combat-D score. Generated `power_sys`, `engine_boxes`, weapon-box
depth, and selected fire-control fields are also part of the catalog projection.

This implements the desired economic shape, but implementation is not proof of
fairness. The middle ladder and mixed fleets still need their own claims and
sign-off evidence.

## Order of operations

0. **Follow [`BALANCE-PROTOCOL.md`](BALANCE-PROTOCOL.md).** Diagnose mechanism
   from match records before picking a lever. Win rates
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
4. **Tune** — fast seeds 1-32, then pooled seeds 1-191, using the tracked A/B/C
   suites and asymmetric policies.
5. **Certify once** — spend a virgin sign-off range only on a frozen candidate.
   The current tracked range is 264-327. A failed or followed-by-tuning run burns
   that range.

```bash
cargo run --release --bin shipsim-sim -- \
  --suite simulation/suites/abc_claims_pooled.toml \
  --output tmp/simulation/reports/abc_claims_pooled.json
```

Current costs include `destroyer_line=100`, `titan_light=762`, and
`titan_heavy=910`. Suite counts are A=7 destroyers vs light, B=10 destroyers vs
heavy, and C=8 destroyers vs heavy. Current measured outcomes belong in
[`BALANCE.md`](BALANCE.md), not in this economic model.

Power / shield / structure **sweeps** remain useful for local levers once games
decide; they do not replace catalog form or engagement rules.

## Related docs

- Current status: `docs/BALANCE.md`
- Agent tuning and seed protocol: `docs/BALANCE-PROTOCOL.md`
- Size ladder: `data/sizes.toml`, `docs/SIZE-VARIANTS.md` (historical D-cost note)
- Suites: `docs/SIMULATION.md`, `simulation/suites/cost_matched.toml`
- FASA component ratios (mass/SS/MCr): `tmp/sfb/Master-Construction-Book.xlsx`
  (Beam / Torpedo / shields / Weight sheets) — input for relative \(c\), not HEAD costs
