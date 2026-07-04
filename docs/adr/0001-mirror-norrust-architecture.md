# ADR-0001 — Mirror the NorRust architecture

Status: Accepted
Date: 2026-07-04

## Context

shipsim is a greenfield hex-based tactical starship game. The sibling project NorRust
(`../norrust`) already solves the same class of problem — a hex strategy game — with an
architecture the maintainer is happy with: a pure-Rust headless simulation core with no rendering
dependencies, a thin frontend that renders JSON received over a C-ABI bridge, data-driven content
in TOML, and a large headless test suite (`cargo test`) that verifies rules without ever opening the
frontend. This design makes the engine AI-agent-friendly (speak JSON, never touch the UI) and cheap
to test.

## Decision

Adopt the same separation for shipsim:

- `shipsim_core` is a pure-Rust library, `crate-type = ["cdylib", "rlib"]`, `panic = "abort"`,
  depending only on `serde`, `serde_json`, `toml`, `thiserror`. It contains **all** game rules and
  state and has **no** dependency on any display system.
- The frontend is a thin client that consumes JSON/snapshots from the core over a C-ABI (`ffi`)
  and/or a CLI/JSON harness. It renders state; it never owns rules.
- Ship and scenario content is loaded from TOML at startup.
- Headless `cargo test` is the primary verification surface for every rule.

## Consequences

- Rules are testable in milliseconds, headless — enables balance testing, Monte Carlo, and
  AI-agent play.
- A hard boundary between simulation and presentation; the frontend can change (or be deferred —
  see ADR-0004) without touching rules.
- Slightly more up-front ceremony (FFI/JSON snapshot layer) than a monolithic game, accepted as the
  cost of the boundary.
