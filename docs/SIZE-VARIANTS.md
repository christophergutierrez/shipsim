# Standard yard catalog

The live catalog contains one yard-generated hull for each size tier. Designs
are compiled with `shipsim-yard`; the resulting `cost` field is the only fleet
budget value.

| Size | Catalog class | Yard cost |
|---:|---|---:|
| 1 | `yard_swarm` | 74 |
| 2 | `yard_destroyer` | 98 |
| 3 | `yard_light_cruiser` | 216 |
| 4 | `yard_heavy_cruiser` | 309 |
| 5 | `yard_battleship` | 546 |
| 6 | `yard_dreadnought` | 1322 |
| 7 | `yard_capital` | 3454 |

The retired light/line/heavy stock variants and the separate cost index are no
longer shipped. See [`SHIPYARD.md`](SHIPYARD.md) for compilation commands and
[`BALANCE.md`](BALANCE.md) for current simulation evidence.
