# Size-ladder design intent (historical)

Status: **historical**. Live rules are [`../PLAY-V2.md`](../PLAY-V2.md) and
[`../PROTOCOL.md`](../PROTOCOL.md). Live catalog:
[`../SIZE-VARIANTS.md`](../SIZE-VARIANTS.md). Open work: [`../TODO.md`](../TODO.md).

This is the design essay that used to sit in `TODO.md`. It is not the v4
path loop.

## FASA class map (accepted)

~3 FASA classes per shipsim tier. Source role labels (Scout, Frigate, …) are
not size IDs. Combat baseline remains size **2** until an explicit retune.
See [`../combat-v2-tables.md`](../combat-v2-tables.md).

| `size` | Name | FASA Class map |
|---:|---|---|
| 1 | Fighter | I–III |
| 2 | Destroyer | IV–VI |
| 3 | Light Cruiser | VII–IX |
| 4 | Heavy Cruiser | X–XII |
| 5 | Battleship | XIII–XV |
| 6 | Dreadnought | XVI–XVIII |
| 7 | Titan | XIX–XX |

## Goal

Hull size should be a strategic tradeoff, not a linear power upgrade:
larger ships get more space, weapons, systems, shields, and hull, and pay
in thrust and responsiveness. Smaller ships should keep fleet roles through
movement efficiency and coverage.

## Candidate fields (then)

`size` (1–7), `power`, `movement_cost`, `system_capacity`, `hull`,
`max_shield_per_facing`, optional agility. Construction: per-size space/cost
loaded by the yard, not by combat scenario load for the stock catalog.

Do not derive every statistic from `size` until simulation evidence supports
stable formulas.

## Candidate rules (then)

```text
movement points = movement power / movement_cost
actual move cost = base movement cost * momentum modifier
```

Initiative was to use effective movement capability, not raw reactor power.

## Evidence that was required

Movement power per hex by size; initiative frequency; turns and range at
first contact; spend ratios; flanking; mirrored win rates; cost-normalized
win rates; stalemate and immobile-large-ship rates.

Red conditions: largest-hull dominant strategy; small ships dying before
mobility matters; movement prohibitively expensive on large hulls; small
ships maxing every subsystem; raw reactor power determining initiative.
