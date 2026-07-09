# D8 Frontend Brief — Love2D thin client

Durable agent brief for graphical frontend work. Decisions live in **ADR-0017**.
Do not put session status in git (no tracked `HANDOFF.md`).

## Goal

A thin Love2D client that:

1. Spawns the `shipsim` harness.
2. Renders `StateSnapshot` JSON.
3. Emits `Order` JSON from player input.
4. Owns **no** combat/movement/energy rules.

## Protocol (v1)

```text
love  →  shipsim --scenario scenarios/combat.toml --stdin
         (or --campaign … / --orders <file> for non-interactive runs)

shipsim stdout: NDJSON StateSnapshot, one object per line
love stdin→shipsim: NDJSON Order, one object per line
```

### Stream shape

1. **Post-load:** first snapshot line (initial paint; no order yet).
2. **Per order:** client writes one order line → one snapshot line back.
3. **Errors:** illegal orders fail the harness process with a message on stderr (v1). Client must
   treat non-zero exit / broken pipe as failure; do not invent success state.

### Order shapes (Rust `Order`)

```json
{"type":"allocate","ship":1,"movement":0,"weapons":4,"shields":0}
{"type":"plot","ship":1,"path":[{"q":1,"r":0},{"q":2,"r":0}]}
{"type":"fire","ship":1,"weapon":"phaser_1","target":2}
{"type":"run_turn"}
```

`allocate` accepts JSON alias `"speed"` for `"movement"`.

### Snapshot shape (Rust `StateSnapshot`)

Top-level fields: `turn`, `impulse`, `status`, `seed`, `prng_state`, `map`, `objective`,
`ships[]`, `seeking[]`, `combat_log[]`.

Authoritative sample stream: `tests/fixtures/d8/snapshots.jsonl` (generated from
`scenarios/combat.toml` + `scenarios/d8_frontend_orders.jsonl`).

Regenerate:

```bash
cargo run -q --bin shipsim -- \
  --scenario scenarios/combat.toml \
  --orders scenarios/d8_frontend_orders.jsonl \
  > tests/fixtures/d8/snapshots.jsonl
```

## Agent verification ladder (mandatory)

Never ask a human to "open Love and check" for correctness. Run this ladder yourself:

| Gate | Command / check | Covers |
|------|-----------------|--------|
| 1. Core rules | `cargo test` | Simulation still green |
| 2. Harness + golden | `cargo test --test harness` (includes D8 fixture lock) | NDJSON contract stable |
| 3. Pure Lua | `luajit path/to/test_*.lua` (no Love window) | Hex math, layout tables, order builders |
| 4. Optional smoke | Headless/scripted Love only if it asserts **no Lua error** / exit code | Lifecycle crash only |
| 5. Human | Visual polish only | Readability, feel, art |

### Do-not-ask checklist

- [ ] Can this be proven with cargo/luajit/fixture diff? → **do not ask the human**
- [ ] Prefer dumping last N order/snapshot lines to a log file over "what do you see?"
- [ ] If still broken after a "fix," re-run gates; do not guess from screenshots in chat
- [ ] Cap each agent pass to **one module or one vertical slice**; green gates before the next

## Recommended module layout (thin)

```text
shipsim_love/          # or love/ — pick one root when scaffolding
  main.lua             # love.load/update/draw; spawn harness
  conf.lua
  harness.lua          # process IO, line buffer, parse JSON
  state.lua            # last snapshot only (view model)
  hex.lua              # pure layout math (unit-test without Love)
  draw_board.lua
  draw_hud.lua
  input.lua            # keys/mouse → Order tables
  tests/               # luajit-runnable pure tests
```

No second copy of SSD damage, IFF, IMC, or AI in Lua.

## Vertical slices (implement in order)

| Slice | Deliverable | Machine gate |
|-------|-------------|--------------|
| D8.1 | Spawn harness, read post-load snapshot, text dump of ships/turn/status | golden fixture still matches; text UI optional |
| D8.2 | Hex board from `map` + ship `q,r,facing` | pure hex tests + screenshot optional |
| D8.3 | Input → allocate/plot/fire/run_turn → refresh from snapshot | order JSON unit tests; play combat.toml headless via orders file |
| D8.4 | HUD: shields, energy buckets, weapons, combat_log | field presence against golden snapshots |
| D8.5 | Polish: camera, art, sound | human visual OK here |

## Anti-patterns (from prior Love pain)

- Monolithic `main.lua` that mixes FFI, rules, and draw
- Asking the user to validate every edit in a live window
- Large multi-file rewrites without a green gate between them
- Reimplementing range/arc/energy checks "for snappier UX" in Lua
- Starting with FFI/`cdylib` before the JSON client plays

## Out of scope for early D8

- Godot / web client (may come later; same JSON contract)
- Full EA Form paper UX, campaign designer, save document format
- In-process FFI bridge (ADR-0017 defers)

## Related docs

- ADR-0001 — core/frontend split
- ADR-0004 — harness-first phase (historical)
- ADR-0017 — Love + JSON subprocess decision
- `docs/ROADMAP.md` — D8 status
- `src/movement.rs` (`Order`), `src/snapshot.rs` (`StateSnapshot`), `src/bin/shipsim.rs`
