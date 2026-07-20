# ADR-0025: Simplified Simultaneous Turn Mechanics

Date: 2026-07-18
Status: Accepted
Supersedes: ADR-0022 (persistent velocity / four-cycle inertial movement)
Supersedes (partial): movement/fire portions of ADR-0020 and ADR-0023 that
assumed velocity, course, four cycles, `ready_fire`, or manual `end_turn`.

Related plan: implementation was driven from a local simplified-turn plan
(ephemeral; not checked in). Authoritative product rules now live in this ADR
plus `docs/PROTOCOL.md` and `docs/PLAY-V2.md`. Protocol target: **v4**
(breaking; no dual v3/v4 engine).

## Context

Protocol v3 used persistent velocity/course, four movement phases per turn,
per-phase maneuver commits, repeated fire windows, `ready_fire`, and manual
`end_turn`. That model was bookkeeping-heavy for players and clients. The
product goal is three consequential decisions per ship per turn without
impulse bookkeeping.

## Decision

Replace the four-cycle inertial turn with three simultaneous **collection
stages**:

1. **Allocate** — stage one power allocation per living ship; apply together.
2. **Movement** — stage one complete path per living ship; resolve all paths.
3. **Firing** — stage one complete volley per living ship; resolve all
   volleys, then automatically begin the next turn's allocation.

Domain terms (frozen):

| Term | Meaning |
|---|---|
| collection stage | Barrier that waits for every living ship to commit before resolving |
| motion power | Reactor power spent on movement during allocation |
| motion point | Unit of path budget after power→motion conversion and hull cap |
| path action | One of five atomic actions costing exactly one motion point |
| stationary | Ship whose path contains no `move_*` action (may still turn) |
| endpoint claim | Final hex a mover requests after tracing its path |
| fallback | Reverse walk of translated positions when a claim loses |
| volley | Zero or more shots submitted once per ship in the firing stage |

### Turn structure

- Internal stage names remain `allocate`, `movement`, and `firing`.
- Every living ship submits exactly one order in each stage.
- Orders remain hidden until every living ship has committed that stage.
- Snapshots may expose committed/pending ship IDs, never unresolved opponent
  allocation, path, or volley payloads.
- After all volleys resolve: increment turn, clear turn ephemera, reset
  shields and motion, enter allocation automatically.
- No `turn_end` stage. No manual `end_turn`.
- If a terminal condition ends the scenario after movement or firing, do not
  enter another collection stage.

### Allocation

- One reactor pool among motion power, weapon charge increases, and six
  shield facings.
- Weapon charge carries between turns; allocation cannot strip carried charge.
- Shields do not carry; motion does not carry.
- Allocation is staged until all living ships commit, then applied together.
- Hull-specific power-to-motion conversion is retained (former thrust
  conversion ratios).

### Motion pool and hull limit

- Catalog field `max_maneuver_actions` is the per-turn maneuver cap (not
  momentum). It replaces `max_velocity` and the legacy dual `speed` /
  `max_velocity` catalog model.
- Engine SSD damage reduces the effective per-turn maneuver cap.
- Usable motion pool = min(converted allocated motion, effective maneuver
  cap).
- All five path actions cost exactly one motion point.
- Unspent motion is wasted after movement resolution and discarded at turn
  rollover.

### Path language

| Action | Position | Facing | Cost |
|---|---|---|---:|
| `move_f` | one hex through current F | unchanged | 1 |
| `move_fr` | one hex through current FR | rotate one face right | 1 |
| `move_fl` | one hex through current FL | rotate one face left | 1 |
| `turn_right` | unchanged | rotate one face right | 1 |
| `turn_left` | unchanged | rotate one face left | 1 |

- Cannot translate through R/RR/RL without turning first (or successive
  F/FR/FL moves).
- Facing persists between turns. Position + facing are the complete
  persistent movement state. Velocity and course do not exist.
- Hard-map scenarios reject paths with an out-of-bounds translated hex.
  Unbounded/floating permit any coordinate.

### Simultaneous path resolution

- Validate and trace each path independently from the same pre-movement
  snapshot.
- Intermediate crossings, swaps, and shared intermediate hexes are legal.
- Only final occupancy must be unique.
- Stationary ships always keep their starting hex and cannot be displaced.
- Among movers claiming the same unreserved final hex, greatest total
  submitted path cost wins. Equal cost → seeded fair random (canonical sort
  by destination then ship ID before draws).
- Losers walk backward through translated position history (skipping
  duplicates) until an unreserved hex is found; contested fallbacks use the
  same cost-then-seeded-tie rule.
- If a mover exhausts its fallback chain at its unique starting hex while
  another mover occupies it, the exhausted mover retains that start and the
  other mover continues backward. This terminal rule avoids a seeded result
  that cannot produce unique final occupancy.
- Fallback changes position only; facing always comes from the full action
  list.
- Resolution must terminate with one unique final hex per living ship.
- Commit insertion order must not affect positions, facings, telemetry, or
  PRNG state.

### Simultaneous fire

- One `commit_volley` order per ship (empty volley = hold fire).
- Each weapon at most once per volley.
- Validate against post-movement / pre-fire snapshot.
- Resolve all valid shots in canonical `(attacker_id, weapon_id, target_id)`
  order.
- A ship alive when firing begins executes its entire accepted volley even if
  destroyed mid-resolution; shots against targets alive at start still
  execute (overkill recorded).
- Geometry is frozen at fire resolution start.
- Charge is consumed only for weapons in the volley.
- Shields absorb the whole simultaneous volley and remain until turn rollover.

### Protocol v4 orders

- Keep `allocate` shape; stage until all commit.
- Add `commit_path` with ordered `actions`.
- Add `commit_volley` with `shots[]`.
- Reject: `commit_maneuver`, legacy `move`/`pass_move`, `commit_fire`,
  `ready_fire`, `end_turn`.
- Queries: `path_preview`, `reach_preview`; keep `fire_preview`; remove
  `maneuver_options` and four-cycle `movement_preview`.
- Saves: protocol 4 only; reject v3 as `UnsupportedVersion` before parsing
  retired shapes.
- Scenarios: no placement `velocity`/`course`. Catalog: `max_maneuver_actions`
  only (no silent aliases).

## Consequences

- Clients submit three stage orders per ship per turn and never reimplement
  path legality (engine previews are authoritative).
- Balance reports produced under v3 are historical and not comparable to v4
  baselines; retuning is a separate campaign.
- Simulation policies emit complete paths and volleys, not per-impulse
  maneuvers.
- Frontends (REPL, Ratatui, Love2D) and tutorial must be rewritten for path
  drafting and volley submission.

## Non-goals

- Combat table / hit / damage / shield / SSD rebalance.
- Waypoints or autonomous movement.
- Network multiplayer secrecy (staging is engine non-disclosure only).
- Backward compatibility with protocol-v3 clients or saves.
