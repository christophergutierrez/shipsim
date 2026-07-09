# D8 frontend contract fixture

Golden NDJSON stream for the Love thin client (ADR-0017).

| File | Role |
|------|------|
| `../../../../scenarios/combat.toml` | Scenario under test |
| `../../../../scenarios/d8_frontend_orders.jsonl` | Order stream |
| `snapshots.jsonl` | Expected stdout: post-load snapshot + one snapshot per order |

Regenerate after intentional harness/snapshot changes:

```bash
cargo run -q --bin shipsim -- \
  --scenario scenarios/combat.toml \
  --orders scenarios/d8_frontend_orders.jsonl \
  > tests/fixtures/d8/snapshots.jsonl
```

Locked by `tests/harness.rs` (`test_d8_frontend_fixture_matches_harness`).
