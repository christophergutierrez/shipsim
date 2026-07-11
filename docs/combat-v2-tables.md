# Combat Model v2 — Working Tables

Authoritative for implementation (ADR-0020 / `docs/PRD.md`).
Tune only with an explicit doc change.

## Max range

| Kind | Max |
|------|-----|
| Beam | 10 |
| Plasma | 14 |
| Torp | 12 |

## To-hit (d20, success if roll ≤ N)

| Range | Beam | Plasma | Torp |
|------:|-----:|-------:|-----:|
| 1 | 18 | 16 | 14 |
| 2 | 17 | 14 | 13 |
| 3 | 15 | 12 | 12 |
| 4 | 13 | 10 | 11 |
| 5 | 11 | 8 | 10 |
| 6 | 10 | 6 | 9 |
| 7 | 8 | 5 | 7 |
| 8 | 7 | 4 | 6 |
| 9 | 5 | 3 | 5 |
| 10 | 4 | 2 | 4 |
| 11 | — | 2 | 3 |
| 12 | — | 2 | 3 |
| 13 | — | 1 | — |
| 14 | — | 1 | — |

## Damage

- **Torp:** cost 1, damage **4** (all in-range).
- **Plasma:** cost 1, damage by range r1..14:  
  `8,6,5,4,3,3,2,2,1,1,1,1,1,1`
- **Beam:** cost = charge 1..max; damage = half_up(charge × factor); illegal if &lt; 1.  
  factor r1..10: `2.0, 1.9, 1.7, 1.6, 1.4, 1.3, 1.2, 1.1, 1.0, 1.0`  
  half_up: floor(x + 0.5) for x ≥ 0.

## Ship power MVP

| Class | Power |
|-------|------:|
| Escort | 14 |
| Cruiser | 22 |

## Weapon Mount Arcs (frozen)

Facing 0 is straight ahead. This table is **locked** (not an implementer choice).
Forward-port and forward-starboard can also bear straight ahead; pure Forward is
only relative facing 0.

| Mount | Relative facings |
|-------|------------------|
| Forward | 0 |
| Forward Starboard | 0, 1 |
| Aft Starboard | 2, 3 |
| Aft | 3 |
| Aft Port | 3, 4 |
| Forward Port | 5, 0 |

## Miss and charge

A committed shot that **misses** the d20 still **consumes** that weapon's charge
and marks the weapon **fired for the turn**.
