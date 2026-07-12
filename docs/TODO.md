# shipsim Design TODO

## Hull size, movement efficiency, and system capacity

Status: Design candidate, not accepted rules.

### Goal

Make hull size a strategic tradeoff rather than a linear power upgrade:

- Larger ships generate more power, carry more weapons and systems, support stronger shields, and have more hull.
- Larger ships require more power to move and may be less responsive.
- Smaller ships convert power into movement more efficiently and should retain meaningful fleet roles.
- The model should later support ship construction, where hull capacity constrains installed systems and better systems consume a monetary budget. Construction cost is not part of the current change.

### Candidate ship-data fields

- `size`: hull classification and future construction input.
- `power`: energy available each turn.
- `movement_cost`: base power required to purchase one movement point.
- `system_capacity`: space available for weapons and future systems.
- `hull`: structural endurance.
- `max_shield_per_facing`: defensive capacity.
- Optional `turn_cost` or `agility`: only if the movement design needs a separate responsiveness property.

Keep these values explicit in ship data initially. Do not derive every statistic from `size` until simulation evidence supports stable formulas.

### Candidate rules

```text
movement points = movement power / movement_cost
actual move cost = base movement cost * momentum modifier
```

Initiative must use effective movement capability, not raw movement power. Otherwise large reactors receive an unintended initiative advantage for buying the same or fewer moves.

Weapons and future systems consume both operational power and permanent system capacity. Scenario loading must reject ship definitions whose installed system space exceeds hull capacity.

### Design constraints

- Power generation must not outscale movement, defense, and system costs so strongly that large hulls become strictly dominant.
- Large ships should gain capacity but lose flexibility per unit of power.
- Small ships need useful roles through movement efficiency, initiative, board coverage, flanking, screening, or later objective play.
- Individual hulls do not need equal combat strength. Balance should ultimately be evaluated at fleet-resource or construction-cost level.
- Momentum, turning, reversal, and initiative rules must be designed together with size-based movement costs.

### Proposed implementation sequence

1. Complete and accept the new movement-system design.
2. Add explicit hull size, movement cost, and system capacity fields.
3. Make movement allocation purchase movement capability according to hull cost.
4. Base initiative on effective movement capability.
5. Add system-space values and validate installed capacity at load time.
6. Add generic hulls smaller and larger than the current escort/cruiser.
7. Add mirrored simulation suites across hull and fleet compositions.
8. Introduce construction money and configurable loadouts only after the combat tradeoffs are stable.

### Simulation evidence required

- Movement power spent per hex by hull size.
- Initiative frequency by hull size.
- Turns and range at first engagement.
- Weapon, shield, movement, and unused-power ratios.
- Shield-facing and flanking outcomes.
- Mirrored win rates for hull and fleet matchups.
- Win rates normalized by future fleet capacity or construction cost.
- Stalemate and non-moving-large-ship rates.

Red conditions include a largest-hull dominant strategy, small ships dying before mobility matters, large ships finding movement prohibitively expensive, small ships maximizing every subsystem without tradeoffs, or raw reactor power inadvertently determining initiative.
