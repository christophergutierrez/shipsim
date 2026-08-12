# Engine test fixtures

Scenario TOMLs used only by automated tests (Rust `cargo test`, REPL/Love2D/TUI
suites) — not meant to be played interactively. Playable scenarios live in
[`scenarios/`](../scenarios/) instead.

Kept one directory level deep (`fixtures/<name>.toml`, not nested further) so
`load_scenario(path)`'s `path.parent().parent()` data-root resolution
(`src/scenario.rs`) still lands on the repo root.

| File | Exercises |
|---|---|
| `combat.toml` | Scripted-ship auto-pump (player + `scripted` escort) across TUI/REPL/Love2D |
| `v4_conflict.toml` | Simultaneous path-conflict resolution |
| `simulation_duel.toml` | Simulation determinism/invariants, save/replay fingerprinting |
| `m7_hull_efficiency.toml` | Hull-efficiency rubric (`huge`-class SSD/zero-translation signal) |
| `class_id_duplicates.toml` | Duplicate `class_id` handling across ship catalogs |
