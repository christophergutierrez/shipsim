# Implementation Plan: Pivot shipsim to Bocchino / FASA STCS combat

## Planning Verdict

- **verdict:** READY
- **task_tier:** full
- **tier_trigger:** Public wire-format change (orders/snapshots), behavioral replacement of turn
  resolution, cross-cutting core + tests + Love client; supersedes prior SFB impulse end-state.
- **execution_policy:** cost_optimized (default)
- **model_routing:** current-model-only
- **autonomy:** checkpoint recommended (blast radius high; confirm ADR-0019 before M1)
- **reason:** Maintainer preferred rules are `tmp/StarshipCombat.pdf` (Bocchino 2013 / FASA STCS
  adaptation). Current core implements SFB-like 32-impulse play (ADR-0002 path). Refactor must
  replace the turn clock and order model while preserving hex/core split and agent-testable gates.

## Repository State (staleness contract)

- VCS: `/mnt/storage/git_home/shipsim`
- Plan authored against master including D8 Love client and ADR-0017/0018.
- Rules PDF: `tmp/StarshipCombat.pdf` (13 pp; not product code; local reference).
- Primary modules today: `turn.rs` (impulse loop), `impulse.rs` (IMC), `movement.rs` (Plot/RunTurn),
  `combat.rs` (IFF + legality), `energy.rs` (buckets), `game_state.rs`, `snapshot.rs`,
  `frontend/love/*`.

**Re-check before implement:** `git rev-parse HEAD` and re-read ADR-0019 + this plan if HEAD moved.

## Problem / goal

| Today (shipsim) | Target (Bocchino §5--§7) |
|-----------------|---------------------------|
| Declare allocate/plot/fire then `RunTurn` | Turn = up to **3 rounds** of **actions** |
| IMC: speed S steps on selected impulses | **Basic move** action costs Movement Point Ratio; Fig. 3 maneuvers |
| Simultaneous multi-ship steps per impulse | **Action order** (skill/D100); ships act sequentially |
| Fire queued; resolve on IFF (4,8,...) | **Fire weapons** action; resolve **immediately** |
| Range checked at declare and again at IFF | Range/arc at **fire action** time (after prior moves this turn) |
| Energy buckets move/weapons/shields | **One power pool**: spend on actions; remainder can absorb shield hits (SPR) |

**Headline product outcome:** Player can **move closer as an action**, then on a later action in
the same turn **fire** while in range -- without 32-impulse machinery.

## Rules scope (v1 product vs later)

### In scope for this plan (MVP FASA loop)

From PDF §5--§7, simplified for headless + Love:

1. **Turn** with **3 rounds**.
2. **Action order** per round (deterministic: sort by captain skill + seeded PRNG roll; or fixed
   scenario order as interim).
3. **Power pool** per turn = sum of functioning engine power (schema field(s)); spent by actions.
4. **Actions (subset):**
   - Basic move (1 MP worth of hex change per action; forward/turn/slip as Fig. 3 subset).
   - Fire weapons (one or more weapons, each at most once per turn; pay weapon power; resolve hit now).
   - Pass / end ship action (optional explicit).
5. **Shield facings** 1--6; hit facing from geometry (Fig. 5 simplified).
6. **Hit resolution (MVP):** range hexes, D10 vs simplified to-hit table (or keep current PRNG to-hit
   curves as stand-in), damage to facing shield then SSD/structure.
7. **Shield absorption:** optional spend remaining power via Shield Point Ratio when hit (MVP may
   use fixed absorb-from-remaining-power).
8. **Harness + Love** speak the new order/snapshot contract.
9. **AI:** greedy: move toward target or fire if in range when its action comes up.

### Explicitly deferred (post-MVP backlog)

- Full FASA firing chart A--Y and all weapon banks tables.
- Crew skill rolls, casualty %, sensor locks, cloaking, transporters.
- Emergency heading change stress charts, warp speed ladder, reverse as separate action.
- Plasma multi-location rolls, detailed damage location tables (use simplified SSD).
- Side-based interleaved action order.
- Perfect STCS movement point accounting for every Fig. 3 edge case.
- Keeping SFB impulse mode as a selectable ruleset (unless a later ADR revives dual-mode).

## Architecture decisions

1. **ADR-0019** accepted before code removal of impulse path.
2. **Clean break on orders** (no dual Plot/RunTurn forever): new `Order` variants for the FASA loop;
   delete or quarantine impulse orders after migration milestones.
3. **Turn state machine** (core):

   ```text
   TurnStart { power_reset }
     -> Round(1..=3)
          -> pick next ship in action_order (not yet acted this round)
          -> ShipAction window (0..N power spends until ship ends action or power 0)
          -> next ship | next round | TurnEnd
   ```

4. **Orders (proposed wire shapes)** -- final names in M1 contract:

   ```json
   {"type":"start_turn"}
   {"type":"set_action_order","ships":[1,2]}
   {"type":"begin_action","ship":1}
   {"type":"move","ship":1,"mode":"forward"} 
   {"type":"move","ship":1,"mode":"turn_port"}
   {"type":"fire","ship":1,"weapons":["phaser_1"],"target":2,"power":[4]}
   {"type":"end_action","ship":1}
   {"type":"end_round"}
   ```

   Snapshot must expose: `turn`, `round` (1..3), `phase` (`planning`/`acting`), `active_ship`,
   `power_remaining` per ship, `weapons_fired_this_turn`, `action_order`, shield facings, positions.

5. **Delete path:** `impulse.rs` IMC as authority; `turn.rs` 32-loop; IFF `fires_on_impulse` for
   direct fire; Plot path store / simultaneous collision-from-plots (replace with sequential move
   rules). Seeking weapons: either map to FASA missiles later or park behind feature flag.

6. **Love UI:** replace Energy/Plot/Fire/Run with **Turn / Round / Your action** -- spend power on
   Move or Fire; show power remaining and whose action it is.

7. **Tests first per milestone:** rewrite acceptance around "move action then fire action in range"
   as the headline gate (the maintainer pain point).

## What to keep (low rewrite)

- `hex.rs`, `board.rs` (map modes may stay).
- Shield facings + SSD damage ideas (`ssd.rs`) -- may simplify tables.
- `prng.rs`, scenario/campaign loaders (extend schema).
- CLI harness NDJSON pattern (ADR-0018 soft errors, post-load snapshot).
- Love harness adapter pattern (orders file replay).
- Generic ship TOML (extend fields: movement_point_ratio, shield_point_ratio, engine_power, etc.).

## Blast radius

| Area | Impact |
|------|--------|
| `Order` / snapshot JSON | **Breaking** -- regenerate all fixtures |
| `tests/*` | Most combat/movement/impulse tests rewrite or delete |
| `scenarios/*` | Rebalance for MP ratio / power, not speed-only plots |
| `frontend/love` | Phase model rewrite |
| ADR-0002 / ROADMAP | Document pivot; mark impulse IDs historical |

## Test commands

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo test --test harness
luajit frontend/love/tests/run_all.lua   # after Love milestones
```

## Tracer bullet (first non-spike milestone)

**M2:** One ship spends power to **move one hex**, then (same turn, later action or same action
policy as designed) **fires** at a target that was out of range before the move and is in range
after -- headless test green. No 32-loop.

---

## Milestones

### M0 -- Decision lock + domain glossary (docs only)

**Scope:** Accept ADR-0019; add `docs/CONTEXT-fasa-stcs.md` glossary (turn, round, action, power,
MP ratio, SPR, facing shields 1--6); ROADMAP section "FASA pivot"; note ADR-0002 superseded for
impulse end-state.

**Gates:**

1. ADR-0019 status Accepted in tree.
2. Glossary defines terms used in later milestones without SFB impulse language as authority.

**Not code.**

---

### M1 -- Turn state machine skeleton (core)

**Scope:** New turn model in core (prefer new module `fasa_turn.rs` or rewrite `turn.rs`):

- Fields: `round: 1..=3`, `action_order: Vec<ship_id>`, `acted_this_round: Set`, `active_ship`,
  `power_remaining[ship]`.
- Orders: begin/end action, end_round / advance, start_turn power reset.
- Snapshot fields for the above.
- **No** call into 32-impulse loop on these paths.
- Leave old `RunTurn` temporarily behind `#[cfg]` or `legacy` module **or** delete if tests allow
  (prefer quarantine in `legacy_impulse/` for one milestone then delete in M6).

**Gates:**

1. `cargo test` has a new test: start turn → 3 rounds of empty end_round → turn advances; power
   resets each turn.
2. Snapshot JSON includes `round` and `power_remaining` for each ship.
3. Clippy clean on touched code.

**File contracts (thin):**

| Path | Responsibility |
|------|----------------|
| `src/turn.rs` or `src/fasa_turn.rs` | Round/action state machine |
| `src/game_state.rs` | Power remaining, active ship, round |
| `src/snapshot.rs` | Serialize new fields |
| `src/movement.rs` | New order variants (or `src/orders.rs` split) |
| `tests/fasa_turn.rs` | State machine tests |

---

### M2 -- Tracer: sequential move action + immediate fire (headline)

**Scope:**

- `move` order: apply one basic-move mode (minimum: forward into facing hex; optional turn_port /
  turn_starboard). Cost = ship `movement_point_ratio` (default 1 if missing).
- Reject move if insufficient power; subtract power.
- `fire` order: pay weapon power, require range/arc **now**, apply damage **now** (reuse simplified
  combat hit + shield facing + structure). Mark weapon fired this turn.
- Action economy: while `active_ship` matches, player may issue multiple move/fire until
  `end_action` or power 0 (MVP policy: **one move OR one fire per action window** is acceptable if
  documented -- prefer **multiple spends until end_action** to match PDF "spend power points on
  actions" in the window).

**Headline gate (must fail on impulse-era code, pass on FASA):**

1. Ship A out of max_range of B → fire rejected.
2. A moves into range (one or more move actions) → fire accepted → B shields/structure change.
3. No dependency on `impulse` 1..32 or `RunTurn`.

**Also:**

4. Second fire same weapon same turn rejected.
5. Fixture or unit test records power before/after move and fire.

---

### M3 -- Three rounds, action order, multi-ship

**Scope:**

- Build action order each round (seeded roll + captain skill field on ship or scenario; deterministic).
- Only `active_ship` may move/fire.
- After all ships `end_action`, auto or explicit `end_round`; after round 3, turn end + power reset.
- AI: on its action, if enemy in range and weapon free, fire; else move toward enemy.

**Gates:**

1. Two-ship test: order ship2 then ship1; ship2 moves first in round 1.
2. After three rounds, `turn` increments and power restored (minus engine damage if any).
3. AI ship in `scenarios/ai.toml`-class setup completes a turn without panic.

---

### M4 -- Power, shields soak (SPR), ship data schema

**Scope:**

- TOML: `engine_power` (or keep `power`), `movement_point_ratio`, `shield_point_ratio`,
  per-weapon power cost / max power.
- Shield hit absorption spends power by SPR when player/AI elects (MVP: auto-absorb up to remaining
  power until SPR depleted or damage applied).
- Facing shield pool reset rules: PDF resets max shield power each **round** -- implement
  simplified: each facing capacity from ship data; damage reduces facing; round reset optional MVP
  note in tests.

**Gates:**

1. Scenario loads new fields with defaults for old TOML.
2. Test: fire damage reduced when defender has spare power and SPR 1/2 (or configured).
3. Power cannot go negative; action rejected at 0 power.

---

### M5 -- Combat fidelity slice (tables lite)

**Scope:**

- Range → to-hit via table or function (can port simplified D10 thresholds from FASA chart for
  ranges 1--10).
- Weapon arcs as today; banked weapons optional stub.
- SSD: keep structure/engine/weapon boxes; map penetrating damage to existing SSD allocator.
- Seeking/drones: **disable or no-op** in FASA path until missile milestone (document).

**Gates:**

1. Deterministic seed: same fire sequence → same hit/miss log.
2. Out of arc / out of range fire rejected at action time with soft-error codes.
3. `cargo test` combat suite under new names green; old IFF tests removed or ignored.

---

### M6 -- Delete impulse legacy + harness/fixtures

**Scope:**

- Remove or fully quarantine: IMC `moves_on_impulse` call path, 32-loop, Plot path resolution,
  IFF drain, old orders `plot`/`run_turn`/`allocate` buckets if replaced.
- Regenerate `tests/fixtures/d8/` or replace with `tests/fixtures/fasa/`.
- Update `docs/D8-FRONTEND-BRIEF.md` protocol section; ADR-0018 remains for soft errors.
- ROADMAP: impulse D* items → historical; FASA milestones listed.

**Gates:**

1. `rg -n "RunTurn|moves_on_impulse|fires_on_impulse" src/` empty (or only legacy module deleted).
2. `cargo test` full green; harness golden matches new stream.
3. Clippy `-D warnings` clean.

---

### M7 -- Love client for rounds/actions

**Scope:**

- UI: show turn, round 1--3, action order, active ship, power remaining.
- Controls: Move (forward/turn), Fire (weapon + target), End action, (auto) advance round.
- Help `?` rewritten for FASA loop (no impulse essay).
- Headless `luajit` tests for pure helpers; optional integration via harness.

**Gates:**

1. `luajit frontend/love/tests/run_all.lua` green against new orders.
2. Manual/smoke: move into range then fire in one turn without impulse language in UI.
3. Soft-error status strip still works.

---

### M8 -- Polish + acceptance scenario

**Scope:**

- `scenarios/fasa_duel.toml` (or retune `combat.toml`) as headline duel.
- Acceptance test: scripted order list plays full turn to damage or win.
- CONTEXT + PRD note "FASA STCS MVP complete"; residual backlog listed.

**Gates:**

1. `tests/acceptance.rs` (or `tests/fasa_acceptance.rs`) proves move-then-fire same turn.
2. Full `cargo test` + Love headless green.
3. Short `docs/PLAY-FASA.md` player-facing turn summary (1 page).

---

## Milestone dependency graph

```text
M0 docs
  -> M1 turn SM
    -> M2 move+fire tracer  *** critical path ***
      -> M3 multi-ship rounds + AI
        -> M4 power/SPR schema
          -> M5 combat tables lite
            -> M6 delete impulse + fixtures
              -> M7 Love
                -> M8 acceptance polish
```

Budget: 8 milestones (fits `max_milestones_unattended` default). Do not expand tables scope inside M2--M3.

## Traceability (outcomes → gates)

| Outcome | Gate |
|---------|------|
| Preferred ruleset locked | M0 ADR-0019 |
| No 32-impulse required for play | M2, M6 |
| Move then fire same turn | M2 headline, M8 |
| Sequential multi-ship | M3 |
| Power pool + shield soak | M4 |
| Thin client playable | M7 |
| Legacy impulse gone | M6 |

## Invariants

1. Core remains pure Rust, no Love deps (ADR-0001).
2. All rules headless-testable; Love never reimplements range/power.
3. Generic ship names/stats (ADR-0003).
4. Deterministic given seed for combat rolls.
5. Soft-reject illegal orders keep harness alive (ADR-0018).
6. One weapon at most once per turn (PDF §6.4).

## Risks and mitigations

| Risk | Mitigation |
|------|------------|
| Scope creeps to full FASA manuals | Hard defer list; M2 tracer only |
| Dual-mode forever | M6 deletes impulse path |
| Love blocks core | Core M1--M6 first; M7 after fixtures |
| Schema break for old TOML | Defaults in loader (M4) |
| AI too weak | Greedy only until later |

## PLAN blast-radius note

Public JSON order/snapshot contract changes. **Not** a silent patch. Fixtures and frontend must
ship in the same program of work. Human should **Accept ADR-0019** before M1 code.

## Downstream handoff

- **Next stage after plan approval:** IMPLEMENT_MILESTONE M0 then M1 (TDD).
- **Redqueen:** optional; plain implementer prompt OK.
- **Do not** implement full FASA chart photocopy in M1--M2.

## Open questions (non-blocking defaults)

1. **Multiple power spends per action window** vs one atomic action -- default: multiple until
   `end_action` (closer to PDF).
2. **Action order** -- default: seeded D100 + `captain_skill` field default 50.
3. **Seeking weapons** -- default: disabled in FASA path until explicit milestone.
4. **Retain impulse as optional ruleset** -- default: **no** (M6 deletes).

---

## Appendix A -- PDF section map

| PDF | Use in shipsim |
|-----|----------------|
| §1 hex + shields 1--6 | Keep / align facing labels |
| §5 Sequence of Play | M1--M3 authority |
| §6 Actions + Table 1 | M2--M4 actions subset |
| §6.1 Fig. 3 moves | M2 move modes |
| §6.4 Weapon fire | M2/M5 |
| §7 Resolving fire | M5 |
| §8 Damage location | simplify via SSD |
| Charts p.12--13 | optional data later |

## Appendix B -- Suggested first commits (after approval)

1. `docs: ADR-0019 + CONTEXT-fasa + ROADMAP pivot`
2. `feat(core): FASA turn state machine (M1)`
3. `feat(core): move/fire actions tracer (M2)`
4. ... follow milestones
