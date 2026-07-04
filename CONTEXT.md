# shipsim — CONTEXT

> A hex-based tactical starship-combat game *in the spirit of* Star Fleet Battles, built as a
> pure-Rust headless simulation core with a thin, deferred frontend. Designed from day one for
> full-fidelity SFB-style rules, but delivered in minimal vertical slices.

Status: **grilling complete** (Killhouse pipeline, post-grill gate).
Origin: greenfield in `shipsim/`. Architectural template: sibling project **NorRust**
(`../norrust`) — pure-Rust headless core + thin client over a JSON/C-ABI bridge + TOML data +
heavy headless tests.

---

## Vision

A two-player, hex-grid tactical game of starship maneuver and combat, modeled closely on the
*mechanics* of Star Fleet Battles (impulse movement, ship facing, 6-shield-facing combat, energy
allocation) but using **our own generic ship classes and stat values** — not ADB's trademarked
names or published numbers. The end state is high-fidelity SFB-style play; we get there through
small, individually shippable slices, never by boiling the ocean.

## Architecture (decided)

Mirror NorRust's separation, **core-first with the frontend deferred**:

- `shipsim_core/` — pure-Rust headless simulation library. No rendering, no display deps.
  - Rust 2021, `crate-type = ["cdylib", "rlib"]`, `panic = "abort"`.
  - Deps: `serde` + `serde_json` + `toml` + `thiserror` (mirror NorRust).
  - Module shape (target, filled in as slices land): `hex`, `board`, `ship`, `movement`,
    `game_state`, `snapshot`, `save`, `scenario`, `loader`, `schema`, `ffi`, later `combat`,
    `energy`, `ai`.
- **CLI/JSON harness** (Phase 1 frontend) — a small `shipsim` binary that loads a scenario, applies
  orders (from a scripted orders file and/or stdin JSON), and emits state snapshots as JSON.
  Deterministic and test-friendly.
- **Graphical frontend — DEFERRED.** Love2D vs. other decided in a later phase once the core plays.
- Content is **data-driven**: ships and scenarios loaded from TOML at startup (`data/ships/*.toml`,
  `scenarios/*.toml`).
- **Headless `cargo test` is the verification surface.** Every rule is provable without any
  frontend.

## First slice — Movement Skeleton (decided)

The smallest honest vertical slice. **No combat.**

In scope:
- Hex map + hex coordinate math + facing (6 directions).
- Turn structure with a ship "speed" (max hexes/turn).
- Two ships on the map: a **player-plotted** ship and a **scripted** ship.
- A scenario (TOML) defining map size, ship placements, the scripted ship's waypoints, and an
  **objective hex**.
- Terminal condition: **a ship entering the objective hex resolves the scenario as WON** (falsifiable).
- CLI/JSON harness to drive it; headless integration test that plots a path to the objective and
  asserts the scenario resolves `WON`.

Explicitly deferred (but designed-for — see ADR-0002):
- The full 32-impulse turn / Impulse Movement Chart (slice uses simplified N-hexes/turn).
- Simultaneous plot-then-resolve movement (slice uses sequential/alternating resolution).
- SFB turn-mode constraint (slice allows free turning; `turn_mode` field exists, unenforced).
- All combat: phasers, disruptors, shields, damage allocation.
- Energy Allocation.
- AI opponent (slice uses scripted waypoints).

## Decisions log (see docs/adr/ for detail)

- **ADR-0001** — Mirror NorRust: pure-Rust headless core + thin client + JSON/C-ABI bridge + TOML.
- **ADR-0002** — Design for full SFB fidelity, implement minimal slices. Turn-structure and
  move-resolution abstractions must accept the 32-impulse chart and simultaneous resolution later
  without rework.
- **ADR-0003** — Generic, SFB-inspired ship data. No ADB-trademarked names or published stats.
- **ADR-0004** — Frontend deferred; Phase 1 is a CLI/JSON harness. Graphical frontend chosen later.

## Logged assumptions (low-risk defaults; revisit if wrong)

1. **Hex coordinates**: axial storage with cube conversion (standard hex math); flat-top vs
   pointy-top is a frontend concern, core is orientation-agnostic.
2. **Map**: bounded rectangle, dimensions from the scenario TOML, **no wraparound**; moving off-map
   is an illegal order. (SFB's fixed floating map can be revisited when it matters.)
3. **Harness I/O**: core exposes an order-application API (`apply_order` / `step`); the CLI can run a
   scenario to completion from a scripted orders file or accept orders as JSON on stdin.
4. **Determinism**: no RNG in the movement skeleton — fully deterministic, so tests assert exact
   end-states.
5. **Ship "speed" in slice 1**: a fixed per-ship max hexes/turn from the ship TOML (energy-driven
   speed arrives with Energy Allocation).

## Open (later slices, not blocking)

- Impulse chart fidelity (ADR-0002 target), simultaneous movement, turn-mode enforcement.
- Combat model + Energy Allocation (the two big subsequent slices).
- Frontend selection (ADR-0004).
- Multi-ship / scenario & campaign structure beyond 1v1.
