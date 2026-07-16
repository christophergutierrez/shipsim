# shipsim — agent entry

Read this when working in this repo or when asked to **play**, test, or extend
the game.

## Architecture (one paragraph)

The **engine** (`shipsim_core`) owns all rules. External clients drive it via
the **NDJSON API** (`shipsim` harness, `docs/PROTOCOL.md`). **UIs** under
`frontend/<name>/` are thin API clients. Many UIs are allowed; none reimplement
combat; dropping one client must not break the engine or other clients.
**Simulation** (`shipsim-sim`) runs policies in-process for volume measurement.

## Three play types

| Name | Drive | Use when |
|---|---|---|
| **UI play** | Live frontend (REPL first) | “Play the game / play itself”, UX |
| **API play** | NDJSON harness + tests | Smoke, protocol, regressions |
| **Sim play** | `shipsim-sim` in-process | Hundreds–thousands of matches, balance |

Full procedures and decision table: **[`docs/AGENT-PLAY.md`](docs/AGENT-PLAY.md)**  
← **start here for any “play” request.**

```bash
cargo build -q
python3 frontend/repl/repl.py scenarios/ai.toml              # UI play
python3 frontend/repl/client.py                              # API play smoke
(cd frontend/repl && python3 -m unittest discover -s tests)  # API play suite
cargo run --release --bin shipsim-sim -- \
  --suite simulation/suites/smoke.toml                       # sim play
```

Default for “play itself” / “play the game”: **UI play** (REPL), unless the
user asks for tests (**API play**) or mass matches (**sim play**).

An explicit “play a UI game” / “play through the UI” request **must** launch a
frontend and enter decisions through that frontend. Driving NDJSON directly or
only running tests does not satisfy a UI-play request. Conversely, “play via the
API” means drive the NDJSON harness without claiming that as UI coverage.

## Where to look

| Need | Doc |
|---|---|
| **Play types & how-to** | [`docs/AGENT-PLAY.md`](docs/AGENT-PLAY.md) |
| External API | [`docs/PROTOCOL.md`](docs/PROTOCOL.md) |
| Batch simulation | [`docs/SIMULATION.md`](docs/SIMULATION.md) |
| System boundaries | [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) |
| Rules summary | [`docs/PLAY-V2.md`](docs/PLAY-V2.md) |
| Hull size tiers (1–7) | [`data/sizes.toml`](data/sizes.toml), [`docs/combat-v2-tables.md`](docs/combat-v2-tables.md) |
| Ship classes | [`data/ships/`](data/ships/) |
| Size variants + costs | [`docs/SIZE-VARIANTS.md`](docs/SIZE-VARIANTS.md), [`docs/BALANCE-COST.md`](docs/BALANCE-COST.md), [`data/ship_costs.toml`](data/ship_costs.toml) |
| REPL commands (UI play) | [`frontend/repl/GAMEPLAY.md`](frontend/repl/GAMEPLAY.md) |
| Frontend isolation | [`frontend/README.md`](frontend/README.md) |

## Local scratch (not authoritative)

If `AGENT-LOCAL.md` exists (often gitignored), it only points at live local
notes under `tmp/`. Checked-in docs above always win for architecture and API.
