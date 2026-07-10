# M6 Verdict: AI + Multi-ship Scenarios

Verdict: PASS

Scope reviewed against `implementation-plan-combat-v2.md` M6 and
`.killhouse/artifacts/m6-slices.md`:

- Extracted shared fire-legality predicate
  `GameState::v2_shot_shield_facing(attacker, weapon, target) -> Option<u8>`.
  `weapon_has_legal_shot`, `can_any_legal_fire`, `end_turn_warning`, and the AI
  (`ai::v2_fire_commits`) all funnel through it. Fire legality now lives in
  three places, not four — the two error-producing validators
  (`validate_fire_commit_v2`, `validate_fire_commit_against_v2_snapshot`) plus
  the shared boolean/facing predicate. This folds M5 debt items #1 (reduce
  copies) and #2 (the always-true `!is_empty()` guard is gone).
- `ai::v2_allocation`: greedy deterministic allocation — movement toward
  nearest enemy (capped by design speed), then arm operational v2 weapons in
  id order (beams to full charge, plasma/torps to 1), then pour remainder
  into forward shield facings [0, 1, 5]. Total always <= `effective_power`,
  so `allocate_v2` never rejects it.
- `ai::v2_move_decision`: turn toward nearest enemy via `arc::bearing_to`,
  step forward when legal and affordable, else turn in place
  (`TurnStarboard`) as the safe fallback that always spends 1 move power.
  Returns `None` when `move_remaining == 0`.
- `ai::v2_fire_commits`: every operational, charged weapon that can bear on
  the seek target, paired with the first geometry-legal shield facing.
  Reuses `v2_shot_shield_facing` (no new legality copy).
- `GameState::resolve_v2_npc_actions`: phase-machine driver. Allocates
  un-allocated AI ships, emits the active AI mover's move/pass, commits each
  AI ship's legal shots then readies it, and ends the turn when only AI
  remain. Bounded by an 8192-step cap. Stops (returns control) when a human
  ship must act in any phase. Legacy `resolve_npc_actions` untouched.
- `scenarios/v2_duel.toml`: fully AI-drivable duel (both ships `controller =
  "ai"`) on a 24x20 map so the driver can play headless.

## Gates (independently re-run)

- `cargo test --test v2_ai` -> 2 passed (G1, G2)
- `cargo test` -> 62 passed (16 suites; +2 vs M5 baseline of 60)
- `cargo clippy --all-targets -- -D warnings` -> exit 0, clean

Gate mapping:

1. AI-only v2 duel, driven entirely by `resolve_v2_npc_actions`, completes
   at least 3 turns (or reaches a decisive win) without panicking. (G1)
2. 2v2 fleet: after all four ships allocate, `move_order` holds all four ids
   and each gets exactly one movement decision before the phase closes. (G2)

## Contract review (CODE_REVIEW_TRIBUNAL)

PASS — no blocking findings. Seven scrutiny points cleared: legality parity
with commit-time `validate_fire_commit_v2` (operational-weapon guard is
stricter but still commit-legal), determinism (Vec/BTreeMap iteration, no
unordered maps), termination (greedy move never passes while move power
remains; turn-in-place always spends 1; weapons fire at most once per turn;
8192 cap is belt-and-suspenders), allocation budget (always <=
effective_power), legacy isolation (`resolve_npc_actions` untouched), human
handback (driver breaks in all four phases when a human must act), and G1/G2
test soundness (guards and assertions are non-vacuous).

Non-blocking findings recorded as debt (not fixed in M6 per one-milestone
scope):

1. **(debt)** Fire legality still lives in three places —
   `validate_fire_commit_v2`, `validate_fire_commit_against_v2_snapshot`,
   and `v2_shot_shield_facing`. Down from four (M5), but a future arc/beam
   rule change must touch all three. Candidate for M7 cleanup.
2. **(info)** `v2_move_decision` fallback is always `TurnStarboard` regardless
   of which direction is shorter when the ship is already facing the target
   but blocked. Harmless — spends 1 move power either way — but a smarter
   fallback could turn toward open space. Not observable in the duel scenario.

## Files changed

- `src/game_state.rs` — `v2_shot_shield_facing`, `weapon_has_legal_shot`
  routed through it, `resolve_v2_npc_actions` driver
- `src/ai.rs` — `v2_allocation`, `v2_move_decision`, `v2_fire_commits`
- `scenarios/v2_duel.toml` — NEW, AI-only duel scenario
- `tests/v2_ai.rs` — NEW, 2 gates (G1, G2)
- `.killhouse/artifacts/m6-slices.md` — slice plan

## Notes

- No commit made (checkpoint policy; commit only on explicit user request).
- Worked in place in the intentionally-dirty tree; no worktree, no reset, no
  unrelated files touched.
- M7 (Delete FASA/legacy orders + rewrite tests) is next.
