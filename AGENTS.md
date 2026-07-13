# shipsim — agent entry

Read this when working in this repo or when asked to **play**, test, or extend
the game.

## Architecture (one paragraph)

The **engine** (`shipsim_core` + `shipsim` harness) owns all rules and is driven
only through the **NDJSON API**. **UIs** under `frontend/<name>/` are thin
clients of that API. Many UIs are allowed; none may reimplement combat logic;
dropping one client must not break the engine or other clients.

## Where to look

| Need | Doc |
|---|---|
| **Play the game / choose how to drive it** | [`docs/AGENT-PLAY.md`](docs/AGENT-PLAY.md) ← **start here for “play”** |
| **External API (orders, snapshots, CLI)** | [`docs/PROTOCOL.md`](docs/PROTOCOL.md) |
| System boundaries | [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) |
| Rules summary | [`docs/PLAY-V2.md`](docs/PLAY-V2.md) |
| REPL as a human | [`frontend/repl/GAMEPLAY.md`](frontend/repl/GAMEPLAY.md) |
| Frontend isolation | [`frontend/README.md`](frontend/README.md) |
| Batch simulation | [`docs/SIMULATION.md`](docs/SIMULATION.md) |

## Two ways to play

1. **Engine / automated** — API and tests (no interactive UI). Use for smoke,
   regressions, protocol bugs.
2. **UI as a user** — primarily the REPL (`python3 frontend/repl/repl.py …`).
   Use when asked to *play* the game or find UX bugs.

Details, commands, and decision table: **`docs/AGENT-PLAY.md`**.

```bash
cargo build -q
python3 frontend/repl/client.py                              # Mode 1 smoke
(cd frontend/repl && python3 -m unittest discover -s tests)  # Mode 1 suite
python3 frontend/repl/repl.py scenarios/ai.toml              # Mode 2 play
```

## Local scratch (not authoritative)

If `AGENT-LOCAL.md` exists (often gitignored), it only points at live local
notes under `tmp/`. Checked-in docs above always win for architecture and API.
