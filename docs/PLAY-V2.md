# Playing shipsim (Combat Model v2)

## Turn structure

Each **turn** has four phases, in order:

1. **Allocate** — spend your power pool on movement points, weapon
   charges, and shield facings. End turn to advance.
2. **Movement** — ships move in id order. The **active ship** is shown
   in the header. Move it (Forward / Turn port / Turn starboard) or
   Pass, then the next ship moves.
3. **Firing** — for each of your ships, pick a weapon, a target, and
   the shield facing the shot strikes, then **Commit Fire**. When done,
   **Ready** the ship. The core resolves all committed shots.
4. **Turn End** — **End Turn** advances to the next turn and refills
   power. If there is unresolved fire or unspent power, a warning
   dialog asks you to confirm.

## Allocate

- **Movement** — points spent on hex moves this turn.
- **Weapon charge** — 1..3; higher charge = more damage.
- **Shields** — 6 facings (F, FR, RR, R, RL, FL); power per facing.

## Move

- **Forward (W)** — one hex in facing direction.
- **Turn port / starboard** — change facing.
- **Pass (P)** — skip this ship's move.

## Fire

- Pick a **weapon** (must be charged in Allocate).
- Pick a **target** (click an enemy ship on the board, or use the panel).
- Pick the **shield facing** the shot strikes (0..5).
- **Commit Fire** — the core resolves the shot immediately and logs
  the result (hit/miss, damage, facing) to the combat log.
- **Ready (R)** — marks this ship as done firing for the turn.

## Controls

- **Right-click drag** — pan the board.
- **Mouse wheel** — zoom.
- **Ctrl - / =** — scale the UI.
- **? or H** — help overlay.
- **Esc** — back to scenario picker.

## Running

```
cargo run -- --scenario scenarios/combat.toml
luajit frontend/love/tests/run_all.lua   # frontend tests
love frontend/love                        # graphical client
```
