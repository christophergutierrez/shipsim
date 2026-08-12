# shipsim

Deterministic, turn-based hex starship combat. Rules live in a Rust engine
(`shipsim_core`); frontends are thin clients over an NDJSON harness. Protocol
version is **4** (allocate → path → volley).

## Requirements

| Piece | Notes |
|---|---|
| **Rust** | Stable toolchain (`cargo`) |
| **Python 3** | REPL client and tools |
| **Love2D 11.x** | Optional; graphical client only (`love` on `PATH`) |
| **LuaJIT** | Optional; Love headless tests |

## Build the engine

From the repo root:

```bash
cargo build -q
# release (smoother for sim batches / TUI):
cargo build --release -q
```

Produces:

- `target/debug/shipsim` (or `release/`) — NDJSON harness used by all UIs  
- `target/debug/shipsim-sim` — in-process batch simulation  

## Play the game

Always run these from the **repository root** so default paths resolve.

### REPL (text UI — simplest)

Python client; good default for agents and quick play.

```bash
cargo build -q
python3 frontend/repl/repl.py scenarios/battle.toml
```

| Variant | Command |
|---|---|
| Tutorial | `python3 frontend/repl/repl.py --tutorial rear-attack` |
| Debug log | `python3 frontend/repl/repl.py scenarios/battle.toml --debug` |
| Commands guide | [`frontend/repl/GAMEPLAY.md`](frontend/repl/GAMEPLAY.md) |
| More flags | [`frontend/repl/README.md`](frontend/repl/README.md) |

### TUI (ratatui terminal UI)

Standalone Rust package; spawns the engine as a subprocess.

```bash
cargo build -q
cargo run --manifest-path frontend/tui/Cargo.toml
# defaults to scenarios/battle.toml
```

| Variant | Command |
|---|---|
| Tutorial | `cargo run --manifest-path frontend/tui/Cargo.toml -- --tutorial` |
| Scenario | `cargo run --manifest-path frontend/tui/Cargo.toml -- scenarios/battle.toml` |
| Details | [`frontend/tui/README.md`](frontend/tui/README.md) |

Engine binary path override: `SHIPSIM_BIN=/path/to/shipsim`.

### Love2D (graphical UI)

Needs Love2D 11.x and a display.

```bash
cargo build -q
./frontend/love/play.sh
```

`play.sh` builds the engine if needed and starts `love frontend/love` (with
window sizing helpers under i3). Bare alternative:

```bash
love frontend/love
```

| Topic | Doc |
|---|---|
| Controls | [`frontend/love/README.md`](frontend/love/README.md) |
| Ship sprites | [`frontend/love/assets/ship_art/README.md`](frontend/love/assets/ship_art/README.md) |
| Art reviewer (outside the game) | `python3 frontend/love/tools/review_ship_art.py` |

### Simulation (batch matches, no UI)

In-process policies for volume play, balance, and rubrics — not interactive UX.

```bash
cargo run --release --bin shipsim-sim -- \
  --suite simulation/suites/smoke.toml \
  --output tmp/simulation/reports/smoke.json
```

`--output` is optional (stdout JSON if omitted). Exit `0` = rubrics passed,
`1` = runtime failure, `2` = rubrics failed.

Full reference: [`docs/SIMULATION.md`](docs/SIMULATION.md).

## Quick reference

| Goal | Command |
|---|---|
| Play (REPL) | `python3 frontend/repl/repl.py scenarios/battle.toml` |
| Play (TUI) | `cargo run --manifest-path frontend/tui/Cargo.toml` |
| Play (Love2D) | `./frontend/love/play.sh` |
| Smoke harness | `python3 frontend/repl/client.py` |
| Sim batch | `cargo run --release --bin shipsim-sim -- --suite simulation/suites/smoke.toml` |
| Engine tests | `cargo test -q` |
| REPL tests | `(cd frontend/repl && python3 -m unittest discover -s tests)` |
| Love headless | `luajit frontend/love/tests/run_all.lua` |
| TUI tests | `cargo test --manifest-path frontend/tui/Cargo.toml` |

Default scenario for interactive clients: **`scenarios/battle.toml`** (player heavy
cruiser vs AI escort).

## Architecture (short)

```text
shipsim_core  ← rules authority
     │
     ├── shipsim          NDJSON harness (stdin/stdout)
     │      ├── frontend/repl/
     │      ├── frontend/tui/
     │      └── frontend/love/
     └── shipsim-sim      in-process policies / suites
```

Clients never reimplement combat. Wire format: [`docs/PROTOCOL.md`](docs/PROTOCOL.md).

## Documentation map

| Doc | Role |
|---|---|
| [`docs/AGENT-PLAY.md`](docs/AGENT-PLAY.md) | UI / API / sim play procedures (agents) |
| [`AGENTS.md`](AGENTS.md) | Short agent entry |
| [`docs/PROTOCOL.md`](docs/PROTOCOL.md) | External NDJSON API (v4) |
| [`docs/PLAY-V2.md`](docs/PLAY-V2.md) | Rules summary |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | System boundaries |
| [`docs/SIMULATION.md`](docs/SIMULATION.md) | Batch simulation |
| [`docs/BALANCE.md`](docs/BALANCE.md) | Balance status |
| [`frontend/README.md`](frontend/README.md) | Frontend isolation policy |

## License / content

Generic ship and scenario data; not a reproduction of any commercial tabletop
product.
