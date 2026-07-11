# shipsim Love2D frontend

Thin client for Combat Model v2 over the `shipsim` NDJSON harness. All movement and combat rules remain in Rust.

**This directory is the entire Love client.** Code, tests, and session scratch live here only. See `frontend/README.md` for isolation rules. Deleting `frontend/love/` must not break the engine or other clients.

```
frontend/love/
  *.lua            # client modules
  tests/           # headless luajit gates
  local/           # gitignored orders/stderr dumps
  .gitignore
  README.md
```

See `docs/ARCHITECTURE.md`, `docs/PRD.md`, and ADR-0017, ADR-0018, and ADR-0020 under `docs/adr/`.

## Requirements

- Love2D 11.x (`love`)
- Rust toolchain (`cargo`)
- LuaJIT for headless frontend tests (`luajit`)

## Launch

From the repository root:

```bash
cargo build -q
love frontend/love
```

The scenario picker loads current TOML scenarios. The client replays accepted orders through the Rust harness and displays its latest snapshot. Order/stderr files go to `frontend/love/local/`, not the repo root or system `/tmp`.

Environment overrides:

- `SHIPSIM_BIN`: shipsim executable path
- `SHIPSIM_ROOT`: repository root

## Turn flow

1. Allocate ship power among movement, weapon charges, and six shield facings, then confirm each player ship.
2. Move or pass for the active ship during each movement phase.
3. Select charged weapons, a target, and a legal shield facing; commit fire and mark each ship ready.
4. After simultaneous resolution, continue the next movement/firing cycle or end the turn.

Implemented actions are available in the right panel. Ships and targets can be selected on the board. Right-drag pans, the mouse wheel zooms, and `Ctrl` with `-` or `=` changes UI scale.

## Verification

```bash
cargo build -q
luajit frontend/love/tests/run_all.lua
```
