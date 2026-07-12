# shipsim REPL — gameplay guide

How to **play** Combat Model v2 through `frontend/repl/`. Rules live in the Rust
engine (`docs/PLAY-V2.md`, ADR-0020); this file is the **REPL-shaped** walkthrough:
phases, commands, what the screen means, and common traps.

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
| Header | Turn, **phase**, status, focus ship, ACTIVE mover, leftover-power warning |
| YOUR SHIP | Callsign, position, facing, hull bar, shields, weapons |
| THREATS | Advisory: enemy ships + weapons that can bear on your focus ship (range shown) |
| CONTACTS | Enemies/allies with range and which of *their* shields face you |
| MAP | Hex board, callsign + facing arrow per ship |
| RECENT | Last few events (allocate echo, Δ lines, fire resolution) |
| ALLOCATE DRAFT | Only while drafting power (local — not on engine yet) — warns on unspent power |
| Hint + prompt | What to do next; `t{turn}/{phase}@focus…>` |

Type `log` to toggle a longer history panel. `cls` / `status` redraws the frame.

### Callsigns and sides

| Letter | Controller |
|---|---|
| **A#** | player (you control these) |
| **B#** | ai |
| **C#** | scripted |

Example: `A1` = player ship id 1, `B2` = AI ship id 2. Same letter ≈ same side
until scenarios grow real fleet/side ids.

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

`m forward` / `m f` always moves the way the ship’s arrow points — not “up the
screen” unless face is 2.

### Shield faces (ship-relative)

On a ship, shields are **relative to its nose**, not map north:

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

## 2. Turn structure (engine loop)

Each **turn**:

```
ALLOCATE  (all living ships, once each)
    ↓
┌→ MOVEMENT  (each ship: one Move or Pass, initiative order)
│      ↓
│  FIRING    (optional commits, then Ready; resolve when all ready)
│      ↓
└── if useful hex-move or legal fire remains → MOVEMENT again
    else → turn ends (or End Turn early)
```

There is no separate “round counter.” Movement and firing **pair** until nobody
can usefully act (or you force **End Turn**).

AI ships (`controller = "ai"`) are advanced by the harness after your orders.
**Scripted** ships (`controller = "scripted"`, e.g. the escort in
`scenarios/combat.toml`) are **not** driven by the harness. The REPL auto-sends
passive allocate / pass_move / ready_fire for them when the phase is blocked
**only** on scripted ships — otherwise the game would wait forever. You still
only type orders for **player** ships.

---

## 3. Phase: Allocate

### Goal

Split each player ship’s **power pool** into:

- **movement** power (units for steps/turns this turn — not “hex count”),
- **weapon charges**,
- **six shield faces** (sum ≤ pool; per-face max applies).

Nothing hits the engine until **`commit`**.

### Commands

| Command | Effect |
|---|---|
| `a` | List ships still needing allocate; auto-open draft if only one |
| `a 1` / `1` | Focus ship and open draft (if not yet allocated) |
| `mov 6` / `m 6` | Set movement power in the draft |
| `mov` then `6` | Same (two lines) |
| bare number (e.g. `8`) | While drafting: **set movement**, not “re-pick ship” |
| `w` | Enter weapons group; list shortcuts |
| `b1 2` / `t1 1` / `p1 1` | Charge beam_1 / torp_1 / plasma_1 |
| `w t1 1` | Same from draft root |
| `sh` | Enter shields group |
| `0 3` / `F 3` | Put 3 power on face 0 / F |
| `show` | Reprint draft bars |
| `reset` | Clear draft (still local) |
| `commit` / `c` / `ok` | Send `allocate` order to the engine |
| `cancel` | Discard draft |

Weapon shortcuts: first letter of kind + index (`beam_1` → `b1`, `torp_1` → `t1`).

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
  engine accepted allocate #1: mov=6  weapons: beam_1=2, torp_1=1  shields=[3, 0, 0, 0, 0, 3]
phase → movement   (if someone has move power)
```

### Traps

| Mistake | Result |
|---|---|
| Commit with all zeros | Movement phase skipped; fire has nothing charged (client asks confirm) |
| Think “I set points” but never `commit` | Engine still unallocated |
| Expect multi-ship allocate without `a` each ship | Each player ship must allocate once |
| Empty weapons map vs all zero charges | Both fine; uncharged guns cannot fire later |

If **every** living ship has **0** movement power after allocate, the engine goes
**straight to firing** (no movement phase). That is rules, not a bug.

---

## 4. Phase: Movement

### Goal

Each living ship with move power left that has not yet decided gets **exactly one**
decision this phase: **Move** (forward / reverse / turn port / turn starboard) or
**Pass**. Then the next ship in initiative order.

Only the **ACTIVE** ship (shown in the header and as `*id` in the prompt) may act.

When every ship has decided or has **0** move remaining, phase becomes **firing**.

### Commands

| Command | Effect |
|---|---|
| `m f` / `m forward` | Forward one hex (usually cost 1) |
| `m r` / `m reverse` | Reverse one hex |
| `m port` / `m stbd` | Turn only (cost 1) |
| `m 0` … `m 5` | **One** order toward absolute map direction |
| `p` / `pass` | Pass (skip this decision) |

### Absolute `m N` (important)

The engine allows **one Move/Pass per ship per movement phase**.

`m N` does **not** “turn until facing N then step” in one go. It does **one** of:

- if already facing N → **forward**,
- if facing opposite N → **reverse**,
- else → **one turn** toward N, and that **uses your decision**.

To walk a path: turn when ACTIVE, then on a **later** movement phase (or next turn)
step when ACTIVE again.

### Momentum costs (engine)

- Forward / turn: usually **1** move power.
- Reverse after going forward (keel flip): often **2**.
- If you lack power, the order soft-fails; state unchanged.

### After you act

AI ships take their decisions automatically. You may land in **firing** immediately
if nobody else has a move left.

### Traps

| Mistake | Result |
|---|---|
| `f` (fire) during movement | Soft error: need firing phase |
| Spam `m 2` expecting multi-hex path | Only one decision; rest fail or confuse phase |
| Fire while still ACTIVE for move | Same — finish move/pass first |
| Assume focus ship moves | **ACTIVE** moves; focus is only your UI default |

---

## 5. Phase: Firing

### Goal

1. Optionally **queue** zero or more legal shots (`commit_fire`).
2. **Ready** each of your living ships when done committing.
3. When **all** living ships are ready, all queues resolve **at once**.
4. Then either another movement phase, or turn end.

### Weapon status on the ship card

| Label | Meaning |
|---|---|
| `CHG n/m (available)` | Still free to queue this phase |
| `QUEUED →#target …` | Committed; charge still listed until resolve |
| `FIRED HIT` / `FIRED MISS` | Resolved; charge spent (`chg=0`) |
| `shots resolved this turn:` | Explicit list of every weapon that resolved |

**Miss still spends charge** and marks the weapon fired for the turn.

### Commands

| Command | Effect |
|---|---|
| `f` / `fire` | Interactive commit (weapon → target → shield face) |
| `r` / `ready` / `done` / `nofire` | `ready_fire` — done committing for this ship |
| `e` / `end` | **End whole turn** (asks confirm if in firing) — not “leave fire phase” |

On **first entry** into firing, the REPL may open the weapon menu **once**. Cancel
with weapon index **`-1`**. It does **not** auto-reopen after `r`/`done`.

### Example volley

```
f                 # pick beam, target, shield face → QUEUED
f                 # pick torp → QUEUED
r                 # ready this ship
# AI readies automatically when it can
# → FIRE RESOLUTION (HIT/MISS), then next phase
```

### Leaving fire **without** shooting

```
r
# or: done / nofire
```

Do **not** use `e` for that — `e` ends the **turn**.

### Traps

| Mistake | Result |
|---|---|
| Expect charge to drop on `f` alone | Charge drops on **resolve**, after all ready |
| Expect auto menu again after `r` | Menu is once per phase entry; type `f` to queue more before ready |
| `e` to “exit fire” | Ends turn; may wipe turn-scoped power on advance |
| Fire same weapon twice before resolve | Soft-reject (already committed / already fired) |
| Ready twice | “already ready — waiting for other ships” |

---

## 6. End turn and the move/fire loop

- After a fire resolution, if anyone can still make a **useful hex-changing** move
  or a **legal fire**, you get another **movement** phase (then firing again).
- **End Turn** (`e`) advances to the next turn’s allocate (illegal during allocate).
  Soft leftover warning may show if you still had useful actions.
- End turn **clears** turn-scoped allocation, charges, and combat log.

---

## 7. Prompt cheat sheet

Examples:

```
t1/allocate@1 draft11/22>
t1/movement@1*1>
t1/firing@1/r=done>
t1/firing@1/ready>
t1/turn_end@1>
```

| Fragment | Meaning |
|---|---|
| `t1` | Turn 1 |
| `allocate` / `movement` / `firing` / `turn_end` | Engine phase |
| `@1` | UI focus ship id |
| `*1` | ACTIVE mover (movement only) |
| `draft11/22` | Local allocate draft used/pool |
| `/r=done` | Fire phase; this ship not ready yet |
| `/ready` | This ship already readied |

---

## 8. Full command index (REPL)

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
| `ship N` / `sel N` | Focus ship (does not wipe a dirty allocate draft) |

### Allocate

`a`, `a N`, `mov`, `w`, `sh`, `b1`/`t1`/…, `commit`, `reset`, `cancel`, `ad` (quick default alloc)

### Movement

`m f|r|port|stbd`, `m 0..5`, `p`

### Firing

`f`, `r` / `done` / `nofire`, `e` (whole turn)

---

## 9. Reading outcomes

### After allocate

```
  engine accepted allocate #1: mov=…  weapons: …  shields=…
Δ phase allocate→movement …
```

### After move

Position/facing update on YOUR SHIP and MAP; `mov=` remaining drops.

### After fire resolve

```
*** FIRE RESOLUTION ***  (or panel)
A1 beam_1 → B2  HIT for N  on shield …
shots resolved this turn:
  beam_1 → #2 HIT …
  torp_1 → #2 MISS …
```

Weapon lines show `FIRED HIT` / `FIRED MISS` with empty charge bars.

### Soft errors

Illegal orders do **not** change state. Message includes `code` and often a short
hint (e.g. wrong phase). Fix the phase or ship and retry.

---

## 10. Suggested first fight (`scenarios/ai.toml`)

1. **Allocate** `A1`: some movement, charge `b1`, put power on forward shields, `commit`.
2. **Movement**: if ACTIVE, `m f` or `p` once.
3. **Firing**: optional `f`, then **`r`**.
4. Watch RECENT for HIT/MISS and shield/hull bars on contacts.
5. Repeat move/fire or `e` when the turn is done.
6. Session text log: path in footer / on quit under `local/session-*.log`.

---

## 11. Related docs

| Doc | Topic |
|---|---|
| `README.md` | Run flags, isolation, file map |
| `ASCII-UI.md` | Terminal presentation / UI engineering notes |
| `docs/PLAY-V2.md` | Rules summary (all clients) |
| `docs/PROTOCOL.md` | NDJSON orders/snapshots |
| ADR-0020 | Combat Model v2 decision |
