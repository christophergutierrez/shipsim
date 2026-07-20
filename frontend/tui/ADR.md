# ADR — ratatui terminal client (frontend/tui)

| Field | Value |
|---|---|
| Status | Accepted (Small tier implemented under `frontend/tui/`) |
| Date | 2026-07-11 |
| Scope | `frontend/tui/` only |
| Related | ADR-0004 (harness first), ADR-0017 (Love thin client), ADR-0018 (soft errors), ADR-0020 (Combat Model v2), ADR-0023 (input/layout), ADR-0025 (protocol v4 turns), `docs/PROTOCOL.md` |

## Context

Combat Model v2 and the NDJSON harness are playable through:

- headless `shipsim` + fixtures,
- `frontend/repl/` (Python line-oriented client with a fixed “play frame”),
- `frontend/love/` (graphical thin client).

The REPL play-frame work made it obvious that a **real TUI** (fixed regions, widgets, key bindings, resize) is the natural shape for terminal play. Hand-rolled ANSI clear/redraw is enough to prove the idea and keep agents happy; it is not the best long-term product UI for keyboard-driven combat.

Community consensus for Rust TUIs in 2025–2026 points at **ratatui** (immediate-mode, widgets, layout, crossterm backend, active ecosystem). That stack matches shipsim’s “redraw from latest snapshot” model.

Constraints that still apply:

- Rules live only in `shipsim_core`.
- Frontends are isolated under `frontend/<name>/` and may be deleted without touching the engine.
- Wire contract is protocol **v4** NDJSON (`docs/PROTOCOL.md`).
- Soft-reject illegal orders; do not reimplement legality in the UI.

## Decision

1. **Add a third frontend tree:** `frontend/tui/` — a Rust terminal client built with **ratatui** + **crossterm** (or ratatui’s recommended backend pairing at implementation time).

2. **Integration:** same as Love/REPL — **JSON subprocess**, not in-process rules embedding and not FFI:
   - Spawn `shipsim --scenario <path> --stdin` (or `SHIPSIM_BIN`).
   - Read post-load snapshot; write one order line; read snapshot or soft error.
   - Rely on harness `resolve_v2_npc_actions` after accepted orders (AI advances without the TUI inventing NPC orders).

3. **No game rules in the TUI.** Layout, input, focus, panels, and presentation only. Illegal orders surface as protocol errors.

4. **Do not put the TUI crate inside `shipsim_core` or the root workspace package as a required dependency of the engine.** Prefer a **separate package** under `frontend/tui/` (its own `Cargo.toml`). Optional later: a workspace member for convenience builds — still no reverse dependency from core → tui.

5. **Keep `frontend/repl/`.** It remains the agent-friendly, non-fullscreen, `--debug` transcript client. TUI does not replace it.

6. **Love stays secondary** for graphical polish; TUI is the preferred terminal product path when implemented.

7. **Presentation language** should reuse lessons already proven in the REPL (callsigns/sides, facing legend aligned to board axes, FIRED+HIT/MISS, allocate as local draft until commit, ship-centric focus). See `frontend/repl/ASCII-UI.md` and `frontend/repl/README.md`.

## Alternatives considered

| Option | Why not (for this decision) |
|---|---|
| Grow Python play-frame only | Good enough for dev; weak for layout, keys, resize, polish |
| `cursive` / other Rust TUI kits | Less momentum and widget ecosystem than ratatui today |
| In-process `shipsim_core` in the TUI binary (v1) | Faster coupling risk; harder isolation; defer until subprocess path is solid |
| Godot / more Love investment first | Different product surface; does not solve terminal play |

## Consequences

- Implementation work is confined to `frontend/tui/` (+ optional docs pointers).
- Engine changes only when the **protocol** needs a field (e.g. past additive snapshot fields); UI must not invent rules.
- Headless CI continues to use `cargo test` and the harness; TUI may add its own non-interactive smoke later (e.g. spawn harness, assert first frame data) without requiring a human terminal.
- Deleting `frontend/tui/` must leave core, REPL, and Love intact.

## Implementation intent (non-binding sketch)

Not part of the acceptance of this ADR as code — guidance for the first implementation PR:

1. Package skeleton + spawn harness + show turn/phase + ship list.
2. Map panel + callsigns/facing.
3. Allocate draft panel → commit order.
4. Movement / fire / ready / end turn.
5. Combat log / message region; soft-error banner.

## Status of tree

As of this ADR: **directory + decision docs only**. No ratatui binary yet.
