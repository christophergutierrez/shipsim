# Implementation Plan: Combat Model v2 (Momentum / Power / Move–Fire Loop)

**Audience:** A fresh agent or human with no prior session context.  
**Do not re-grill.** Specs are locked in ADR-0020 + PRD + this plan.  
**Do not implement Bocchino 3-round FASA or SFB impulse as the product path.**

---

## Planning Verdict

| Field | Value |
|-------|--------|
| **verdict** | READY |
| **task_tier** | full |
| **tier_trigger** | New combat ruleset; breaking order/snapshot contract; core + tests + Love rewrite |
| **execution_policy** | cost_optimized |
| **model_routing** | current-model-only |
| **autonomy** | checkpoint (user default) unless told Autopilot |
| **spec** | `docs/adr/0020-combat-model-v2-momentum-power.md` |
| **prd** | `docs/PRD-combat-model-v2.md` |
| **grill** | `CONTEXT.md` section “Combat model grill” |
| **seed notes** | `tmp/game_mechinics.md` |

---

## Fresh session bootstrap (do this first)

```bash
cd /mnt/storage/git_home/shipsim   # or repo root
git status
git log --oneline -5
cargo test 2>&1 | tail -30
```

**Read in order (do not skip):**

1. This file (entirety)
2. `docs/adr/0020-combat-model-v2-momentum-power.md`
3. `docs/PRD-combat-model-v2.md` (at least Problem/Solution/Out of Scope)
4. `CONTEXT.md` — combat grill decisions only
5. Current code reality: `src/movement.rs`, `src/game_state.rs`, `src/combat.rs`, `src/snapshot.rs`, `frontend/love/main.lua`

**Current tree reality (as of plan authorship):**

- Product code still implements a **short FASA-style** loop (`Move` / `Fire` / `EndAction`, 3 rounds, immediate fire) after an earlier impulse deletion.
- Love client speaks those FASA orders.
- Tests: `tests/fasa.rs`, `tests/acceptance.rs`, `tests/harness.rs` green for FASA MVP.
- **That is legacy relative to ADR-0020.** Replace it; do not extend it.

**Re-check staleness:** If `movement.rs` orders differ from this plan’s Order enum, trust **ADR-0020 + this plan**, not the old FASA orders.

---

## Goal

Ship combat where a player can:

1. Allocate power (move / weapons / shield facings).
2. Move one hex (momentum costs).
3. In the next fire phase, commit shots that are legal **after** that move.
4. See simultaneous resolution, shield depletion, hit/miss.

**Headline acceptance (must never regress):**

> Ship A is out of effective range (or beam charge would deal 0). After one or more movement phases close the gap, A commits fire in a firing phase and can hit. No 32-impulse clock; no “declare fire before any movement of the turn.”

---

## Frozen rules summary (implement exactly)

### Turn state machine

```text
TurnStart
  -> Phase::Allocate   (each living ship must Allocate once, or AI auto-allocates)
  -> build_move_order()  // sort by movement_allocated desc; ties PRNG once; freeze
  -> Phase::Movement
       each ship in move_order with move_remaining>0 OR all may pass:
         Move | Pass  (exactly one decision per ship per movement phase)
       when all ships decided -> Phase::Firing
  -> Phase::Firing
       each ship may CommitFire (0..N weapons) or PassFire
       when all ships finished committing -> resolve_simultaneous()
       then:
         if any ship can still move OR can still fire legally -> Phase::Movement
         else -> TurnEnd
  -> TurnEnd
       clear turn ephemera; reset for next turn; check win
```

**Early EndTurn:** allowed anytime after Allocate. If any ship still has (move_remaining>0) OR (uncharged? no — charged weapon not yet fired and at least one legal shot exists for some ship), surface warning to UI; core may still accept EndTurn. If no legal actions exist for anyone, EndTurn without warning flag.

### Power allocation (locked after submit)

Per ship, per turn:

| Bucket | Meaning |
|--------|---------|
| `movement` | Integer steps budget (each step costs 1, reverse costs 2 from remaining) |
| `weapons[weapon_id]` | Charge on that weapon (beam 1..max; plasma 0 or 1; torp 0 or 1) |
| `shields[0..5]` | Power on facings 0..5 (UI may label 1..6) |

Constraint: `movement + sum(weapon charges) + sum(shields) <= ship.power`  
(`ship.power` design total: escort **14**, cruiser **22**.)

### Momentum

Keel state: `Stopped | Forward | Reverse` (relative to current facing).

| Action | Cost | Keel after |
|--------|------|------------|
| Forward (into facing hex) | 1 if keel was Stopped or Forward; **2** if was Reverse (reverse-to-forward) | Forward |
| Reverse (into opposite hex) | 1 if Stopped or Reverse; **2** if was Forward | Reverse |
| Turn port / starboard in place | 1 | unchanged keel |
| Pass | 0 | unchanged |

Start of turn: **Stopped**.  
Insufficient remaining move power for cost → illegal.

**MVP move set:** Forward, Reverse, TurnPort, TurnStarboard. (Sideslip deferred.)

### Movement phase

- Order: frozen `move_order` for the turn.
- Each living ship, in order, must **Move** or **Pass** once.
- Ships with `move_remaining == 0` auto-Pass (or only Pass allowed).
- After all decided → Firing phase.

### Firing phase

- Each living ship, in any order (or same move_order): submit zero or more `CommitFire`, then `PassFire` / `ReadyFire` to mark done.
- **Simpler MVP protocol:** ships submit all commits then one `ReadyFire { ship }`; when all living ships Ready → resolve.
- Commit fields: `ship`, `weapon_id`, `target`, `shield_facing` (0..5).
- Legality at commit time (and re-check at resolve from pre-fire snapshot):
  - weapon has charge > 0 and not yet fired this turn
  - target exists, not self, not destroyed
  - range ≤ weapon max and (beam) rounded damage ≥ 1
  - target in arc for weapon mount
  - shield_facing is geometry-legal for attacker→target
- Resolve: freeze ship positions/shields/hull; for each commit in deterministic order (attacker id, weapon id): d20 to-hit; on hit apply damage to facing then hull; mark weapon fired; clear charge.
- Miss: log `kind=miss`, damage 0, weapon still consumed (fired this turn), charge spent.

### Combat tables (MVP constants — put in `src/combat_tables.rs` or `data/`)

#### Max range

| Kind | Max range |
|------|-----------|
| Beam | 10 |
| Plasma | 14 |
| Torp | 12 |

#### To-hit: d20 ≤ threshold (inclusive)

| Range | Beam | Plasma | Torp |
|------:|-----:|-------:|-----:|
| 1 | 18 | 16 | 14 |
| 2 | 17 | 14 | 13 |
| 3 | 15 | 12 | 12 |
| 4 | 13 | 10 | 11 |
| 5 | 11 | 8 | 10 |
| 6 | 10 | 6 | 9 |
| 7 | 8 | 5 | 7 |
| 8 | 7 | 4 | 6 |
| 9 | 5 | 3 | 5 |
| 10 | 4 | 2 | 4 |
| 11 | — | 2 | 3 |
| 12 | — | 2 | 3 |
| 13 | — | 1 | — |
| 14 | — | 1 | — |

#### Damage

| Kind | Cost | Damage |
|------|------|--------|
| Torp | 1 | **4** flat if in range |
| Plasma | 1 | by range: `[8,6,5,4,3,3,2,2,1,1,1,1,1,1]` for r1..14 |
| Beam | 1..max_charge | `half_up(charge * beam_factor(range))`; illegal if result &lt; 1 |

**Beam factor(range)** for r=1..10:

```text
2.0, 1.9, 1.7, 1.6, 1.4, 1.3, 1.2, 1.1, 1.0, 1.0
```

**half_up(x):** standard half away from zero / `floor(x + 0.5)` for positive x.

#### Arc mounts → hex facings (implementer default — lock in code + test)

Ship facing `F` ∈ 0..5. Relative bearing `rel` = (absolute_bearing - F) mod 6.

| Mount | `rel` values that hit (hex sides) |
|-------|-------------------------------------|
| forward (F) | 0 |
| forward_starboard (FS) | 0, 1 |
| aft_starboard (AS) | 2, 3 |
| aft (A) | 3 |
| aft_port (AP) | 3, 4 |
| forward_port (FP) | 5, 0 |

This locks the maintainer clarification: each named hex face is the primary
arc, and forward-port / forward-starboard can also hit directly forward but do
not reach past their neighboring face. Document the chosen table in `docs/` or
module rustdoc; unit-test exhaustively.

#### Geometry-legal shield facings for a shot

Use existing “which shield is hit” geometry: primary facing from `relative_bearing(target.facing, target.pos, attacker.pos)`.  
If the center-to-center line is near a **corner**, allow **two** adjacent facings; commit must pick one of the legal set.  
If only one, force that facing (reject others).

### Win

- `structure`/hull 0 → destroyed.  
- Scenario `terminal.type = destruction` + `target` id, or all enemies destroyed.  
- Snapshot `status: Won` when terminal met.

---

## Proposed wire protocol (NDJSON orders)

Replace current FASA orders entirely. Final serde names:

```rust
// Pseudocode — implement in movement.rs or orders.rs
enum Order {
  Allocate {
    ship: u32,
    movement: u32,
    /// map weapon_id -> charge
    weapons: BTreeMap<String, u32>,
    /// [6] facing powers
    shields: [u32; 6],
  },
  Move {
    ship: u32,
    mode: MoveMode, // forward | reverse | turn_port | turn_starboard
  },
  PassMove { ship: u32 },
  CommitFire {
    ship: u32,
    weapon: String,
    target: u32,
    shield_facing: u8, // 0..5
  },
  /// Clears this ship's commits this fire phase (optional)
  ClearCommits { ship: u32 },
  ReadyFire { ship: u32 },
  /// Resolve if all ready; or core auto-resolves when last ReadyFire arrives
  // (prefer auto-resolve on last ready — no separate order required)
  EndTurn,
}
```

**Harness:** unchanged pattern — post-load snapshot; one snapshot or error line per order (ADR-0018 soft reject).

### Snapshot fields (minimum)

```json
{
  "turn": 1,
  "phase": "allocate|movement|firing|turn_end",
  "move_order": [2, 1],
  "ships_moved_this_phase": [2],
  "ships_ready_fire": [],
  "status": "InProgress",
  "seed": 1,
  "prng_state": 1,
  "map": { "width": 24, "height": 20, "mode": "hard" },
  "ships": [{
    "id": 1,
    "class": "Heavy Cruiser",
    "controller": "player",
    "q": 0, "r": 0, "facing": 0,
    "power": 22,
    "movement_allocated": 6,
    "move_remaining": 5,
    "keel": "forward",
    "shields_powered": [4,4,2,0,0,2],
    "shields_remaining": [4,4,1,0,0,2],
    "weapons": [{
      "id": "beam_1", "kind": "Beam", "mount": "forward",
      "max_range": 10, "charge": 3, "fired": false, "max_charge": 4
    }],
    "structure": 12,
    "destroyed": false
  }],
  "fire_commits": [],
  "combat_log": [],
  "end_turn_warning": false
}
```

Names may be adjusted but **semantics must match**. Prefer serde `snake_case`.

---

## Architecture / module plan

| Module | Role in v2 |
|--------|------------|
| `src/hex.rs` | Keep; facing, distance, neighbors |
| `src/board.rs` | Keep; hard/floating |
| `src/prng.rs` | Keep; d20 = roll 20 or `1 + roll(20)` — **define once** and test (prefer `prng.roll(20)` meaning 1..=20) |
| `src/ship.rs` | Redesign fields for v2 allocation/momentum/weapons |
| `src/schema.rs` / TOML | power 14/22; weapon kind, mount, max_charge; max_shield_per_facing |
| `src/combat_tables.rs` | **New** — pure tables + half_up + projected_beam_damage |
| `src/combat.rs` | Legality, to-hit, damage apply; remove IFF as authority |
| `src/arc.rs` | **New** — mount ↔ relative facings; legal shield facings |
| `src/momentum.rs` | **New** — cost(mode, keel) → (cost, new_keel) |
| `src/orders.rs` or `movement.rs` | v2 Order enum + apply_order |
| `src/game_state.rs` | Phase machine, commits, resolve |
| `src/snapshot.rs` | v2 fields |
| `src/ai.rs` | Greedy allocate / step / commit |
| `src/turn.rs` | Thin or delete; logic in game_state |
| `src/impulse.rs` | Unused; delete in cleanup milestone or allow dead_code until then |
| `frontend/love/*` | Allocation UI; move/fire phases; facing picker |
| `tests/v2_*.rs` | All new gates |
| `tests/fasa.rs` | Delete or rewrite empty after cutover |

**Dependency rule:** Love never reimplements damage/to-hit; may call pure Lua copies of **display-only** projection only if duplicated from documented formulas — prefer server-side projection field later. MVP: client computes beam projected damage from same factors hardcoded once + unit test parity, or only enable Fire when core would accept (try order).

---

## Milestones (testable)

### M0 — Docs freeze + baseline

**Scope:** Point ROADMAP/CONTEXT at this plan; ensure ADR-0020/PRD linked; copy working tables into `docs/combat-v2-tables.md` (optional but recommended for agents).

**Gates:**

1. `implementation-plan-combat-v2.md` present and referenced from ROADMAP.
2. `cargo test` still green on **pre-cutover** tree (record baseline).

**No production behavior change required** if docs-only.

---

### M1 — Pure rules modules (no full game loop)

**Scope:** Add `combat_tables.rs`, `momentum.rs`, `arc.rs` with pure functions + unit tests in-module or `tests/v2_tables.rs`.

**Gates (each must fail if tables wrong):**

1. `half_up(0.4)==0`, `half_up(0.5)==1`, `half_up(0.8)==1`, `half_up(2.3)==2`.
2. Beam projected: charge 1 at factor 0.4 → illegal; charge 2 → 1 damage.
3. Momentum: Forward from Forward costs 1; Forward from Reverse costs 2; TurnPort costs 1 keel unchanged.
4. Torp damage 4 at r1 and r12; plasma 8 at r1, &lt;4 at r10.
5. d20 thresholds: beam r1 threshold 18.

**Files:** new modules only; wire into `lib.rs`.

---

### M2 — Data model + Allocate order

**Scope:**

- Ship/schema: `power` 14/22; weapons with `kind` beam|plasma|torp, `mount`, `max_charge`; `max_shield_facing` default 6 or 8.
- GameState: phase Allocate; `Allocate` order validates sum; stores allocation; builds move_order.
- Snapshot exposes phase, allocation, move_order.

**Gates:**

1. Over-allocate → soft error / OrderError, state unchanged.
2. Valid allocate → phase becomes Movement (or stays Allocate until all ships allocated — **prefer: each ship allocates; when all living ships allocated → Movement**).
3. Move order: higher movement allocation earlier; equal → stable after PRNG tie (test with fixed seed).

**Tracer partial:** two ships allocate different movement → order [high, low].

---

### M3 — Movement phase + momentum (**tracer half**)

**Scope:** `Move` / `PassMove`; enforce one decision per ship per phase; spend move_remaining; update pos/facing/keel; advance to Firing when all decided.

**Gates:**

1. Active sequencing: ship cannot move twice in one movement phase.
2. Reverse after forward with 1 remaining → illegal; with 2 → legal, remaining -= 2.
3. Pass with 0 remaining auto or explicit.
4. After all passed/moved → `phase == Firing`.
5. Off-map / occupied → error, no spend.

---

### M4 — Firing phase + simultaneous resolve (**headline tracer complete**)

**Scope:** CommitFire, ReadyFire, simultaneous resolve, combat_log hit/miss, weapon once, shield deplete, hull destroy.

**Headline gate:**

```text
GIVEN ship1 out of beam effective range at charge 1
WHEN allocate, move phases close to range 1, firing phase commit beam
THEN hit or miss per PRNG but commit legal; shield or hull changes on hit
AND a fire attempt before closing is illegal
```

**Additional gates:**

1. Two ships commit; both damages applied from pre-fire shields (document mutual strike).
2. Miss still marks weapon fired and clears charge.
3. Unpowered facing absorbs 0; damage goes to hull.
4. Powered facing 3 hit for 5 → facing 0, hull takes 2.
5. Multi-weapon commit same phase OK if both legal.

---

### M5 — Turn loop, EndTurn, multi-cycle, win

**Scope:** Multiple move/fire cycles per turn; EndTurn + `end_turn_warning`; turn advance; power reset; destruction win; clear commits between fire phases.

**Gates:**

1. After resolve, if move_remaining&gt;0, return to Movement (new phase instance).
2. EndTurn mid-turn sets warning true if legal actions remain; false if none.
3. Destroy target → status Won.
4. New turn: allocations cleared; keel Stopped; shields need re-allocate; weapons unfired.

---

### M6 — AI + multi-ship scenarios

**Scope:** Greedy AI allocate (move toward, arm weapons, power front shields); move step; commit fire when legal. Scenario `scenarios/v2_duel.toml` on ~24×20 map.

**Gates:**

1. AI-only or player+AI completes 3 turns without panic (`cargo test`).
2. Fleet 2v2: all four appear in move_order; each gets move decisions.

---

### M7 — Delete FASA/legacy orders + rewrite tests

**Scope:** Remove FASA `EndAction`/3-round fields as product API; delete/empty `tests/fasa.rs` content; harness fixtures for v2; `rg` for old order types in src.

**Gates:**

1. `cargo test` full green.
2. `cargo clippy --all-targets -- -D warnings` clean.
3. No `Order::EndAction` / three-round FASA as primary path in `src/`.

---

### M8 — Love client v2

**Scope:**

- Screen/phase: Allocate → Move → Fire commit → (auto resolve) → …
- Show move_order and whose move decision is next.
- Shield facing picker on commit.
- Projected beam damage for current charge.
- Hit/miss status strip; enemy status panel.
- End turn with warning dialog if `end_turn_warning`.
- Update `frontend/love/tests/run_all.lua` and `docs/PLAY-FASA.md` → replace with `docs/PLAY-V2.md`.

**Gates:**

1. `luajit frontend/love/tests/run_all.lua` green (harness integration allocate+move).
2. Manual smoke checklist in PLAY-V2.md (human optional).
3. Pure Lua tests: order JSON shape; no game rules beyond projection formula if duplicated.

---

### M9 — Acceptance package

**Scope:** `tests/acceptance.rs` multi-order script: allocate, two move cycles, fire, end turn; golden `tests/fixtures/v2/duel.jsonl`.

**Gates:**

1. Acceptance test green and deterministic with fixed seed.
2. Fixture byte-lock harness test.
3. CONTEXT/ROADMAP mark v2 REALIZED MVP.

---

## Milestone dependency graph

```text
M0 docs
 └─ M1 pure tables/momentum/arc
     └─ M2 Allocate + move_order
         └─ M3 Movement phase
             └─ M4 Fire + simultaneous  ★ headline
                 └─ M5 Turn loop / EndTurn / win
                     └─ M6 AI + scenarios
                         └─ M7 legacy purge
                             └─ M8 Love
                                 └─ M9 acceptance + fixtures
```

**Budget:** 9 milestones (within autopilot default 8? slightly over — if forced to 8, merge M0 into M1 and M9 into M8).

---

## Traceability (PRD outcomes → milestones)

| Outcome | Milestone |
|---------|-----------|
| Allocate tradeoffs | M2 |
| Move order by movement power | M2 |
| One step then fire phase | M3–M4 |
| Momentum reverse cost 2 | M1, M3 |
| Simultaneous multi-weapon fire | M4 |
| Facing choose + powered shields | M4 |
| Beam half-up / charge scaling | M1, M4 |
| Torp/plasma roles | M1, M4 |
| Early end warning | M5 |
| Destruction win | M5 |
| Love playable | M8 |
| Single ruleset | M7 |

---

## File contracts (production)

### `src/combat_tables.rs` (new)

- `half_up(f64) -> u32`
- `beam_factor(range: u32) -> f64`
- `beam_damage(charge, range) -> Option<u32>` // None if &lt;1 after round
- `plasma_damage(range) -> u32`
- `torp_damage() -> u32` // 4
- `to_hit_threshold(kind, range) -> Option<u8>` // None if OOR
- `max_range(kind) -> u32`

### `src/momentum.rs` (new)

- `pub enum Keel { Stopped, Forward, Reverse }`
- `pub enum MoveMode { Forward, Reverse, TurnPort, TurnStarboard }`
- `pub fn move_cost(keel, mode) -> Option<(u32, Keel)>` // None if impossible

### `src/arc.rs` (new)

- `pub enum Mount { Forward, ForwardPort, ForwardStarboard, Aft, AftPort, AftStarboard }`
- `pub fn in_arc(mount, attacker_facing, from, to) -> bool`
- `pub fn legal_shield_facings(attacker_pos, attacker_facing, target_pos, target_facing) -> Vec<u8>`

### `src/orders.rs` (new) or rewrite `movement.rs`

- `Order` enum as above
- `apply_order(game, order) -> Result<(), OrderError>`
- Soft errors map 1:1 to messages for harness

### `GameState` responsibilities

- phase, allocations, move_order, per-phase moved set, fire commits, ready set
- `can_any_legal_fire()`, `can_any_move()`, `end_turn_warning()`
- `resolve_fire_phase()`

---

## Invariants

1. Rules only in Rust core.
2. Deterministic given seed (to-hit + initiative ties).
3. Soft-reject illegal orders; no state change on error.
4. One weapon fire per turn max after successful resolve/miss.
5. Allocation immutable after all ships allocated until turn end.
6. Simultaneous fire uses pre-resolve snapshot for legality and damage base shields.
7. Generic ship names/stats only.

---

## Risks

| Risk | Mitigation |
|------|------------|
| Scope creep to full STCS book | Tables frozen in this plan |
| Fire phase multi-player protocol fiddly | ReadyFire + auto-resolve on last |
| Hex 90° arc ambiguity | Fixed table + tests in M1 |
| Love blocked on core | M1–M7 headless first |
| Leftover FASA confusion | M7 purge; PLAY-V2.md |

---

## Explicit non-goals

- Dual ruleset (v2 + FASA + impulse)
- Sideslip, objectives, cloak, sensors, crew skills
- Variable plasma charge
- Full SSD critical tables beyond hull/structure apply

---

## Implementation notes for agents

1. **TDD:** For each milestone, write failing tests first when possible.
2. **Do not** keep supporting `EndAction` / 3-round API after M7.
3. **PRNG:** Document whether `roll(n)` is `0..n-1` or `1..n`; all d20 thresholds assume **1..=20**. If existing `roll` is 0-based, use `1 + prng.roll(20)` and test distribution once.
4. **Shield index:** Prefer 0-based in JSON; UI may show 1–6 labels F,FR,RR,R,RL,FL consistent with facing 0 = forward.
5. **Commit early, commit often** only if user asks; otherwise leave working tree for review at checkpoint.
6. If blocked on ambiguity: re-read ADR-0020; if still blocked, stop with question (checkpoint) rather than inventing rules that contradict the PRD.

---

## Test commands (definition of done for a milestone)

```bash
cargo test
cargo clippy --all-targets -- -D warnings
# after M8:
luajit frontend/love/tests/run_all.lua
```

---

## Suggested first code milestone after plan approval

**Start at M1** (pure modules). Do not begin Love until M4 headline is green.

---

## Appendix A — Example turn (narrative)

1. Cruiser power 22: move 6, beam 3, torp 1, plasma 1, shields [3,3,2,1,1,1] sum 21.  
2. Move order: cruiser (6) before escort (3).  
3. Movement: cruiser forward; escort forward.  
4. Firing: cruiser commits beam@enemy facing 0 + torp; both ready; resolve — beam hits for half_up(3*2)=6 at r1, torp 4.  
5. Movement again… until move budgets empty and no weapons left to fire.  
6. End turn / auto end → next turn allocate again.

---

## Appendix B — Superseded plans

| Plan | Status |
|------|--------|
| `implementation-plan-fasa-stcs.md` | Historical; do not execute |
| `implementation-plan-d8.md` | Love shell useful; phases/orders obsolete |
| Impulse plans / ADRs 0002 path | Historical |

---

*End of plan. Next Killhouse step: IMPLEMENT_MILESTONE starting M0/M1.*
