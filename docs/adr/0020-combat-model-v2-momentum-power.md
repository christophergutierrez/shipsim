# ADR-0020 -- Combat model v2 (momentum, powered shields, move/fire loop)

Status: Accepted (partially superseded for turn/movement loop)
Date: 2026-07-10
Note: Powered shields, weapon profiles, d20 combat, and SSD remain. The
per-step move/fire loop, initiative-from-allocation, and manual end-turn were
replaced by ADR-0025 (protocol v4 allocate → path → volley).

## Context

shipsim briefly targeted Bocchino/FASA STCS-style 3-round actions (ADR-0019) after leaving
SFB-style 32-impulse play. The maintainer's preferred design is captured in
`tmp/game_mechinics.md` and refined by grilling. It is a distinct model: turn-start power
allocation, power-ordered single-step movement phases alternating with simultaneous fire
phases, momentum costs, and per-facing powered shields.

## Decision

**Product combat rules** are the grilled v2 model (summary below). This **supersedes
ADR-0019** as the combat end-state. Hex board, generic ships (ADR-0003), pure core + thin
Love client (ADR-0001/0017), and soft-error harness (ADR-0018) remain.

### Turn loop

1. **Allocate** power once: movement budget, each weapon charge, each shield facing (1--6).
   Single pool; sum ≤ ship power; locked for the turn.
2. **Repeat** until no useful move/fire remains (or players end turn):
   - **Movement phase:** Order ships once per turn by **allocated movement** (high first;
     ties coin-flip once). Each ship with remaining move power may take **one** step or pass.
   - **Firing phase:** Commit any number of legal powered weapons; then resolve
     **simultaneously** from pre-fire state.
3. **Early end turn** allowed; **warn** if the player still has move/weapon power that could
   act. No warn if no legal actions remain for anyone.

### Movement

- Momentum: start **stopped**. Continue forward (and forward±60° if implemented) cost **1**.
  Reverse keel cost **2**. Turn in place cost **1** (no keel flip).
- One movement power spent per step (except reverse needs 2 available).

### Fire

- Commit target ship + **shield facing** (geometry-legal; often two choices; PB often one).
- Arcs **90°**; mounts: F, FP, FS, A, AP, AS.
- Weapons once per turn after resolve.
- **Torp:** charge 1, damage 4 flat, max range 12.
- **Plasma:** charge 1, high close (8 at r1), steep falloff, max 14.
- **Beam:** charge 1..max, all-or-nothing; damage = charge × factor(range) with factor 2 at
  PB down to ~1 at max range 10; **half-up** rounding; rounded damage &lt; 1 ⇒ illegal for that
  charge.
- To-hit: **d20** vs range thresholds (tables in CONTEXT / plan).

### Shields / hull

- Per-facing powered pools; unpowered = 0 absorb; deplete within turn; next turn repower to
  ship max. Overflow to hull; hull 0 = destroyed.

### Scale / ships

- Maps ~20--28 hexes; tight-to-medium fights after range double.
- MVP power: escort **14**, cruiser **22**.

### Win

- Destruction (hull 0). Objectives/disengage later.

## Consequences

- Requires a new implementation plan (not a small patch on FASA 3-round MVP).
- Love UI: allocation screen; move-order display; move phase / fire commit phase; shield
  facing pick on fire.
- Prior impulse and Bocchino loops are historical; remove or quarantine when v2 lands.

## Related

- `docs/PRD.md` (current product spec)
- `docs/ARCHITECTURE.md` (current system structure)
- `tmp/game_mechinics.md` (seed notes)
- Supersedes: ADR-0019 combat end-state (infrastructure decisions may still apply)
