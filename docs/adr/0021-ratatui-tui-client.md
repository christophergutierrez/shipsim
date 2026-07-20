# ADR-0021 — ratatui terminal client

Status: Accepted (Small tier implemented; see `frontend/tui/README.md`)  
Date: 2026-07-11

Note: Simulation policy boundary was formerly also numbered ADR-0021; it is now
**ADR-0026**.

## Canonical text

The full decision record lives with the client tree:

**[`frontend/tui/ADR.md`](../../frontend/tui/ADR.md)**

## Summary

- Add optional frontend `frontend/tui/` using **ratatui** (+ crossterm).
- Integrate via **NDJSON subprocess** to `shipsim` (same contract as Love/REPL).
- No rules in the TUI; keep `frontend/repl/` for agent/debug play.
- Implementation not started at acceptance — docs/scaffold only.

## See also

- `frontend/tui/README.md` — handoff for implementers  
- ADR-0017 — Love thin client (same integration pattern)  
- ADR-0020 — Combat Model v2  
- ADR-0023 — input model, responsive layout tiers, and verification strategy (refines this ADR's open implementation details)
