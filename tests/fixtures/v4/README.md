# Protocol v4 golden fixtures

- `smoke_orders.jsonl` / `smoke_out.jsonl` — full allocate → path → empty volley → turn+1 on `ai.toml` (AI auto-fills).
- `conflict_orders.jsonl` / `conflict_out.jsonl` — two escorts claim the same hex; `path_results` show cost/tie/fallback (`v4_conflict.toml`).

Older protocol versions are rejected by the engine. There is no checked-in
`v2/` or `v3/` fixture tree; regenerate goldens under this directory only.
