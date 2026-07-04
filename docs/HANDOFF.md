# shipsim - Killhouse Pipeline Handoff (Slice 2 / D5 combat)

Purpose: resume the Killhouse `ask-kh` pipeline for shipsim SLICE 2 (direct-fire combat) in a fresh
agent (Claude Code or Codex). Everything needed is on disk; read this first, then the plan/docs it
references. This file is transient (not part of durable history) - do not treat it as a spec.

## Where we are

- Project: `/home/chris/git_home/shipsim` (symlink to `/mnt/storage/git_home/shipsim`; git toplevel is
  `/mnt/storage/git_home`).
- Slice 1 (movement skeleton) is BUILT, tested, and COMMITTED: commits `a48ec08` + `ea3a39c` on
  branch `master`. `cargo test` green (18 tests).
- Slice 2 (D5 direct-fire combat) has been driven through: triage(major) -> grill -> to-prd ->
  REVIEW_DOCUMENT (CONVERGED) -> PLAN (**READY**, standard tier, blast-radius clear).
- **NEXT STAGE: `loops/IMPLEMENT_MILESTONE` for M1..M7** (see list below). Then CODE_REVIEW_TRIBUNAL,
  then ARCHITECTURE_DESIGN.
- No slice-2 production code exists yet; only slice-2 docs + the plan are written.

## Run state to carry forward

- classification: major
- stage: PLAN complete (READY) -> entering IMPLEMENT_MILESTONE (M1 first)
- autonomy: **NOT YET CHOSEN** - the user deflected the autonomy question to request this handoff.
  The resuming agent MUST confirm Checkpoint vs Autopilot before implementing. Default to Checkpoint
  if unsure. (7 milestones under the 8 unattended cap, so Autopilot is within budget.)
- execution_policy: cost_optimized
- model_tiers: none configured -> current-model-only
- model_routing: current-model-only
- budget: 7 milestones planned (cap 8); max_pipeline_reentries default 3; 0 consumed
- herdr: run delegated loop subagents in the BACKGROUND with explicit names (e.g. `d5-m1`,
  `d5-tribunal`) so they are visible in the user's herdr monitor.

## Slice-2 artifacts (the handoff surface - read these, not the transcript)

- `docs/PRD-slice2.md` - converged PRD (22 user stories, single test seam, seed-pinned destruction gate)
- `docs/CONTEXT-slice2.md` - slice-2 scope + logged assumptions
- `docs/adr/0005-seeded-prng-for-combat.md` - seeded PRNG; AMENDS slice-1 assumption A4 ("no RNG" ->
  reproducibility invariant)
- `docs/adr/0006-combat-model-slice2.md` - weapons/arcs, 6-facing shields + bleed-through, single
  structure pool, destruction win, fire-freely
- `implementation-plan-slice2.md` - **READY** plan, 7 milestones, falsifiable gates (proven RED vs the
  current committed baseline). AUTHORITATIVE for implementation.
- `docs/ROADMAP.md` - D5 in progress; D6/D5a/D7/D1/D2/D10 deferred (explicit non-goals)
- Slice-1 durable docs still apply: `CONTEXT.md`, `docs/adr/0001-0004`.

## The 7 milestones (from implementation-plan-slice2.md - that file is authoritative)

1. M1-tracer-combat-slice - TRACER: load scenarios/combat.toml, apply one fire order, defender's
   toward-attacker shield facing drops in the JSON snapshot (snapshot now also carries structure,
   destroyed, weapons, seed). START HERE.
2. M2-fire-validation - illegal fire (out-of-arc, out-of-range, fire-at-self, unknown weapon/target,
   refire-this-turn) rejected with distinct typed OrderErrors and zero mutation; facing-relative arc.
3. M3-shield-facing-and-bleedthrough - damage depletes the defender-facing-relative shield; overflow
   bleeds to the single structure pool same-step; non-overflow leaves structure unchanged.
4. M4-weapon-resolution-tables - phaser range-table damage + disruptor to-hit-by-range, seed-pinned
   exact AND proven range-dependent.
5. M5-destruction-win-terminal - HEADLINE: seed-pinned test fires until enemy structure 0, asserts
   status == Won AND enemy destroyed == true; slice-1 objective terminal preserved.
6. M6-reproducibility-and-determinism-guard - same seed+orders -> byte-identical snapshot; no
   thread_rng/SystemTime/Instant.
7. M7-roadmap-and-nongoals - ROADMAP marks D5 realized, keeps D6/D5a/D7/D1/D2/D10 deferred.

## Killhouse loop files (read directly; plugin install optional)

In `/mnt/storage/git_home/killhouse/`:
- `skills/ask-kh/SKILL.md` - pipeline driver + autonomy/checkpoint rules
- `loops/IMPLEMENT_MILESTONE.md` - **next**: TDD red-green per milestone; exits only when the
  milestone's acceptance gates pass in the terminal
- `loops/CODE_REVIEW_TRIBUNAL.md` - after M1-M7
- `loops/ARCHITECTURE_DESIGN.md` - final

## Rules in force (do not drift)

- TDD: write each milestone's falsifiable gate first, prove it RED at the CURRENT baseline (slice 1 is
  green, so combat gates fail because Order::Fire / combat scenario / shields / structure / seed are
  absent), then implement to green.
- Reproducibility (ADR-0005): all dice from a SEEDED PRNG owned by game_state; seed serialized in the
  snapshot; deterministic fire-resolution order (by ship id). thread_rng/SystemTime/Instant forbidden;
  seeded `rand` allowed. Add a reproducibility test.
- Combat model (ADR-0006): data-driven weapon ARCS (facing matters for offense), 6-facing shields with
  bleed-through to a SINGLE structure pool, fire-freely (no energy cost), destruction win.
- Do NOT regress slice-1's objective-hex terminal (keep it working).
- Do NOT build deferred mechanics: itemized damage/systems (D6), seeking weapons (D5a), energy arming
  (D7), 32-impulse (D1), simultaneous resolution (D2 - only reuse the declare/resolve seam), fleets
  (D10). Reject shortcuts that PRECLUDE the hooks AND scope-creep that BUILDS the deferred mechanics.
- All combat rules in pure-Rust shipsim_core; the CLI binary stays a thin harness. Generic ship/weapon
  data only (no ADB-trademarked names/values). Keep docs 7-bit ASCII.
- After each milestone's gate goes green, in Checkpoint mode stop and report before the next.

## Prompt to paste into a fresh session

Resume the Killhouse ask-kh pipeline for shipsim SLICE 2 (D5 combat). Do not restart it.

Project: `/home/chris/git_home/shipsim`.

Read in order:
1. `/home/chris/git_home/shipsim/docs/HANDOFF.md`   (this file)
2. `/home/chris/git_home/shipsim/implementation-plan-slice2.md`   (authoritative plan, M1-M7)
3. `docs/PRD-slice2.md`, `docs/CONTEXT-slice2.md`, `docs/adr/0005`, `docs/adr/0006`, `docs/ROADMAP.md`

Then read:
- `/mnt/storage/git_home/killhouse/skills/ask-kh/SKILL.md`
- `/mnt/storage/git_home/killhouse/loops/IMPLEMENT_MILESTONE.md`

Resume state:
- Slice 1 committed (a48ec08, ea3a39c), cargo test green. Slice 2 planned READY (standard tier).
- NEXT: IMPLEMENT_MILESTONE starting at M1 (tracer combat), through M7, then CODE_REVIEW_TRIBUNAL,
  then ARCHITECTURE_DESIGN.
- autonomy = NOT yet chosen: ask me Checkpoint vs Autopilot before writing code (default Checkpoint).
- execution_policy = cost_optimized; model_routing = current-model-only.
- Run delegated loops as named BACKGROUND subagents so they appear in herdr.
- TDD red-green; honor ADR-0005 (seeded-PRNG reproducibility) and ADR-0006 (arcs / 6-facing shields /
  single structure pool / fire-freely / destruction win); build ONLY milestone scope; do NOT build
  deferred D1/D2/D6/D5a/D7/D10; keep slice-1 objective terminal working; generic data; 7-bit ASCII.
- shipsim/ slice-2 docs are untracked; do not stage/commit without deciding the boundary.
