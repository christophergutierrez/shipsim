# shipsim Design TODO

> Motion today is protocol-v4 **path** actions (ADR-0025): power → motion points
> via hull `thrust_per_power` / `power_per_thrust`, one ordered path per ship.
> Sections below that mention `movement_cost`, initiative-from-allocation, or
> momentum are **design candidates / history**, not the live loop. See
> `docs/PROTOCOL.md` and `docs/PLAY-V2.md` for current rules.

## Hull size, movement efficiency, and system capacity

Status: Partially implemented. Explicit hull `size` and size-adjusted to-hit are
accepted rules. The **seven-tier size ladder** is catalogued in
`data/sizes.toml` (Fighter … Titan). Construction capacity, component catalogs,
and size-derived yard limits remain design candidates.

### Size ladder (accepted catalog)

| `size` | Name | FASA Class map |
|---:|---|---|
| 1 | Fighter | I–III |
| 2 | Destroyer | IV–VI |
| 3 | Light Cruiser | VII–IX |
| 4 | Heavy Cruiser | X–XII |
| 5 | Battleship | XIII–XV |
| 6 | Dreadnought | XVI–XVIII |
| 7 | Titan | XIX–XX |

~3 FASA classes per shipsim tier. Source role labels (Scout, Frigate, …) are not
size IDs — map scraped ships by STCS class number when present. Combat baseline
remains size **2** until an explicit retune. See `docs/combat-v2-tables.md`.

**Draft variants:** three hulls per size (`*_light` / `*_line` / `*_heavy`).
HEAD `cost` is Combat-D ratioed (destroyer_line = 100) — **diseconomies of scale
vs the intended frame-sunk model**; see `docs/BALANCE-COST.md`.
**Suite:** `simulation/suites/cost_matched.toml`. **Before regenerating costs:**
engagement/termination instrumentation (stalemates dominate); then frame/module
catalog regen; certify A/B/C with asymmetric policies.

### Goal

Make hull size a strategic tradeoff rather than a linear power upgrade:

- Larger ships generate more power, carry more weapons and systems, support stronger shields, and have more hull.
- Larger ships require more power to move and may be less responsive.
- Smaller ships convert power into movement more efficiently and should retain meaningful fleet roles.
- The model should later support ship construction, where hull capacity constrains installed systems and better systems consume a monetary budget. Construction cost is not part of the current change.

### Candidate ship-data fields

- `size`: implemented as the relative target silhouette (`1..=7` per
  `data/sizes.toml`) and future construction input. Size 2 is the neutral d20 baseline.
- `power`: energy available each turn.
- `movement_cost`: base power required to purchase one movement point.
- `system_capacity`: space available for weapons and future systems.
- `hull`: structural endurance.
- `max_shield_per_facing`: defensive capacity.
- Optional `turn_cost` or `agility`: only if the movement design needs a separate responsiveness property.
- Future construction: per-size hull space/cost; weapon and engine component
  TOMLs with their own space/cost (not loaded yet).

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
2. ~~Catalog seven hull sizes (`data/sizes.toml`).~~ Align shipped ship TOMLs
   and to-hit baseline with the ladder when ready.
3. Add movement cost and system capacity fields (per size and/or per hull).
4. Make movement allocation purchase movement capability according to hull cost.
5. Base initiative on effective movement capability.
6. Add weapon/engine component TOMLs (space, cost, combat knobs); validate
   installed capacity at load time.
7. Add generic hulls for each size tier (or migrate escort / heavy / huge / base).
8. Add mirrored simulation suites across hull and fleet compositions.
9. Introduce construction money and configurable loadouts only after the combat tradeoffs are stable.

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
