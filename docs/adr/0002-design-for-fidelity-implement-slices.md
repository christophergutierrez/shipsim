# ADR-0002 — Design for full SFB fidelity, implement minimal slices

Status: Accepted
Date: 2026-07-04

## Context

Star Fleet Battles is one of the most rules-dense wargames ever published. Its signature
mechanics — a 32-impulse turn driven by the Impulse Movement Chart, simultaneous pre-plotted
movement, and per-turn Energy Allocation — are exactly what make it *feel* like SFB, and exactly
what make a naive full implementation a scope disaster. The maintainer wants the finished game to
include these (32-impulse movement and simultaneous resolution were both explicitly named as
end-state requirements), but wants the first slice to stay minimal.

## Decision

Build minimal slices, but shape the load-bearing abstractions so the high-fidelity mechanics slot
in **without a rewrite**:

- **Turn structure.** Slice 1 uses simplified "up to N hexes per turn" movement, but the turn model
  is expressed as a container that can subdivide into impulses. The eventual 32-impulse chart is a
  policy plugged into that container, not a re-architecture of it.
- **Move resolution.** Slice 1 resolves the two ships **sequentially**, but movement is modeled as
  *declare order → resolve order* so a later slice can collect both ships' declarations for a given
  impulse and resolve them **simultaneously** without inverting the flow.
- **Ship data.** Fields required only by later fidelity (e.g. `turn_mode`) exist in the ship schema
  from the start, unenforced, so enabling the rule is validation logic — not a data migration.

## Consequences

- Slightly more abstraction than a truly minimal skeleton would need (declare/resolve split,
  impulse-capable turn container).
- The full-fidelity slices (impulse chart, simultaneous movement, turn-mode enforcement) become
  additive, not disruptive.
- Reviewers should reject slice-1 shortcuts that would *preclude* the deferred mechanics, while
  still keeping slice 1 minimal in behavior.
