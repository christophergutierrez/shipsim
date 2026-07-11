# NDJSON Protocol v1

shipsim uses newline-delimited JSON between external clients and the Rust harness. Protocol version 1 is intentionally breaking from the unversioned development stream.

## Orders

Every order is one JSON object on one line and must include:

```json
{"protocol_version":1,"type":"pass_move","ship":1}
```

Supported `type` values are `allocate`, `move`, `pass_move`, `commit_fire`, `ready_fire`, and `end_turn`. Their fields are defined by `src/movement.rs::Order`; representative complete streams live in `tests/fixtures/v2/duel_orders.jsonl`.

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
