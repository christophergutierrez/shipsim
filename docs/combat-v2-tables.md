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

The table is the **size-2 baseline**. Scale its threshold by the target ship's
explicit size before rolling:

```text
adjusted threshold = round_half_up(table threshold × target size / 2)
adjusted threshold = clamp(adjusted threshold, 1, 20)
```

Lever #6 trials (softer mults for titans) capital-stomped abc claims A/B at
n=1k; **classic `size/2` is the locked curve** after levers #1–#5.

| Size | Name | mult |
|---:|---|---:|
| 1 | Fighter | ×0.5 |
| 2 | Destroyer | ×1 (baseline) |
| 3 | Light Cruiser | ×1.5 |
| 4 | Heavy Cruiser | ×2 |
| 5 | Battleship | ×2.5 |
| 6 | Dreadnought | ×3 |
| 7 | Titan | ×3.5 |

Target size changes accuracy, not damage. Named tiers: `data/sizes.toml`.

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

## Target sizes

Canonical hull-size ladder: **`data/sizes.toml`** (seven tiers, `size` 1..=7).
Combat baseline remains **size 2** (Destroyer): multiplier = `size / 2`.

| Size | Name | FASA Class | Hit-threshold multiplier |
|---:|---|---|---:|
| 1 | Fighter | I–III | ×0.5 |
| 2 | Destroyer | IV–VI | ×1 (baseline) |
| 3 | Light Cruiser | VII–IX | ×1.5 |
| 4 | Heavy Cruiser | X–XII | ×2 |
| 5 | Battleship | XIII–XV | ×2.5 |
| 6 | Dreadnought | XVI–XVIII | ×3 |
| 7 | Titan | XIX–XX | ×3.5 |

### Draft size variants

Twenty-one hulls (`{tier}_{light|line|heavy}`) with JSONL-ratioed `cost`:
see `docs/SIZE-VARIANTS.md` and `data/ship_costs.toml`.

### Legacy scenario hulls (provisional)

Older `data/ships/*.toml` values predate the ladder; kept for existing scenarios.

| Class TOML | Current `size` | Ladder intent |
|---|---:|---|
| escort | 1 | prefer `destroyer_*` (size 2) |
| heavy_cruiser | 2 | prefer `heavy_cruiser_*` (size 4) |
| huge | 4 | Battleship / Dreadnought variants |
| starbase | 4 | capital silhouette; immobile engine |

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
