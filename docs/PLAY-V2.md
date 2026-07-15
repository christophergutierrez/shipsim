# Playing shipsim (Combat Model v3 / protocol 3)

Product rules for interactive play. Wire details: `docs/PROTOCOL.md`.
Play types (UI / API / sim): `docs/AGENT-PLAY.md`.

## Turn structure

Each **turn**:

1. **Allocate** — split power into engine (→ thrust), weapon charge **top-ups**, and
   six shield facings. Shields always start at **0** this turn. Weapon charge
   **carries** from last turn (you may only raise it, never strip it).
2. **Four movement/fire cycles** (fixed):
   - **Movement** — every living ship commits one maneuver (`coast` / `accel` /
     `turn` / `turn_accel`). Then every ship slides **`speed` hexes** along course
     (constant rate). Opposite-course ships may pass through the same hex.
   - **Firing** — optional `commit_fire`s, then all ships `ready_fire`; resolve
     simultaneously (hit **or miss** spends charge).
3. **End Turn** — next turn’s allocate. Velocity/course persist; thrust and
   shields do not; unspent weapon charge persists.

## Allocate

| Bucket | Notes |
|---|---|
| **Engine** | Power → thrust via hull `thrust_per_power` / `power_per_thrust` |
| **Weapons** | Desired totals ≤ max; cost = **increase** over carried charge only |
| **Shields** | Always rebuy from 0; unpowered face = no protection |

## Movement (protocol 3)

| Order | Effect |
|---|---|
| **coast** | 0 thrust; keep speed/course/facing; still slide `speed` hexes |
| **accel** | Thrust along **nose**: +1 speed if aligned (1); −1 if reverse (1); **oblique revector** costs `speed+1` → course=face, speed=1 |
| **turn N** | Face absolute 0..5; cost 1/2/3 by hex-ring distance; course unchanged |
| **turn N accel** | Turn then accel in **one** commit; cost = turn + accel |

**Course** = travel direction. **Facing** = nose/weapons/thrust axis. Max speed 8 (per-hull cap may be lower).

## Fire

- Queue zero or more legal shots (`commit_fire`), then **ready**.
- Miss still spends charge.
- Hit chance uses range and target size tables (`docs/combat-v2-tables.md`).
  Hull sizes are the seven tiers in `data/sizes.toml` (Fighter … Titan);
  baseline silhouette is size 2.

## Running

```bash
cargo build -q
python3 frontend/repl/repl.py scenarios/ai.toml          # UI play
python3 frontend/repl/client.py                            # API smoke
(cd frontend/repl && python3 -m unittest discover -s tests)
cargo test
# Love2D is protocol-2 and not supported until the TUI path is solid
```

Session logs: `frontend/<name>/local/` only.
