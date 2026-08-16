# shipsim external APIs (game protocol v4 and session protocol v1)

This is the **public engine API**. Frontends, scripts, and agent harnesses talk
to shipsim only through this boundary.

**Protocol version is `4`.** Clients and saves must use `protocol_version: 4`.
v1–v3 are rejected (breaking; no dual-version engine).

Pre-match network lobby messages use the independent
`session_protocol_version: 1` contract documented below. Session version 1
does not change or wrap game protocol v4.

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
cargo run --bin shipsim-session -- --listen 127.0.0.1:4100
```

The host starts with an empty lobby. A client first sends session-v1 `hello`,
then the first compatible client receives host permission and the catalog. It
creates exactly one match using a catalog ID; Side A is that client's Human
seat and Side B may be Human, a server Bot, or an external LLM agent. A
preselected convenience launch is also available:

```bash
cargo run --bin shipsim-session -- --scenario shipyard_assault --listen 127.0.0.1:4100
```

Human and LLM Side-B seats use a one-time private join token. Bot seats are
internal and become ready immediately. Once assigned, clients exchange the
ordinary protocol-v4 orders and snapshots on the same connection. Authorization
is by assigned side, not connection index or the ship's presentation
`controller` label; accepted snapshots are viewer-relative and errors/previews
are private.

The session host uses blocking collection: it waits indefinitely for the next
order from either connected client. There is no per-turn timeout; a slow client
can therefore pause progress until it submits or disconnects.

Snapshots use `InProgress`, `Won`, `Lost`, or `Draw` for `status`. `Draw` is a
terminal neutral result: neither side is the winner, and both session viewers
receive `Draw`.

The server uses blocking collection during a running match: a slow participant
leaves the server alive and progress waits for that participant or a
disconnect. Pre-hello sockets have a bounded handshake timeout and do not
claim host permission until negotiation succeeds.

## Session/lobby protocol v1

Session v1 is NDJSON: one JSON object per line. Every lobby message carries
`session_protocol_version: 1`. A client starts with `hello`; the server chooses
the highest mutually supported session and game versions. Once the server has
sent `seat_assigned` and the lobby reaches `running`, ordinary protocol-v4
orders, preview requests, responses, and snapshots are sent unwrapped on the
same connection.

The lifecycle is:

```text
Disconnected → Connected → HelloNegotiated → LobbyUnconfigured
  → LobbyConfigured → WaitingForSeats → Running → Finished
```

Out-of-order lobby messages are rejected without changing lobby state. The
first successfully negotiated client in an unconfigured lobby receives Match
host permission (`can_configure: true`). Host permission permits
`create_match`; authorization during a Match comes only from the Side assigned
by `seat_assigned`.

### Controller and lobby values

| Type | JSON value | Meaning |
|---|---|---|
| Human controller | `{"kind":"human"}` | A person using any compatible client |
| Bot controller | `{"kind":"bot","policy":"greedy"}` | A server-owned, seeded policy |
| LLM agent controller | `{"kind":"llm_agent"}` | An external model-backed API client |
| Lobby phase | `unconfigured`, `configured`, `waiting_for_seats`, `running`, `finished` | Public lobby state |
| Seat occupancy | `vacant`, `reserved`, `occupied`, `internal` | Whether a network participant or server Bot fills a Seat |
| Participant status | `ready`, `thinking`, `error` | Advisory agent/client status; never a game order |

`create_match.controllers.a` and `.b` specify both Seats. In session v1 the
initial host normally configures A as Human and chooses B as Human, Bot, or LLM
agent. The data shape remains side-symmetric.

### Client messages

| `type` | Required fields | Purpose |
|---|---|---|
| `hello` | `client_kind`, `display_name`, `supported_session_versions`, `supported_game_protocol_versions` | Negotiate before any other request |
| `create_match` | `scenario_id`, `controllers.a`, `controllers.b` | Match-host configuration using a catalog ID |
| `join_match` | `join_token`, `display_name` | Consume a one-time token for a reserved Seat |
| `participant_status` | `status` | Advisory readiness/thinking/error state |

`client_kind` is one of `tui`, `repl`, `love2d`, `agent`, or `test`.
Unknown fields are rejected. In particular, `create_match` has no API-key,
authorization, provider-URL, model, prompt, or conversation fields.

### Server messages

| `type` | Required fields | Purpose |
|---|---|---|
| `welcome` | negotiated game version, connection ID, host permission, capabilities | Completes negotiation |
| `scenario_catalog` | `scenarios[]` containing only `id` and `display_name` | Projects the server allowlist without paths |
| `lobby_state` | phase, optional scenario/controllers, Seats, Bot policies, waiting reason | Complete public pre-match status |
| `seat_assigned` | Side, Match ID, participant ID | Binds the connection to one Seat |
| `seat_invitation` | Side, display code, one-time join token | Private invitation sent only to the host |
| `error` | stable code and message | Private session error |

Capabilities are `scenario_catalog`, `preview_requests`, and
`participant_status`. Bot policy IDs are server-advertised; clients must not
invent or treat their local labels as authoritative.

### Stable session error codes

Session v1 defines:

```text
unsupported_session_version  unsupported_game_protocol
host_required                lobby_already_configured
unknown_scenario             unknown_controller
unknown_bot_policy           seat_not_ready
invalid_join_token           seat_already_occupied
ownership                    request_rejected
invalid_state                invalid_message
participant_disconnected
```

Version errors occur during negotiation. Catalog, controller, host, token, and
state errors occur before play. `ownership` and `request_rejected` apply once a
Seat submits game-v4 traffic. Errors are private to the responsible
connection.

### Scenario catalog and path safety

`data/scenario_catalog.toml` is the server-owned allowlist. Clients see only
enabled identifiers and display names and submit the identifier, never a path.
Identifiers are lowercase ASCII tokens containing digits, `_`, or `-`.
Resolution rejects empty identifiers, `.`, `..`, absolute paths, `/` or `\`,
unknown IDs, disabled entries, unsafe configured paths, missing files, and
symlinks resolving outside the configured `scenarios/` directory.

### Credentials and invitations

Provider credentials, authorization headers, provider URLs, prompts, and model
conversations belong to the external agent process. They are never valid
session messages. A Seat join token is session data but is private: the full
token is sent only in `seat_invitation`, consumed once, masked by clients, and
excluded from ordinary logs. Display codes are non-authorizing presentation
helpers.

### Complete Human-vs-Bot trace

The single Human connection negotiates, configures the lobby, receives Side A,
and reaches `running`; the Bot occupies Side B internally. Prefixes indicate
direction and are not part of NDJSON.

```text
C→S {"type":"hello","session_protocol_version":1,"client_kind":"tui","display_name":"Captain A","supported_session_versions":[1],"supported_game_protocol_versions":[4]}
S→C {"type":"welcome","session_protocol_version":1,"game_protocol_version":4,"connection_id":"connection-a","can_configure":true,"capabilities":["scenario_catalog","preview_requests","participant_status"]}
S→C {"type":"scenario_catalog","session_protocol_version":1,"scenarios":[{"id":"shipyard_assault","display_name":"Shipyard Assault"}]}
S→C {"type":"lobby_state","session_protocol_version":1,"state":"unconfigured","scenario":null,"controllers":null,"seats":[],"bot_policies":[{"id":"greedy","display_name":"Greedy"}],"waiting_reason":"host must configure the lobby"}
C→S {"type":"create_match","session_protocol_version":1,"scenario_id":"shipyard_assault","controllers":{"a":{"kind":"human"},"b":{"kind":"bot","policy":"greedy"}}}
S→C {"type":"seat_assigned","session_protocol_version":1,"side":"a","match_id":"match-1","participant_id":"participant-a"}
S→C {"type":"lobby_state","session_protocol_version":1,"state":"running","scenario":{"id":"shipyard_assault","display_name":"Shipyard Assault"},"controllers":{"a":{"kind":"human"},"b":{"kind":"bot","policy":"greedy"}},"seats":[{"side":"a","controller":{"kind":"human"},"occupancy":"occupied","display_name":"Captain A","ready":true},{"side":"b","controller":{"kind":"bot","policy":"greedy"},"occupancy":"internal","display_name":"Greedy","ready":true}],"bot_policies":[{"id":"greedy","display_name":"Greedy"}],"waiting_reason":null}
```

### Complete Human-vs-Human trace

The host receives Side A and a private invitation. The second TUI negotiates,
joins the reserved Seat, and receives Side B.

```text
A→S {"type":"hello","session_protocol_version":1,"client_kind":"tui","display_name":"Captain A","supported_session_versions":[1],"supported_game_protocol_versions":[4]}
S→A {"type":"welcome","session_protocol_version":1,"game_protocol_version":4,"connection_id":"connection-a","can_configure":true,"capabilities":["scenario_catalog","preview_requests","participant_status"]}
S→A {"type":"scenario_catalog","session_protocol_version":1,"scenarios":[{"id":"shipyard_assault","display_name":"Shipyard Assault"}]}
S→A {"type":"lobby_state","session_protocol_version":1,"state":"unconfigured","scenario":null,"controllers":null,"seats":[],"bot_policies":[{"id":"greedy","display_name":"Greedy"}],"waiting_reason":"host must configure the lobby"}
A→S {"type":"create_match","session_protocol_version":1,"scenario_id":"shipyard_assault","controllers":{"a":{"kind":"human"},"b":{"kind":"human"}}}
S→A {"type":"seat_assigned","session_protocol_version":1,"side":"a","match_id":"match-2","participant_id":"participant-a"}
S→A {"type":"seat_invitation","session_protocol_version":1,"side":"b","display_code":"ABCD-EFGH","join_token":"fixture-token-b"}
S→A {"type":"lobby_state","session_protocol_version":1,"state":"waiting_for_seats","scenario":{"id":"shipyard_assault","display_name":"Shipyard Assault"},"controllers":{"a":{"kind":"human"},"b":{"kind":"human"}},"seats":[{"side":"a","controller":{"kind":"human"},"occupancy":"occupied","display_name":"Captain A","ready":true},{"side":"b","controller":{"kind":"human"},"occupancy":"reserved","display_name":null,"ready":false}],"bot_policies":[],"waiting_reason":"waiting for side b"}
B→S {"type":"hello","session_protocol_version":1,"client_kind":"tui","display_name":"Captain B","supported_session_versions":[1],"supported_game_protocol_versions":[4]}
S→B {"type":"welcome","session_protocol_version":1,"game_protocol_version":4,"connection_id":"connection-b","can_configure":false,"capabilities":["scenario_catalog","preview_requests","participant_status"]}
B→S {"type":"join_match","session_protocol_version":1,"join_token":"fixture-token-b","display_name":"Captain B"}
S→B {"type":"seat_assigned","session_protocol_version":1,"side":"b","match_id":"match-2","participant_id":"participant-b"}
S→A,B {"type":"lobby_state","session_protocol_version":1,"state":"running","scenario":{"id":"shipyard_assault","display_name":"Shipyard Assault"},"controllers":{"a":{"kind":"human"},"b":{"kind":"human"}},"seats":[{"side":"a","controller":{"kind":"human"},"occupancy":"occupied","display_name":"Captain A","ready":true},{"side":"b","controller":{"kind":"human"},"occupancy":"occupied","display_name":"Captain B","ready":true}],"bot_policies":[],"waiting_reason":null}
```

### Complete Human-vs-LLM-agent trace

The lobby knows only that Side B is an external LLM agent. Provider profile,
model, URL, credentials, prompts, and conversation remain in that process.

```text
A→S {"type":"hello","session_protocol_version":1,"client_kind":"tui","display_name":"Captain A","supported_session_versions":[1],"supported_game_protocol_versions":[4]}
S→A {"type":"welcome","session_protocol_version":1,"game_protocol_version":4,"connection_id":"connection-a","can_configure":true,"capabilities":["scenario_catalog","preview_requests","participant_status"]}
S→A {"type":"scenario_catalog","session_protocol_version":1,"scenarios":[{"id":"shipyard_assault","display_name":"Shipyard Assault"}]}
S→A {"type":"lobby_state","session_protocol_version":1,"state":"unconfigured","scenario":null,"controllers":null,"seats":[],"bot_policies":[{"id":"greedy","display_name":"Greedy"}],"waiting_reason":"host must configure the lobby"}
A→S {"type":"create_match","session_protocol_version":1,"scenario_id":"shipyard_assault","controllers":{"a":{"kind":"human"},"b":{"kind":"llm_agent"}}}
S→A {"type":"seat_assigned","session_protocol_version":1,"side":"a","match_id":"match-3","participant_id":"participant-a"}
S→A {"type":"seat_invitation","session_protocol_version":1,"side":"b","display_code":"WXYZ-1234","join_token":"fixture-agent-token"}
S→A {"type":"lobby_state","session_protocol_version":1,"state":"waiting_for_seats","scenario":{"id":"shipyard_assault","display_name":"Shipyard Assault"},"controllers":{"a":{"kind":"human"},"b":{"kind":"llm_agent"}},"seats":[{"side":"a","controller":{"kind":"human"},"occupancy":"occupied","display_name":"Captain A","ready":true},{"side":"b","controller":{"kind":"llm_agent"},"occupancy":"reserved","display_name":null,"ready":false}],"bot_policies":[],"waiting_reason":"waiting for side b LLM agent"}
L→S {"type":"hello","session_protocol_version":1,"client_kind":"agent","display_name":"Fireworks Agent","supported_session_versions":[1],"supported_game_protocol_versions":[4]}
S→L {"type":"welcome","session_protocol_version":1,"game_protocol_version":4,"connection_id":"connection-agent","can_configure":false,"capabilities":["scenario_catalog","preview_requests","participant_status"]}
L→S {"type":"join_match","session_protocol_version":1,"join_token":"fixture-agent-token","display_name":"Fireworks Agent"}
S→L {"type":"seat_assigned","session_protocol_version":1,"side":"b","match_id":"match-3","participant_id":"participant-agent"}
L→S {"type":"participant_status","session_protocol_version":1,"status":"ready"}
S→A,L {"type":"lobby_state","session_protocol_version":1,"state":"running","scenario":{"id":"shipyard_assault","display_name":"Shipyard Assault"},"controllers":{"a":{"kind":"human"},"b":{"kind":"llm_agent"}},"seats":[{"side":"a","controller":{"kind":"human"},"occupancy":"occupied","display_name":"Captain A","ready":true},{"side":"b","controller":{"kind":"llm_agent"},"occupancy":"occupied","display_name":"Fireworks Agent","ready":true}],"bot_policies":[],"waiting_reason":null}
```

The canonical prefix-free NDJSON forms are checked byte-for-byte in
[`tests/fixtures/session-v1/`](../tests/fixtures/session-v1/README.md).

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
