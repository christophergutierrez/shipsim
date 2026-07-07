# ADR-0007 -- 32-impulse movement and the Impulse Movement Chart (Slice 3 / D1, D3)

Status: Accepted
Date: 2026-07-06

## Context

Slice 1 modeled a turn as a container that could subdivide into impulses, but shipped a simplified
"N hexes per turn" movement policy inside it (ADR-0002). The `turn_mode` field has existed in the
ship schema since slice 1, unenforced. This slice replaces the simplified policy with the authentic
SFB heartbeat: a 32-impulse turn driven by the Impulse Movement Chart (IMC), with turn-mode
enforced on facing changes. Energy Allocation (which makes speed energy-driven) remains deferred to
D7 / slice 4.

## Decision

- **Impulse Movement Chart.** The canonical SFB 32-impulse x 32-speed table is encoded as a static
  lookup in a new `src/impulse.rs` module: `moves_on_impulse(speed: u8, impulse: u8) -> bool`.
  Speeds 0-31 are supported; speed 0 never moves. This is the single source of truth for *when* a
  ship moves. A ship's `speed` (a fixed per-ship TOML field, repurposed from `speed_max`) *is* its
  IMC speed. The player does not choose speed per turn this slice (that arrives with D7).
- **Movement input.** The player submits a movement **plot** -- an ordered list of adjacent hexes
  (a path) -- via a new `Plot { ship, path: Vec<Hex> }` order. The system advances the ship one step
  along the path on each impulse the IMC says it moves. The old one-hex `Move` order and the
  standalone `Face` order are removed. Facing is **implied by the path**: each step's direction is
  the ship's facing at that point.
- **Turn-mode enforcement (D3).** A ship with `turn_mode` N must move N hexes straight (in its
  current facing) between facing changes. A "facing change" is any path step whose direction differs
  from the previous step's direction. The plot is **validated at submission time**: the whole plot is
  rejected (no partial mutation) if any turn violates `turn_mode`. Speed-0 ships and plots that never
  turn trivially pass.
- **Turn driver.** A single `RunTurn` order resolves the entire turn -- all 32 impulses -- atomically.
  The player plots all ships, issues `RunTurn`, and the system plays out impulses 1-32 applying
  movement. The old `EndTurn` order is removed. Impulse-by-impulse stepping is a frontend concern,
  deferred.
- **Declare/resolve seam.** `Plot` declares (validates turn-mode + path adjacency, stores the plot);
  `RunTurn` resolves. The ADR-0002 declare/resolve seam is preserved; only the order *shapes* change.

## Consequences

- Movement is now pre-plotted and impulse-driven, matching the SFB "feel." The simplified N-hexes
  policy is gone.
- The `Order` API breaks cleanly: `Move`/`Face` -> `Plot`, `EndTurn` -> `RunTurn`. All existing tests
  and the CLI bin are updated in the same slice. No compatibility shim (two movement code paths
  would be the rewrite ADR-0002 says to avoid).
- The snapshot contract grows: per-ship `speed` and `turn_mode`, and a turn-level `impulse` field
  (0 between turns). The IMC itself is tested as a pure function.
- Speed remains a fixed per-ship field; energy-driven speed (D7) is cleanly deferred.
- Win conditions are checked once, at turn end, after all movement and fire (see ADR-0008).
