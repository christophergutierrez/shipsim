# shipsim - Slice 4 CONTEXT (Energy Allocation / D7 minimal)

## Scope

Make movement speed energy-driven via a per-turn allocation, without full EA Form.

In scope:
- Ship `power` (TOML) + max `speed`
- Per-turn `turn_speed` allocation; default full `min(power, speed)`
- Order `Allocate { ship, speed }`
- Plot / IMC use `turn_speed`
- `src/energy.rs` pure helpers
- Snapshot shows `power` and `turn_speed`
- Tests for allocate + plot limit

Out of scope: weapon/shield energy costs, reinforcement, overload, full EA Form UI, D1-fire.

## Decisions

See ADR-0009.
