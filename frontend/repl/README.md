# shipsim REPL (playable dev client)

Ship-centric text client for Combat Model v2. Spawns the Rust `shipsim` harness
over stdin/stdout NDJSON (`protocol_version: 3`).

**This directory is the entire REPL client.** See `frontend/README.md`.

Agents: play types (**UI play** / **API play** / **sim play**) are in
**`docs/AGENT-PLAY.md`** and root **`AGENTS.md`**. API wire format:
**`docs/PROTOCOL.md`**.

```
frontend/repl/
  repl.py client.py commands.py view.py hexutil.py style.py screen.py
  screen_audit.py # PTY+pyte grid invariants (I1–I3); needs pexpect+pyte
  README.md       # this file — run, flags, map of docs
  GAMEPLAY.md     # how to play (phases, commands, traps)  ← start here for UI play
  ASCII-UI.md     # terminal presentation practices (for UI work)
  tests/          # API play suite (unittest; not live UI)
  .gitignore
  local/          # gitignored: session logs, orders, readline history
```

## Docs in this tree

| File | Audience |
|---|---|
| **[`GAMEPLAY.md`](GAMEPLAY.md)** | Players / agents learning the **live** play loop (UI play) |
| **[`ASCII-UI.md`](ASCII-UI.md)** | Anyone changing layout, colors, map glyphs, draft UX |
| `README.md` | Run commands, logging, isolation |
| `../../docs/AGENT-PLAY.md` | UI play · API play · sim play |
| `../../docs/PROTOCOL.md` | Engine API |

Rules of the game (engine): `docs/PLAY-V2.md`, `docs/PROTOCOL.md`, ADR-0020.

## Run

```bash
cargo build -q
python3 frontend/repl/repl.py scenarios/ai.toml
python3 frontend/repl/repl.py scenarios/ai.toml --bin /path/to/shipsim
python3 frontend/repl/repl.py scenarios/ai.toml --save local/battle.json
python3 frontend/repl/repl.py scenarios/ai.toml --debug              # verbose file log
python3 frontend/repl/repl.py scenarios/ai.toml --log-file /tmp/x.log
python3 frontend/repl/repl.py scenarios/ai.toml --no-session-log
python3 frontend/repl/repl.py scenarios/ai.toml --scroll             # old long scrolling UI
python3 frontend/repl/repl.py --tutorial rear-attack                 # strict narrated victory
python3 frontend/repl/client.py                                      # non-interactive smoke
```

**Rear-attack tutorial (protocol 3):** `--tutorial rear-attack` is a strict
narrated pass — accelerate east, `turn 3` to aim west, stern volleys to Won.
See `tutorial.py` (sequence verified against the dedicated deterministic
`scenarios/tutorial_rear_attack.toml`) through the live engine. Guidance
immediately above every input states the mission, explains why the next
tactical choice matters, and gives the exact
required command. Incorrect gameplay commands are blocked before they can change
the draft or game; `help`, `status`, `board`, `ships`, and `log` remain available
for inspection without advancing the lesson.

**Play frame (default):** redraws map + ships each step so shield/hull/weapon bars
update in place. **RECENT** holds the last events; `log` toggles longer scrollback.

**Session log (default on):** `frontend/repl/local/session-YYYYMMDD-HHMMSS.log`
(gitignored). Shown in the footer and at quit. `--log-file PATH` overrides;
`--no-session-log` disables. **`--debug`** keeps that file but adds timestamps and
full `ORDER` JSON lines (does not change the play frame).

`--bin PATH` selects a specific harness binary. `--save PATH` asks the harness
to write its save under `frontend/repl/local/`.

Arrow-up recalls prior command lines (`local/history`).

**Automated tests (API play — regression / basic bugs, not live UI):**

```bash
cd frontend/repl
python3 -m unittest discover -s tests -v
python3 -m unittest tests.test_bar_honesty -v
```

See `docs/AGENT-PLAY.md` § API play.

**Screen audit (grid invariants, not self-play):**

```bash
pip install pexpect pyte   # once
python3 frontend/repl/screen_audit.py
```

Asserts bar-label honesty (I1), no duplicate panels on one screen (I2), and that
the frame header stays on a 40-row terminal (I3).

## Play loop (summary)

Full detail: **[`GAMEPLAY.md`](GAMEPLAY.md)**.

```
allocate (draft → commit) → movement (one decision / ship) → firing (queue → ready)
    ↑________________________ move/fire may repeat _________________________|
```

| Phase | Typical commands |
|---|---|
| Allocate | `a` → `mov` / `w` / `sh` → `commit` |
| Movement | `motion`, then `accel`, `turn 0..5`, or `coast` (**one** per ship) |
| Firing | `f` (optional) → `r` / `done` / `nofire` (not `e`) |
| End turn | `e` (whole turn; confirm in firing) |

**Essentials:**

- Facing arrows match **forward on the board** (0→ is +q / right). See GAMEPLAY.
- `m N` is **one** turn *or* one step — not turn-then-step in one command.
- `commit_fire` **queues**; charge drops when **all** ships **ready**.
- Weapon lines: `CHG` → `QUEUED` → `FIRED HIT/MISS`.

## Isolation

Scratch only under `frontend/repl/local/`. Do not write REPL logs to the repo root.
Love and the future ratatui client are sibling trees under `frontend/`.
