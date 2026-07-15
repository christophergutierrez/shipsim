# Protocol 3 golden streams

| File | Scenario | Checks |
|---|---|---|
| `smoke_orders.jsonl` / `smoke_out.jsonl` | `scenarios/p3_smoke.toml` | allocate, accel, coast, end_turn, **weapon charge carry** |
| `revector_orders.jsonl` / `revector_out.jsonl` | same | `turn_accel` revector (oblique / turn+accel) |

Regenerate after intentional protocol changes:

```bash
cargo build -q
target/debug/shipsim --scenario scenarios/p3_smoke.toml \
  --orders tests/fixtures/v3/smoke_orders.jsonl \
  > tests/fixtures/v3/smoke_out.jsonl
```
