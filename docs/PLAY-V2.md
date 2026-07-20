# Playing shipsim (simplified turns / protocol 4)

Product rules for interactive play. Wire details: `docs/PROTOCOL.md`.
Play types (UI / API / sim): `docs/AGENT-PLAY.md`.
ADR: `docs/adr/0025-simplified-simultaneous-turns.md`.

## Turn structure

Each **turn** has three collection stages:

1. **Allocate** — split reactor power into motion power, weapon charge
   **top-ups**, and six shield facings. Shields always start at **0**. Weapon
   charge **carries** (raise only). Orders are staged until every living ship
   commits, then applied together.
2. **Path** — each living ship submits one ordered list of path actions
   (`move_f` / `move_fr` / `move_fl` / `turn_right` / `turn_left`). Cost is 1
   motion point each, capped by converted allocation and
   `max_maneuver_actions`. All paths resolve simultaneously.
3. **Volley** — each living ship submits one complete volley (zero or more
   shots). Empty volley = hold fire. Resolve simultaneously, then the engine
   **automatically** advances to the next turn's allocate.

There is **no** `end_turn`, **no** four-cycle impulses, **no** persistent
velocity/course, and **no** `ready_fire`.

## Allocate

| Bucket | Notes |
|---|---|
| **Motion** | Power → motion points via hull `thrust_per_power` / `power_per_thrust` |
| **Weapons** | Desired totals ≤ max; cost = **increase** over carried charge only |
| **Shields** | Always rebuy from 0; unpowered face = no protection |

## Path language

| Action | Position | Facing |
|---|---|---|
| `move_f` | one hex through current F | unchanged |
| `move_fr` | through FR | rotate right |
| `move_fl` | through FL | rotate left |
| `turn_right` | unchanged | rotate right |
| `turn_left` | unchanged | rotate left |

- Cannot translate through R/RR/RL without turning first (or successive
  F/FR/FL moves).
- Facing persists between turns. Position + facing are the full motion state.
- Stationary ships (no `move_*`) cannot be displaced.
- Contested endpoints: higher path cost wins; equal cost uses seeded PRNG;
  losers fall back along translated history.

## Fire (volley)

- One `commit_volley` per ship with zero or more shots.
- Miss still spends charge for weapons in the volley.
- Geometry frozen at fire start; ships alive at start complete their volley
  even if destroyed mid-resolution.
- Hit chance uses range and target size tables (`docs/combat-v2-tables.md`).

## Running

```bash
cargo build -q
python3 frontend/repl/repl.py scenarios/ai.toml          # UI play (reference)
python3 frontend/repl/client.py                            # API smoke
(cd frontend/repl && python3 -m unittest discover -s tests)
cargo test -q
cargo run --release --bin shipsim-sim -- --suite simulation/suites/smoke.toml
```

**Clients:** REPL is the protocol-v4 reference. Ratatui TUI and Love2D still
need full path/volley UX migration (see plan Phases 7–8).

Session logs: `frontend/<name>/local/` only.

## Balance note

Protocol-v3 balance reports and win rates are **not comparable** to v4.
Post-migration baselines are advisory until a separate retuning campaign.
