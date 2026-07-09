# ADR-0009 -- Energy Allocation minimal slice (D7 / Slice 4)

Status: Accepted
Date: 2026-07-08

## Context

Slice 3 fixed IMC speed as a static per-ship field. SFB's signature economy is Energy
Allocation: power is limited and movement speed is chosen each turn from that budget.
Full EA Form (weapons, shields, EW, etc.) is large; this slice lands the minimum vertical
path so speed is energy-driven.

## Decision

- Each ship has `power` (energy generated each turn) and `speed` (maximum legal speed / IMC cap).
- Each turn the ship has an **allocated movement speed** `turn_speed` in `0..=min(power, speed)`.
- Default at turn start / after RunTurn: `turn_speed = min(power, speed)` (full movement budget)
  so existing scenarios keep working without mandatory Allocate orders.
- New order `Allocate { ship, speed }` sets `turn_speed` for the current turn (must be
  `<= min(power, speed)`). Illegal allocation rejects with no mutation.
- Plot length and IMC move schedule use **`turn_speed`**, not the static max alone.
- Weapons and shields do **not** spend energy this slice (still fire-freely / fixed shields).
- Pure helpers live in `src/energy.rs`; allocation mutates `GameState` via setup/order path.

## Consequences

- Speed is energy-shaped without boiling the ocean.
- Later slices can spend power on weapons/shields by reducing default free movement and
  requiring explicit multi-bucket Allocate.
- Ship TOML gains `power` (default = `speed` when omitted for data compatibility).

## Amendment (deepen)

Multi-bucket `Allocate { ship, movement, weapons, shields }`:
- cost = sum of buckets <= power; movement <= max speed
- default: max movement, remainder to weapons, shields 0
- fire spends 1 weapons_energy at declare resolve
- shield_reinforce absorbs before facing shields
- ship power raised above max speed so defaults leave fire energy
