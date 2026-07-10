# ADR-0018 — D8 harness soft errors + controller on snapshot

Status: Accepted
Date: 2026-07-09

## Context

D8 Love client (ADR-0017) needs multi-turn play to win/loss. The v1 harness exited on illegal
orders, which kills the Love session mid-fight. Snapshots also omitted `controller`, so the client
could not tell player ships from AI without parsing scenario TOML.

## Decision

1. **Soft reject:** On `OrderError`, `shipsim` prints one NDJSON **error** line and **stays alive**.
   Game state is unchanged. Process exit is reserved for I/O / fatal setup failures.
2. **Error envelope** (stdout NDJSON line):

   ```json
   {
     "type": "error",
     "ok": false,
     "code": "order_illegal",
     "message": "human-readable reason",
     "order": {},
     "source": "harness"
   }
   ```

   Success lines remain bare `StateSnapshot` objects (distinguished by presence of `ships`/`turn`,
   not by a success wrapper).

3. **`ShipSnapshot.controller`:** string matching scenario controllers (`player`, `ai`,
   `scripted`, …) so the thin client never parses TOML for ownership.

4. **Client-derived defeat:** No core `Lost` status in this ADR. Frontend treats all
   `controller == "player"` ships destroyed as defeat. Core `Lost` is roadmap residual.

## Consequences

- Love can show a standard status strip for rejects without restarting the sim.
- Golden fixtures and harness tests must cover error lines and the new snapshot field.
- Fixture regeneration required when snapshot shape changes.
