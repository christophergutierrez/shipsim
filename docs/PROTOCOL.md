# shipsim external API (NDJSON protocol v4)

This is the **public engine API**. Frontends, scripts, and agent harnesses talk
to shipsim only through this boundary.

**Protocol version is `4`.** Clients and saves must use `protocol_version: 4`.
v1–v3 are rejected (breaking; no dual-version engine).

Play types: `docs/AGENT-PLAY.md`. Architecture: `docs/ARCHITECTURE.md`.
Mechanics ADR: `docs/adr/0025-simplified-simultaneous-turns.md`.

## Combat model (v4 summary)

| Topic | Rule |
|---|---|
| **Turn** | Three collection stages: **allocate → path → volley**, then auto next allocate. No `end_turn`. |
| **Shields** | Re-bought every allocate from **0**. |
| **Weapons** | Charge **carries** across turns. Allocate pays only for **increases**; cannot strip. Hit or miss spends charge for weapons in the volley. |
| **To-hit** | Rules-table d20 threshold × target `size / 2`, half-up, clamped by the accuracy ceiling. Size 2 is neutral. |
| **Motion** | Engine power → motion points via hull `thrust_per_power` / `power_per_thrust`. Cap = `max_maneuver_actions` (engine SSD may lower). |
| **Path** | One ordered list of actions per living ship: `move_f`, `move_fr`, `move_fl`, `turn_right`, `turn_left` (cost 1 each). No velocity/course. |
| **Path resolve** | Simultaneous; intermediate crossings OK; final hex unique; stationary immovable; cost then seeded ties; losers fall back along translated history. |
| **Volley** | One `commit_volley` per ship (empty = hold fire). Simultaneous fire; destroyed attackers still fire full accepted volley. |

## CLI

```bash
cargo build -q
target/debug/shipsim --scenario scenarios/ai.toml --stdin
# one JSON order per line → one snapshot or error per line
```

Flags: `--scenario` / `--campaign` / `--resume`, `--stdin` / `--orders`, optional `--save`.

## Orders

Every order is one JSON object per line with `protocol_version: 4`.

### `allocate`

```json
{"protocol_version":4,"type":"allocate","ship":1,"movement":4,"weapons":{"beam_1":2},"shields":[2,0,0,0,0,2]}
```

- Staged until **every living ship** commits; then all applied together.
- `movement` = engine **power** (converted to motion points).
- `weapons` = desired **total** charge per weapon id (≥ carried; ≤ max_charge).
- `shields` = six face powers (always from 0 this turn).
- Partial allocation does **not** mutate public ship state.

### `commit_path`

```json
{"protocol_version":4,"type":"commit_path","ship":1,
 "actions":["move_f","move_fr","turn_left"]}
```

| Action | Position | Facing | Cost |
|---|---|---|---:|
| `move_f` | one hex through current F | unchanged | 1 |
| `move_fr` | one hex through current FR | +1 face | 1 |
| `move_fl` | one hex through current FL | −1 face | 1 |
| `turn_right` | unchanged | +1 face | 1 |
| `turn_left` | unchanged | −1 face | 1 |

- Path cost ≤ `motion_available`. Empty path is legal (stationary).
- Resolves when every living ship has committed a path.

### `commit_volley`

```json
{"protocol_version":4,"type":"commit_volley","ship":1,"shots":[
  {"weapon":"beam_1","target":2,"shield_facing":3},
  {"weapon":"torp_1","target":2,"shield_facing":3}
]}
```

- Empty `shots` holds fire.
- Each weapon at most once.
- Resolves when every living ship has committed; then turn advances automatically.

### Retired (rejected under v4)

`commit_maneuver`, `move`, `pass_move`, `commit_fire`, `ready_fire`, `end_turn`.

## Read-only requests

A line with a `request` field is **not** an order (no mutation, no save stream).

### `path_preview`

```json
{"protocol_version":4,"request":"path_preview","ship":1,
 "actions":["move_f","turn_right"]}
```

Returns cost, remaining motion, traced steps, final state, and optional first
illegal action index/reason.

### `reach_preview`

```json
{"protocol_version":4,"request":"reach_preview","ship":1,"budget":4}
```

Reachable `(q,r,facing)` endpoints up to `budget` (default: current
`motion_available`).

### `fire_preview`

```json
{"protocol_version":4,"request":"fire_preview","ship":1,"weapon":"beam_1","target":2}
```

Authoritative hit chance, projected damage, legal shield facings.

### Retired requests

`movement_preview`, `maneuver_options` → use `path_preview` / `reach_preview`.

## Snapshots

Every successful order (and the post-load line) emits a state snapshot with
`protocol_version: 4`.

| Field | Meaning |
|---|---|
| `phase` | `allocate` \| `movement` \| `firing` |
| `ships_allocated_this_turn` | Commitment progress for allocate |
| `ships_committed_path` | Path stage commits |
| `ships_committed_volley` | Volley stage commits |
| `path_results` | Last movement resolution telemetry (cost, fallback, conflicts, final hex/facing) |
| ship `max_maneuver_actions` | Hull path-action cap |
| ship `motion_available` | Usable motion points in movement stage |
| `combat_log` | Resolved shots only (cleared at turn rollover) |
| `fire_opportunity` | Optional player legal-shot advisory |

**Never** includes staged opponent allocation/path/volley payloads, velocity,
course, `movement_phase`, `ready_fire`, or `end_turn_warning`.

### `path_results` entry

```text
ship, submitted_cost, translated_steps, final_q, final_r, final_facing,
fallback_steps, blocked_kind?, conflicting_ships[]
```

## Saves

- Save documents use `protocol_version: 4`.
- v3 (and older) saves fail with `UnsupportedVersion` before order deserialization.
- Replay is exact, including collision-tie PRNG outcomes.

## Turn loop

```text
allocate (all living) → apply together
  → commit_path (all living) → resolve paths
  → commit_volley (all living) → resolve fire → turn++ → allocate
```

Terminal conditions are checked after movement and after firing. If the
scenario ends, no further collection stage starts.

## Errors

Soft errors (process stays alive): `parse_error`, `unsupported_protocol`,
`order_illegal`, `preview_invalid`, `unknown_request`, `retired_request`.
