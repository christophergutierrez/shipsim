# NDJSON Protocol v2

shipsim uses newline-delimited JSON between external clients and the Rust harness. Protocol version 2 (ADR-0022, M6) supersedes v1: the external order contract now uses maneuver commitment semantics, snapshots expose inertial-movement state, and protocol-v1 saves are rejected by version.

> **M6 changes (ADR-0022):** the `move` and `pass_move` order types are retained as deserialization-only legacy variants — they parse but are rejected at apply time with a soft error. External clients must use `commit_maneuver` (with `maneuver: {"type":"coast"}` for the former `pass_move` behavior). Snapshots now expose `velocity`, `course`, `facing`, `thrust_remaining`, `movement_phase`, and `ships_committed_this_phase`.

## Orders

Every order is one JSON object on one line and must include:

```json
{"protocol_version":2,"type":"commit_maneuver","ship":1,"maneuver":{"type":"coast"}}
```

Supported `type` values are `allocate`, `commit_maneuver`, `commit_fire`, `ready_fire`, and `end_turn`. The legacy `move` and `pass_move` types still deserialize but are soft-rejected at apply time. Order fields are defined by `src/movement.rs::Order`; representative complete streams live in `tests/fixtures/v2/duel_orders.jsonl`.

### Phase protocol (v2)

1. **allocate** — each living ship once; when all allocated, phase becomes `movement`.
2. **commit_maneuver** — each living ship commits one maneuver (or `coast`) per movement phase. There are up to four movement phases per turn; after each phase resolves, a fire window opens. When every living ship has committed, the phase resolves and either opens a fire window or advances to the next movement phase.
3. **commit_fire** — zero or more per ship while not yet `ready_fire` for that ship. Illegal commits do not mutate state.
4. **ready_fire** — marks a ship done committing. When **all** living ships have readied, the core resolves all commits simultaneously (hit **or miss** both clear that weapon's charge and mark it fired), then either returns to `movement` if another movement phase remains or goes to `turn_end`.
5. **end_turn** — legal after allocation; always advances turn. Snapshot `end_turn_warning` reports whether a legal fire action remains; movement follows the fixed four-phase schedule.

Missing or unsupported versions produce an `unsupported_protocol` soft error. Malformed and illegal orders also produce soft errors and do not mutate state. The process continues reading subsequent lines.

## Snapshots

The harness emits one snapshot after scenario load or resume and after every accepted order. Every snapshot includes `protocol_version: 2`, aggregate phase/status, deterministic PRNG checkpoint, map data, ships, pending fire, combat log, and end-turn warning.

M6 snapshots additionally expose inertial-movement state per ship (`velocity`, `course`, `facing`, `thrust_remaining`) and at the game level (`movement_phase`, `ships_committed_this_phase`).

`status` is `InProgress`, `Won`, or `Lost`. `phase` is `allocate`, `movement`, `firing`, or `turn_end`.

## Errors

Errors use this shape:

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

`order` is omitted when no JSON value could be parsed.

## Compatibility policy

- Consumers must reject protocol versions they do not implement.
- Protocol-v1 saves are rejected by version at `SaveDocument::read` before replay.
- Additive fields may be introduced within v2; consumers should ignore unknown snapshot/error fields.
- Removing or changing existing fields, order semantics, enum values, or required inputs requires a new protocol version.
- Golden fixtures, CLI tests, and the Love2D/REPL clients must change together.
