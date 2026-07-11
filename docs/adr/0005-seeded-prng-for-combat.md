# ADR-0005 - Seeded deterministic PRNG for combat dice

Status: Accepted
Date: 2026-07-04
Amends: the historical Slice 1 assumption A4 ("no RNG in slice 1")

## Context

Slice 1 was fully deterministic with no RNG, which let tests assert exact end-states. Slice 2 (D5,
direct-fire combat) uses SFB-style dice: phasers roll dice for damage on a range table, disruptors
roll to-hit by range. That requires randomness, which appears to conflict with the determinism that
made the engine cheaply testable.

## Decision

Introduce randomness only through an explicit **seeded PRNG owned by game state**, never through
ambient/global RNG (no `rand::thread_rng`, no wall clock). The seed is part of the scenario / game
state and is serialized in the snapshot contract. Given the same seed and the same orders, a game is
bit-for-bit reproducible.

- The PRNG is a small, explicit, seedable generator advanced deterministically as dice are rolled.
- Combat tests pin the seed and assert exact outcomes; reproducibility tests assert that two runs
  with the same seed and orders produce identical snapshots.
- The slice-1 determinism-grep gate is amended: `rand`/`rng` may appear, but `thread_rng`,
  `SystemTime`, and `Instant` remain forbidden. The real invariant is *reproducibility*, not
  *absence of randomness*.

## Consequences

- SFB dice authenticity without losing testability or reproducibility.
- The snapshot/state contract grows a seed and a PRNG position; the future frontend and AI can rely
  on reproducible replays.
- Any future nondeterminism (threads, time) is a regression against the reproducibility invariant and
  should be rejected in review.
