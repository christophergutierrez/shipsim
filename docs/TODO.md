# Open work

One living list. Product *why* is [`PRD.md`](PRD.md). Rules are
[`PLAY-V2.md`](PLAY-V2.md) and [`PROTOCOL.md`](PROTOCOL.md). Plans stay in
`tmp/` ([`DOC-LIFECYCLE.md`](DOC-LIFECYCLE.md)).

Protocol v4 path loop is live: power → motion via `thrust_per_power` /
`power_per_thrust`. Names like `movement_cost`, initiative-from-allocation,
and momentum are **not** the current loop.

## Now

- Record the standard catalog evidence matrix. Diagnostic fixed-count suites
  are not equal-cost certification.
- Tune only on seeds 1–191, evaluate once on 328–391, sign off once on
  264–327. Record catalog and rules fingerprints
  ([`BALANCE-PROTOCOL.md`](BALANCE-PROTOCOL.md)).
- Decide whether `torpedo_potent` is dropped or differentiated after that
  evidence (SKU decision, not an implementation gap).
- REPL/Love rubric reviews. REPL repair still uses a stale 1/2 size split
  and must consume snapshot `repair_cap`.
- Polish client UX: TUI Medium/Large tiers, Love presentation, combat-log
  clarity and replay navigation.
- Simulation runner: confidence intervals, parameter sweeps, stochastic
  table-verification, tactical-diversity metrics.
- Use sim results to drive playthroughs of high-value paths and UI.
- Screenshot / layout checks against [`UI-RUBRIC.md`](UI-RUBRIC.md).
- Campaign save/resume after semantics are settled
  ([`SAVE-FORMAT.md`](SAVE-FORMAT.md)).

TUI UI-rubric and first-play passes are **closed**. Do not “fix” the
falsified playtest claims (budget header, arrows, power-bar fill). Re-run
via [`UI-PLAYTEST-PROTOCOL.md`](UI-PLAYTEST-PROTOCOL.md).

## Later

Electronic warfare, sensors, crew quality. More catalog content. Campaign
persistence and UI. Richer crits and repair. Network multiplayer. New rules
stay in the Rust core.

## Shipped

- v4 loop: allocate → path → volley; thin clients (REPL, TUI, Love2D);
  `shipsim-sim`; golden fixtures under `tests/fixtures/v4/`.
- Seven-tier ladder (`data/sizes.toml`); to-hit `size / 2` (baseline 2) plus
  MOO natural defense. Catalog classes and costs:
  [`SIZE-VARIANTS.md`](SIZE-VARIANTS.md), [`SHIPYARD.md`](SHIPYARD.md).
- Yard compile → `data/ships/<id>.toml`. Stock catalog is not regenerated
  from the yard.
- Combat systems: Repeat/Pierce, PD, computer/cloak/repair, squads, graviton,
  missiles, ECM, budget fleets.

Historical size-ladder *intent* (candidate fields, old sequence) is
[`history/SIZE-LADDER-INTENT.md`](history/SIZE-LADDER-INTENT.md), not current
rules.
