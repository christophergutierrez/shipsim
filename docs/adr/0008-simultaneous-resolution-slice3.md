# ADR-0008 -- Simultaneous plot-then-resolve movement (Slice 3 / D2)

Status: Accepted
Date: 2026-07-06

## Context

Slice 1 resolved the two ships sequentially (ADR-0002). SFB resolves all ships' movement on a given
impulse simultaneously -- no ship gains a positional advantage from being resolved first. The classic
edge case is two ships moving into the same hex on the same impulse. This slice makes resolution
simultaneous, per impulse, inside the 32-impulse loop established by ADR-0007.

## Decision

- **Per-impulse atomic step.** On each impulse, the system collects every ship's next path step
  (only for ships that move on that impulse per the IMC), then applies all steps atomically. No ship
  is resolved before another within an impulse.
- **Collisions.** If two ships end up in the same hex after an impulse's step, that is a collision:
  both ships stop in their pre-step hexes (no movement that impulse for either), and the scenario
  continues. This is deterministic, requires no tie-breaking by ship order, and matches SFB's
  simultaneous intent.
- **Plot-submission occupancy check.** A plot may not pass through a hex currently occupied at plot
  submission time (the slice-1 "occupied hex rejected" rule becomes a plot-submission check). Mid-turn
  collisions (ships converging during the turn) are resolved at impulse time per above.
- **Fire timing.** Fire stays a once-per-turn declaration, resolved at the end of the turn after all
  32 movement impulses complete, in declaration order. Combat mechanics (to-hit, shields, destruction)
  are unchanged from slice 2 (ADR-0006). Bearing and shield-facing for a fire are computed from
  post-movement positions. Impulse-gating fire is a separate deferred item.
- **Scripted ship.** The scripted ship auto-generates a turn-mode-valid plot each turn from its
  waypoints (greedy step toward the next waypoint, up to the hexes its speed entitles it to this turn,
  shortened to the longest turn-mode-valid prefix if needed). No player input required.
- **Terminal check.** Win conditions (objective-hex from slice 1, destruction from slice 2) are
  checked once, at turn end, after all movement and fire. Mid-impulse wins would interact badly with
  simultaneous collisions and mid-turn fire.

## Consequences

- Movement resolution is now simultaneous and collision-aware, paying off the declare/resolve seam
  from slice 1.
- The collision rule is the one non-determinism-adjacent surface; it is made fully deterministic by
  the "both stop" rule (no ship-order tie-break).
- Combat is deliberately untouched: this slice changes only *movement*. Fire timing moves from
  "immediate" to "turn end," but fire mechanics do not change.
- The scripted ship gains plot-generation logic, reusing the existing waypoint machinery.
