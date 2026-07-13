# shipsim external API (NDJSON protocol v3)

This is the **public engine API**. Frontends, scripts, and agent harnesses talk
to shipsim only through this boundary.

**Protocol version is `3`.** Clients and saves must use `protocol_version: 3`.
v1/v2 are rejected.

Play types: `docs/AGENT-PLAY.md`. Architecture: `docs/ARCHITECTURE.md`.

## Combat model (v3 summary)

| Topic | Rule |
|---|---|
| **Shields** | Re-bought every allocate from **0**. Unpowered facings give **no** protection. |
| **Weapons** | Charge **carries** across turns. Allocate pays only for **increases**; cannot strip carried charge. Hit or miss spends charge. |
| **To-hit** | Range-table d20 threshold × target `size / 2`, half-up, clamped to 1..20. Size 2 is neutral. |
| **Thrust** | Engine power → thrust via hull `thrust_per_power` / `power_per_thrust`. |
| **Max speed** | Global cap 8; per-hull `max_velocity` may be lower. |
| **Maneuvers** | `coast` (0), `accel` (1, along facing), `turn` `{facing}` (ring cost 1–3). |
| **Slide** | Each of 4 cycles: after maneuvers, each ship slides **`speed` hexes** along course. |

## CLI

```bash
cargo build -q
target/debug/shipsim --scenario scenarios/ai.toml --stdin
# one JSON order per line → one snapshot or error per line
```

Flags: `--scenario` / `--campaign` / `--resume`, `--stdin` / `--orders`, optional `--save`.

## Orders

Every order is one JSON object per line with `protocol_version: 3`.

### `allocate`

```json
{"protocol_version":3,"type":"allocate","ship":1,"movement":4,"weapons":{"beam_1":2},"shields":[2,0,0,0,0,2]}
```

- `movement` = engine **power** (converted to thrust).
- `weapons` = desired **total** charge per weapon id (≥ carried; ≤ max_charge). Power cost = sum of increases only.
- `shields` = six face powers (always from 0 this turn).

### `commit_maneuver`

```json
{"protocol_version":3,"type":"commit_maneuver","ship":1,"maneuver":{"type":"coast"}}
{"protocol_version":3,"type":"commit_maneuver","ship":1,"maneuver":{"type":"accel"}}
{"protocol_version":3,"type":"commit_maneuver","ship":1,"maneuver":{"type":"turn","facing":3}}
```

| `maneuver.type` | Effect |
|---|---|
| `coast` | No thrust; keep velocity/facing |
| `accel` | 1 thrust along **facing**: +speed if face=course; −speed if opposite; from stop → course=face, speed 1 |
| `turn` | Set facing 0..5; cost = hex ring distance (1/2/3) |

### `commit_fire` / `ready_fire` / `end_turn`

Unchanged in shape from v2 (still simultaneous fire resolve; miss spends charge).

```json
{"protocol_version":3,"type":"commit_fire","ship":1,"weapon":"beam_1","target":2,"shield_facing":0}
{"protocol_version":3,"type":"ready_fire","ship":1}
{"protocol_version":3,"type":"end_turn"}
```

## Phase loop

allocate (all ships) → 4× (movement commit all → slide → firing → ready all) → end_turn → allocate …

## Snapshots

Post-load and after each accepted order. `protocol_version: 3`. Includes phase,
ships (`size`, velocity, course, facing, thrust_remaining, weapon charge, shields),
combat_log, etc. See `src/snapshot.rs`.

`ships[].size` is a positive relative target silhouette. The engine scales the
range-table d20 hit threshold by `size / 2`; clients should use it when showing
hit previews rather than inferring size from class, structure, or drive ratio.

## Errors

Soft errors: `type: "error"`, `ok: false`, `code`, `message` — state unchanged.

## Compatibility

- Reject other protocol versions.
- Additive snapshot fields may appear; ignore unknowns.
- Golden fixtures under `tests/fixtures/` are optional and must match v3 when present.

## Related

| Doc | Role |
|---|---|
| `docs/AGENT-PLAY.md` | UI / API / sim play |
| `docs/SIMULATION.md` | Sim play |
| `frontend/repl/GAMEPLAY.md` | REPL commands |
