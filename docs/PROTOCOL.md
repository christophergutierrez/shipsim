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
| **Weapons** | Charge **carries** across turns. Allocate pays only for **increases**; cannot strip. Hit or miss spends charge for weapons in the volley. Repeat beams emit one packet per charge; Pierce bypasses shields after halving damage. Torpedoes and missiles use finite magazines; missiles deal flat 2 damage. |
| **To-hit** | Rules-table d20 threshold × target `size / 2`, half-up, clamped by the accuracy ceiling. Size 2 is neutral. Weapon and computer accuracy apply before cloak, ECM, evasion, natural defense, and the final clamp. Hull fire-control remains size-2-only. Cloak is −4 to incoming fire; ECM is −2 against missiles. |
| **Motion** | Engine power → motion points via hull `thrust_per_power` / `power_per_thrust`. Cap = `max_maneuver_actions` (engine SSD may lower). |
| **Path** | One ordered list of actions per living ship: `move_f`, `move_fr`, `move_fl`, `turn_right`, `turn_left` (cost 1 each), plus optional `evasive` motion points from the same budget. No velocity/course. |
| **Path resolve** | Simultaneous; intermediate crossings OK; independent final hexes stay unique; squads may share one hex and move on the leader's path. Enemy occupancy is illegal; losers fall back along translated history. |
| **Volley** | One `commit_volley` per ship (empty = hold fire). Simultaneous fire; destroyed attackers still fire full accepted volley. |

## CLI

```bash
cargo build -q
target/debug/shipsim --scenario scenarios/battle.toml --stdin
# one JSON order per line → one snapshot or error per line
```

Flags: `--scenario` / `--campaign` / `--resume`, `--stdin` / `--orders`, optional `--save`.

## Two-player sessions

The optional session host preserves the same NDJSON messages over TCP:

```bash
cargo run --bin shipsim-session -- --scenario scenarios/shipyard_assault.toml --listen 127.0.0.1:4100
```

Connect two ordinary NDJSON clients to the address. The first connection is
side `a`; the second is side `b`. The host rejects orders naming a ship owned
by the other connection, broadcasts accepted snapshots to both clients, and
does not add a turn-taking phase: allocation, movement, and volley barriers
remain simultaneous. A client disconnect ends the session.

The session host uses blocking collection: it waits indefinitely for the next
order from either connected client. There is no per-turn timeout; a slow client
can therefore pause progress until it submits or disconnects.

Snapshots use `InProgress`, `Won`, `Lost`, or `Draw` for `status`. `Draw` is a
terminal neutral result: neither side is the winner, and both session viewers
receive `Draw`.

## Orders

Every order is one JSON object per line with `protocol_version: 4`.

### `allocate`

```json
{"protocol_version":4,"type":"allocate","ship":1,"movement":4,"weapons":{"beam_1":2},"shields":[2,0,0,0,0,2]}
```

- Staged until **every living ship** commits; then all applied together.
- `movement` = engine **power** (converted to motion points).
- `weapons` = desired **total** charge per weapon id (≥ carried; ≤ max_charge).
  Charge *increases* on a weapon with 0 remaining ammo are rejected
  (`weapon_out_of_ammo`). Listing a dry weapon at its current charge is a no-op.
- `shields` = six face powers (always from 0 this turn), each ≤ that face's
  cap (`max_shields[i]` when present, otherwise `max_shield_per_facing`).
- Partial allocation does **not** mutate public ship state.

### `commit_path`

```json
{"protocol_version":4,"type":"commit_path","ship":1,
 "actions":["move_f","move_fr","turn_left"],"evasive":2}
```

| Action | Position | Facing | Cost |
|---|---|---|---:|
| `move_f` | one hex through current F | unchanged | 1 |
| `move_fr` | one hex through current FR | −1 face | 1 |
| `move_fl` | one hex through current FL | +1 face | 1 |
| `turn_right` | unchanged | −1 face | 1 |
| `turn_left` | unchanged | +1 face | 1 |

**Turn handedness:** `turn_right` rotates **clockwise** (decreases the facing
index); `turn_left` rotates **counterclockwise** (increases it). On the standard
map (`r` increases downward, `x ∝ q + r/2`) these read as on-screen right/left.
Facing 0 = +q (East); turning right from East points Southeast (facing 5 `↘`).

- Path cost + optional `evasive` (default 0) ≤ `motion_available`. Empty path is
  legal (stationary); pure jink (`actions:[]` with `evasive > 0`) is legal.
- `evasive` reduces the ship's chance to be hit during that turn's volley stage
  (`threshold -= evasive * combat.accuracy.evasion_per_point`, floor 1). It is
  visible as `evasion_committed` on ship snapshots after path resolve and resets
  to 0 at the next allocate.
- Resolves when every living ship has committed a path.
- Known limitation: `path_preview` does not yet accept drafted `evasive` (only
  committed state after resolve is reflected in fire previews).

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

### `purchase`

```json
{"protocol_version":4,"type":"purchase","side":"a","class":"basic_swarm"}
```

Purchases are accepted during `allocate`. The engine charges the requesting
side's credits and spawns the selected class on an adjacent free hex at that
side's shipyard. Credits start at 100, carry over, and increase by 100 at each
new turn. Prices and legal classes come from the snapshot; `shipyard` is never
purchasable. A blocked yard rejects the purchase without charging credits.

### `purchase_custom`

```json
{"protocol_version":4,"type":"purchase_custom","side":"a","design":{"id":"in_match_scout","name":"In-match scout","size":1,"material":"titanium","engine":"fission","engine_size":"s","shields":[0,0,0,0,0,0],"weapons":[{"component":"plasma","mount":"forward"}],"systems":[]}}
```

The engine validates the design through the same yard projection and charges
that projection's `design_cost`. Custom designs are reusable for the rest of
the match, but are held in memory and never written into `data/designs/`.

Allocate accepts optional `cloak`, `repair`, `unsquad`, and `squad_leader`
fields. `commit_path` accepts `follow: true` for a squad member; followers
must submit an empty action list. Ship snapshots expose `squad_id`,
`squad_leader`, and `squad_members` when applicable.

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
| `winner` | Optional winning side (`a` or `b`) after a side-relative terminal |
| `credits` | Per-side credit balances, keyed by `a` and `b` |
| `purchasable` | Engine catalog entries: `{class, cost}`; excludes scenario furniture |
| `ships_allocated_this_turn` | Commitment progress for allocate |
| `ships_committed_path` | Path stage commits |
| `ships_committed_volley` | Volley stage commits |
| `path_results` | Last movement resolution telemetry (cost, fallback, conflicts, final hex/facing) |
| ship `class_id` | Canonical catalog key (ship-definition file stem). Presentation clients key art off this; distinct from numeric `id` and display `class`. Art contract: `frontend/love/assets/ship_art/README.md` |
| ship `side` | `a` or `b`; team membership, independent of `controller` |
| ship `max_maneuver_actions` | Hull path-action cap |
| ship `effective_max_maneuver_actions` | Current path-action cap after engine/SSD modifiers. **`0` means engines are gone** (cannot move). Missing field (old snapshot) is the only case that falls back to `max_maneuver_actions`. |
| ship `max_shield_per_facing` | Scalar face cap (stock catalog; also the max of `max_shields`) |
| ship `max_shields` | Optional six face caps from the yard (F…FL). Absent → use the scalar |
| ship `motion_available` | Usable motion points in movement stage |
| `combat_log` | Resolved shots only (cleared at turn rollover) |
| ship `systems` | Installed computer, cloak, repair, or ECM systems |
| ship `repair_cap` | Optional engine-authoritative repair-box ceiling; present for ships with repair installed |
| ship `cloaked` | Whether cloak was purchased for the current turn |
| ship `squad_id` / `squad_leader` / `squad_members` | Declared squad membership |
| `fire_opportunity` | Optional player legal-shot advisory |

Weapon snapshots may additionally contain `accuracy_bonus` and `damage_bonus` for
custom quality weapons. They are omitted for zero-valued stock weapons so
existing snapshot JSON remains byte-identical. `accuracy_bonus` raises the final
to-hit threshold (and is capped below an automatic hit); `damage_bonus` is added
after the base kind/range damage calculation.

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
