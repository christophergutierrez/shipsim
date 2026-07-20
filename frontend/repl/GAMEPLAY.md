# shipsim REPL — gameplay guide

How to **play** Combat Model v2 through `frontend/repl/` under **protocol v4**.
Rules live in the Rust engine (`docs/PLAY-V2.md`, ADR-0025); this file is the
**REPL-shaped** walkthrough: stages, commands, what the screen means, and
common traps.

This is **UI play** (`docs/AGENT-PLAY.md`). For harness/tests without the live
frame, use **API play** and `docs/PROTOCOL.md`. For mass matches, use **sim play**
(`docs/SIMULATION.md`).

Start a game:

```bash
cargo build -q
python3 frontend/repl/repl.py scenarios/ai.toml
```

See `README.md` for flags (`--debug`, session logs, `--scroll`).

---

## 1. What you are looking at

### Play frame (default)

Each step the client **clears and redraws** from the latest engine snapshot:

| Region | Contents |
|---|---|
| Header | Turn, **phase**, status, focus ship, next pending path/volley |
| YOUR SHIP | Callsign, position, facing, hull bar, shields, weapons, **motion** |
| THREATS | Advisory: enemy ships + weapons that can bear on your focus ship |
| ENGAGEMENT | Range, bearing, exposed shield face, weapon range/arc status |
| CONTACTS | Enemies/allies with range and which of *their* shields face you |
| MAP | Hex board, callsign + facing arrow per ship |
| RECENT | Last few events (allocate echo, Δ lines, fire resolution) |
| DRAFT | Local allocate / path / volley drafts (not on engine until commit) |
| Hint + prompt | What to do next; `t{turn}/{phase}@focus…>` |

Type `log` to toggle a longer history panel. `cls` / `status` redraws the frame.

### Callsigns and sides

| Letter | Controller |
|---|---|
| **A#** | player (you control these) |
| **B#** | ai |
| **C#** | scripted |

### Facing (map arrows)

Board is **q right, r down**. Facing index and arrow **are** forward:

| Face | Arrow | On screen |
|---|---|---|
| 0 | → | +q (right) |
| 1 | ↗ | |
| 2 | ↑ | −r (up) |
| 3 | ← | −q (left) |
| 4 | ↙ | |
| 5 | ↓ | +r (down) |

### Shield faces (ship-relative)

| Index | Label | Meaning |
|---|---|---|
| 0 | F | Forward |
| 1 | FR | Forward-starboard |
| 2 | RR | Rear-starboard |
| 3 | R | Rear |
| 4 | RL | Rear-port |
| 5 | FL | Forward-port |

`←` on a contact’s shield row marks facings that currently face **you**.

---

## 2. Turn structure (protocol v4)

Each **turn** is three simultaneous **collection stages**:

```
ALLOCATE  (every living ship stages power once; apply together)
    ↓
MOVEMENT  (every living ship stages one complete path; resolve together)
    ↓
FIRING    (every living ship stages one volley; resolve together)
    ↓
next ALLOCATE automatically  (no turn_end, no end_turn)
```

Snapshots may show **pending / committed ship IDs**, never opponent path or
volley payloads.

AI ships (`controller = "ai"`) are advanced by the harness after your orders.
**Scripted** ships are **not** driven by the harness. The REPL auto-sends
passive allocate / empty path / empty volley for them when the stage is blocked
**only** on scripted ships. You only type orders for **player** ships.

---

## 3. Phase: Allocate

### Goal

Split each player ship’s **power pool** into:

- **engine** power (converted to a turn-scoped **motion** pool by the hull),
- **weapon charges**,
- **six shield faces** (sum ≤ pool; per-face max applies).

Nothing hits the engine until **`commit`**.

### Commands

| Command | Effect |
|---|---|
| `a` | List ships still needing allocate; auto-open draft if only one |
| `a 1` / `1` | Focus ship and open draft (if not yet allocated) |
| `mov 6` / `engine 6` / `m 6` | Set engine power in the draft |
| bare number (e.g. `8`) | While drafting: **set movement** |
| `w` | Enter weapons group |
| `b1 2` / `t1 1` / `p1 1` | Charge beam_1 / torp_1 / plasma_1 |
| `sh` | Enter shields group |
| `0 3` / `F 3` | Put 3 power on face 0 / F |
| `show` | Reprint draft bars |
| `reset` | Clear draft (still local) |
| `commit` / `c` / `ok` | Send `allocate` order to the engine |
| `cancel` | Discard draft |

### Example

```
a
mov 6
w
b1 2
t1 1
done
sh
0 3
5 3
done
commit
```

After a good commit you should see something like:

```
  engine accepted allocate #1: engine=6 power → motion=6  weapons: beam_1=2, torp_1=1  shields=[3, 0, 0, 0, 0, 3]
phase → movement
```

### Traps

| Mistake | Result |
|---|---|
| Commit with all zeros | No motion, no new charge; empty path later |
| Never `commit` | Engine still unallocated |
| Expect multi-ship allocate without `a` each ship | Each player ship must allocate once |

---

## 4. Phase: Movement (path draft)

### Goal

Each living ship submits **exactly one** `commit_path` with an ordered list of
path actions. Paths resolve simultaneously when every ship has committed.

There is **no velocity or course**. Only position + facing persist. Each action
costs **exactly one** motion point from the turn’s motion pool.

| Action | Wire name | Effect |
|---|---|---|
| `f` | `move_f` | one hex through F; facing unchanged |
| `fr` | `move_fr` | one hex through FR; then turn right |
| `fl` | `move_fl` | one hex through FL; then turn left |
| `tr` / `r` | `turn_right` | in-place face −1 (clockwise / starboard) |
| `tl` / `l` | `turn_left` | in-place face +1 (counterclockwise / port) |

### Commands

| Command | Effect |
|---|---|
| `path` / `motion` / `m` | Show motion pool + draft help |
| `path f fr tl` | Append actions to the local draft |
| bare `f` / `fr` / `fl` / `tr` / `tl` / `r` / `l` | Same append (movement phase) |
| `undo` | Drop last drafted action |
| `clear` | Empty the draft |
| `preview` | Engine `path_preview` (authoritative legality) |
| `commit` / `path commit` | Send `commit_path` once |
| `hold` / `p` / `pass` | Commit an **empty** path (stay put) |

The engine owns path legality — use `preview` or accept soft-rejects on commit.
Do not invent extra rules in the client.

### Example

```
path f f fr tl
preview
commit
```

Or stay put:

```
hold
```

### Traps

| Mistake | Result |
|---|---|
| Expect multi-cycle move/fire | One path + one volley per turn |
| Spam old `accel` / `turn N` | Retired; use path actions |
| Fire during movement | Soft error: need firing stage |
| Assume focus chooses who acts | Commands default to the first pending player ship |

---

## 5. Phase: Firing (volley draft)

### Goal

1. Optionally **draft** zero or more legal shots into a local volley.
2. **Submit** `commit_volley` once (empty shots = hold fire / `nofire`).
3. When **all** living ships have committed, all volleys resolve **at once**.
4. The engine advances to the **next turn’s allocate** automatically.

### Commands

| Command | Effect |
|---|---|
| `f` / `fire` | Interactive shot picker (adds to draft) |
| `fire b1 B2` | One-line: add shot to draft |
| `undo` | Drop last drafted shot |
| `clear` | Empty the volley draft |
| `r` / `ready` / `done` / `nofire` / `commit` | Send `commit_volley` |

### Example volley

```
f                 # pick beam, target, shield face → drafted
f                 # pick torp → drafted
r                 # commit_volley with both shots
# → FIRE RESOLUTION (HIT/MISS), then next allocate
```

### Leaving fire **without** shooting

```
r
# or: done / nofire / commit
```

There is **no** `end_turn`. After all volleys resolve, allocate begins.

### Traps

| Mistake | Result |
|---|---|
| Expect charge to drop on `f` alone | Charge drops on **volley resolve** |
| `e` to “exit fire” | Removed in v4; use r/nofire |
| Fire same weapon twice in one draft | Soft-reject / draft refuses duplicate |
| Submit twice | “already committed a volley” |

---

## 6. Prompt cheat sheet

Examples:

```
t1/allocate@1 draft11/22>
t1/movement@1*1 path3 actions=motion:5>
t1/firing@1/v=2 actions=charged:3>
t1/firing@1/volley_ok>
```

| Fragment | Meaning |
|---|---|
| `t1` | Turn 1 |
| `allocate` / `movement` / `firing` | Engine stage |
| `@1` | UI focus ship id |
| `*1` | Next player ship still needing a path |
| `draft11/22` | Local allocate draft used/pool |
| `path3` | Local path draft length |
| `/v=2` | Local volley draft shot count |
| `/volley_ok` | This ship already committed its volley |

---

## 7. Full command index (REPL)

### Always

| Command | Action |
|---|---|
| `help` / `?` | Command help |
| `hint` | Phase tip |
| `status` / `s` | Redraw play frame |
| `board` / `b` | Board dump to RECENT |
| `ships` | Compact ship lines |
| `log` | Toggle history panel |
| `cls` | Redraw |
| `raw` | Compact phase JSON |
| `quit` / `q` | Exit |
| `order {…}` | Raw protocol JSON |
| `ship N` / `sel N` | Focus ship |

### Allocate

`a`, `a N`, `mov`, `w`, `sh`, `b1`/`t1`/…, `commit`, `reset`, `cancel`, `ad`

### Movement

`path f|fr|fl|tr|tl …`, `undo`, `clear`, `preview`, `commit`, `hold`/`p`

### Firing

`f`, `fire b1 B2`, `r` / `done` / `nofire`, `commit`

---

## 8. Reading outcomes

### After allocate

```
  engine accepted allocate #1: engine=… power → motion=…  weapons: …  shields=…
Δ phase allocate→movement …
```

### After path resolve

Position/facing update on YOUR SHIP and MAP; `path_results` may appear in the
snapshot for fallback/telemetry.

### After fire resolve

```
*** FIRE RESOLUTION ***  (or panel)
A1 beam_1 → B2  HIT for N  on shield …
```

### Soft errors

Illegal orders do **not** change state. Message includes `code` and often a short
hint (e.g. wrong phase). Fix the phase or ship and retry.

---

## 9. Suggested first fight (`scenarios/ai.toml`)

1. **Allocate** `A1`: some movement, charge `b1`, forward shields, `commit`.
2. **Movement**: `path f f f` then `commit`, or `hold` to stay.
3. **Firing**: optional `f`, then **`r`**.
4. Watch RECENT for HIT/MISS; next allocate starts automatically.
5. Session text log: path in footer / on quit under `local/session-*.log`.

### Guided tutorial (protocol 4)

```bash
python3 frontend/repl/repl.py --tutorial rear-attack
```

Narrated path + volley lesson on `scenarios/tutorial_rear_attack.toml`.

---

## 10. Related docs

| Doc | Topic |
|---|---|
| `README.md` | Run flags, isolation, file map |
| `ASCII-UI.md` | Terminal presentation / UI engineering notes |
| `docs/PLAY-V2.md` | Rules summary (all clients) |
| `docs/PROTOCOL.md` | NDJSON orders/snapshots |
| ADR-0025 | Simplified simultaneous turns (protocol v4) |
