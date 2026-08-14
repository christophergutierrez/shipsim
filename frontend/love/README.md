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
./frontend/love/play.sh          # scenario picker
./frontend/love/play.sh yard     # component shipyard
```

From the picker, **Y** (or the Shipyard button) opens the Love yard. Compile
uses `shipsim-yard`; Play starts a duel with the compiled class. **TUI
`--yard` is the current authoring UI** (`docs/SHIPYARD.md`).

Environment: `SHIPSIM_BIN`, `SHIPSIM_ROOT`. Session files under `frontend/love/local/`.

## Controls (protocol v4)

| Input | Action |
|---|---|
| Click ship / roster | Select focused ship |
| **+** / **−** / panel steppers | Allocate draft (movement + systems) |
| Quick-set buttons | Max weapons / balance shields / all engine / clear |
| **Enter** (allocate) | Commit allocate for the active ship |
| **W** | Path: `move_f` |
| **A** / **D** | Path: `move_fl` / `move_fr` |
| **Z** / **X** | Path: `turn_left` / `turn_right` |
| Backspace / Delete | Undo last path action / clear path draft |
| **Enter** / **Commit Path** | Submit draft (requires ≥1 action) |
| **P** / **Hold Position** | Explicit empty path (`actions: []`) |
| ↑ / ↓ | Cycle fireable weapon |
| **Enter** (firing) | Queue shot into the volley draft |
| **R** | **Commit Volley** (empty = hold fire) |
| Board click (firing) | Set fire target |
| **C** | Auto-fit camera |
| **F** | Toggle auto-fit camera |
| PageUp / PageDown / Home / End | Scroll sidebar |
| **?** or **H** | Help |
| **Esc** | Scenario picker |
| Exit / **Q** | Quit |
| Right-drag / wheel / Ctrl −/= | Pan / zoom / UI scale |

## Turn flow

1. **Allocate** — every living ship commits power (motion, weapons, shields).
2. **Path** — every living ship commits one ordered path (or Hold Position).
3. **Volley** — every living ship commits one volley (empty = hold fire).
4. Engine resolves fire and **automatically** advances to the next allocate.

There is no inertial multi-cycle movement and no manual end-turn.

Path drafting is owned by `path_editor.lua` (shared by production input and
headless tests). Legality and cost come from engine `path_preview`. Read-only
previews: `path_preview`, `reach_preview`, `fire_preview`.

## Module map

| Module | Role |
|---|---|
| `main.lua` | Love entry, input, session lifecycle |
| `path_editor.lua` | Path draft seam (append/undo/clear/commit/hold) |
| `orders.lua` | NDJSON order builders only (no requests) |
| `harness.lua` | Live `--stdin` shipsim pipe |
| `scripted_pump.lua` | Scripted/AI stage completion |
| `phases.lua` | allocate / movement / firing |
| `preview.lua` | Format engine preview responses |
| `status_fmt.lua` | Status strip wording + lifecycle |
| `draw_board.lua` / `draw_hud.lua` | Board + sidebar rendering |
| `selection.lua` | Focus / weapon cycle helpers |
| `allocation.lua` | Allocate draft budget helpers |
| `input_policy.lua` | Key → intent helpers |
| `tutorial.lua` | Rear-attack coach (28 Love steps) |
| `events.lua` / `fx.lua` / `slide.lua` | Combat log ring + resolution theater |
| `layout.lua` / `ui.lua` / `ui_status.lua` | Layout, widgets, status |
| `camera.lua` / `settings.lua` / `toast.lua` | Camera, persistence, toasts |
| `debounce.lua` | Coalesce `reach_preview` traffic |
| `geom.lua` / `hex.lua` | Geometry + hex math |
| `subprocess.lua` / `paths.lua` / `json.lua` | Process I/O, paths, JSON |
| `end_condition.lua` | Win/lose presentation |
| `ship_art.lua` / `ship_art_runtime.lua` | Sprite lookup by snapshot `class_id` |

## Ship art (associate images with ship TOMLs)

Play uses snapshot `class_id` (the `data/ships/<class_id>.toml` stem). Drop
images and rebuild the runtime manifest — no generator required:

```bash
# board sprite (source facing up); optional portrait for the HUD
mkdir -p frontend/love/assets/ship_art/destroyer_line
# place top_down.png (and optional portrait.png) in that directory

python3 frontend/love/tools/ship_art_catalog.py --register-images
```

Details: [`assets/ship_art/README.md`](assets/ship_art/README.md).

## Docs

| Doc | Status |
|---|---|
| **This README** | Live |
| `assets/ship_art/README.md` | Image ↔ `class_id` association |
| `FIX-PLAN.md` | Landed F1–F4 history; suite note current |
| `REVIEW-FINDINGS.md` | Hygiene review + residual notes |
| `UPGRADE-PLAN.md`, `UPGRADE-LOG.md`, `HANDOFF-REVIEW.md`, `REVIEW-VERDICT.md` | **SUPERSEDED** (pre-v4 cycle) |

## Verification

```bash
cargo build -q
luajit frontend/love/tests/run_all.lua  # headless LuaJIT; stock Lua is unsupported
```
