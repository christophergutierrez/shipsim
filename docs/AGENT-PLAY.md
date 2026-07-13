# Agent play guide

**Start here** when asked to play shipsim, smoke the game, hunt UX/rules bugs, or
drive a session yourself.

There are **two intentional ways** an agent (or human) interacts with the product:

| Mode | What it is | When to use |
|---|---|---|
| **1. Engine / automated play** | Talk to the API (or run tests that do). No interactive UI. | Regression, protocol bugs, scripted flows, CI, quick “does it still work?” |
| **2. UI play (as a user)** | Drive a frontend that already uses the API. | Presentation bugs, command UX, “play until win/loss”, feel of the game |

Architecture (always true):

```text
  [ Engine: shipsim_core + shipsim NDJSON harness ]  ← sole rules authority
           ▲
           │  docs/PROTOCOL.md  (the API)
           │
  [ UI clients under frontend/<name>/ ]  ← many allowed; each isolated
```

- Engine-only play = API only.
- Every UI is an API client.
- Adding/removing a UI must not break the engine or other UIs (`frontend/README.md`).

---

## Mode 1 — Engine / automated (tests & API)

Use this for **basic bug finding** without sitting in a live terminal UI: order
legality, phase progress, scripted-ship deadlock, bar math units, golden
replays. This is the bulk of what automated “play” has been.

### 1a. REPL unit/integration tests (recommended default)

From **repo root** after `cargo build -q`:

```bash
cd frontend/repl
python3 -m unittest discover -s tests -v
```

Or run subsets:

```bash
cd frontend/repl
python3 -m unittest tests.test_m1_transport tests.test_m1_commands -v
python3 -m unittest tests.test_m3_scripted_driver tests.test_m3_fixes_flow -v
python3 -m unittest tests.test_bar_honesty -v
```

| Suite area | Catches |
|---|---|
| `test_m1_*` | Transport, command dispatch, view basics |
| `test_m2_*` | Tactical/targeting presentation helpers |
| `test_m3_*` | Scripted auto-drive, fire/allocate flow fixes |
| `test_bar_honesty` | Bar labels show `filled/total` honestly |
| `test_characterization` / `test_legacy_output` | Guard rails against silent output drift |

Scratch from these tests stays under `frontend/repl/local/` (gitignored).

### 1b. Non-interactive harness smoke

```bash
cargo build -q
python3 frontend/repl/client.py
```

Spawns `shipsim --scenario scenarios/combat.toml --stdin`, checks post-load
snapshot + one allocate. Good first health check.

### 1c. Raw API (no Python client)

```bash
cargo build -q
target/debug/shipsim --scenario scenarios/ai.toml --stdin
# then write one JSON order per line; read one snapshot/error per line
```

Full contract: **`docs/PROTOCOL.md`**.
Example order stream: `tests/fixtures/v2/duel_orders.jsonl`.

### 1d. Rust engine tests

```bash
cargo test
```

Rules, AI, fixtures, save/resume, simulation invariants — not UI chrome.

### 1e. Screen grid audit (presentation invariants, not self-play)

Self-play that only reads command transcripts **misses** double-paint, scrolled
frames, and dishonest bars. Use:

```bash
pip install pexpect pyte   # once
python3 frontend/repl/screen_audit.py
```

Asserts I1–I3 (bar honesty on a real grid, no duplicate panels, header on a
40-row terminal). Lives in `frontend/repl/screen_audit.py`.

### 1f. Batch simulation (balance / many matches)

Not interactive play. See `docs/SIMULATION.md` and `shipsim-sim`. Use when
evaluating policies and rubrics, not when debugging a single UX path.

### 1g. Love client unit checks

```bash
luajit frontend/love/tests/run_all.lua
```

### What Mode 1 is *bad* at

- Judging whether the **play frame** is readable or confusing
- Discovering stuck fire menus, focus confusion, draft UX
- “Play like a human until win/loss and narrate bugs”

For those, use Mode 2.

---

## Mode 2 — UI play (as a user)

Use this when the request is to **play the game**, exercise the live client, or
find bugs a user would hit. The agent types (or drives) the **same commands** a
human would.

### Primary UI: REPL (recommended)

```bash
cargo build -q
python3 frontend/repl/repl.py scenarios/ai.toml
```

| Resource | Why |
|---|---|
| **`frontend/repl/GAMEPLAY.md`** | Commands, phases, traps — **read before / while playing** |
| `frontend/repl/README.md` | Flags, session logs, screen audit |
| `docs/PLAY-V2.md` | Engine rules summary |

**Useful scenarios:**

| Scenario | Notes |
|---|---|
| `scenarios/ai.toml` | Default duel vs AI — best first play |
| `scenarios/combat.toml` | Includes **scripted** ship (REPL auto-pumps passive orders) |
| `scenarios/v2_duel.toml` | Cleaner duel fixture sibling |

**Session logs** (default): `frontend/repl/local/session-*.log` — use these when
reporting bugs. Prefer `--debug` only when you need raw ORDER JSON lines.

**How to play as an agent:**

1. Build and start the REPL (command above). Prefer a real TTY / PTY so the
   play frame redraws correctly.
2. Learn the loop from the on-screen hint + `GAMEPLAY.md` — do **not** invent
   engine orders by hand unless debugging transport.
3. Typical first turn:
   - Allocate: `a` → `mov N` → weapon/shield draft → `commit` (each player ship)
   - Movement: `motion` then one of `coast` / `accel` / `decel` / `course …` / `rotate …`
   - Firing: optional commits via fire flow, then `r` / `done` / `nofire` (not bare `e`)
   - End turn when appropriate: `e`
4. Play until `Won` / `Lost`, or until stuck. If stuck: try `status` / `cls`,
   then `quit`. If `quit` fails, stop and report a bug.
5. Report: phase, focus ship, last commands, session log path, expected vs actual.

**Do not** reimplement AI for `controller = "ai"` ships — the harness advances
them. **Scripted** ships are pumped by the REPL when they alone block the phase.

Flags agents often want:

```bash
python3 frontend/repl/repl.py scenarios/ai.toml --debug
python3 frontend/repl/repl.py scenarios/ai.toml --no-session-log
python3 frontend/repl/repl.py scenarios/ai.toml --bin target/debug/shipsim
```

### Secondary UI: Love2D (graphical)

```bash
cargo build -q
love frontend/love
```

Human mouse/keyboard play. Weaker for headless agents. Isolation and notes:
`frontend/love/README.md`. Prefer REPL for agent sessions.

### Future UI: ratatui TUI

Not implemented. Scaffold: `frontend/tui/`. When it exists, it will be another
Mode 2 client on the same API.

---

## Choosing a mode (decision table)

| Ask / goal | Mode |
|---|---|
| “Run the tests” / “smoke after a change” | **1** — unittest + `client.py` + `cargo test` |
| “Find basic rules/protocol bugs” | **1** — fixtures + REPL tests + raw API |
| “Does the scripted ship still deadlock?” | **1** — `tests.test_m3_scripted_driver` + optional Mode 2 on `combat.toml` |
| “Play the game yourself” / “play until win” | **2** — REPL as user |
| “UI is wrong / bars / double paint” | **2** + `screen_audit.py` (1e) |
| “Balance / many matches” | **1f** simulation, not REPL |
| “Wire a new client” | Read `docs/PROTOCOL.md` + copy REPL isolation rules |

---

## Isolation rules (do not break)

1. All frontend work under `frontend/<name>/` only.
2. Logs and agent scratch → `frontend/<name>/local/` (not repo root, not shared `/tmp`).
3. No cross-client imports; no engine dependence on `frontend/`.
4. Clients speak **protocol v2** only (`docs/PROTOCOL.md`).

---

## Quick command cheat sheet

```bash
# Build once
cargo build -q

# Mode 1 — automated
python3 frontend/repl/client.py
(cd frontend/repl && python3 -m unittest discover -s tests -v)
cargo test
python3 frontend/repl/screen_audit.py

# Mode 2 — play as user
python3 frontend/repl/repl.py scenarios/ai.toml
```

Docs map:

| Path | Audience |
|---|---|
| `AGENTS.md` | Repo entry for agents |
| `docs/AGENT-PLAY.md` | This file |
| `docs/PROTOCOL.md` | API |
| `docs/ARCHITECTURE.md` | Boundaries |
| `frontend/repl/GAMEPLAY.md` | Live REPL how-to |
