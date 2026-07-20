# ADR-0022: Persistent Velocity and Inertial Movement

Date: 2026-07-01
Status: Superseded by ADR-0025
Supersedes: ADR-0020 movement portions (historical; later replaced by simplified turns)
Note: Four-cycle inertial velocity/course is **not** current product behavior.
Current turn loop: ADR-0025 / protocol v4.

## Context

The current movement model (ADR-0020) treats movement as a power-purchase
system: ships allocate reactor power to buy movement points, then spend those
points one hex at a time during a single movement phase. Initiative is derived
from allocated movement power. This has several limitations:

- No persistence of motion between turns — a ship stops instantly when it
  stops spending power.
- No inertia or coasting — ships cannot drift.
- Course and facing are conflated — a ship's heading determines both its
  movement direction and its weapon/shield geometry.
- Single-active-mover movement phase prevents simultaneous resolution.
- Initiative based on raw reactor expenditure creates gameable exploits.

The plan in `docs/MOVEMENT-INERTIA-PLAN.md` proposes replacing this with a
persistent velocity, inertial movement system with four movement phases per
turn and simultaneous maneuver resolution.

## Decision

We accept the following design decisions as frozen for the M1–M4
implementation scope:

### 1. Persistent velocity

Ships carry a velocity (0–4) that persists across turns. Velocity represents
hexes of translation per turn, distributed across four movement phases. A ship
with velocity 0 is stationary; a ship with velocity 3 translates in phases 1,
2, and 4. The full schedule is:

| Velocity | Translation phases |
|---:|---|
| 0 | none |
| 1 | 4 |
| 2 | 2, 4 |
| 3 | 1, 2, 4 |
| 4 | 1, 2, 3, 4 |

This schedule is not "the first V phases"; it is the explicit table above.

### 2. Independent facing and course

Facing (weapon/shield geometry) and course (translation direction) are
independent. A ship can rotate its facing without changing course, and change
course without rotating facing. Course is one of six hex directions (0–5).
Facing is also one of six directions. When velocity is zero, course retains
its last value for display.

### 3. Four movement phases per turn

Each turn has four movement phases. A ship translates in the phases listed in
the schedule table above (not simply the first V phases). After each movement
phase, a fire window occurs. The turn thus has four movement/fire windows, then
ends.

### 4. Simultaneous maneuver resolution

All ships commit maneuver decisions (accelerate, decelerate, change course,
rotate facing) before any are resolved. Maneuvers are resolved simultaneously,
then translations are applied simultaneously. Destination conflicts (two ships
entering the same hex) block all contenders. Hex swaps and path crossings are
permitted.

### 5. Thrust replaces movement points

Engine power is converted to thrust via a rational conversion
(`thrust_per_power` / `power_per_thrust`). Thrust is spent on maneuvers:
acceleration, deceleration, course changes, and facing rotations. Coasting
(translation without maneuver) consumes no thrust.

### 6. Turn-loop termination

The turn is bounded by the four-phase schedule. The turn ends after phase 4's
fire window completes. There is no `can_any_move` re-entry heuristic — the
old predicate that re-enters movement whenever any ship can move is deleted.

### 7. Legacy bridge

During M2–M3, the old `move_remaining` and `keel` fields are kept as derived
fields so the legacy movement path continues to compile. The bridge is deleted
in M4 when the simultaneous maneuver/translation core replaces it.

## Consequences

- **Breaking change:** The NDJSON protocol increments from v1 to v2. Old saves
  are rejected by version at `SaveDocument::read`. `Order::Move` and
  `Order::PassMove` remain as deserialization-only variants until M9.
- **Snapshot changes:** Snapshots gain velocity, course, thrust_remaining, and
  movement-phase index fields.
- **AI changes:** The AI movement layer is bridged with a coast stub during
  M3–M6, then replaced with real maneuver logic in M7.
- **Simulation changes:** Baseline policies learn to allocate engine power and
  choose maneuvers in M7.
- **Love2D changes:** The UI is migrated to maneuver controls in M8.
- **Legacy removal:** `Keel`, momentum move costs, old movement orders, and old
  snapshot fields are removed in M9.

## Open questions

- **Weapon refire across fire windows:** `fired_weapons_this_turn` currently
  persists across the whole turn. Under four fire windows, a weapon fires at
  most once per turn. This semantics is preserved; per-window refire would
  require a separate design change.
- **Love2D M6 boundary:** production Love2D controls emit only coast maneuver
  commitments. Directional maneuver controls are intentionally deferred to M8;
  the Rust `Order::Move` variant remains deserialization-only and is rejected at
  apply time for compatibility coverage.
