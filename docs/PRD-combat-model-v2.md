# PRD -- shipsim Combat Model v2 (Momentum / Power / Move-Fire Loop)

Source alignment:

- Seed notes: `tmp/game_mechinics.md`
- Grill log: `CONTEXT.md` (Combat model grill)
- Decision: **ADR-0020 Accepted** (supersedes ADR-0019 combat end-state and ADR-0002 impulse end-state)
- Infrastructure retained: ADR-0001 (core/frontend split), ADR-0003 (generic ships), ADR-0017/0018 (Love JSON + soft errors)

Pipeline stage: **to-prd complete**.  
Autonomy: **Checkpoint**.  
Execution policy: **cost_optimized**.

## Problem Statement

shipsim has cycled through SFB-like 32-impulse combat and a short Bocchino/FASA-style three-round
action MVP. Neither matches the combat game the maintainer wants to play.

The desired game emphasizes:

- **Turn-start power allocation** across movement, specific weapons, and specific shield facings
- **Momentum** (continuing forward is cheap; reversing is expensive)
- **Power-ordered movement** one step at a time, alternating with **simultaneous fire**
- **Range-dependent hit chance and damage**, with distinct beam / plasma / torpedo roles
- **Shields that only work if powered**, deplete within the turn, and can be repowered next turn

Without implementing this model as the product ruleset, further client polish only packages the
wrong loop.

## Solution

Implement **Combat Model v2** (ADR-0020) in `shipsim_core`, then retarget the harness contract and
Love client:

1. **Turn start:** Player (and AI) allocates a single power pool to movement budget, weapon charges,
   and six shield facings (sum ≤ ship power). Allocation is locked for the turn.
2. **Initiative:** Once per turn, order ships by allocated movement (high first; ties random once).
3. **Loop** until no useful actions remain (or players end the turn):
   - **Movement phase:** Each ship with remaining movement power may take **one** step or pass
     (momentum costs apply).
   - **Firing phase:** Ships commit any number of legal powered weapons (target + facing); when all
     have chosen, resolve **simultaneously**.
4. **Weapons:** Torpedo (cost 1, flat damage), plasma (cost 1, high close / steep falloff), beam
   (variable charge, all-or-nothing fire, damage scales with charge and range; half-up rounding;
   rounded damage &lt; 1 is illegal for that charge).
5. **Shields:** Per-facing powered pools; geometry-constrained facing choice on hit; overflow to
   hull; hull 0 destroys the ship.
6. **Win:** Destruction-based scenarios for MVP.
7. **Headless tests** prove the loop; Love provides allocation UI, move order, move/fire phases,
   facing pick, hit/miss feedback.

MVP ship power: escort **14**, cruiser **22**. Map scale ~20--28 hexes; weapon max ranges beam 10 /
plasma 14 / torp 12; to-hit on **d20**.

## User Stories

1. As a player, I want to allocate power at the start of each turn to movement, weapons, and each
   shield facing, so that I make explicit tradeoffs.
2. As a player, I want unused allocation to be locked for the turn, so that I cannot reshuffle after
   seeing mid-turn outcomes.
3. As a player, I want movement order determined by how much power I put into movement, so that
   speed investment also buys initiative.
4. As a player, I want ties in movement initiative broken randomly once and then frozen, so that
   order is stable and visible.
5. As a player, I want each movement phase to allow every ship with move power left to take one step
   or pass, so that no one runs their full budget before others can react with fire.
6. As a player, I want continuing forward (and forward slips if present) to cost one move power, so
   that pursuit is efficient.
7. As a player, I want reversing keel direction to cost two move power, so that momentum matters.
8. As a player, I want turning in place to cost one move power without flipping keel state, so that
   I can reorient carefully.
9. As a player, I want a firing phase after each movement phase, so that closing range can enable
   shots the same turn cycle.
10. As a player, I want to commit multiple legal weapons in one firing phase, so that alpha strikes
    are possible.
11. As a player, I want all committed shots to resolve simultaneously from pre-fire state, so that
    mutual kills and fairness hold.
12. As a player, I want to choose target ship and shield facing when firing, so that geometry and
    shield investment matter.
13. As a player, I want only geometry-legal facings available (often two; point-blank often one), so
    that facing choice is constrained.
14. As a player, I want unpowered shield facings to absorb nothing, so that powering the wrong face
    is punished.
15. As a player, I want powered shield points to deplete as they absorb damage within the turn, so
    that repeated hits punch through.
16. As a player, I want to repower shields next turn up to ship maximums, so that shields recover
    between turns if powered again.
17. As a player, I want hull reduced by overflow and ships destroyed at hull 0, so that fights end
    cleanly.
18. As a player, I want beam weapons to accept variable charge with higher charge dealing more
    damage, so that beams scale with investment.
19. As a player, I want a beam's full charge spent on one shot (all-or-nothing), so that I cannot
    dribble charge across many weak shots.
20. As a player, I want beam damage to fall with range (~2 per charge at PB to ~1 at max), so that
    range matters but a hit still matters.
21. As a player, I want half-up rounding and a ban on shots that would deal 0 after rounding, so that
    "in range" for low charge can require more power or closer range.
22. As a player, I want plasma to cost 1, deal high damage up close, and fall off faster than beams,
    so that plasma has a clear identity.
23. As a player, I want torpedoes to cost 1 and deal flat damage in range, so that they are reliable
    punch-through tools.
24. As a player, I want hit chance on a d20 that worsens with range, so that long shots are risky.
25. As a player, I want each weapon to fire at most once per turn, so that recharge limits apply.
26. As a player, I want weapon arcs of about 90 degrees on mounts F/FP/FS/A/AP/AS, so that mounting
    and heading matter.
27. As a player, I want to end my turn early with a warning if I still have usable move/weapon power,
    so that I do not discard resources by accident.
28. As a player, I want no nag warning when nobody has a legal action left, so that clean turn ends
    are smooth.
29. As a player, I want clear hit/miss and damage feedback after fire resolution, so that outcomes
    are readable.
30. As a player, I want to see enemy shield facings and hull when selecting a target, so that I can
    aim power and weapons intelligently.
31. As a player, I want MVP power budgets (escort 14 / cruiser 22) that force tradeoffs, so that
    allocation is interesting.
32. As a player, I want maps sized for roughly doubled prior ranges (~20--28 hexes), so that
    maneuver has room.
33. As a developer, I want all rules in the Rust core, so that Love stays thin (ADR-0001).
34. As a developer, I want NDJSON orders/snapshots for allocate/move/pass/commit-fire/resolve/end-
    turn, so that agents test without a window.
35. As a developer, I want soft-reject on illegal orders (ADR-0018), so that sessions survive mistakes.
36. As a developer, I want deterministic PRNG given seed for to-hit and initiative ties, so that
    fixtures lock.
37. As a developer, I want generic ship data only (ADR-0003), so that content stays unencumbered.
38. As a developer, I want the old impulse and Bocchino loops removed or quarantined after v2 lands,
    so that one ruleset remains.
39. As a maintainer, I want disengage/objectives, full SSD crits, sensors/cloak, and dual rulesets
    deferred, so that MVP ships.
40. As an agent implementer, I want a milestone plan with falsifiable gates after this PRD, so that
    implementation can proceed under Killhouse.

## Implementation Decisions

### Rules authority

- ADR-0020 and this PRD; seed notes in `tmp/game_mechinics.md`.
- Working combat tables (d20 thresholds, plasma falloff, beam factors) as specified in grill;
  tunable constants in data or core tables.

### Core

- Replace current FASA 3-round `Move`/`Fire`/`EndAction` product loop with v2 state machine:
  `Allocate` → (`MovementPhase` → `FiringPhase`)* → `TurnEnd`.
- Track: power allocation, remaining move power, weapon charged amounts and fired flags, shield
  facing remaining, momentum keel state, move order list, fire commitments.
- New/updated orders (names finalized in plan): allocate; move/pass; commit_fire / clear_commit;
  resolve_fire_phase (or auto after all committed/passed); end_turn.
- Snapshot: turn, phase, move order, remaining move, power breakdown or remaining shields/weapons,
  commitments, combat log with hit/miss.
- Simultaneous fire: snapshot positions/shields; compute all; apply in deterministic order.
- AI: allocate greedily; move toward enemies; commit legal fire.

### Frontend (Love)

- Allocation UI (move / weapons / six facings, sum ≤ power).
- Display move order; highlight active mover in movement phase.
- Fire phase: multi-select weapons, target, facing; lock when ready; show projected beam damage.
- Status strip: hit/miss; leftover-power warning on end turn.
- Help text for v2 loop only.

### Delivery

- New implementation plan (Killhouse PLAN) after this PRD; do not silently extend ADR-0019 plan.
- Breaking JSON change expected; regenerate fixtures.
- Checkpoint autonomy for subsequent stages unless user switches.

### Seams

- Primary: harness NDJSON.
- Secondary: pure unit tests for momentum costs, beam rounding, range legality, initiative sort.

## Testing Decisions

Good tests assert external behavior: allocation rejection over cap; move cost 1 vs 2 reverse; fire
illegal when rounded beam damage 0; simultaneous mutual damage; shield deplete then hull;
turn end conditions.

### Required gate themes

1. Allocate sum ≤ power; lock for turn.
2. Move order stable after tie break.
3. One step per ship per movement phase; fire phase multi-commit simultaneous.
4. Momentum reverse costs 2.
5. Beam half-up and &lt;1 illegal; multi-charge enables longer range.
6. Torp flat / plasma steep / beam mild falloff.
7. Facing absorb then hull; destroy at 0.
8. Soft errors keep process alive.
9. Love pure tests for order builders after UI milestone.

### Prior art

- `tests/fasa.rs` patterns (rewrite for v2); harness golden streams; soft-reject tests.

## Out of Scope (MVP)

- Bocchino 3-round fidelity as product mode.
- SFB 32-impulse IMC / IFF.
- Objectives, disengage, campaign UI.
- Full FASA chart photocopy, sensors, cloak, crew skills.
- Dual concurrent rulesets after v2 ships.
- Perfect continuous 90° geometry (hex facing table is enough).
- Variable plasma charge (fixed cost 1).

## Further Notes

- ADR-0019 remains historical for the brief FASA implementation; ADR-0020 is combat authority.
- Current tree may still run FASA MVP until v2 implementation replaces it.
- Open plan defaults: sideslip as optional continue-forward; arc-to-hex map chosen consistently at
  implement; AI greedy.

## Assumptions and Open Questions

### Assumptions

1. Half-up rounding and effective OOR for beam damage &lt; 1 (agreed).
2. Map/ranges doubled; d20 tables as grilled working set.
3. Checkpoint + cost_optimized for next pipeline stages.
4. Single product ruleset (v2), not parallel modes.

### Open questions (non-blocking for PRD; plan may fix)

1. Exact hex facing sets for F/FP/FS/A/AP/AS.
2. Whether fire phase requires explicit "all players ready" order or auto when all ships passed
   commit UI.
3. Sample weapon max charge per class (beam max 2 escort / 4 cruiser suggested).

---

**Handoff:** PRD complete. Next at checkpoint: user continues → `loops/REVIEW_DOCUMENT` (optional)
or `loops/PLAN` → `implementation-plan` for v2 → IMPLEMENT milestones.
