# ADR-0011 -- Simultaneous fire resolution (D2-fire)

Status: Accepted
Date: 2026-07-09

## Context

After D1-fire, shots still resolved **sequentially** in declaration order within an impulse.
That lets ship A kill ship B before B's shot lands. SFB-style simultaneous fire avoids first-mover
advantage: both shots use the same pre-fire world state.

## Decision

On each impulse, after movement:

1. Drain queued fires whose weapon window matches the impulse (D1-fire).
2. Sort ready shots by `(ship_id, weapon_id, target_id)` ascending for deterministic PRNG order.
3. **Phase 1 (compute):** for each shot, legality and damage rolls use a **frozen snapshot** of
   all ships at the start of the fire phase. A ship destroyed only by this phase's hits still
   shoots if it was alive in the snapshot.
4. **Phase 2 (apply):** apply all computed hits. Mutual kill is possible.

`combat::compute_fire` produces a `FireHit` without mutating the target; application is separate.

## Consequences

- Declaration order no longer affects combat outcomes within an impulse.
- Combines with D1-fire: simultaneity is per impulse fire window, not only turn-end.
- AS2 (geometry recomputed) remains acceptable: legality uses the snapshot once per shot.
