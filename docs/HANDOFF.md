# shipsim Killhouse Handoff - post Slice 4 energy (minimal)

Purpose: resume the Killhouse `ask-kh` pipeline. Transient process state, not durable product spec.

## Current State

- Project / git root: `/mnt/storage/git_home/shipsim` (standalone repo).
- Branch: `master` (not pushed; see git log).
- **Slices 1–3 + state encapsulation + multi-ship readiness + Slice 4 D7 minimal: DONE.**
- `cargo test` green; `cargo clippy --all-targets -- -D warnings` clean.

## Recent commits (newest first, local)

1. Slice 4 energy allocation (Allocate / power / turn_speed)
2. Multi-ship readiness (Fire ship id, Terminal, BTreeMap, load hygiene)
3. GameState encapsulation
4. Slice 3 architecture + feature

## Next recommended slices

- Full D7 multi-bucket EA, **or** D1-fire impulse-gated fire, **or** D9 AI.
- Read `docs/ROADMAP.md`.

## Constraints

Pure core, thin CLI, generic ship data, 7-bit ASCII docs, declare/resolve seam.
