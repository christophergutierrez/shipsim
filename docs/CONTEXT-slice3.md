# shipsim - Slice 3 CONTEXT (Movement fidelity / D1, D2, D3)

Point-in-time alignment for slice 3. Builds on slices 1–2 (movement skeleton + direct-fire combat,
committed). Durable decisions are in ADR-0007 and ADR-0008; deferred work is in docs/ROADMAP.md.

## Scope (decided)

Replace the simplified N-hexes/turn movement with the authentic SFB movement feel: a 32-impulse
turn driven by the Impulse Movement Chart, simultaneous per-impulse resolution, and turn-mode
enforcement. Combat is unchanged.

In scope:
- The canonical SFB 32-impulse × 32-speed Impulse Movement Chart as a static lookup in
  `src/impulse.rs` (`moves_on_impulse(speed, impulse) -> bool`).
- Ship `speed` (repurposed from `speed_max`) as the IMC speed; fixed per-ship, no Energy Allocation.
- A `Plot { ship, path: Vec<Hex> }` order replacing `Move`/`Face`; facing implied by the path.
- Turn-mode enforcement at plot submission: reject the whole plot if any facing change violates
  `turn_mode` (N straight hexes between turns).
- A `RunTurn` order replacing `EndTurn`: resolves all 32 impulses atomically.
- Simultaneous per-impulse resolution: collect all ships' next steps, apply atomically; same-hex
  collision = both stop (no move that impulse).
- Fire declared per-turn, resolved after all movement at turn end (mechanics unchanged from slice 2).
- Scripted ship auto-generates a turn-mode-valid plot from waypoints each turn.
- A new `scenarios/impulse.toml` headline scenario; existing scenarios updated to `Plot`.
- Snapshot gains `impulse` (turn-level, 0 between turns) and per-ship `speed`/`turn_mode`.

## Key decisions

- Speed is a fixed per-ship TOML field = IMC speed (ADR-0007). Energy-driven speed deferred to D7.
- Movement is pre-plotted: player submits a path, system advances one step per move-impulse
  (ADR-0007). Facing is implied by the path; no standalone Face order.
- Turn-mode is a plot-submission validation, not an impulse-time rule (ADR-0007).
- Resolution is simultaneous per impulse with a deterministic "both stop" collision rule
  (ADR-0008). No ship-order tie-break.
- Fire resolves at turn end, after all movement (ADR-0008). Combat mechanics unchanged.
- Win conditions checked once at turn end (ADR-0008).
- The `Order` API breaks cleanly: `Move`/`Face` → `Plot`, `EndTurn` → `RunTurn`. No shim. All
  existing tests and the CLI bin updated in-slice. The declare/resolve seam (ADR-0002) is preserved.

## Logged assumptions (low-risk; revisit if wrong)

1. The IMC is the canonical SFB table (speeds 0–31, impulses 1–32). It is a movement schedule, not
   trademarked content.
2. A plot's path is a sequence of mutually adjacent hexes; the first step must be adjacent to the
   ship's current hex.
3. A ship moves at most one hex per move-impulse (one step along the path), regardless of speed.
4. Turn-mode counts hexes moved straight since the last facing change; the ship's initial facing at
   turn start is the facing implied by the first path step's predecessor (its current facing).
5. A collision only considers the destination hex of a step; pass-through (two ships swapping
   adjacent hexes) is allowed this slice.
6. Fire is resolved in declaration order at turn end; simultaneous fire (D2-fire) is deferred.
7. The scripted ship's auto-plot is greedy toward the next waypoint and never intentionally collides;
   if its greedy plot would collide, the collision rule handles it at impulse time.

## Deferred (tracked in ROADMAP.md)

Energy Allocation and energy-driven speed (D7), impulse-gated fire windows (D1-fire), simultaneous
fire resolution (D2-fire), itemized damage allocation / destroyable systems (D6), seeking weapons,
pass-through/overlap movement rules, reinforcement and advanced arcs.
