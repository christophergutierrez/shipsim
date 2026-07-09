# shipsim Killhouse Handoff - Slice 2 closed / Slice 3 docs ready

Purpose: resume the Killhouse `ask-kh` pipeline. Transient process state, not durable product spec.

## Current State

- Project / git root: `/mnt/storage/git_home/shipsim` is its OWN standalone git repo. Use it as
  the git root. Do NOT use `/mnt/storage/git_home` (the old parent monorepo) as this project's root.
- Branch: `master`.
- **Slice 2 (D5 direct-fire combat): DONE.** Full ask-kh gauntlet completed: IMPLEMENT_MILESTONE
  (M1-M7) -> CODE_REVIEW_TRIBUNAL -> ARCHITECTURE_DESIGN -> done. Both review loops PASSed with no
  blocking / Critical / High findings. Deferred TS1-TS4 / AS1-AS4 live in `docs/ROADMAP.md`.
- **Slice 3 (movement fidelity D1/D2/D3): docs only.** Grilling, PRD, and post-PRD spec-audit fixes
  are committed. No `implementation-plan-slice3.md` yet. No green Slice 3 code.
- Verified clean baseline (this handoff):
  - Working tree clean (discarded non-compiling impulse/plot WIP; was experimental only).
  - `cargo test`: 40 tests green.
  - `cargo clippy --all-targets`: clean (no warnings with `-D warnings` on lib/tests).
- Killhouse: Grok plugin `killhouse` v0.2.4 enabled; redqueen submodule synced + `uv sync` (mock
  evolve plumbing OK; meaningful fitness needs a real model endpoint). Local clone also at
  `/mnt/storage/git_home/killhouse`.

## Slice 2 review findings (non-blocking, all deferred)

Full text in `docs/ROADMAP.md`. None block Slice 2 or Slice 3 start.

Code Review Tribunal (PASS): TS1 dead `fire_attacker_index`; TS2 bare global `weapon_id`;
TS3 snapshot seed without PRNG position; TS4 silent weapon parse fallback.

Architecture Review (PASS; no Critical/High): AS1 dual terminal Options; AS2 fire geometry
recomputed; AS3 discarded `FireOutcome`; AS4 combat indexes + clones attacker.

## Slice 3 pipeline position (NEXT)

| Stage | Status |
|-------|--------|
| grill-with-docs | DONE — `docs/CONTEXT-slice3.md`, ADR-0007, ADR-0008 |
| to-prd | DONE — `docs/PRD-slice3.md` |
| REVIEW_DOCUMENT (spec audit) | DONE — fixes in `7461ecb` (ROADMAP D1-fire/D2-fire, CONTEXT vs ADR-0007) |
| PLAN | **NEXT** — produce `implementation-plan-slice3.md` |
| redqueen | optional; degrade if fitness 0 |
| IMPLEMENT_MILESTONE | after PLAN READY |
| CODE_REVIEW_TRIBUNAL / ARCHITECTURE_DESIGN | after all milestones |

Scope reminder (from PRD/CONTEXT; do not expand): 32-impulse IMC, `Plot`/`RunTurn` replacing
`Move`/`Face`/`EndTurn`, simultaneous per-impulse resolution + both-stop collisions, turn-mode at
plot submit, fire still turn-end (mechanics unchanged), scripted auto-plot, snapshot `impulse`,
`scenarios/impulse.toml`. Combat rules unchanged. No Energy Allocation (D7).

## Constraints still in force

- Pure-Rust `shipsim_core`; CLI harness thin. Generic ship/weapon data only (ADR-0003).
- Docs 7-bit ASCII. ADR-0002 declare/resolve seam preserved (`Plot` declares, `RunTurn` resolves).
- Seeded PRNG / combat model (ADR-0005, ADR-0006) unchanged this slice.

## Resume prompt (Slice 3)

Project / git root: `/mnt/storage/git_home/shipsim`.

Read in order:
1. `docs/HANDOFF.md` (this file)
2. `docs/PRD-slice3.md`, `docs/CONTEXT-slice3.md`
3. `docs/adr/0007-impulse-movement-slice3.md`, `docs/adr/0008-simultaneous-resolution-slice3.md`
4. `docs/ROADMAP.md` (deferred backlog + D1-fire / D2-fire)
5. Slice 2 code + tests as baseline (must stay green until orders API is cut over in-slice)
6. Killhouse: `skills/ask-kh/SKILL.md`, then `loops/PLAN.md` as the next stage

Do **not** resume Slice 2 implementation. Enter PLAN (or full ask-kh if scope is unclear).
