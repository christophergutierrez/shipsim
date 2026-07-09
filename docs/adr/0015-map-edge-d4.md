# ADR-0015 -- Map edge policy (D4)

Status: Accepted
Date: 2026-07-09

## Decision

- Scenario TOML `map_mode`: `"hard"` (default) or `"floating"`.
- **Hard:** off-map hexes illegal at plot validation (existing behavior).
- **Floating:** plots may leave the rectangle; after each impulse movement (+ seeking step),
  the board **recenters** by translating all ship and munition positions so the fleet's
  axial bounding box is shifted into the map if it fits. If the formation is larger than the
  map, positions are clamped onto the board edge (ships remain playable).

## Consequences

- Fights can drift without hard walls when floating is enabled.
- Hard mode keeps small training scenarios simple.
