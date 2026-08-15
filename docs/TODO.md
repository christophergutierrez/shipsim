# shipsim Design TODO

> Motion today is protocol-v4 **path** actions (ADR-0025): power → motion points
> via hull `thrust_per_power` / `power_per_thrust`, one ordered path per ship.
> Sections below that mention `movement_cost`, initiative-from-allocation, or
> momentum are **design candidates / history**, not the live loop. See
> `docs/PROTOCOL.md` and `docs/PLAY-V2.md` for current rules.

## Hull size, movement efficiency, and system capacity

Status: **Implementation milestones shipped; catalog remediation and balance
evidence are in progress.** Implementation and remediation plans are ephemeral
and are not tracked; see `docs/DOC-LIFECYCLE.md`.

**Shipped (yard + combat silhouette):**

- Seven-tier ladder in `data/sizes.toml`; combat to-hit uses `size / 2`
  (baseline 2) plus MOO natural defense vs that baseline.
- Larger hulls: more space / frame / HP, worse thrust, fewer `mv`.
- Component yard (`docs/SHIPYARD.md`, TUI `--yard`): space cap, engines,
  exterior armor, per-face shield banks, weapon SKUs, design-time cost.
- Compile → ordinary `data/ships/<id>.toml`. Stock catalog is **not**
  regenerated from the yard.

**Shipped implementation:**

- Seven standard yard classes and four controlled quality fixtures are
  compiled and checked by the catalog drift gate.
- Typed combat systems, Repeat/Pierce, PD, computer/cloak/repair, squads,
  graviton, missiles, ECM, budget fleets, and their regression tests are
  implemented.

**Still open (this TODO):**

- Run and record the current standard catalog evidence matrix without treating
  diagnostic fixed-count suites as equal-cost certification.
- Tune only on seeds 1–191, evaluate once on 328–391, then sign off once on
  264–327, with catalog and rules fingerprints recorded.
- Decide whether `torpedo_potent` should be dropped or differentiated after
  evidence; this is a deliberate SKU decision, not an implementation gap.
- Named `movement_cost` / initiative-from-motion / momentum are **not** the
  live v4 path loop (see the note at the top of this file).
- TUI rubric pass against `docs/UI-RUBRIC.md` is closed (M0–M5). Remaining
  rubric notes are Minor/Polish (`Tab`/`v` header-only; yard-browse Esc
  unlabeled quit).
- TUI **first-play** remediation is closed for the executed blind-playtest
  findings (M0–M5). Do not implement the falsified claims from that study
  (budget header, arrow keys, power-bar fill): they were verified correct.
  Re-run the study via `docs/UI-PLAYTEST-PROTOCOL.md`.
- Follow with separate REPL/Love rubric reviews. REPL repair still uses a
  stale 1/2 size split and must consume snapshot `repair_cap`; neither TUI
  plan claims those clients are fixed.

**Recommended next (product, not a second list):**

1. Polish client UX (TUI Medium/Large tiers, Love presentation, combat-log
   clarity and replay navigation).
2. Add confidence intervals and parameter sweeps to the simulation runner.
3. Add stochastic table-verification and tactical-diversity metrics.
4. Use simulation results to drive manual playthroughs of high-value paths
   and UI behavior.
5. Add screenshot capture and deterministic layout checks for
   [`UI-RUBRIC.md`](UI-RUBRIC.md).
6. Add campaign save/resume after campaign semantics are finalized
   ([`SAVE-FORMAT.md`](SAVE-FORMAT.md)).

**Later (not current work):** electronic warfare / sensors / crew quality;
more catalog content; campaign persistence and UI; richer crits and repair;
network multiplayer. Keep new rules in the Rust core.

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

**Standard catalog:** one yard-generated hull per size tier; costs live on the
compiled ship definitions. **Suites:**
`simulation/suites/catalog_standard.toml` for standard classes and
`simulation/suites/weapon_quality_matched.toml` for controlled SKU comparisons.
Diagnostic claim suites are explicitly calibrated-count experiments.

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
- Future construction: per-size hull space/cost and component TOMLs — **loaded
  by the yard**, not by combat scenario load for the stock catalog.

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
