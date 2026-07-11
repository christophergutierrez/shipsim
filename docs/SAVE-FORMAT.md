# Save and Resume Format v1

Save files are deterministic replay documents. They preserve scenario identity, every accepted order, and the PRNG checkpoint reached after replay. They do not serialize private `GameState` fields.

```json
{
  "protocol_version": 1,
  "scenario": "scenarios/combat.toml",
  "orders": [
    {
      "type": "allocate",
      "ship": 1,
      "movement": 4,
      "weapons": {"beam_1": 1},
      "shields": [2, 1, 0, 0, 0, 1]
    }
  ],
  "prng_state": 4242
}
```

Orders inside a save omit the external `protocol_version`; the document version governs their schema.

## Create

```bash
cargo run -q --bin shipsim -- \
  --scenario scenarios/combat.toml \
  --orders orders.jsonl \
  --save tmp/duel.save.json
```

Only accepted orders are stored. Soft-rejected input remains in harness output but is not persisted.

## Resume

```bash
cargo run -q --bin shipsim -- \
  --resume tmp/duel.save.json \
  --orders more-orders.jsonl
```

Resume loads the saved scenario, replays stored orders without emitting intermediate snapshots, verifies the resulting PRNG state, emits the resumed snapshot, and then applies new orders. By default it updates the same save file. Provide `--save another.json` to write a new branch.

## Validation and limits

- Unsupported document versions fail before scenario load.
- Illegal saved orders and PRNG mismatches fail rather than loading ambiguous state.
- Scenario and ship data must still be available and compatible with the recorded order stream.
- Campaign save/resume is not supported in v1.
- Replay time grows with order history; checkpointed aggregate serialization can be added in a future version if profiling justifies it.
