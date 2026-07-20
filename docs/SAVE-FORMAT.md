# Save and Resume Format v4

Save files are deterministic replay documents. They preserve scenario identity, every accepted order, and the PRNG checkpoint reached after replay. They do not serialize private `GameState` fields.

```json
{
  "protocol_version": 4,
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
  "prng_state": 4242,
  "rules_fingerprint": "fnv1a-..."
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

Only accepted orders are stored. This includes automatically applied AI
orders, so replay does not need to invent hidden barrier-stage actions.
Soft-rejected input remains in harness output but is not persisted.

## Resume

```bash
cargo run -q --bin shipsim -- \
  --resume tmp/duel.save.json \
  --orders more-orders.jsonl
```

Resume loads the saved scenario, replays stored orders without emitting intermediate snapshots, verifies the resulting PRNG state, emits the resumed snapshot, and then applies new orders. By default it updates the same save file. Provide `--save another.json` to write a new branch.

## Validation and limits

- Unsupported document versions (anything other than 4) fail **before** order deserialization or scenario load — the version is probed first so an incompatible order shape yields `UnsupportedVersion`, not `Parse`.
- Illegal saved orders and PRNG mismatches fail rather than loading ambiguous state.
- New saves record the semantic fingerprint of `data/rules/default.toml` (ADR-0024);
  replay rejects a mismatched fingerprint before applying orders. The field is
  optional when reading older protocol-v4 saves created before rules
  fingerprints were added (`SaveDocument::rules_fingerprint: None`). Protocol-v3
  save documents are rejected by the version check above.
- Scenario and ship data must still be available and compatible with the recorded order stream.
- Campaign save/resume is not supported in protocol v4.
- Replay time grows with order history; checkpointed aggregate serialization can be added in a future version if profiling justifies it.
