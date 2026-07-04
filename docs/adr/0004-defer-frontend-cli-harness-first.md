# ADR-0004 — Defer the graphical frontend; Phase 1 is a CLI/JSON harness

Status: Accepted
Date: 2026-07-04

## Context

NorRust pairs its core with a Love2D frontend. shipsim will eventually want a graphical frontend,
but committing to one now (Love2D vs. Godot vs. web) would be a premature decision before the core
even plays, and would slow the first slice.

## Decision

Phase 1 ships **no graphical frontend**. The core is driven by a small **CLI/JSON harness**: a
`shipsim` binary that loads a scenario, applies orders (scripted orders file and/or JSON on stdin),
and prints state snapshots as JSON. This is also the deterministic driver for headless tests.

The graphical frontend (Love2D or otherwise) is chosen in a later phase, once the core is playable
through the harness. Because rules live entirely in `shipsim_core` behind a JSON/C-ABI boundary
(ADR-0001), that later choice does not touch game logic.

## Consequences

- Fastest path to a playable, testable core.
- The JSON snapshot / order API is exercised from day one (by the harness and tests), so the eventual
  frontend has a proven contract to render against.
- No sunk cost in a rendering stack that might be reconsidered.
