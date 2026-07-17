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
| **To-hit** | Rules-table d20 threshold × target `size / 2`, half-up, clamped by the configured accuracy ceiling below the die maximum. Size 2 is neutral. |
| **Thrust** | Engine power → thrust via hull `thrust_per_power` / `power_per_thrust`. |
| **Max speed** | Global cap 8; per-hull `max_velocity` may be lower. |
| **Maneuvers** | `coast` (0); `accel` along facing (+1 / −1 / revector cost `speed+1`); `turn` `{facing}` (ring 1–3); `turn_accel` (turn then accel, sum of costs). |
| **Slide** | Each of 4 cycles: after maneuvers, slide **`speed` hexes** along course. Opposite-course ships may **pass through** the same mid-hex. |

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
{"protocol_version":3,"type":"commit_maneuver","ship":1,"maneuver":{"type":"turn_accel","facing":1}}
```

| `maneuver.type` | Effect |
|---|---|
| `coast` | No thrust; keep velocity/facing |
| `accel` | Along **facing**: +1 speed if aligned (cost 1); −1 if reverse (1); from stop → course=face, speed 1 (1); **oblique revector** → course=face, speed 1, cost `speed+1` |
| `turn` | Set facing 0..5; cost = hex ring distance (1/2/3); course unchanged |
| `turn_accel` | Turn to facing, then one accel from that nose; cost = turn + accel |

### `commit_fire` / `ready_fire` / `end_turn`

Unchanged in shape from v2 (still simultaneous fire resolve; miss spends charge).

```json
{"protocol_version":3,"type":"commit_fire","ship":1,"weapon":"beam_1","target":2,"shield_facing":0}
{"protocol_version":3,"type":"ready_fire","ship":1}
{"protocol_version":3,"type":"end_turn"}
```

## Read-only requests

A line carrying a `request` field is **not an order**: it never mutates game
state, is excluded from save/replay, and returns a request-specific envelope
instead of a snapshot. The `request` field is checked after `protocol_version`
and before order dispatch, so an unknown `request` is reported as
`unknown_request` rather than mis-parsed as an order.

### `movement_preview`

Computes the reachable endpoint set for one ship over the four movement phases
of the current turn, given a complete local allocation draft. The engine — not
the client — owns the power→thrust conversion, so the request fields mirror
`allocate` exactly.

```json
{"protocol_version":3,"request":"movement_preview","ship":1,
 "movement":4,"weapons":{"beam_1":2},"shields":[2,0,0,0,0,2]}
```

| Field | Meaning |
|---|---|
| `ship` | Ship id (integer). Required. |
| `movement` | Engine **power** allocated to movement (converted to thrust). Defaults to 0. |
| `weapons` | Desired **total** charge per weapon id, same semantics as `allocate`. |
| `shields` | Six face powers, same semantics as `allocate`. Missing/short arrays are zero-padded. |
| `clamp` | Optional bool (default `false`). When `true`, an over-allocated draft does not reject: movement power is clamped down to the affordable residual (after weapons + shields) and the reachable set for that clamped thrust is returned. Weapon/shield *validity* (charge caps, per-facing caps, no-stripping) is still enforced — only the total budget is relaxed. Used by the TUI for live slider-drag previews. |

Legal during `allocate` and `movement` phases only; rejected (`preview_invalid`)
during firing/turn-end.

Response envelope (`type: "movement_preview"`, `ok: true`):

```json
{"type":"movement_preview","protocol_version":3,"ok":true,"ship":1,
 "endpoints":[{"q":0,"r":4,"facing":0,"course":0,"speed":0,"thrust_remaining":4}, ...],
 "coast":{"q":0,"r":4,"facing":0,"course":0,"speed":0,"thrust_remaining":4},
 "occupied":[{"q":8,"r":4}]}
```

| Field | Meaning |
|---|---|
| `endpoints` | Sorted reachable endpoints after four cycles. Two sequences arriving at the same hex with different facing/course/speed are both retained. |
| `coast` | The single endpoint reached by coasting all four cycles (the do-nothing trajectory). |
| `occupied` | Endpoints that coincide with another ship's current hex (collisions the client should flag). |
| `clamped_movement` | Present only when `clamp: true` was requested. The effective movement power after clamping to the affordable residual, so the client can show the actual thrust being previewed. |

The preview is a pure projection: it shares maneuver and hard-map edge rules
with live movement, but never touches game state. Other ships' future moves
remain unknown, so occupied endpoints are advisory rather than a promise that
their current occupants will still be there at resolution. See
`src/movement_preview.rs` (ADR-0022 preview contract).

### `maneuver_options`

Returns every immediate maneuver with its authoritative thrust cost and whether
the focused ship can afford it. Legal only during movement.

```json
{"protocol_version":3,"request":"maneuver_options","ship":1}
```

```json
{"type":"maneuver_options","protocol_version":3,"ok":true,"ship":1,
 "options":[
   {"maneuver":{"type":"coast"},"thrust_cost":0,"affordable":true},
   {"maneuver":{"type":"turn","facing":3},"thrust_cost":3,
    "affordable":false,"reason":"need 3, have 2"}
 ]}
```

Invalid maneuvers remain in the list with `thrust_cost:null`,
`affordable:false`, and an explanatory `reason`. This lets clients explain or
disable choices without reproducing motion rules.

### `fire_preview`

Returns the hit threshold, percentage, projected damage, and geometrically
legal shield facings for one weapon/target pairing. Legal only during firing.
It does not queue the shot, roll the die, consume charge, or advance the PRNG.

```json
{"protocol_version":3,"request":"fire_preview","ship":1,
 "weapon":"beam_1","target":2}
```

```json
{"type":"fire_preview","protocol_version":3,"ok":true,"legal":true,
 "ship":1,"weapon":"beam_1","target":2,"range":3,
 "threshold":19,"die_sides":20,"hit_percent":95,"projected_damage":7,
 "legal_shield_facings":[3]}
```

An illegal pairing is still a successful read-only request and returns
`legal:false` with `reason`. Malformed requests return `preview_invalid`.

## Phase loop

allocate (all ships) → 4× (movement commit all → slide → firing → ready all) → end_turn → allocate …

## Snapshots

Post-load and after each accepted order. `protocol_version: 3`. Includes phase,
ships (`size`, velocity, course, facing, thrust_remaining, weapon charge, shields),
combat_log, etc. See `src/snapshot.rs`.

**Additive snapshot fields (no protocol bump):**

| Field | Meaning |
|---|---|
| `translation_results` | After each resolved movement phase: per living ship that attempted a slide, `{ship, requested, moved, blocked?}`. `requested` is post-maneuver velocity (hexes scheduled); `moved` is hexes actually translated; `blocked.kind` is `edge` \| `occupied` \| `contested` with optional `ships` ids. Replaced on the next movement resolution; empty before the first. |
| `fire_opportunity` | Optional one legal shot `{ship, weapon, target, legal_shield_facings}` when any living ship still has a currently legal fire. Absent when none. |
| `end_turn_warning` | Boolean advisory: **true iff `fire_opportunity` is present**. Never blocks `end_turn`. |

`ships[].size` is a positive relative target silhouette. The engine scales the
range-table d20 hit threshold by `size / 2`; clients should use it when showing
hit previews rather than inferring size from class, structure, or drive ratio.

`ships[].attack_accuracy_bonus`, when present, is the catalog fire-control
modifier applied against exact size-2 targets. It is omitted when zero. Clients
can combine it with `docs/combat-v2-tables.md` when showing hit previews; the
final threshold is capped at the configured `combat.accuracy.ceiling_max`
(`data/rules/default.toml`, ADR-0024), itself always kept below the die
maximum — no attack, modified or not, is ever a guaranteed hit. Schema version
1 (the only version this protocol currently supports) requires a d20
(`combat.die_sides = 20`); alternate dice systems are a future schema version.

Each snapshot weapon's `max_range` is its effective instance range. The engine
also validates it against the configured kind table, so clients must not infer
that every weapon of a kind shares the table maximum.

Top-level `rules_id` and `rules_fingerprint` identify the canonical ruleset
(`data/rules/default.toml`, ADR-0024) the game was loaded with. These are
diagnostic-only: two snapshots, saves, or simulation reports with different
fingerprints used different combat data and should not be compared as if they
were equivalent. Clients do not load rules TOML themselves and never decide
order legality from these fields — the engine remains authoritative.

## Errors

Soft errors: `type: "error"`, `ok: false`, `code`, `message` — state unchanged.

| `code` | When |
|---|---|
| `unsupported_protocol` | `protocol_version` is not 3. |
| `parse_error` | Order JSON does not deserialize. |
| `order_illegal` | Order violates a rule (phase, power, thrust, range, …). |
| `unknown_request` | `request` field names a request the engine does not know. |
| `preview_invalid` | `movement_preview` rejected (bad/missing `ship`, wrong phase, ship not found, …). |

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
