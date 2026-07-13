# Playing shipsim (Combat Model v2)

## Turn structure

Each **turn**:

1. **Allocate** — split each ship's **power pool** into movement power, weapon
   charges, and six shield facings (sum ≤ power), then confirm.
2. **Movement phase** — ships act **one at a time** in **initiative order**
   (highest **movement allocation** first; ties broken once per turn).
   Only the **ACTIVE** ship can move. Forward / Reverse / Turn / Pass, then
   the next ship. When every living ship has decided (or has no move power
   left), go to Firing.
3. **Firing phase** — commit zero or more legal shots (weapon + target +
   shield facing), then **Ready** each of your ships. When **all** living
   ships are Ready, all commits resolve **simultaneously**.
4. Repeat Movement → Firing while anyone can still make a **useful hex move**
   or a **legal fire**. Otherwise the turn ends (or use **End Turn** early).
5. **End Turn** — advances to the next turn. If useful move/fire still
   exists, the UI warns about leftover power.

There is no separate “end round” counter; the turn is a loop of move/fire pairs.

## Allocate

- **Movement** — **power units** for steps this turn (not “hex count”).
  Reverse after going forward costs **2** units; forward/turn usually **1**.
- **Weapon charge** — beam 1..max (all spent on one shot); plasma/torp 0 or 1.
- **Shields** — power per facing (F, FR, RR, R, RL, FL), up to ship max per face.

## Move (ACTIVE ship only)

- **Forward** — one hex ahead (cost 1, or 2 if reversing keel from reverse).
- **Reverse** — one hex aft (cost 1, or 2 if keel was forward).
- **Turn port / starboard** — change facing (cost 1; does not flip keel).
- **Pass** — skip this ship's decision this movement phase.

With two player ships: finish **End/Pass on ship #1**, then ship #2 becomes Active.

## Fire

- Pick a **weapon** (must still have charge; not already fired this turn).
- Pick a **target** and **shield facing** (only geometry-legal facings).
- **Commit Fire** — queues the shot; does not resolve yet.
- **Ready** — this ship is done committing. When every living ship is Ready,
  resolve all shots together (hit **or miss**).
- **A miss still uses the weapon charge** and marks the weapon fired for the turn.
- Beam: more charge → more damage; long range may need higher charge or the
  shot is illegal (would deal 0 after rounding).

## Controls

- **Right-click drag** — pan the board.
- **Mouse wheel** — zoom.
- **Ctrl - / =** — scale the UI.
- **? or H** — help overlay.
- **Esc** — back to scenario picker.

## Running

```
cargo build
python3 frontend/repl/repl.py scenarios/ai.toml   # interactive UI (play as user)
python3 frontend/repl/client.py                   # non-interactive API smoke
(cd frontend/repl && python3 -m unittest discover -s tests)  # REPL automated suite
luajit frontend/love/tests/run_all.lua            # Love unit tests
love frontend/love                                # graphical client (secondary)
```

All clients and their session logs live under `frontend/<name>/` (see
`frontend/README.md`). Do not scatter frontend scratch at the repo root, in
repo `/tmp`, or system `/tmp`.

**API:** `docs/PROTOCOL.md`. **Agents (tests vs UI play):** `docs/AGENT-PLAY.md`,
root `AGENTS.md`.
