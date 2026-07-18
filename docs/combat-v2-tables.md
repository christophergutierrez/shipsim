# Combat Model v2 — Working Tables

The authoritative values are in `data/rules/default.toml` (ADR-0024). This
document explains the typed algorithms and provides a readable projection of
the current default values. Tune the TOML and update this projection together.

## Max range

| Kind | Max |
|------|-----|
| Beam | 10 |
| Plasma | 14 |
| Torp | 12 |

Individual weapon definitions may declare a shorter `max_range`; that value is
the effective range enforced by the engine. A definition cannot exceed the
kind's rules table max.

## To-hit (d20, success if roll ≤ N)

The table is the **size-2 baseline**. Scale its threshold by the target ship's
explicit size before rolling:

```text
adjusted threshold = round_half_up(table threshold × target size / 2)
range ceiling = min(`combat.accuracy.ceiling_max`,
                    max(table threshold, `combat.accuracy.ceiling_floor`))
adjusted threshold = clamp(adjusted threshold, 1, range ceiling)
```

The range-aware ceiling is never below the size-2 table value, so increasing
target size can raise or retain the threshold but never lower it. At lower
base chances, the ceiling of 15 limits the size multiplier without flattening
the original range curve. No unmodified attack can become an automatic hit.

Catalog fire-control bonuses apply only against exact size-2 targets, after the
target-size adjustment. `titan_light` currently has +12 and `titan_heavy` +10. The
final threshold is capped below the configured die maximum. Other hulls, size-1 fighters, and size-3+
targets receive no implicit attacker-size bonus.

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

### Fleet catalog

Twenty-one hulls (`{tier}_{light|line|heavy}`) with frame-plus-modules `cost`:
see `docs/SIZE-VARIANTS.md`, `docs/BALANCE-COST.md`, and
`data/ship_costs.toml`. Current evidence and limitations are in
`docs/BALANCE.md`.

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

## Provenance

Simulation match records include `rules_fingerprint`. Save documents include an
optional `rules_fingerprint` and refuse replay when it does not match the
scenario's loaded rules. Live snapshots include `rules_id` and
`rules_fingerprint` too (`docs/PROTOCOL.md`). This prevents balance reports and
deterministic replays from silently using different combat data.

The fingerprint (`src/rules.rs`, FNV-1a over the parsed rules data as
canonical JSON) is a content hash of *data*, not of engine code: it changes
when `data/rules/default.toml`'s values change, and is stable across
whitespace/comment/formatting edits to that file. It does **not** change when
`src/combat_tables.rs`'s formulas change with the data held fixed — engine
version or commit identity is a separate provenance concern from rules
identity, and this fingerprint answers only "was the same rules *data* used?".

`Ruleset::builtin()` (the compiled-in copy of `data/rules/default.toml`, used
by tests and by any in-process helper that has no scenario data root) and
`Ruleset::load(data_root)` (the disk copy, used by production scenario
loading) must be the same file in-repo, so both report the same id and
fingerprint in a normal build (`src/rules.rs` tests assert this). Only
scenario loading — never a bare `GameState` constructor — decides which one a
running game actually uses.
