# Gameplay Rubrics

Gameplay rubrics are TOML specifications evaluated against aggregate simulation metrics. They convert selected design expectations into reproducible red/green checks.

## Supported checks

| Field | Meaning |
|---|---|
| `min_termination_rate` | Minimum fraction ending in `Won` or `Lost` |
| `max_stalemate_rate` | Maximum fraction still in progress at the suite bound |
| `min_win_rate` | Minimum player-side win fraction |
| `max_win_rate` | Maximum player-side win fraction |
| `min_average_turns` | Lower pacing bound |
| `max_average_turns` | Upper pacing bound |
| `max_rejected_orders` | Policy/rules-boundary safety bound |
| `min_average_damage` | Minimum evidence that combat occurred |

Example:

```toml
id = "gameplay_smoke"
description = "Baseline policies should fight and neither side should dominate."
min_termination_rate = 0.25
max_stalemate_rate = 0.75
min_win_rate = 0.25
max_win_rate = 0.75
min_average_damage = 1.0
```

## Framework mapping

- **Equilibrium:** allocation use, damage flow, shield depletion, and match pacing over repeated turns.
- **Sensitivity:** compare reports across seeds, scenarios, and later parameter-sweep suites.
- **Tactical invariants:** bound dominance, stalemates, unused systems, and strategy viability.
- **Stochastic verification:** compare observed hit/damage distributions with table expectations across large seeded samples.
- **MDA aesthetics:** use simulation metrics as warning signals for pacing, tension, and meaningful choice; do not treat them as proof of player experience.

## Blocking versus advisory

Safety, determinism, state validity, and bounded termination are appropriate CI blockers. Balance and aesthetic proxies should begin as advisory reports until their sample sizes and thresholds have been reviewed. Subjective visual or experiential judgments require separate evidence, including screenshots and human play.

Rubric changes are product-design changes and should be reviewed like combat-table changes. Do not loosen a threshold merely to make a suite green; record the reason and expected gameplay effect.
