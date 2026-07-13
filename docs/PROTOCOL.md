# shipsim external API (NDJSON protocol v2)

This is the **public engine API**. Frontends, scripts, and agent harnesses talk
to shipsim only through this boundary. Game rules live in Rust; clients send
orders and render snapshots.

| Layer | Owns |
|---|---|
| **Engine + harness** | Rules, state, AI NPCs, NDJSON I/O (`shipsim` binary) |
| **Clients** (`frontend/*`) | Display and human/agent input; never reimplement combat |

Rules narrative: `docs/PLAY-V2.md`, ADR-0020 / ADR-0022.
How agents should *play*: `docs/AGENT-PLAY.md`.

Protocol version is `2` (`src/protocol.rs::PROTOCOL_VERSION`). Missing or
unsupported versions produce an `unsupported_protocol` soft error.

> **M6 (ADR-0022):** external clients must use `commit_maneuver` (with
> `maneuver: {"type":"coast"}` for the old pass-move behavior). The legacy
> `move` / `pass_move` order types still deserialize but are soft-rejected at
> apply time. Snapshots expose `velocity`, `course`, `facing`,
> `thrust_remaining`, `movement_phase`, and `ships_committed_this_phase`.

---

## 1. Process model

Build the harness:

```bash
cargo build -q
# binary: target/debug/shipsim  (or target/release/shipsim)
# override: SHIPSIM_BIN=/path/to/shipsim
```

### CLI

```text
shipsim --scenario <path.toml> --stdin
shipsim --scenario <path.toml> --orders <orders.jsonl>
shipsim --campaign <path.toml> --stdin
shipsim --resume <save.json> [--save <path>] --stdin
shipsim --scenario <path.toml> --stdin --save <path>
```

| Flag | Meaning |
|---|---|
| `--scenario PATH` | Load one scenario TOML (relative to cwd; clients use repo root) |
| `--campaign PATH` | Run campaign scenarios in sequence |
| `--resume PATH` | Replay a save, then accept more orders |
| `--stdin` | Read one JSON order per line from stdin |
| `--orders PATH` | Read orders from a JSONL file instead of stdin |
| `--save PATH` | After orders finish (or on resume), write a deterministic save |

Exactly one of `--scenario` / `--campaign` / `--resume`, and exactly one of
`--stdin` / `--orders`.

### Wire semantics

1. After load (or resume replay), the harness runs `resolve_v2_npc_actions` so
   `controller = "ai"` ships act when the phase is entirely NPC-driven.
2. It emits **one JSON snapshot line** (post-load paint).
3. For each order line:
   - parse JSON → `Order`;
   - apply or soft-fail;
   - emit **one** snapshot line (accepted) **or** one error line (rejected);
   - after an accepted order, run `resolve_v2_npc_actions` again.
4. Illegal orders **do not** mutate state. The process keeps reading.
5. Same scenario + seed + accepted order stream ⇒ deterministic snapshots.

Clients spawn the binary with cwd = repo root, e.g.:

```bash
target/debug/shipsim --scenario scenarios/ai.toml --stdin
```

Python helper: `frontend/repl/client.py` (`ShipsimSession`).

---

## 2. Orders

Every order is **one JSON object on one line**. Always include
`protocol_version: 2`.

```json
{"protocol_version":2,"type":"commit_maneuver","ship":1,"maneuver":{"type":"coast"}}
```

Supported `type` values (snake_case):

| `type` | Phase | Required fields |
|---|---|---|
| `allocate` | `allocate` | `ship`, `movement`, `weapons`, `shields` |
| `commit_maneuver` | `movement` | `ship`, `maneuver` |
| `commit_fire` | `firing` | `ship`, `weapon`, `target`, `shield_facing` |
| `ready_fire` | `firing` | `ship` |
| `end_turn` | after allocate | *(none beyond type)* |

Authority for shapes: `src/movement.rs::Order`, maneuvers in `src/motion.rs`.
Complete streams: `tests/fixtures/v2/duel_orders.jsonl`.

### 2.1 `allocate`

Split one living ship’s power pool for the turn.

```json
{
  "protocol_version": 2,
  "type": "allocate",
  "ship": 1,
  "movement": 4,
  "weapons": {"beam_1": 1, "torp_1": 0},
  "shields": [2, 0, 0, 0, 0, 2]
}
```

| Field | Meaning |
|---|---|
| `ship` | Ship id |
| `movement` | Engine **power** units (converted to thrust by hull) |
| `weapons` | Map weapon id → charge this turn (0..max) |
| `shields` | Six ints: power on facings F, FR, RR, R, RL, FL |

Sum of movement + all weapon charges + all shield power ≤ `power_available`.
Each living ship allocates **once** per turn. When all have allocated, phase →
`movement`.

### 2.2 `commit_maneuver`

One commitment per living ship per movement phase (up to four phases / turn).
Resolution is simultaneous when everyone has committed.

```json
{"protocol_version":2,"type":"commit_maneuver","ship":1,"maneuver":{"type":"coast"}}
{"protocol_version":2,"type":"commit_maneuver","ship":1,"maneuver":{"type":"accelerate","course":0}}
{"protocol_version":2,"type":"commit_maneuver","ship":1,"maneuver":{"type":"decelerate"}}
{"protocol_version":2,"type":"commit_maneuver","ship":1,"maneuver":{"type":"turn_course_port"}}
{"protocol_version":2,"type":"commit_maneuver","ship":1,"maneuver":{"type":"turn_course_starboard"}}
{"protocol_version":2,"type":"commit_maneuver","ship":1,"maneuver":{"type":"rotate_port"}}
{"protocol_version":2,"type":"commit_maneuver","ship":1,"maneuver":{"type":"rotate_starboard"}}
```

| `maneuver.type` | Effect (summary) |
|---|---|
| `coast` | Keep velocity/course; 0 thrust |
| `accelerate` | Speed +1; optional `course` 0..5 only when stopped |
| `decelerate` | Speed −1 |
| `turn_course_port` / `turn_course_starboard` | Turn travel direction; cost ≈ current speed |
| `rotate_port` / `rotate_starboard` | Hull facing only; cost 1 |

**Course** = travel direction. **Facing** = where the nose/weapons point.

### 2.3 `commit_fire`

Queue one legal shot. Does **not** resolve until all living ships `ready_fire`.

```json
{
  "protocol_version": 2,
  "type": "commit_fire",
  "ship": 1,
  "weapon": "beam_1",
  "target": 2,
  "shield_facing": 0
}
```

`shield_facing` is the **target’s** relative facing (0=F … 5=FL) being hit.
Illegal commits soft-fail. Zero or more commits per ship before ready.

### 2.4 `ready_fire`

```json
{"protocol_version":2,"type":"ready_fire","ship":1}
```

When **all** living ships are ready, commits resolve simultaneously. Hit **or
miss** both clear that weapon’s charge and mark it fired. Then either another
movement phase or `turn_end`.

### 2.5 `end_turn`

```json
{"protocol_version":2,"type":"end_turn"}
```

Legal after allocation; advances the turn. Snapshot `end_turn_warning` is
advisory only (never blocks).

### 2.6 Phase protocol (v2)

1. **allocate** — each living ship once → `movement`.
2. **commit_maneuver** — each living ship once per movement phase (1..4); then fire window or next movement.
3. **commit_fire** / **ready_fire** — optional queues, then all ready → simultaneous resolve.
4. After four move/fire windows → turn end (or early `end_turn`).
5. **end_turn** → next turn’s allocate.

### Controllers

| Snapshot `controller` | Who drives orders |
|---|---|
| `player` | Client / human / agent UI |
| `ai` | Harness via `resolve_v2_npc_actions` after load and accepted orders |
| `scripted` | **External only** — not auto-advanced by the engine. Interactive clients (REPL) may pump passive allocate/coast/ready when the phase is blocked **only** on scripted ships |

Clients must **not** invent AI orders. They may drive scripted ships or leave that to recorded order files.

### Retired orders

`move` and `pass_move` deserialize for old payloads but always soft-fail at apply.
Use `commit_maneuver` + `coast` instead of pass.

---

## 3. Snapshots

One object per line after load/resume and after each **accepted** order.

| Field | Notes |
|---|---|
| `protocol_version` | Always `2` |
| `turn` | 1-based turn number |
| `status` | `InProgress`, `Won`, or `Lost` |
| `phase` | `allocate`, `movement`, `firing`, or `turn_end` |
| `movement_phase` | 1..4 during movement |
| `ships_committed_this_phase` | Ship ids that have maneuvered this phase |
| `ships_allocated_this_turn` | Ship ids done allocating |
| `ships_ready_fire` | Ship ids that have readied |
| `seed` / `prng_state` | Determinism / resume |
| `map` | `{width, height, mode}` (`hard` or `floating`) |
| `objective` | Optional `{q,r}` |
| `ships` | Array of ship snapshots |
| `fire_commits` | Pending simultaneous fire queue |
| `combat_log` | Resolved shots this engagement window |
| `end_turn_warning` | bool; leftover useful action |

### Ship snapshot (important fields)

| Field | Meaning |
|---|---|
| `id`, `class`, `controller` | Identity |
| `q`, `r`, `facing` | Position and nose (0..5) |
| `destroyed` | Out of the fight |
| `power`, `power_available` | Pool / after systems damage |
| `movement_allocated` | Engine power allocated this turn |
| `shields_powered` / `shields_remaining` | Six facings |
| `max_shield_per_facing` | Cap per face |
| `structure`, `engine`, `power_sys`, `bridge` | SSD boxes |
| `weapons[]` | `id`, `kind`, `mount`/`arc`, `charge`, `fired`, `max_charge`, `operational`, `max_range` |
| `velocity`, `course`, `facing` | Inertial travel state |
| `thrust_remaining` | Thrust left this turn |
| `max_velocity`, `thrust_per_power`, `power_per_thrust` | Hull constants |

Serialize shapes: `src/snapshot.rs`.

---

## 4. Errors

Rejected lines emit an error object (not a snapshot). State is unchanged.

```json
{
  "protocol_version": 2,
  "type": "error",
  "ok": false,
  "code": "order_illegal",
  "message": "...",
  "source": "harness",
  "order": {}
}
```

`order` is omitted when the line was not parseable JSON. Clients should show
`message` and keep the session open.

---

## 5. Example session

```bash
cargo build -q
printf '%s\n' \
  '{"protocol_version":2,"type":"allocate","ship":1,"movement":4,"weapons":{"beam_1":1},"shields":[0,0,0,0,0,0]}' \
  '{"protocol_version":2,"type":"allocate","ship":2,"movement":2,"weapons":{"beam_1":1},"shields":[0,0,0,0,0,0]}' \
  '{"protocol_version":2,"type":"commit_maneuver","ship":1,"maneuver":{"type":"coast"}}' \
  '{"protocol_version":2,"type":"commit_maneuver","ship":2,"maneuver":{"type":"coast"}}' \
  | target/debug/shipsim --scenario scenarios/v2_duel.toml --stdin
```

Golden full duel: `tests/fixtures/v2/duel_orders.jsonl` + matching snapshot
fixture under `tests/fixtures/v2/`.

---

## 6. Compatibility policy

- Consumers must reject protocol versions they do not implement.
- Protocol-v1 saves are rejected by version before replay.
- Additive snapshot/error fields may appear within v2; ignore unknowns.
- Removing or changing fields, order semantics, enums, or required inputs
  requires a **new** protocol version and coordinated fixture/client updates.

---

## 7. Related docs

| Doc | Role |
|---|---|
| **`docs/PROTOCOL.md`** | This file — wire API |
| `docs/ARCHITECTURE.md` | System boundaries |
| `docs/PLAY-V2.md` | Player-facing rules summary |
| `docs/AGENT-PLAY.md` | How agents run tests vs play via UI |
| `docs/SAVE-FORMAT.md` | Save/resume documents |
| `docs/SIMULATION.md` | Batch match runner (`shipsim-sim`) |
| `frontend/README.md` | Client isolation policy |
| `frontend/repl/GAMEPLAY.md` | REPL command vocabulary |
