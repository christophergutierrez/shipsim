# Protocol v4 golden fixtures

- `smoke_orders.jsonl` / `smoke_out.jsonl` — full allocate → path → empty volley → turn+1 on `ai.toml` (AI auto-fills).
- `conflict_orders.jsonl` / `conflict_out.jsonl` — two escorts claim the same hex; `path_results` show cost/tie/fallback (`v4_conflict.toml`).

v3 fixtures under `tests/fixtures/v3/` are historical and rejected by the engine.
