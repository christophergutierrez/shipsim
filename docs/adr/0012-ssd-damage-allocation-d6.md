# ADR-0012 -- SSD and damage allocation (D6)

Status: Accepted
Date: 2026-07-09

## Context

Slice 2 used a single `structure` pool for all internal damage. That cannot knock out
weapons or engines. SFB uses an SSD (Ship Systems Display) plus a damage-allocation chart.

## Decision

- Each ship has an itemized SSD: `hull`, `engine`, `power_sys`, `bridge`, and per-weapon boxes.
- TOML `structure` seeds **hull** boxes. Engine boxes default to max speed; power_sys to 2;
  bridge to 1; each weapon gets 1 box.
- Shield overflow is allocated **one box at a time** via a fixed DAC (damage allocation chart),
  advanced with a per-ship cursor (deterministic, no extra RNG). Empty slots spill to the next
  chart entry, then hull.
- **Destroyed** when `hull == 0` or `bridge == 0`.
- **Weapon** with 0 boxes cannot fire. **Engine** boxes scale max movement speed.
  **Power_sys** boxes scale turn power generation on energy reset.

## Consequences

- Combat can cripple without full destruction.
- Snapshot exposes SSD fields for harness/frontend.
- Full SFB DAC table and multi-box weapons remain future polish.
