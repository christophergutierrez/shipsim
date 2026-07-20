# ADR-0006 - Direct-fire combat model (Slice 2 / D5)

Status: Superseded (historical; see ADR-0020 / ADR-0025 for current combat/turns)
Date: 2026-07-04

## Context

Slice 2 adds the first combat: direct-fire weapons, shields, damage, and a destruction win
condition. SFB's full combat (seeking weapons, itemized damage allocation, energy-armed weapons) is
enormous, so this slice takes the authentic *shape* while bounding the scope.

## Decision

- **Weapons (data-driven).** Ships carry weapons defined in TOML: a type (phaser, disruptor), a
  firing **arc** (e.g. FA forward, RA rear, 360), and the parameters its resolution needs. Phasers
  roll dice for damage on a range table; disruptors roll to-hit by range. Resolution uses SFB-style
  tables and the seeded PRNG (ADR-0005).
- **Firing.** A new order type: fire(weapon, target). To-hit requires the bearing to the target to
  fall within the weapon's arc and within range. Firing uses the declare-order -> resolve-order seam
  (D2) already in place. Weapons **fire freely** this slice: no energy/arming cost (that arrives with
  D7 Energy Allocation) - logged assumption.
- **Shields.** Each ship has **6 shield facings**, one per hex direction. Incoming damage hits the
  shield facing on the bearing toward the attacker; it depletes that facing, and overflow **bleeds
  through to internals**.
- **Internals.** A **single internal-structure pool**. When it reaches 0 the ship is **destroyed**.
  Itemized SSD systems (weapons/engines knocked out by a damage-allocation chart) are deferred to D6.
- **Terminal.** Scenario status gains a **destruction win**: status becomes `Won` when the enemy ship
  is destroyed. (Slice-1's objective-hex terminal remains available for movement scenarios; a combat
  scenario declares the destruction terminal.)

## Consequences

- Facing now matters for both defense (which shield is hit) and offense (whether a target is in arc),
  paying off the maneuver system from slice 1.
- The snapshot contract grows: per-ship shields (6), internal structure, weapon state, and the seed.
- Deferred-but-adjacent (tracked in ROADMAP): itemized damage allocation / destroyable systems (D6),
  seeking/other weapon classes, energy arming (D7), and richer arcs/reinforcement.
