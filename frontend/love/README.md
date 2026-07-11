# shipsim Love2D frontend (D8)

Thin client over the `shipsim` JSON harness. **No game rules in Lua.**

See `docs/adr/0017-love2d-json-thin-client.md`,
`docs/adr/0018-d8-harness-soft-errors-and-controller.md`, `docs/ARCHITECTURE.md`, and
`docs/PRD.md`.

## Requirements

- Love2D 11.x (`love`)
- Built `shipsim` binary (`cargo build`)
- LuaJIT for headless tests (`luajit`)

## Launch

From the **repository root**:

```bash
cargo build -q
love frontend/love
```

Overrides:

- `SHIPSIM_BIN` - path to shipsim executable
- `SHIPSIM_ROOT` - repository root (Cargo.toml + scenarios/)

## Play

**Everything important is clickable** on the right panel (and scenario list).

1. **Picker:** click a scenario row (or Up/Down + Enter)
2. **Energy:** right panel Move/Wpn/Shd buttons → **CONFIRM allocate**
3. **Plot:** left-click hexes → **CONFIRM plot**
4. **Weapons:** click weapon buttons, click enemy ship → **FIRE** (or Skip to Resolve)
5. **Resolve:** **RUN TURN**
6. **Ships:** click blue ships on board or player ship buttons in the panel
7. **UI text size:** `Ctrl -` / `Ctrl =` (or Ctrl + mouse wheel)
8. Pan: right-drag · Zoom: mouse wheel · Esc: back to scenarios

Keyboard still works (1–4 phases, `[` `]` move, `,` `.` weapons, `-` `=` shields, Enter confirms).

## Headless tests (agents)

```bash
cargo build -q
luajit frontend/love/tests/run_all.lua
```

Do not gate correctness on opening the Love window.
