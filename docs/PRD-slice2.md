# PRD - shipsim Slice 2: Direct-fire Combat (D5)

Source alignment: `docs/CONTEXT-slice2.md`, `docs/adr/0005` (seeded PRNG), `docs/adr/0006` (combat
model). Builds on slice 1 (committed). Pipeline stage: post-grill to-prd. Autonomy: Checkpoint.
Execution policy: cost_optimized.

## Problem Statement

Slice 1 gave us ships that move on a hex map, but they cannot fight. The maintainer wants the
movement skeleton to become a **killable duel**: ships that shoot, shields that absorb by facing,
damage that destroys, and a game that ends when a ship dies. Without combat the maneuver system has
no payoff and the product is not yet playable in any real sense.

## Solution

Add a `combat` capability to the headless `shipsim_core`. Ships gain data-driven weapons (phasers,
disruptors) with firing arcs; a new `fire(weapon, target)` order resolves through the existing
declare/resolve seam. To-hit and damage use SFB-style tables driven by a **seeded PRNG** so runs stay
reproducible. Each ship has **6 shield facings** that absorb damage on the bearing toward the
attacker and **bleed through** to a single internal-structure pool; at 0 structure the ship is
**destroyed**, and the scenario resolves `Won`. Correctness is proven by seed-pinned headless
`cargo test`.

## User Stories

1. As a player, I want my ship to carry weapons defined in its TOML, so that armament is data-driven.
2. As a player, I want to issue a fire order naming a weapon and a target ship, so that I can attack.
3. As a player, I want a weapon to fire only if the target is within its firing arc, so that facing
   matters for offense.
4. As a player, I want a weapon to fire only if the target is within range, so that positioning
   matters.
5. As a player, I want phasers to roll damage on a range-based table, so that closer shots hurt more
   in the SFB style.
6. As a player, I want disruptors to roll to-hit by range, so that they can miss at long range.
7. As a player, I want incoming damage to hit the shield facing toward the attacker, so that which
   way I face changes what gets hit.
8. As a player, I want damage beyond a shield facing's strength to bleed through to internals, so
   that sustained fire eventually gets through.
9. As a player, I want my ship destroyed when its internal structure reaches 0, so that combat has a
   decisive outcome.
10. As a player, I want the scenario to resolve `Won` when the enemy ship is destroyed, so that there
    is a clear combat victory.
11. As a player, I want an illegal fire order (out of arc, out of range, no such weapon/target,
    firing at self) rejected with a clear typed reason and no state change, so that state stays valid.
12. As a player, I want the JSON snapshot to show each ship's 6 shield facings, internal structure,
    and weapon state, so that I can see the combat situation (and render it later).
13. As a player, I want a combat scenario (two armed ships) loadable from TOML, so that a duel is
    data-defined.
14. As a developer, I want all dice to come from a seeded PRNG owned by game state, so that a game is
    reproducible from its seed and orders (ADR-0005).
15. As a developer, I want the seed serialized in the snapshot/state, so that replays and tests are
    exact.
16. As a developer, I want combat resolution to reuse the declare-order to resolve-order seam, so
    that simultaneous fire resolution (D2) can be added later without inverting control flow.
17. As a developer, I want weapons to fire without an energy/arming cost this slice, so that combat
    is decoupled from the deferred Energy Allocation system (D7).
18. As a developer, I want the internal-structure model to be a single pool (not itemized systems),
    so that the slice stays bounded; itemized SSD is D6.
19. As a developer, I want combat rules to live entirely in `shipsim_core`, so that the CLI harness
    stays a thin driver (ADR-0001).
20. As a developer, I want the headline acceptance test to pin a seed, fire until the enemy is
    destroyed, and assert `status == Won`, so that the slice gate is falsifiable and exact.
21. As a developer, I want ship/weapon data to stay generic (no ADB-trademarked names/values), so
    that no trademarked content is shipped (ADR-0003).
22. As a maintainer, I want combat's deferred pieces (itemized damage D6, seeking weapons D5a, energy
    arming D7) tracked in ROADMAP, so nothing is lost.

## Implementation Decisions

- **Modules.** Add a `combat` module (weapon definitions, to-hit, damage, shield resolution) to
  `shipsim_core`; extend `ship` with weapons, 6 shield facings, and internal structure; extend
  `schema`/`loader` for weapon TOML; extend `snapshot` for the new state; add a seeded PRNG to
  `game_state`.
- **Weapon schema (TOML).** Each weapon: type (phaser | disruptor), firing arc (e.g. FA, RA, LS, RS,
  360 - a set of allowed hex bearings), max range, and its resolution parameters (phaser dice count;
  disruptor to-hit-by-range and damage). Values are generic (ADR-0003).
- **Fire order.** New order variant fire { weapon_id, target_ship_id }. Validated: weapon exists on
  the acting ship, target exists and is not self, target in arc, target in range. Invalid -> typed
  `OrderError`, no mutation. Resolves via declare -> resolve (D2 hook).
- **Resolution order.** Fire orders declared within a turn resolve one at a time in a deterministic
  order (e.g. by ship id), never simultaneously (D2 deferred). A deterministic order is required by
  the reproducibility invariant (ADR-0005): it fixes the PRNG draw sequence so the same seed and the
  same orders always produce the same snapshot. A ship whose structure reaches 0 mid-resolution is
  destroyed at that point and resolves no further fire order.
- **To-hit / damage.** SFB-style tables via the seeded PRNG (ADR-0005). Phaser: roll dice, damage from
  a range table. Disruptor: roll to-hit vs a by-range threshold; on hit, apply damage.
- **Shields.** 6 facings per ship indexed to hex directions 0-5. The hit facing is the bearing from
  the target ship to the attacker (same hex-direction math as movement facing). Damage depletes that
  facing; overflow bleeds to internals in the same resolution step.
- **Internals / destruction.** Single `structure` pool; at <= 0 the ship enters destroyed state and is
  removed from play / marked destroyed in the snapshot.
- **Terminal.** Scenario status gains a destruction win: `Won` when the (enemy) ship is destroyed.
  Slice-1's objective-hex terminal remains for movement scenarios; a scenario declares which terminal
  applies.
- **PRNG.** Small explicit seedable generator in game state; seed from scenario TOML (default fixed);
  advanced deterministically per roll; serialized in snapshot. No `thread_rng`, no clock.
- **Determinism gate (amended).** `thread_rng`/`SystemTime`/`Instant` remain forbidden; seeded `rand`
  usage is allowed. The invariant is reproducibility (ADR-0005).

## Testing Decisions

- **Same single seam as slice 1:** the scenario-run surface of `shipsim_core` (load scenario, apply
  orders incl. fire, read JSON snapshot and status). Tests assert external behavior, never combat
  internals.
- **Headline acceptance gate (falsifiable, seed-pinned):** load a combat scenario, fire until the
  enemy's structure reaches 0, assert snapshot `status == Won` and the enemy marked destroyed. Must
  be demonstrably RED before combat exists (baseline: fire order / combat scenario absent).
- **Behavioral unit tests at the seam:** out-of-arc fire rejected; out-of-range fire rejected;
  fire-at-self rejected; damage lands on the correct shield facing given attacker bearing;
  bleed-through reduces structure only after the facing is depleted; a shielded hit that does not
  overflow leaves structure unchanged; phaser damage matches the range table for a pinned seed;
  disruptor miss vs hit for a pinned seed; **reproducibility**: same seed + same orders -> identical
  final snapshot.
- **No-mutation-on-rejection:** an invalid fire order leaves the snapshot byte-identical.
- **Prior art:** slice-1 `tests/{acceptance,movement,harness,tracer}.rs` - same construct/drive/assert
  headless pattern; combat tests mirror it and pin the seed for exactness.

## Out of Scope

Itemized damage allocation / destroyable systems (D6); seeking weapons and other weapon classes,
reinforcement, richer arc rules (D5a); energy arming and the Energy Allocation Form (D7); 32-impulse
firing windows (D1); simultaneous fire resolution (D2 - only the seam is reused, not simultaneous
resolution); multi-ship / fleet combat beyond 1v1 (D10). All tracked in ROADMAP.

## Further Notes

- Combat is the slice that makes the game playable; keep the SFB *shape* (arcs, 6-facing shields,
  dice) while holding the line on the single-structure-pool and fire-freely simplifications.
- The snapshot contract grows meaningfully (shields x6, structure, weapon state, seed); treat its
  shape as a deliberate interface for the future frontend and AI.

## Assumptions and Edge-case Decisions

Assumptions (from CONTEXT-slice2, low-risk):
1. One shot per weapon per turn; no refire within a turn (ties to D1 impulses).
2. Range = hex distance between firing and target ship.
3. Arc check and shield-facing selection use the same hex-direction math (0-5) as movement facing.
4. Overflow past a shield facing bleeds to internals in the same step (no partial carry-over rules).
5. 1v1 duel; single target per fire order; no friendly fire.
6. Weapons fire freely (no energy/arming cost) - amended when D7 lands.
7. Seed comes from the scenario TOML with a fixed default; tests pin it.

Edge-case decisions (resolved during the spec audit; each follows from the ADRs / CONTEXT-slice2, so
none are left for a human):
- Tie / mutual destruction (was Q1): fire orders resolve one at a time in a deterministic order (see
  the resolution-order decision under Implementation Decisions), not simultaneously (D2 deferred). A
  ship whose structure reaches 0 is destroyed at that point and resolves no later fire order, so
  mutual destruction is unreachable this slice and no tie rule is needed. Revisit when D2
  (simultaneous resolution) lands.
- Shield regeneration (was Q2): a depleted shield facing stays down for the rest of the scenario; no
  per-turn regeneration or reinforcement this slice (reinforcement is deferred with D5a).
- Point-blank / minimum range (was Q3): range is uniform 0..=max with no special point-blank rule;
  the phaser range table and the disruptor by-range thresholds cover close range directly.
