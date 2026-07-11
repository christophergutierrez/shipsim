# ADR-0017 — Love2D thin client over JSON subprocess (D8)

Status: Accepted
Date: 2026-07-09

## Context

ADR-0004 deferred the graphical frontend until the core was playable through the CLI/JSON
harness. That work is done (core through D10). The remaining product gap is D8: a desktop UI.

Prior experience with a Love2D + agent workflow produced avoidable friction: agents asked humans
to "open Love and check," oscillated on broken UI state, and could only land tiny change batches.
Root cause was process opacity (no headless client gates), not Love itself. Godot remains out of
scope: heavier stack, same need for agent-verifiable gates, no better fit for a thin renderer of
an existing JSON core.

ADR-0001 already requires: all rules in `shipsim_core`, frontend renders only, headless tests for
rules. The harness proves the order/snapshot contract.

## Decision

1. **Stack:** Love2D (11.x) is the D8 desktop frontend. No Godot for this phase. Additional thin
   frontends later are fine if they consume the same contract.
2. **Integration (v1):** **JSON subprocess**, not FFI.
   - Frontend spawns `shipsim --scenario <path> --stdin` (or `--campaign` / `--orders` for
     scripted runs).
   - Protocol: **one JSON object per line**.
   - On load (and on each campaign scenario advance): harness prints one `StateSnapshot` line
     **before** any order.
   - Client writes one `Order` JSON line; harness applies it and prints one `StateSnapshot` line.
   - Orders use `#[serde(tag = "type", rename_all = "snake_case")]` (`allocate`, `plot`, `fire`,
     `run_turn`). Snapshots are serde JSON of `StateSnapshot`.
3. **No game rules in Lua.** Layout, input mapping, and drawing only. Illegal orders are rejected
   by the core; the client surfaces errors, it does not reimplement validation.
4. **FFI / `cdylib` bridge:** deferred. Optional later if packaging or latency requires it; not
   required for a playable D8.
5. **Verification primary path:** headless and agent-runnable — `cargo test`, harness golden
   fixtures, pure Lua unit tests (no `love.*`). Human visual check is last resort for pixels/feel
   only. See `docs/ARCHITECTURE.md`.

## Consequences

- D8 work is a thin Love tree + docs; `shipsim_core` stays rules-only.
- Agents can drive and assert the simulation without a window; UI work slices against a fixed
  JSON contract (`tests/fixtures/d8/`).
- Protocol is line-oriented NDJSON; easy to log, replay, and golden-test.
- Tradeoff: process boundary is slower than in-process FFI; acceptable for turn-based SFB-style
  play. Revisit only with measured need.
- ADR-0004's "choose later" is resolved for stack + integration mode; its harness-first approach
  remains the foundation.
