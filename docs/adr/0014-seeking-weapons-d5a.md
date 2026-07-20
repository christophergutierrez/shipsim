# ADR-0014 -- Seeking weapons / drones (D5a)

Status: Superseded (historical; see ADR-0020 / ADR-0025 for current combat/turns)
Date: 2026-07-09

## Context

Only direct-fire weapons existed. ROADMAP D5a calls for seeking weapons (drones/plasma).

## Decision

- New `WeaponKind::Drone`: launched by `Fire`, not resolved as direct-fire on an IFF window.
- Launch checks range + arc + energy + weapon boxes (same declare path).
- On resolve, a **seeking munition** is spawned at the launcher hex, tracking `target` ship id.
- Each impulse after ship movement: munition moves **one hex** greedily toward the target's
  current position (lowest facing index on ties).
- On entering the target's hex: deals `weapon.damage` to the shield facing from approach
  direction (no to-hit roll). Munition removed.
- If target is destroyed or leaves the board, munition is discarded.
- Direct-fire phaser/disruptor behavior unchanged (still D1/D2 impulse fire).

## Consequences

- Timing + maneuver matter for drone intercept geometry.
- Plasma / multi-turn arming remain future extensions.
