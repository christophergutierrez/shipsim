# shipsim REPL (dev client)

Ugly but usable interactive client for Combat Model v2 playtesting. Spawns the
Rust `shipsim` harness over stdin/stdout NDJSON (`protocol_version: 1`) and
prints text snapshots with menu-ish commands.

**This directory is the entire REPL client.** Code, helpers, and session scratch
live here only. See `frontend/README.md` for isolation rules. Do not drop REPL
logs or helpers at the repo root, in repo `/tmp`, or system `/tmp` — use
`frontend/repl/local/` (gitignored). Deleting this tree must not break the
engine or other frontends.

```
frontend/repl/
  README.md       # this file
  repl.py         # interactive entrypoint
  client.py       # harness subprocess + smoke test
  view.py         # snapshot / board formatting
  commands.py     # command parser + order builders
  .gitignore      # ignores local/ and caches
  local/          # session orders-*.jsonl, stderr-*.log (not committed)
```

Love2D is a sibling under `frontend/love/` and shares nothing with this tree
except the public shipsim protocol.

## Prerequisites

```bash
cargo build                 # produces target/debug/shipsim
python3 --version           # 3.10+ (stdlib only; no pip deps)
```

Optional: `export SHIPSIM_BIN=/path/to/shipsim`.

## Run

```bash
# from repo root
python3 frontend/repl/repl.py
python3 frontend/repl/repl.py scenarios/ai.toml
python3 frontend/repl/repl.py combat          # bare name ok

# non-interactive smoke (loads combat.toml, one allocate)
python3 frontend/repl/client.py
```

## Commands

| Command | Order / action |
|---|---|
| `help` | command list |
| `status` / `s` | reprint snapshot |
| `board` / `b` | board only |
| `allocate` / `a` `[ship]` | interactive power split |
| `alloc-default` / `ad` `[ship]` | rest of power → movement (+1 beam) |
| `move` / `m` `<fwd\|rev\|port\|starboard>` `[ship]` | ACTIVE ship default |
| `pass` / `p` `[ship]` | `pass_move` |
| `fire` / `f` `[ship]` | interactive `commit_fire` |
| `ready` / `r` `[ship]` | `ready_fire` |
| `end` / `e` | `end_turn` |
| `order {…}` | raw JSON order |
| `quit` | exit |

Phase flow matches `docs/PLAY-V2.md` and `docs/PROTOCOL.md`. Soft-rejected
orders print `! code: message` and leave state unchanged.

## AI opponents

Scenarios with `controller = "ai"` (e.g. `scenarios/ai.toml`) rely on the
harness calling core `resolve_v2_npc_actions` after load and each accepted
order. The REPL never reimplements AI — it only submits player orders.

## Isolation rule

If you add REPL helpers, tests, fixtures, or temporary dumps, put them under
`frontend/repl/` (use `local/` for anything ephemeral). Update this README if
you introduce a new subpath.
