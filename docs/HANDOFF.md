# shipsim Killhouse Handoff - Slice 2 / D5 Direct-Fire Combat

Purpose: resume the Killhouse `ask-kh` pipeline for shipsim Slice 2. Read this before the plan. This
is transient process state, not durable product spec.

## Current State

- Project / git root: `/mnt/storage/git_home/shipsim` is its OWN standalone git repo. Use it as
  the git root. Do NOT use `/mnt/storage/git_home` (the old parent monorepo) as this project's root.
- Branch: `master`.
- Committed history (HEAD first):
  - `c982435` docs: record slice-2 review findings (tribunal TS1-TS4, architecture AS1-AS4)
  - `c408068` M7
  - `553a794` M6
  - `51ea201` M5
  - `311c75a` M4
  - `a9da667` M3
  - `1245324` Completed slice 1
- **Pipeline stage: DONE.** Slice 2 (D5 direct-fire combat) is COMPLETE. The full ask-kh gauntlet
  ran to its terminal state: IMPLEMENT_MILESTONE (M1-M7) -> CODE_REVIEW_TRIBUNAL -> ARCHITECTURE_DESIGN
  -> done. Both review loops PASSed with no blocking / Critical / High findings.
- HEAD is `c982435`. All code is committed. Working tree is clean except this file.
- `cargo test`: 40 tests green. `cargo clippy --all-targets`: clean. No uncommitted code.

## How this state was reached (crash recovery)

Killhouse crashed during the final HANDOFF bookkeeping, AFTER both review loops had already passed
and the review-findings commit (`c982435`) had landed. The crash left a stale HANDOFF.md edit in the
working tree (it still said "NEXT: M6"). This file was rewritten on recovery to reflect the true
terminal state. No substantive pipeline stage was interrupted; nothing needs re-running.

## Review findings (non-blocking, all deferred)

Both verdicts are PASS. None block Slice 2. Full text in `docs/ROADMAP.md`.

Code Review Tribunal (verdict PASS):
- TS1. `fire_attacker_index` is dead code (`src/game_state.rs`, trivial).
- TS2. Weapon identity is a bare global `weapon_id` (`src/game_state.rs`, latent correctness).
- TS3. Snapshot serializes the seed but not the PRNG's current position (`src/snapshot.rs`).
- TS4. `parse_weapon` silently falls back on bad data (`src/scenario.rs`, data-hygiene, low).

Architecture Review (verdict PASS; no Critical/High):
- AS1. Terminal modeled as two parallel Options (`src/game_state.rs`, Medium, domain).
- AS2. Fire geometry recomputed (`src/movement.rs`, `src/combat.rs`, Low/Medium).
- AS3. `FireOutcome` discarded (`src/movement.rs`, Low).
- AS4. Combat indexes the ships Vec and clones the attacker (`src/combat.rs`, Low).

## What is left (NOT part of Slice 2)

Slice 2 scope is fully delivered. The following are explicit non-goals tracked in `docs/ROADMAP.md`
with resume cues; do NOT build them as part of Slice 2:
- D1. Full 32-impulse turn + Impulse Movement Chart
- D2. Simultaneous plot-then-resolve movement
- D3. Turn-mode enforcement
- D4. SFB fixed/floating map + off-map rules
- D5a. More weapon classes / seeking weapons
- D6. Itemized damage allocation / destroyable systems (SSD)
- D7. Energy Allocation Form
- D8/D9/D10. Frontend, AI opponent, etc.

The TS1-TS4 / AS1-AS4 findings above are also deferred quality items, not Slice 2 work.

## Constraints honored throughout Slice 2

- All combat rules in pure-Rust shipsim_core; the CLI binary stays a thin harness. Generic ship/weapon
  data only (no ADB-trademarked names/values). Keep docs 7-bit ASCII.
- ADR-0005 (seeded-PRNG reproducibility) and ADR-0006 (arcs / 6-facing shields / single structure
  pool / fire-freely / destruction win) honored.
- The slice-1 objective-hex terminal was preserved alongside the new destruction terminal.
- The ADR-0002 declare/resolve seam held: `fire` reused it, no rewrite.

## Resume prompt (if a future slice is started)

Start a NEW slice; do not resume Slice 2. Slice 2 is done.

Project / git root: `/mnt/storage/git_home/shipsim` (standalone repo; NOT the parent monorepo).

Read in order:
1. `docs/HANDOFF.md` (this file)
2. `docs/ROADMAP.md` (deferred backlog + resume cues)
3. `docs/PRD.md`, `docs/CONTEXT.md` (slice-1 durable spec)
4. `docs/adr/` (architecture decisions)
5. `/mnt/storage/git_home/killhouse/skills/ask-kh/SKILL.md`
6. `/mnt/storage/git_home/killhouse/AGENTS.md`

Then classify the next request as trivial or major and enter the ask-kh flow at `/classify`.
