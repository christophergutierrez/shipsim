# ADR-0016 -- Multi-ship fleets and campaigns (D10)

Status: Accepted
Date: 2026-07-09

## Decision

- Core already supports N ships; add `scenarios/fleet.toml` (2v2) as the headline multi-ship
  scenario.
- Campaigns: `campaigns/*.toml` lists ordered scenario paths; `campaign::load` +
  `Campaign::advance_on_win` walks the list when a scenario ends `Won`.
- CLI: `shipsim --campaign campaigns/demo.toml --orders ...` runs the current scenario file
  from the campaign state (in-memory advance between scenarios when status is Won).

## Consequences

- Multi-ship AI/scripted fleets use existing controllers.
- Full campaign UI/persistence remains frontend concern; core exposes load + advance.
