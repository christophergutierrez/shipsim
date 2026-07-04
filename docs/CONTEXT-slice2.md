# shipsim - Slice 2 CONTEXT (Direct-fire combat / D5)

Point-in-time alignment for slice 2. Builds on slice 1 (movement skeleton, committed). Durable
decisions are in ADR-0005 and ADR-0006; deferred work is in docs/ROADMAP.md.

## Scope (decided)

Add the first combat to the headless `shipsim_core`, turning the movement skeleton into a **killable
duel**.

In scope:
- Weapons as data-driven TOML on ships: phasers (dice damage by range table) and disruptors (to-hit
  by range), each with a firing **arc**.
- A `fire(weapon, target)` order, resolved through the existing declare/resolve seam; to-hit requires
  target in arc and in range.
- **6 shield facings** per ship; damage hits the facing toward the attacker, depletes it, then bleeds
  through to a **single internal-structure pool**.
- **Destruction win**: scenario status becomes `Won` when the enemy ship's structure reaches 0.
- **Seeded PRNG** in game state for all dice; reproducible per seed (ADR-0005).
- A combat scenario (TOML) and the headline acceptance gate: a headless `cargo test` that fires until
  the enemy is destroyed and asserts `status == Won` (seed pinned, exact outcome).

## Key decisions

- Combat resolution: SFB-style dice tables via a seeded PRNG (ADR-0005). This **amends slice-1
  assumption A4** (no RNG) - the real invariant is reproducibility, not absence of randomness.
- Win condition: enemy destroyed (ADR-0006). Objective-hex terminal from slice 1 stays available for
  movement scenarios.
- Shields: 6 facings with bleed-through (ADR-0006).
- Firing arcs: data-driven per weapon (ADR-0006).
- Internals: single structure pool; itemized systems deferred to D6.
- Weapons fire freely (no energy/arming cost) this slice; arming arrives with D7.

## Logged assumptions (low-risk; revisit if wrong)

1. One shot per weapon per turn (no impulse subdivision yet; ties to D1). No refire within a turn.
2. Range is hex distance between the firing and target ship.
3. Bearing-to-target for both arc check and shield-facing selection uses the same hex-direction math
   as movement facing (0-5).
4. Damage that exceeds a shield facing's remaining strength bleeds the remainder to internals in the
   same resolution step (no partial carry-over rules).
5. No friendly fire / multi-target complexity: 1v1 duel, single target per fire order.
6. Weapons fire freely (no energy/arming cost) - A-D7.

## Deferred (tracked in ROADMAP.md)

Itemized damage allocation / destroyable systems (D6), seeking and other weapon classes, energy
arming and the Energy Allocation Form (D7), 32-impulse firing windows (D1), simultaneous fire
resolution (D2), reinforcement and advanced arcs.
