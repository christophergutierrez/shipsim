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

From the repository root (preferred on i3 / tiling WMs):

```bash
cargo build -q
./frontend/love/play.sh
```

`play.sh` starts Love and, when `i3-msg` is available, **floats** the window at
1280×800. Bare `love frontend/love` under i3 often tiles into a ~200px strip that
looks like the game “did nothing.”

Direct launch (floating WMs / when you already float windows yourself):

```bash
cargo build -q
love frontend/love
```

**i3 manual fix** if the window is a thin horizontal bar:

- `$mod+Shift+Space` — toggle floating  
- then resize, or:  
  `i3-msg '[class="^love$"] floating enable, resize set 1280 800, move position center'`

The scenario picker loads current TOML scenarios. The client drives the Rust
harness and displays its latest snapshot. Order/stderr files go to
`frontend/love/local/`, not the repo root or system `/tmp`.

Environment overrides:

- `SHIPSIM_BIN`: shipsim executable path
- `SHIPSIM_ROOT`: repository root

### Controls (summary)

| Input | Action |
|---|---|
| ↑/↓ Enter | Scenario picker |
| +/− / panel | Allocate draft; **Enter** or **Allocate** commits |
| P / T / 0–5 / Shift+0–5 | Coast / accel / turn / turn+accel |
| Enter / R / E | Commit fire / ready fire / end turn |
| ? or H | Help · Esc | Back to picker |
| Right-drag / wheel / Ctrl −/= | Pan / zoom / UI scale |

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
