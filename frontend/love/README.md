# shipsim Love2D frontend

Thin client for protocol **v4** simultaneous turns over the `shipsim` NDJSON
harness. All movement and combat rules remain in Rust.

**This directory is the entire Love client.** Code, tests, and session scratch
live here only. See `frontend/README.md` for isolation rules.

## Requirements

- Love2D 11.x (`love`)
- Rust toolchain (`cargo`)
- LuaJIT for headless frontend tests (`luajit`)

## Launch

```bash
cargo build -q
./frontend/love/play.sh
```

Environment: `SHIPSIM_BIN`, `SHIPSIM_ROOT`. Session files under `frontend/love/local/`.

## Controls (protocol v4)

| Input | Action |
|---|---|
| +/− / panel / Enter | Allocate draft; **Allocate** / Enter commits |
| **W** | Path: `move_f` |
| **A** / **D** | Path: `move_fl` / `move_fr` |
| **Z** / **X** | Path: `turn_left` / `turn_right` |
| Backspace / Delete | Undo last path action / clear path draft |
| **Enter** / **Commit Path** | Submit draft (requires ≥1 action) |
| **P** / **Hold Position** | Explicit empty path (`actions: []`) |
| ↑/↓ / Enter / **R** | Weapon cycle / queue shot / **Commit Volley** |
| ? or H | Help |
| Esc | Scenario picker |
| Exit / Q | Quit |
| Right-drag / wheel / Ctrl −/= | Pan / zoom / UI scale |

## Turn flow

1. **Allocate** — every living ship commits power (motion, weapons, shields).
2. **Path** — every living ship commits one ordered path (or Hold Position).
3. **Volley** — every living ship commits one volley (empty = hold fire).
4. Engine resolves fire and **automatically** advances to the next allocate.

There is no inertial multi-cycle movement and no manual end-turn.

Path drafting is owned by `path_editor.lua` (shared by production input and
headless tests). Legality and cost come from engine `path_preview`.

## Verification

```bash
cargo build -q
luajit frontend/love/tests/run_all.lua
```
