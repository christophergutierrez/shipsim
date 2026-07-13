# Frontends

Optional clients for shipsim. The **game engine is independent** of everything here.

```
frontend/
  README.md          # this policy
  love/              # Love2D graphical client
  repl/              # Python interactive dev client (playable now)
  tui/               # ratatui terminal client (decision accepted; not implemented)
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
   `SHIPSIM_BIN`). They must not patch `src/`, rewrite scenarios for private wire
   hacks, or add frontend-only crates into the core workspace.
5. **Core must not depend on frontends** — no `include!`, build scripts, or tests
   under `src/` / `tests/` that load files from `frontend/`. Engine tests use
   `scenarios/` and `tests/fixtures/` only.
6. **Adding a client** — create `frontend/<name>/`, put a short README and a
   `.gitignore` that ignores `local/`, keep all untracked mess under that tree.

## Run (examples)

```bash
cargo build -q
python3 frontend/repl/repl.py scenarios/ai.toml          # UI play
python3 frontend/repl/client.py                          # API play smoke
(cd frontend/repl && python3 -m unittest discover -s tests)
love frontend/love
luajit frontend/love/tests/run_all.lua
# frontend/tui/ — see frontend/tui/README.md (ratatui; not built yet)
```

See each client's README for details. TUI decision: `frontend/tui/ADR.md`.

**Engine API:** `docs/PROTOCOL.md`.  
**Play types (UI / API / sim):** `docs/AGENT-PLAY.md`, root `AGENTS.md`.
