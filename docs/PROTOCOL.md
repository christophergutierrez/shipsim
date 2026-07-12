# NDJSON Protocol v1

shipsim uses newline-delimited JSON between external clients and the Rust harness. Protocol version 1 is intentionally breaking from the unversioned development stream.

> **Temporary drift (ADR-0022, M4):** the inertial-movement milestone plan (`docs/MOVEMENT-INERTIA-PLAN.md`)
> deliberately defers the protocol v2 bump to M6, so this document is currently stale in one respect: as of
> M4, `move` always soft-rejects (the single-active-mover model it describes is gone), and `pass_move` now
> means "commit a coast maneuver for the current movement phase (one of four per turn)" rather than "pass
> the current active mover's turn." This is a known, milestone-plan-sanctioned exception to the
> compatibility policy below, not an accident — the full rewrite (new order/snapshot shapes, version bump)
> lands at M6.

## Orders

Every order is one JSON object on one line and must include:

```json
{"protocol_version":1,"type":"pass_move","ship":1}
```

Supported `type` values are `allocate`, `move`, `pass_move`, `commit_fire`, `ready_fire`, and `end_turn`. Their fields are defined by `src/movement.rs::Order`; representative complete streams live in `tests/fixtures/v2/duel_orders.jsonl`.

### Phase protocol (v2)

1. **allocate** — each living ship once; when all allocated, phase becomes `movement` and `move_order` is fixed.
2. **move** / **pass_move** — only the current `active_mover` (first in `move_order` with move power left that has not decided this phase). When every living ship has decided or has zero move power, phase becomes `firing`.
3. **commit_fire** — zero or more per ship while not yet `ready_fire` for that ship. Illegal commits do not mutate state.
4. **ready_fire** — marks a ship done committing. When **all** living ships have readied, the core resolves all commits simultaneously (hit **or miss** both clear that weapon's charge and mark it fired), then either returns to `movement` if useful actions remain or `turn_end`.
5. **end_turn** — legal after allocation; always advances turn. Snapshot `end_turn_warning` is true if useful move/fire still existed.

Missing or unsupported versions produce an `unsupported_protocol` soft error. Malformed and illegal orders also produce soft errors and do not mutate state. The process continues reading subsequent lines.

## Snapshots

The harness emits one snapshot after scenario load or resume and after every accepted order. Every snapshot includes `protocol_version: 1`, aggregate phase/status, deterministic PRNG checkpoint, map data, ships, pending fire, combat log, and end-turn warning.

`status` is `InProgress`, `Won`, or `Lost`. `phase` is `allocate`, `movement`, `firing`, or `turn_end`.

## Errors

Errors use this shape:

```json
{
  "protocol_version": 1,
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
- Additive fields may be introduced within v1; consumers should ignore unknown snapshot/error fields.
- Removing or changing existing fields, order semantics, enum values, or required inputs requires a new protocol version.
- Golden fixtures, CLI tests, and the Love2D client must change together.
