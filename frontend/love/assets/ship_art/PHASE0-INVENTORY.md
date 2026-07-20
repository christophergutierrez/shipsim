# Ship Art — Phase 0 Baseline Inventory and Contract Lock

Status: Complete
Plan: `docs/SHIP-ART-IMPLEMENTATION-PLAN.md` (Phase 0)
PRD: `docs/SHIP-ART-PRD.md`
Baseline commit: `1f76ae5` (`Directional fix`)

## 1. Ship Definition Inventory

Inventory produced with Python `tomllib` over `data/ships/*.toml`.
Every definition's internal `id` field equals its catalog key (file stem).

| # | class_id (catalog key) | display name | size tier | tutorial? |
|---|---|---|---|---|
| 1 | battleship_heavy | Battleship (Heavy) | 5 | |
| 2 | battleship_light | Battleship (Light) | 5 | |
| 3 | battleship_line | Battleship (Line) | 5 | |
| 4 | destroyer_heavy | Destroyer (Heavy) | 2 | |
| 5 | destroyer_light | Destroyer (Light) | 2 | |
| 6 | destroyer_line | Destroyer (Line) | 2 | |
| 7 | dreadnought_heavy | Dreadnought (Heavy) | 6 | |
| 8 | dreadnought_light | Dreadnought (Light) | 6 | |
| 9 | dreadnought_line | Dreadnought (Line) | 6 | |
| 10 | escort | Escort | 1 | |
| 11 | fighter_heavy | Fighter (Heavy) | 1 | |
| 12 | fighter_light | Fighter (Light) | 1 | |
| 13 | fighter_line | Fighter (Line) | 1 | |
| 14 | heavy_cruiser | Heavy Cruiser | 2 | |
| 15 | heavy_cruiser_heavy | Heavy Cruiser (Heavy) | 4 | |
| 16 | heavy_cruiser_light | Heavy Cruiser (Light) | 4 | |
| 17 | heavy_cruiser_line | Heavy Cruiser (Line) | 4 | |
| 18 | huge | Huge | 4 | |
| 19 | light_cruiser_heavy | Light Cruiser (Heavy) | 3 | |
| 20 | light_cruiser_light | Light Cruiser (Light) | 3 | |
| 21 | light_cruiser_line | Light Cruiser (Line) | 3 | |
| 22 | starbase | Generic Starbase | 4 | |
| 23 | titan_double | Titan (Double Armament) | 7 | |
| 24 | titan_heavy | Titan (Heavy) | 7 | |
| 25 | titan_light | Titan (Light) | 7 | |
| 26 | titan_line | Titan (Line) | 7 | |
| 27 | tutorial_escort | Escort | 2 | tutorial |
| 28 | tutorial_heavy_cruiser | Heavy Cruiser | 2 | tutorial |

**Total definitions: 28.**

## 2. Duplicate Display Names

Two display names are shared by distinct canonical classes and must never be
used as runtime art keys:

| display name | class_ids |
|---|---|
| Escort | `escort`, `tutorial_escort` |
| Heavy Cruiser | `heavy_cruiser`, `tutorial_heavy_cruiser` |

## 3. Catalog Decision Table

26 primary records (own art) + 2 explicit aliases (borrow base visuals):

| class_id | kind | alias target | notes |
|---|---|---|---|
| battleship_heavy | primary | — | size 5 |
| battleship_light | primary | — | size 5 |
| battleship_line | primary | — | size 5 |
| destroyer_heavy | primary | — | size 2 |
| destroyer_light | primary | — | size 2 |
| destroyer_line | primary | — | size 2 |
| dreadnought_heavy | primary | — | size 6 |
| dreadnought_light | primary | — | size 6 |
| dreadnought_line | primary | — | size 6 |
| escort | primary | — | size 1; pilot hull |
| fighter_heavy | primary | — | size 1 |
| fighter_light | primary | — | size 1 |
| fighter_line | primary | — | size 1 |
| heavy_cruiser | primary | — | size 2; pilot hull |
| heavy_cruiser_heavy | primary | — | size 4 |
| heavy_cruiser_light | primary | — | size 4 |
| heavy_cruiser_line | primary | — | size 4 |
| huge | primary | — | size 4; pilot hull |
| light_cruiser_heavy | primary | — | size 3 |
| light_cruiser_light | primary | — | size 3 |
| light_cruiser_line | primary | — | size 3 |
| starbase | primary | — | size 4; immobile |
| titan_double | primary | — | size 7 |
| titan_heavy | primary | — | size 7 |
| titan_light | primary | — | size 7 |
| titan_line | primary | — | size 7 |
| tutorial_escort | alias | `escort` | tutorial target |
| tutorial_heavy_cruiser | alias | `heavy_cruiser` | tutorial target |

P0 minimum generation calls: 26 primary × 2 states (top-down + portrait) = **52**.

## 4. Frozen Runtime Contracts

These contracts are locked before implementation:

- **Canonical identity is `class_id`.** It is the catalog key (file stem), not
  the numeric scenario-instance `id` and not the display `class` name.
- **Source top-down art points upward** (canonical source orientation = up).
- **Runtime angle** = `geom.facing_angle(facing)` + source-orientation offset.
  It is NOT a bare `facing × 60°`.
- **Board footprint** does not exceed the existing circle-marker footprint in P0.
- **Controller color** remains an underlay or outline; full-sprite tinting is
  avoided so authored color is preserved.
- **Portraits** disappear before any actionable control is clipped at the
  minimum window size (1024×720).
- **Invalid art always falls back** to the geometric marker and emits at most
  one diagnostic per asset.
- **Protocol remains v4.** `class_id` is an additive snapshot field.

## 5. Provisional Asset Limits (pilot-tunable)

These are pilot-tunable values, not release requirements. NorRust's 30 KB
threshold is prior art, not a copied release requirement.

- Authoring resolution: 256×256 PNG (pilot may adjust).
- Per-asset byte ceiling: pilot-tunable (measured at Phase 5).
- Total catalog budget: recorded at Phase 5 pilot gate.

## 6. Provider/Model Verification (deferred)

Per the plan, provider model, quota, terms, and estimated pilot cost are
verified immediately before Phase 5, not earlier. No network or paid generation
occurs before the Phase 4 checkpoint.

## 7. Toolchain Availability

| tool | version |
|---|---|
| LuaJIT | 2.1.1780076327 |
| Love | 11.5 (Mysterious Mysteries) |
| Pillow | 12.2.0 |
| tkinter | available |
| tomllib | available (Python stdlib) |

## 8. Baseline Suite Results (commit 1f76ae5)

| suite | result |
|---|---|
| `cargo test` (root) | green |
| `cargo test --test harness` | 6 passed |
| `cargo test --manifest-path frontend/tui/Cargo.toml` | 160 passed |
| `python3 -m unittest discover -s frontend/repl/tests` (REPL) | 188 passed |
| `luajit frontend/love/tests/run_all.lua` (Love headless) | 92 checks passed |

All baseline suites are green. No pre-existing failures require an exception.
