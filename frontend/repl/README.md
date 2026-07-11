# shipsim REPL (playable dev client)

Ship-centric text client for Combat Model v2. Spawns the Rust `shipsim` harness
over stdin/stdout NDJSON (`protocol_version: 1`).

**This directory is the entire REPL client.** See `frontend/README.md`.

```
frontend/repl/
  repl.py client.py commands.py view.py hexutil.py style.py
  README.md
  ASCII-UI.md     # terminal / ASCII presentation practices (read for UI work)
  .gitignore
  local/          # gitignored: orders, stderr, readline history
```

## Presentation

Terminal/ASCII design notes for this client (and future sessions) live in
**[`ASCII-UI.md`](ASCII-UI.md)** — model/view split, hex-on-character-grid,
color restraint, glyphs, bars, allocate UX, and a change checklist. Keep UI
work aligned with that file; rules stay in Rust.

## Run

```bash
cargo build -q
python3 frontend/repl/repl.py scenarios/ai.toml
python3 frontend/repl/repl.py scenarios/ai.toml --debug              # verbose file log
python3 frontend/repl/repl.py scenarios/ai.toml --log-file /tmp/x.log
python3 frontend/repl/repl.py scenarios/ai.toml --no-session-log
python3 frontend/repl/repl.py scenarios/ai.toml --scroll             # old long scrolling UI
```

**Play frame (default):** clears and redraws map + ships each step so shield/hull/weapon
bars update in place. A **RECENT** strip holds the last few events; type `log` to
toggle longer scrollback. Controls stay under the board.

**Session log (default on):** full text transcript under
`frontend/repl/local/session-YYYYMMDD-HHMMSS.log` (gitignored). Path is printed at
start/end. Use `--log-file PATH` to override, `--no-session-log` to disable.

**`--debug`:** same session file, but **verbose** (timestamps + every outbound
`ORDER` JSON line). Does not change the on-screen play frame.

Arrow-up recalls prior command lines (`local/history`).

## Play loop

### Focus / allocate
`a` lists player ships still needing allocate (auto-opens draft if only one).
Ship id `1` also focuses and opens a draft in allocate phase.

```
a                 # pick ship (or auto)
mov 6             # or: mov  then  6  on the next line
w                 # weapons group — then shortcuts, no leading w
t1 1              # b1=beam_1  t1=torp_1  p1=plasma_1
b1 2
done
sh                # shields group
0 3
done
commit            # only now hits the engine
```

Root shortcuts still work: `w t1 1`, bare `t1 1`, `sh F 3`.

**Pitfall fixed:** a lone number while drafting is **movement power**, not
“select ship again”. Re-picking a ship used to **wipe** the draft to all zeros;
committing that skipped movement and left weapons uncharged. Empty commit now
asks for confirmation. After commit, the engine echo shows applied mov/weapons.

### Movement (facing 0..5 universal)
```
m 0               # step absolute map dir 0 (auto-turns then forward/reverse)
m 1 … m 5         # other absolute directions
m f / m r         # relative forward / reverse
m port / m stbd   # turn only
p                 # pass movement (ACTIVE ship)
```

Absolute `m N` may issue several turn orders then one step. Engine modes remain
forward / reverse / turn_port / turn_starboard.

### Firing
```
f                 # commit optional shot (shows legal shields on target)
r / nofire / done # leave fire phase WITHOUT shooting
e                 # whole turn (confirms if in firing)
```

After resolution: **HIT/MISS**, shield face, and target card (hull + shields).

### Status
`s` / `status` — your ship card + contacts (which of *their* shields face you,
shield rem/powered bars, hull bars).

## Isolation

Scratch only under `frontend/repl/local/`.
