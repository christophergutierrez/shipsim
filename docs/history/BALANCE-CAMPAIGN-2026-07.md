# July 2026 balance campaign history

Status: **historical and non-authoritative**. Current status is maintained in
[`../BALANCE.md`](../BALANCE.md). Current values live in rules and ship TOML,
not in this narrative.

This document curates the useful evidence from the local campaign logs without
promoting generated multi-gigabyte reports or obsolete "final" claims into the
authoritative documentation set.

## Goal

The campaign introduced a frame-plus-modules construction-cost model and tested
three destroyer-versus-titan claims:

- A: a minimum-fill titan should usually lose to a near-cost swarm;
- B: a maximum-fill titan should be competitive with a near-cost swarm;
- C: a maximum-fill titan should dominate a fixed eight-destroyer scenario;
- control: a symmetric destroyer mirror should remain broadly neutral.

The campaign also established bounded forced-engagement scenarios, asymmetric
policy probes, per-hull power-utilization metrics, and tracked rubric bands.

## Phase 1: apparent lock, later rejected

An early seeds-1-63 run appeared to satisfy A and B:

| Claim | Player result |
|---|---:|
| A | 66.7% swarm |
| B | 41.3% swarm |
| C | 0.8% swarm |
| Control | 46.0% player, 43.7% opponent |

That result must not be treated as certification. B fell to 29.7% swarm on the
then-held-out seeds 64-191, and the middle size ladder showed severe
concentrated-hull dominance in small samples.

More importantly, later review found that simulation fire-order generation did
not filter targets by side. Swarms frequently selected wounded friendly ships,
so the original balance was partly a fleet-size-dependent friendly-fire tax.
All pre-fix lever conclusions were invalidated.

## Mechanism fixes

The campaign fixed or characterized several non-balance mechanisms:

- simulation policies now generate enemy-only fire orders;
- mutual disarm terminates instead of consuming the remaining turn cap;
- `damage_diff` adjudication became real output rather than a parse-only option;
- large hulls received explicit SSD power/engine depth rather than a universal
  two-box reactor;
- traces exposed arc loss, long-range fire, and weapon-bank destruction as
  separate causes of poor capital output;
- shield-versus-beam exact breakpoints were documented and guarded by the
  catalog generator.

The honest post-friendly-fire baseline produced 100% swarm wins in A, B, and C.
That became the baseline for Phase 3.

## Phase 3: accuracy and weapon-survival retune

The first attempted accuracy fix used a discontinuous target-size cap: size 1-2
could reach threshold 20 while size 3+ was capped at 15. It reduced automatic
hits on capitals, but made a size-3 target harder to hit than a size-2 target in
some cases. It was superseded.

The retained rule is range-aware and monotonic:

```text
range ceiling = min(19, max(base range-table threshold, 15))
```

Larger silhouettes retain an accuracy advantage without becoming automatic
hits. Titan fire-control bonuses apply only against exact size-2 targets. Titan
weapons also have additional SSD box depth so one localized weapon hit does not
erase an entire capital mount.

The first retained catalog (`titan_light +10`, `titan_heavy +8`) passed reusable
pooled seeds, but no valid sign-off was performed. Seeds 200-263 had already
been reused during tuning and were declared burned.

## Current candidate at campaign close

The checked-in catalog later moved to `titan_light +12` and
`titan_heavy +10`. It was remeasured on 2026-07-18 using reusable seeds 1-191:

- engine revision: `4634cc0`;
- rules fingerprint: `fnv1a-3fb61002d562620d`;
- suite: `simulation/suites/abc_claims_pooled.toml`.

| Engagement | n | Player win | Opponent win | In progress |
|---|---:|---:|---:|---:|
| A | 764 | 74.9% | 17.5% | 7.6% |
| B | 764 | 59.4% | 40.6% | 0.0% |
| C | 764 | 5.2% | 94.2% | 0.5% |
| Control | 764 | 40.1% | 49.5% | 10.5% |

Aggregate termination was 95.4%. `fleet_safety` and `abc_claims` passed.
This candidate remains **pooled-balanced but not certified**. The replacement
sign-off range 264-327 was still untouched when this history was written.

## Reports and provenance

The following local report names may exist in a developer's ignored
`tmp/simulation/reports/` directory. Their labels are important:

| Report | Status | Reason |
|---|---|---|
| `abc_claims_1k.json(.gz)` | rejected historical | pre-friendly-fire fix; seeds reused for tuning |
| `abc_claims_holdout.json(.gz)` | rejected historical | exposed B overfit, then invalidated by mechanism fix |
| `abc_claims_pooled_postfix.json(.gz)` | baseline historical | honest post-fix 100% swarm baseline, not current candidate |
| `claimA_longturn.json(.gz)` | diagnostic historical | characterized pre-fix disarm deadlock |
| `cost_matched*.json(.gz)` | diagnostic historical | different revisions; not current ladder certification |

Generated reports are not checked-in dependencies. Reproduce current evidence
from tracked suites and record commit plus `rules_fingerprint`; report files do
not identify ship TOML or Rust formula changes by themselves.

## Lessons retained

1. Mechanism diagnosis precedes tuning.
2. A symmetric control is necessary but not sufficient.
3. One lever per cycle makes reversions and causal claims defensible.
4. A die cap can silently erase the entire range table.
5. Kill rate is not hit rate; range, arc, damage, and weapon survival matter.
6. Exact shield/damage divisibility creates discontinuous outcomes.
7. Reusing a holdout burns it, even if the run was informal.
8. Pooled success is a candidate gate, not certification.
9. Cost-vs-cost claims do not generalize to untested hull ladders or policies.
10. Raw reports need status and provenance before anyone should cite them.
