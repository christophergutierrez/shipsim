# PRD -- shipsim: FASA / Bocchino STCS combat pivot

Source alignment:

- Rules: `tmp/StarshipCombat.pdf` (Bocchino 2013; FASA STCS adaptation), sections 5--9 and tables
- Decision: **ADR-0019 Accepted**
- Domain: `docs/CONTEXT-fasa-stcs.md`
- Executable plan: `implementation-plan-fasa-stcs.md` (M0--M8)
- Architecture kept: ADR-0001 (pure core + thin client), ADR-0003 (generic ships), ADR-0017/0018
  (Love JSON client + soft errors) with **retargeted** turn UX after core lands

Pipeline stage: **to-prd complete**. Stop here pending review/plan implementation.
Autonomy for later implement: **Checkpoint** recommended.
Execution policy: **cost_optimized**.

## Problem Statement

shipsim's core and Love client implement an **SFB-style** loop: energy buckets, pre-plotted paths,
and a 32-impulse resolution where direct fire is declared early and resolves on fixed weapon
windows. That is a coherent design, but it is **not** the starship combat game the maintainer
enjoys.

The preferred model is **FASA Starship Tactical Combat Simulator**-family play as described in
Bocchino's RPG rules: a **turn of three rounds**, ships acting in **action order**, spending a
**shared power pool** on **immediate actions** (move, fire, etc.). In that model, a ship can
**close range with a move action and fire on a later action in the same turn**. The current product
makes that natural pattern hard or impossible and forces players to learn impulse clocks they do
not want.

Without a rules pivot, further Love polish only packages the wrong game.

## Solution

Refactor `shipsim_core` (and then the harness + Love client) so the **authoritative combat loop**
matches Bocchino/FASA STCS **MVP** scope:

1. Each **turn** restores engine **power** (less damage).
2. Each turn has **three rounds**.
3. Each round, ships act in **action order** (deterministic skill + seeded roll, or scenario order).
4. On a ship's **action window**, it spends power on **move** and/or **fire** (and end action),
   subject to power and once-per-turn weapon limits.
5. **Fire resolves immediately** at current geometry (range, arc, facing shield, damage).
6. Remaining power may **absorb shield hits** via Shield Point Ratio (simplified MVP allowed).
7. Headless tests prove the headline loop; Love is retargeted to rounds/actions (not Energy/Plot/Run
   impulse language).
8. **Remove** 32-impulse IMC, pre-plot `RunTurn`, and IFF fire queues as the product path (plan M6).

Ship presentation stays **generic** (ADR-0003). Hex map and six shield facings remain. Full FASA
manual photocopy (every chart, sensors, cloak, crew skills) is **out of MVP**.

## User Stories

1. As a player, I want combat to use **turns and three rounds**, so that play matches FASA-style
   pacing I know.
2. As a player, I want ships to act in an **action order** each round, so that who moves/fires when
   is clear and sequential.
3. As a player, I want a **single power pool** per turn from my engines, so that I choose how much
   to spend on actions versus saving for defense.
4. As a player, I want a **move action** that costs Movement Point Ratio and changes hex/facing, so
   that I can maneuver without pre-plotting a full impulse path.
5. As a player, I want to **fire as an action** that resolves now, so that shots use the positions
   after my earlier moves this turn.
6. As a player, I want to **move into range and then fire in the same turn**, so that closing and
   shooting feels natural.
7. As a player, I want fire that is out of range or arc **rejected at fire time** with a clear
   error, so that I am not guessing impulse windows.
8. As a player, I want each weapon to fire **at most once per turn**, so that recharge limits match
   the PDF.
9. As a player, I want to spend power on **multiple weapons in one fire action** if affordable, so
   that a alpha strike is possible in one action.
10. As a player, I want **facing shields 1--6** to take hits based on geometry, so that orientation
    matters.
11. As a player, I want remaining power to help **absorb shield damage** (SPR), so that defense and
    offense trade off from one pool.
12. As a player, I want to see **whose action it is**, which **round**, and **power left** in the
    UI/snapshot, so that I can plan.
13. As a player, I want AI ships to act on their turn (move or fire greedily), so that solos are
    playable.
14. As a player, I want multi-turn fights to a win or loss in Love after the pivot, so that the
    desktop client still completes a game.
15. As a player, I want help text that describes rounds and actions, not 32 impulses, so that the
    UI matches the rules.
16. As a developer, I want all rules in `shipsim_core`, so that Love stays a thin client (ADR-0001).
17. As a developer, I want NDJSON orders/snapshots for the new loop, so that harness tests stay
    headless (ADR-0017/0018 patterns).
18. As a developer, I want soft-reject on illegal orders without killing the process, so that multi-
    action sessions survive mistakes.
19. As a developer, I want a **headline acceptance test**: out of range, move, then fire hits, so
    that the pivot cannot regress the preferred play pattern.
20. As a developer, I want the 32-impulse path **removed** after migration, so that two combat
    models do not rot in parallel.
21. As a developer, I want ship TOML extended with FASA-ish fields (engine power, MP ratio, SPR,
    weapon power costs) with **defaults** for old files, so that content migrates gently.
22. As a developer, I want deterministic PRNG for hit rolls, so that fixtures are stable.
23. As a developer, I want SSD/structure damage to remain testable when shields are penetrated, so
    that internals still matter in MVP.
24. As a maintainer, I want full FASA tables, sensors, cloak, and crew skills **deferred**, so that
    MVP ships.
25. As a maintainer, I want ADR-0002's impulse end-state **superseded** by ADR-0019, so that the
    roadmap is honest.
26. As a maintainer, I want generic ship names/stats only (ADR-0003), so that trademarked FASA ship
    stats are not required.
27. As an agent implementer, I want milestones M0--M8 with falsifiable gates, so that work can proceed
    without re-deriving the design.

## Implementation Decisions

### Rules authority

- **In:** Bocchino §5 Sequence of Play, §6 action list (subset), §6.1 basic move (Fig. 3 subset),
  §6.4 weapon fire limits, §7 resolve fire (simplified tables OK), shield facings, power vs soak.
- **Out of MVP:** sensors/cloak/transporters, full firing chart A--Y, detailed damage location
  photocopy, emergency stress charts, dual SFB mode.

### Core architecture

- Replace impulse `RunTurn` loop with **turn → round 1..3 → action order → ship action window**.
- New orders (names finalized in M1; shapes as in plan): start turn, action order, begin/end action,
  move modes, fire with power allocation, end round.
- Snapshot: `turn`, `round`, `active_ship`, `action_order`, per-ship `power_remaining`,
  weapons fired this turn, positions, facings, shields, structure.
- Power: one pool per ship per turn; move/fire subtract; soak uses SPR.
- Combat: immediate resolution; one weapon once per turn; multi-weapon fire action if power allows.
- AI: greedy move or fire on its action.
- Seeking/drone path: disabled or deferred in FASA MVP.
- Legacy impulse code: quarantine then delete (M6).

### Frontend

- Love remains JSON subprocess, soft errors, scenario picker.
- UI phases become **round / active ship / move / fire / end action**, not Energy→Plot→Fire→Run
  impulse narrative.
- Help documents FASA loop only after M7.

### Delivery

- Follow `implementation-plan-fasa-stcs.md` milestones M0--M8.
- Headline gate at M2 and M8: move-then-fire same turn.
- Breaking JSON change is explicit; regenerate fixtures with core.

### Seams

- Primary seam: harness NDJSON (existing).
- Secondary: pure unit tests for move legality, power, to-hit.
- Prefer extending one order API over a second parallel engine long-term.

## Testing Decisions

Good tests assert external behavior: order rejected/accepted, power delta, position change, damage
applied, round/turn advance -- not private impulse indices.

### Required suites

1. Turn state machine (3 rounds, power reset).
2. **Tracer:** out of range fire fails; after move(s) into range, fire succeeds.
3. Weapon once per turn; insufficient power rejects.
4. Multi-ship action order ordering.
5. Soft-error harness still alive after illegal fire.
6. Golden NDJSON fixture for a short duel script.
7. Love pure tests updated for new order builders (M7).

### Prior art

- `tests/harness.rs`, `tests/combat.rs`, `tests/acceptance.rs` patterns -- rewrite for new orders;
  delete IFF/IMC-specific cases in M6.

### Agent policy

- Do not require opening Love for core correctness.
- Prefer log dumps of order/error/snapshot lines.

## Out of Scope

- Remaining SFB-as-product impulse fidelity (ADR-0002 end-state).
- Full FASA STCS book implementation in MVP.
- Crew skill economy, casualties UI, cloaking, sensor locks, transporters.
- Godot/web clients.
- FFI bridge.
- Core `Lost` status (still optional residual).
- Perfect Fig. 3 / warp-speed / reverse edge-case parity in first shippable slice.
- Dual concurrent rulesets (impulse + FASA) after M6.

## Further Notes

- Rules PDF is reference under `tmp/` (not shipped as product IP claims beyond generic mechanics
  reimplementation; keep generic content).
- D8 Love client remains valuable infrastructure; its **interaction model** must change with the
  core (M7), not only labels.
- Open plan defaults (unless later overridden): multiple power spends until `end_action`; seeded
  action order; seeking disabled in FASA path; no dual-mode.

## Assumptions and Open Questions

### Assumptions

1. ADR-0019 is **Accepted** (this PRD).
2. Plan M0--M8 is the delivery vehicle; no plan revision requested.
3. Simplified to-hit and SSD mapping are acceptable stand-ins for full FASA charts in MVP.
4. Checkpoint before implementation code is preferred; this PRD stops before IMPLEMENT.

### Open questions (non-blocking; plan defaults)

1. Exact default captain skill / roll formula for action order.
2. Whether `end_round` is explicit order or automatic when all ships acted.
3. Minimal Fig. 3 move mode set for M2 (forward + turns only vs slips).

---

**Handoff:** PRD complete. Next Killhouse stage when resumed: `loops/REVIEW_DOCUMENT` on this PRD
(optional) or **IMPLEMENT_MILESTONE M0** (docs already largely done) / **M1** per
`implementation-plan-fasa-stcs.md`.
