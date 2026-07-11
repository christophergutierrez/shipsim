# shipsim Roadmap

The current product definition is `docs/PRD.md`; system structure is documented in `docs/ARCHITECTURE.md`. Combat Model v2 (ADR-0020) is the only supported ruleset.

## Shipped MVP

- Turn-start allocation across movement, individual weapons, and six shield facings.
- Momentum-aware movement with stable per-turn initiative.
- Alternating movement and simultaneous firing phases.
- Beam, plasma, and torpedo range/damage profiles with deterministic d20 resolution.
- Powered shields, SSD hull/internals, destruction victory, multi-ship scenarios, and greedy AI.
- TOML ship/scenario/campaign loading and an NDJSON CLI harness with soft errors.
- Love2D thin client and deterministic Rust/Lua acceptance coverage.

The retired impulse, FASA, seeking-munition, and multi-bucket compatibility implementations have been removed. Their design history remains in `docs/adr/`.

## Recommended next work

1. Build a simulation runner for automated gameplay evaluation and balance analysis.
2. Use simulation results to drive a manual Love2D playthrough focused on high-value gameplay paths and UI behavior.
3. Add campaign save/resume after campaign state and progression semantics are finalized.
4. Improve combat-log presentation and replay navigation.

## Later features

- Electronic warfare, overloads, sensors, cloak, and crew quality.
- More ship and weapon content within the generic-content policy.
- Campaign persistence, branching campaigns, and campaign UI.
- Richer critical damage and repair.
- Network multiplayer.

Keep future rules in the Rust core and treat Love2D as a projection and order-entry client.
