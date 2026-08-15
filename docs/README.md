# Documentation map

Tracked docs describe **what the system is**. Plans, findings, and dated
working notes live under `tmp/` — see [`DOC-LIFECYCLE.md`](DOC-LIFECYCLE.md).

Start here, then pick a group. Root [`README.md`](../README.md) is the human
launch table; [`AGENTS.md`](../AGENTS.md) is the short agent entry.

## Hot path (stay at `docs/` root)

| Doc | Role |
|---|---|
| [`PROTOCOL.md`](PROTOCOL.md) | NDJSON API (v4) |
| [`PLAY-V2.md`](PLAY-V2.md) | Rules summary |
| [`AGENT-PLAY.md`](AGENT-PLAY.md) | UI / API / sim play procedures |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | System boundaries |
| [`SHIPYARD.md`](SHIPYARD.md) | Component yard, compile, catalog |
| [`TODO.md`](TODO.md) | Living open work (includes former roadmap) |
| [`PRD.md`](PRD.md) | Why the product exists |
| [`SAVE-FORMAT.md`](SAVE-FORMAT.md) | Save / resume format |

## Balance and simulation

| Doc | Role |
|---|---|
| [`BALANCE.md`](BALANCE.md) | Current claim status (must stay maintained) |
| [`BALANCE-PROTOCOL.md`](BALANCE-PROTOCOL.md) | How evidence is gathered |
| [`BALANCE-COST.md`](BALANCE-COST.md) | Construction-cost model |
| [`SIMULATION.md`](SIMULATION.md) | `shipsim-sim` reference |
| [`SIM-RUBRICS.md`](SIM-RUBRICS.md) | Sim-suite TOML checks (not the UI rubric) |
| [`combat-v2-tables.md`](combat-v2-tables.md) | Combat constants |
| [`SIZE-VARIANTS.md`](SIZE-VARIANTS.md) | Size ladder and cost method |

## UI judgment

| Doc | Role |
|---|---|
| [`UI-RUBRIC.md`](UI-RUBRIC.md) | How we judge a screen |
| [`UI-PLAYTEST-PROTOCOL.md`](UI-PLAYTEST-PROTOCOL.md) | How to run a blind playtest |

## Decisions and archive

| Path | Role |
|---|---|
| [`adr/`](adr/) | Architecture decision records |
| [`history/`](history/) | Deliberate historical campaigns |

Love2D ship-art **runtime** is [`frontend/love/assets/ship_art/README.md`](../frontend/love/assets/ship_art/README.md). The authoring PRD sits next to those assets, not here.

Per-client play: [`frontend/repl/GAMEPLAY.md`](../frontend/repl/GAMEPLAY.md), [`frontend/tui/README.md`](../frontend/tui/README.md), [`frontend/love/README.md`](../frontend/love/README.md).
