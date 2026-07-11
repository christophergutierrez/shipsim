# ADR-0021 — ratatui terminal client

Status: Accepted  
Date: 2026-07-11

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
