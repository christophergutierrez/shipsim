# ADR-0019 -- Pivot combat model to FASA STCS / Bocchino RPG rules

Status: Accepted
Date: 2026-07-10

## Context

shipsim was built under ADR-0001/0002 toward SFB-style fidelity: 32-impulse IMC movement,
pre-plotted paths, and impulse-gated fire (D1-fire). The Love client and harness expose that
model. The maintainer's preferred ruleset is instead documented in
`tmp/StarshipCombat.pdf` (Robert L. Bocchino Jr., 2013): FASA Starship Tactical Combat Simulator
adapted for RPG play -- **three rounds per turn**, **action order**, **power spent on actions**,
**immediate fire resolution**, not a 32-impulse tape.

The SFB-like core is a coherent simulation of a different game. Continuing to polish impulse UX
will not produce the preferred feel.

## Decision

1. **Canonical rules source** for turn structure and combat resolution becomes Bocchino §5--§9
   (and FASA STCS tables referenced therein), with generic ship data (ADR-0003) retained.
2. **Supersede** ADR-0002's end-state requirement of 32-impulse IMC + simultaneous pre-plot as the
   product target. Impulse code may remain temporarily as dead/legacy until removed by plan
   milestones; it is not the long-term model.
3. The superseded product spec and execution plan are retained only in local project archives.
3. **Keep** where possible: hex board, six shield facings, SSD/internals ideas, pure Rust core +
   JSON harness + thin Love client, headless tests, generic TOML ships.
4. **Replace**: turn loop (`RunTurn` / IMC / IFF fire queue), order set (plot-then-run), energy
   buckets-as-SFB-EA, AI that only plots/fires under impulse assumptions.
5. Deliver via **vertical slices** with falsifiable gates (Killhouse plan), not a big-bang rewrite
   of every FASA table on day one.

## Consequences

- Large behavioral break: existing scenarios/orders/tests that encode impulse semantics must be
  rewritten or retired.
- Love frontend phase model (Energy/Plot/Fire/Run) must change to round/action UI.
- Roadmap D1--D5a impulse/SFB items are historical or residual; new roadmap tracks FASA rounds.
- ADR-0001 (core/frontend split) remains in force.
