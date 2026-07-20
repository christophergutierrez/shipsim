# Play types — UI play, API play, sim play

**Start here** when a human or agent is asked to play shipsim, smoke the game,
hunt bugs, or “play itself.”

There are **three intentional play types**. Use the names below in reports and
prompts so everyone picks the same path.

| Name | What drives the rules | When to use | Not for |
|---|---|---|---|
| **UI play** | A frontend (REPL, Love, …) as a user | Feel, UX, “play until win/loss”, command traps | Thousands of matches |
| **API play** | NDJSON harness (`shipsim`) + client/tests (protocol **v4**) | Protocol, smoke, regressions, scripted flows | Screen layout / menus |
| **Sim play** | Rust core in-process (`shipsim-sim` / policies) | Hundreds–thousands of seeded matches, balance, rubrics | Interactive UX |

Architecture:

```text
                    ┌── UI play ──── frontend/*  (many allowed)
                    │
  shipsim_core  ◄───┼── API play ─── shipsim NDJSON  (docs/PROTOCOL.md)
  (rules)           │
                    └── sim play ─── shipsim-sim / apply_order in-process
```

- Rules live only in the engine. UIs never reimplement combat.
- Every UI is an **API play** client under the hood.
- **Sim play** uses the same order validation path, without a harness process
  or a frontend — see `docs/SIMULATION.md`.
- Isolation: `frontend/README.md`. API contract: `docs/PROTOCOL.md`.

**Default when an agent is told to “play the game” or “play itself”:**
prefer **UI play** (REPL) unless the ask is clearly tests/smoke (**API play**)
or balance/volume (**sim play**).

**Explicit routing is binding:** “play a UI game” / “use the UI” means launch
`frontend/repl/repl.py` (or another named frontend) and submit choices through
its user-facing controls. A raw order stream, `client.py`, or tests are not a
substitute. “Play via the API” means use the NDJSON harness/client and does not
count as UI validation.

---

## UI play

Drive a **live frontend** with the same commands/clicks a human uses.
Session logs under `frontend/<name>/local/`.

### Primary: REPL

```bash
cargo build -q
python3 frontend/repl/repl.py scenarios/ai.toml
```

| Resource | Why |
|---|---|
| **`frontend/repl/GAMEPLAY.md`** | Commands, phases, traps — read while playing |
| `frontend/repl/README.md` | Flags, session logs |
| `docs/PLAY-V2.md` | Engine rules summary |

| Scenario | Notes |
|---|---|
| `scenarios/ai.toml` | Default duel vs AI — best first play |
| `scenarios/combat.toml` | Includes **scripted** ship (REPL pumps passive orders) |
| `scenarios/v2_duel.toml` | AI-vs-AI headless self-play (rejected by the interactive harness; use `shipsim-sim` instead) |

**Agent procedure:**

1. Build and start the REPL. Prefer a real TTY/PTY so the play frame works.
2. Use on-screen hints + `GAMEPLAY.md` — do not hand-write raw JSON unless
   debugging transport.
3. Loop: allocate (`mov` / `w` / `sh` → `commit`) → draft a path
   (`path f fr tl` / `commit` or `hold`) → draft a volley (`fire …` then
   `r`/`nofire`). Turn advances automatically after volleys — no `end_turn`.
4. Play until `Won` / `Lost`, or until stuck (`status` / `cls`, then `quit`).
   If `quit` fails, stop and report a bug.
5. Report phase, focus, last commands, session log path, expected vs actual.

Do **not** invent AI orders — the harness advances `controller = "ai"`.
Scripted ships are pumped by the REPL when they alone block the phase.

```bash
python3 frontend/repl/repl.py scenarios/ai.toml --debug
python3 frontend/repl/repl.py scenarios/ai.toml --no-session-log
```

Logs: `frontend/repl/local/session-*.log`, `orders-*.jsonl`.

### Secondary: Ratatui TUI / Love2D

Protocol **v4** is live on the engine and on all three clients. **REPL** remains
the agent reference path for UI play. Prefer it unless the ask names TUI or Love.

| Client | Run | Notes |
|---|---|---|
| **REPL** | `python3 frontend/repl/repl.py scenarios/ai.toml` | Reference; best for agents |
| **TUI** | `cargo run --manifest-path frontend/tui/Cargo.toml` | Small-tier ratatui; see `frontend/tui/README.md` |
| **Love2D** | `./frontend/love/play.sh` | Graphical thin client; see `frontend/love/README.md` |

### Screen grid audit (UI presentation, not a full game)

Self-play that only watches orders **misses** double-paint and dishonest bars:

```bash
pip install pexpect pyte   # once
python3 frontend/repl/screen_audit.py
```

---

## API play

Talk to the **public NDJSON API** (or tests that do). No interactive UI frame.
Contract: **`docs/PROTOCOL.md`**.

### Harness smoke

```bash
cargo build -q
python3 frontend/repl/client.py
```

### Raw harness

```bash
cargo build -q
target/debug/shipsim --scenario scenarios/ai.toml --stdin
# one JSON order per line → one snapshot or error per line
```

Golden order/snapshot streams: `tests/fixtures/v4/` (see that directory’s
README). Older `scenarios/d8_frontend_orders.jsonl` is protocol **v3** and is
**rejected** by the current engine.

### REPL automated suite (command → order; not live UI)

```bash
cd frontend/repl
python3 -m unittest discover -s tests -v
```

| Suite | Catches |
|---|---|
| `test_m1_*` | Transport, commands, view basics |
| `test_m2_*` | Tactical/targeting helpers |
| `test_m3_*` | Scripted auto-drive, fire/allocate flows |
| `test_bar_honesty` | Bar labels `filled/total` |
| `test_characterization` | Guard rails on order emission |

These use fakes/helpers; they are **API/command-layer** checks, not UI play.

### Engine integration tests

```bash
cargo test
```

Rules, fixtures, save/resume, sim invariants — not frontend chrome.

### Love unit checks

```bash
luajit frontend/love/tests/run_all.lua
```

### What API play is *bad* at

- Readable play frame, stuck menus, draft UX, “feels wrong”
- For those: **UI play** (+ `screen_audit.py` when it’s paint/grid)

---

## Sim play

**Batch simulation** through the Rust core in-process: same `GameState` /
`apply_order` path, no NDJSON process and no UI. Built for volume.

Full docs: **`docs/SIMULATION.md`**. CLI: `shipsim-sim`.

```bash
cargo run --release --bin shipsim-sim -- \
  --suite simulation/suites/smoke.toml \
  --output tmp/simulation/reports/smoke.json
```

| Piece | Role |
|---|---|
| Policies (`random`, `greedy`, …) | Both sides choose legal orders |
| Suites under `simulation/suites/` | Scenario × seed × policy matrix |
| Rubrics under `simulation/rubrics/` | Pass/fail thresholds |
| Report JSON | Aggregates + per-match traces |

Use sim play for balance, termination rates, stalemates, policy comparison —
**not** for “does the REPL make sense?”

Do not call ordinary `cargo test` unit cases “sim play” unless they are
simulation suite/policy matches. **Sim play** means the batch runner and its
suites.

---

## Decision table (agents: pick one)

| Ask / goal | Play type |
|---|---|
| “Play the game” / “play itself” / “play until win” | **UI play** (REPL) |
| “UI is wrong / bars / menus / double paint” | **UI play** + `screen_audit.py` |
| “Smoke / run the tests / protocol bug” | **API play** |
| “Scripted ship deadlock?” | **API play** (`test_m3_scripted_driver`) ± UI on `combat.toml` |
| “Balance / many matches / rubrics” | **Sim play** |
| “Wire a new client” | Read `docs/PROTOCOL.md`; implement as API client; optional UI play later |

---

## Isolation (all types)

1. Frontend code and scratch only under `frontend/<name>/` (`local/` gitignored).
2. No cross-client imports; engine must not depend on `frontend/`.
3. External clients: protocol **v4** only (`docs/PROTOCOL.md`).
4. Sim reports under ignored `tmp/simulation/` (or equivalent local paths).

---

## Quick commands

```bash
cargo build -q

# UI play
python3 frontend/repl/repl.py scenarios/ai.toml

# API play
python3 frontend/repl/client.py
(cd frontend/repl && python3 -m unittest discover -s tests -v)
cargo test
python3 frontend/repl/screen_audit.py

# Sim play
cargo run --release --bin shipsim-sim -- \
  --suite simulation/suites/smoke.toml \
  --output tmp/simulation/reports/smoke.json
```

## Docs map

| Path | Role |
|---|---|
| **`AGENTS.md`** | Repo entry for agents |
| **`docs/AGENT-PLAY.md`** | This file — play types |
| `docs/PROTOCOL.md` | API (API play wire format) |
| `docs/SIMULATION.md` | Sim play details |
| `docs/ARCHITECTURE.md` | System boundaries |
| `frontend/repl/GAMEPLAY.md` | UI play command vocabulary |
