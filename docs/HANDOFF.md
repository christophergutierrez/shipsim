# shipsim Killhouse Handoff - Slice 3 movement fidelity COMPLETE

Purpose: resume the Killhouse `ask-kh` pipeline. Transient process state, not durable product spec.

## Current State

- Project / git root: `/mnt/storage/git_home/shipsim` (standalone repo).
- Branch: `master`.
- **Slice 2 (D5 combat): DONE** (prior).
- **Slice 3 (D1/D2/D3 movement fidelity): DONE.**
  - PLAN: `implementation-plan-slice3.md` (verdict READY).
  - IMPLEMENT: M1 IMC + M2 cutover (Plot/RunTurn, simultaneous collision, fire-at-turn-end,
    speed rename, impulse scenario, full test migration) through M7 docs.
  - `cargo test`: all green (51 tests: 6 unit IMC + 45 integration).
  - `cargo clippy --all-targets -- -D warnings`: clean.
- Killhouse: Grok plugin v0.2.4; redqueen submodule synced (`uv sync`); mock evolve plumbing OK
  (fitness 0.0 without a live model endpoint — IMPLEMENT used plan contracts, not evolved prompt).

## What landed (Slice 3)

- `src/impulse.rs` — IMC pure functions + unit tests.
- Order API: `Plot` / `Fire` / `RunTurn` only (Move/Face/EndTurn removed).
- Turn-mode + occupancy + adjacency plot validation at submit.
- Per-impulse simultaneous apply; both-stop collision; scripted auto-plot.
- Fire queued at declare, resolved after movement at RunTurn end.
- Snapshot: `impulse`, ship field `speed` (was `speed_max`); TOML `speed`.
- `scenarios/impulse.toml` + updated orders/tests/CLI data.

## Deferred (not Slice 3)

See `docs/ROADMAP.md`: D1-fire, D2-fire, D4 map, D5a weapons, D6 SSD, D7 energy, D8-D10, plus
slice-2 TS*/AS* quality notes.

## Resume prompt (next work)

Start a NEW slice via ask-kh / triage. Do not reopen Slice 2 or 3 unless fixing a regression.

Read: `docs/HANDOFF.md`, `docs/ROADMAP.md`, latest PRD/CONTEXT for the new request, `docs/adr/`.
