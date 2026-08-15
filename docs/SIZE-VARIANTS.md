# Standard yard catalog

The live catalog contains one yard-generated hull for each size tier. Designs
are compiled with `shipsim-yard`; the resulting `cost` field is the only fleet
budget value.

| Size | Catalog class | Yard cost |
|---:|---|---:|
| 1 | `basic_swarm` | 74 |
| 2 | `basic_destroyer` | 98 |
| 3 | `basic_light_cruiser` | 216 |
| 4 | `basic_heavy_cruiser` | 309 |
| 5 | `basic_battleship` | 546 |
| 6 | `basic_dreadnought` | 1322 |
| 7 | `basic_capital` | 3470 |

The retired light/line/heavy stock variants and the separate cost index are no
longer shipped. See [`SHIPYARD.md`](SHIPYARD.md) for compilation commands and
[`BALANCE.md`](BALANCE.md) for current simulation evidence.
