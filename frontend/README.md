# Frontends

Optional clients for shipsim. The **game engine is independent** of everything here.

```
frontend/
  README.md          # this policy
  love/              # Love2D graphical client (protocol v4)
  repl/              # Python interactive dev client — agent reference (protocol v4)
  tui/               # ratatui terminal client — Small tier (protocol v4)
  <other>/           # future clients go here
```

## Isolation rules

1. **One directory tree per client** — `frontend/<name>/` owns all of that client's
   code, tests, docs, helpers, and scratch. Dropping a client means deleting that
   tree only.
2. **Scratch stays local** — session logs, order dumps, stderr captures, saves for
   UI experiments: `frontend/<name>/local/` (gitignored). Never write frontend
   scratch to the repo root, repo `/tmp`, or system `/tmp`.
3. **No cross-client imports** — `love/` does not require `repl/` and vice versa.
   Shared rules live in Rust, not in a third frontend.
4. **Engine boundary only** — clients talk to shipsim via the public NDJSON protocol
   (`docs/PROTOCOL.md`) by spawning `target/{debug,release}/shipsim` (or
   `SHIPSIM_BIN`). The Rust TUI may additionally use `shipsim_core` as a pure,
   deterministic **read-only projection** for presentation and shipyard views;
   it must still spawn the harness for play and must not decide legality or
   mutate `GameState`. REPL and Love remain wire-only. Clients must not patch
   `src/`, rewrite scenarios for private wire hacks, or add frontend-only crates
   into the core workspace.
5. **Core must not depend on frontends** — no `include!`, build scripts, or tests
   under `src/` / `tests/` that load files from `frontend/`. Engine tests use
   `scenarios/` and `tests/fixtures/` only.
6. **Adding a client** — create `frontend/<name>/`, put a short README and a
   `.gitignore` that ignores `local/`, keep all untracked mess under that tree.

## Launch (from repo root)

Build the engine first (`cargo build -q`). All clients need
`target/debug/shipsim` (or `SHIPSIM_BIN`).

| Client | Command | Needs |
|---|---|---|
| **REPL** | `python3 frontend/repl/repl.py scenarios/battle.toml` | Python 3 |
| **TUI** | `cargo run --manifest-path frontend/tui/Cargo.toml` | Rust |
| **Love2D** | `./frontend/love/play.sh` | Love2D 11.x + display |

```bash
cargo build -q

# REPL — text UI (agent reference)
python3 frontend/repl/repl.py scenarios/battle.toml

# TUI — ratatui
cargo run --manifest-path frontend/tui/Cargo.toml

# Love2D — graphical (prefers play.sh for window sizing)
./frontend/love/play.sh
# or: love frontend/love

# Headless checks
python3 frontend/repl/client.py
(cd frontend/repl && python3 -m unittest discover -s tests)
cargo test --manifest-path frontend/tui/Cargo.toml
luajit frontend/love/tests/run_all.lua
```

Batch simulation is **not** a frontend; use `shipsim-sim` from the repo root
(see root [`README.md`](../README.md) and [`docs/SIMULATION.md`](../docs/SIMULATION.md)).

See each client's README for flags, controls, and tests. TUI decision:
`frontend/tui/ADR.md`.

**Engine API:** `docs/PROTOCOL.md`.  
**Play types (UI / API / sim):** `docs/AGENT-PLAY.md`, root `AGENTS.md`.  
**Human quick start:** root [`README.md`](../README.md).
