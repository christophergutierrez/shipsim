# ADR-0012 -- SSD and damage allocation (D6)

Status: Accepted
Date: 2026-07-09

## Context

Slice 2 used a single `structure` pool for all internal damage. That cannot knock out
weapons or engines. SFB uses an SSD (Ship Systems Display) plus a damage-allocation chart.

## Decision

- Each ship has an itemized SSD: `hull`, `engine`, `power_sys`, and per-weapon boxes.
  A `bridge` field remains on the snapshot for compatibility but is **not** damaged and
  is **not** a kill condition (bridge-kill deferred; may return later).
- TOML `structure` seeds **hull** boxes. Engine boxes default to max speed; power_sys to 2;
  each weapon gets 1 box.
- Shield overflow is allocated **one box at a time** via a fixed DAC (damage allocation chart),
  advanced with a per-ship cursor (deterministic, no extra RNG). Empty slots spill to the next
  chart entry, then hull. Chart slots: Hull, Engine, Power, Weapon (no Bridge).
- **Destroyed** only when `hull == 0`.
- **Weapon** with 0 boxes cannot fire. **Engine** boxes scale max movement speed.
  **Power_sys** boxes scale turn power generation on energy reset.

## Consequences

- Combat can cripple without full destruction (engines, power, weapons).
- Snapshot exposes SSD fields for harness/frontend.
- Full SFB DAC table, multi-box weapons, and optional bridge-kill remain future polish.
