# shipsim — Roadmap & Deferred Backlog

Durable tracker for everything intentionally cut from earlier slices, so we get back to it.
Each item notes **why deferred**, the **slice-1 hook** that keeps it non-disruptive (per ADR-0002),
and a **resume cue**. Ordering is indicative, not committed.

Legend: 🎯 target slice · 🪝 designed-for hook already in slice 1 · ▶ resume cue

---

## Slice 1 — Movement Skeleton (CURRENT)

Simplified N-hexes/turn sequential movement, 6-facing, player-plotted + scripted ship, objective-hex
win, CLI/JSON harness, headless test. See `CONTEXT.md`.

## Slice 2 - Direct-fire Combat (REALIZED)

Direct-fire combat (D5) landed in Slice 2 (through commit 553a794, milestone M6). Combat/PRNG
modules, a `fire` order through the existing declare/resolve seam, 6-facing shields with
bleed-through to a single structure pool, seeded dice (ADR-0005), and the destruction-win terminal.

### D5. Direct-fire combat: phasers & disruptors  [REALIZED - slice 2]
- Range-based to-hit, 6-facing shields, bleed-through to a single structure pool, destruction win.
- Delivered: SFB-style dice via seeded PRNG (ADR-0005); data-driven weapon arcs; fire-freely (no
  arming); 6 shield facings; single internal-structure pool (ADR-0006). See docs/CONTEXT-slice2.md.
- Landed as: `combat` + `prng` modules and a `fire` order; `scenarios/combat.toml`; the snapshot
  gained shields/structure/destroyed/weapons and the seed.
- Hook check: the ADR-0002 declare/resolve seam held (fire reused it, no rewrite) and the slice-1
  objective-hex terminal was preserved (the destruction terminal was added alongside it).

## Deferred — Movement fidelity (the SFB "feel")

### D1. Full 32-impulse turn + Impulse Movement Chart  🎯 slice 2
- **Why deferred:** biggest complexity driver; slice 1 proves the loop with simplified movement.
- 🪝 Turn modeled as an impulse-capable container; simplified movement is a policy inside it.
- ▶ Replace the "N hexes/turn" policy with the Impulse Chart lookup (Speed → which impulses move).
  End-state requirement confirmed by maintainer.

### D2. Simultaneous plot-then-resolve movement  🎯 slice 2–3
- **Why deferred:** slice 1 resolves the two ships sequentially.
- 🪝 Movement is modeled as *declare order → resolve order*; collect both declarations per impulse to
  resolve together.
- ▶ Add a per-impulse declaration collection phase; resolve all ships' declared moves simultaneously.
  End-state requirement confirmed by maintainer.

### D3. Turn-mode enforcement  🎯 slice 2
- **Why deferred:** slice 1 allows free facing changes.
- 🪝 `ship.turn_mode` field exists in the schema, unenforced.
- ▶ Enforce "must move `turn_mode` hexes straight between facing changes"; validate in movement.

### D4. SFB fixed/floating map + off-map rules  🎯 later
- **Why deferred:** slice 1 uses a bounded rectangle from the scenario TOML, no wraparound.
- ▶ Revisit map model when it matters (fixed floating map, edge handling).

## Combat - deferred extensions (base direct-fire realized; see Slice 2 above)

### D1-fire. Impulse-gated fire windows  🎯 later
- Weapons fire only on specific impulses as defined by the 32-impulse IFF. This is the timing
  layer that makes SFB movement meaningful for combat. (Slice 3 uses fire-at-turn-end simplification.)
- ▶ Gate `Fire` declarations behind impulse-window checks in the `RunTurn` driver; each weapon
  class defines which impulses are fire windows.

### D2-fire. Simultaneous fire resolution  🎯 later
- Fire declared by both ships resolves simultaneously at turn end (not in declaration order).
  Slice 3 resolves fire sequentially in declaration order.
- ▶ Collect fire orders at plot/turn-end, resolve all fire simultaneously; deterministic tie-break
  by ship ID (ascending).

### D6. Itemized damage allocation / destroyable systems (SSD)  🎯 after D5
- Damage-allocation chart distributes hits across systems; weapons/engines can be knocked out;
  crippling. (Slice 2 uses a single structure pool instead.)
- ▶ Replace the single structure pool with an itemized SSD; add a damage-allocation table.

### D5a. More weapon classes / seeking weapons  🎯 after D5
- Seeking weapons (drones/plasma), reinforcement, richer arcs. (Slice 2 ships phasers + disruptors,
  direct-fire only.)

## Deferred — Energy Allocation (SFB signature)

### D7. Energy Allocation Form  🎯 slice 4
- Per-turn power budget across movement/weapons/shields; speed becomes energy-driven (slice 1 uses a
  fixed per-ship max speed).
- ▶ Add `energy` module; ship gains power stats; movement speed reads from the allocated budget.

## Deferred — Presentation & opponents

### D8. Graphical frontend selection  🎯 post-core (ADR-0004)
- Love2D vs. Godot vs. web. Core stays untouched (renders JSON over the bridge).
- ▶ Decide once the core plays through the CLI/JSON harness.

### D9. AI opponent  🎯 slice 3+
- **Why deferred:** slice 1 uses scripted waypoints.
- ▶ Add `ai` module (start with a NorRust-style greedy seek), replacing/augmenting scripted ships.

### D10. Scenarios, campaigns, multi-ship, fleets  🎯 later
- Beyond 1v1: multi-ship scenarios, campaign progression (NorRust has a model to borrow).

---

---

## Deferred findings from the Slice 1 Code Review Tribunal (non-blocking)

Recorded so they are not lost; none block Slice 1 (tribunal verdict PASS).

- **T1. Scripted-ship iteration order is HashMap-random** (`src/game_state.rs`, determinism, low).
  `scripted_plans: HashMap<u32, ScriptedPlan>` is iterated when advancing scripted ships; with >1
  scripted ship the advance/collision order is non-deterministic. Safe now (every scenario has exactly
  one scripted ship). ▶ Fix before any second scripted ship / multi-ship scenario (ties to D10): use
  `BTreeMap` or sort ids.
- **T2. Initial placements not checked for mutual occupancy** (`src/scenario.rs`, robustness, low).
  Two ships could be authored onto the same hex. ▶ Add a mutual-occupancy validation as scenario count
  grows.
- **T3. `ShipDef.id` is deserialized but never read** (`src/schema.rs`, maintainability, trivial).
  Runtime id comes from placement. ▶ Clean up on the next schema edit.
- **T4. `Turn` is a thin counter; per-turn policy lives in `GameState::end_turn`** (design note).
  Satisfies the impulse-container invariant and does not preclude D1, but the hook is minimal. ▶
  Revisit when implementing the D1 impulse chart.

---

## Deferred notes from the Slice 1 Architecture Review (non-blocking, Low/Medium)

Architecture verdict PASS; these are quality notes, none block Slice 1.

- **A1. Scripted-move legality duplicated** (`src/game_state.rs`, Medium). `advance_scripted_ships`
  re-implements adjacency/on-board/occupancy checks that already live in `movement::declare`. ▶ Route
  the scripted path through declare/resolve when D9 (AI) or D2 (simultaneous resolution) lands, so
  legality has one home. Safe now (single scripted ship; ties to T1).
- **A2. Facing `0..=5` invariant duplicated** (`src/scenario.rs`, `src/movement.rs`, Low). The rule is
  a magic `> 5` in two validators (each with its own appropriate typed error). ▶ Extract a shared
  predicate/const (NOT a `Facing` newtype - that would cross the JSON/wire contract).
- **A3. GameState collaboration helpers over-exposed** (`src/game_state.rs`, Low).
  `is_occupied_by_other`, `hexes_moved_this_turn`, `record_hex_moved`, `refresh_status` could be
  `pub(crate)`. Visibility tidiness only; no external caller impact.

---

## Deferred findings from the Slice 2 (D5 combat) Code Review Tribunal (non-blocking)

Tribunal verdict PASS; none block Slice 2. All contained by the 1v1 scope (assumption A5).

- **TS1. `fire_attacker_index` is dead code** (`src/game_state.rs`, Ponytail, trivial). Both `declare`
  and `resolve_fire` use `weapon_owner_index`. Delete next time the fire path is touched.
- **TS2. Weapon identity is a bare global `weapon_id`** (`src/game_state.rs`, correctness, latent).
  `weapon_owner_index` returns the first non-destroyed owner and `fired_weapons_this_turn` is keyed on
  id alone; both shipped ships define `"phaser_1"`. Safe only under 1v1 with a scripted (never-firing)
  enemy and player-listed-first ordering. ▶ Before a second firing ship / enemy fire AI / D10 fleets:
  `Order::Fire` must name the acting ship, and fired-weapon tracking must key on `(ship_id, weapon_id)`
  (this is what the "deterministic order by ship id" invariant O16/DD4 will require).
- **TS3. Snapshot serializes the seed but not the PRNG's current position** (`src/snapshot.rs`,
  completeness, low). Satisfies reproducibility (same seed+orders replays), but not mid-game resume.
  The `Prng::state()` accessor already exists as the hook. ▶ Serialize `prng.state()` when save/resume
  mid-game is added.
- **TS4. `parse_weapon` silently falls back on bad data** (`src/scenario.rs`, data-hygiene, low).
  Unknown weapon kind -> Phaser, unknown arc -> Forward, instead of a typed `LoadError`; the phaser
  `damage` field is inert when `phaser_dice_by_range` is populated. ▶ Make unknown kind/arc a typed
  load error; reconcile the redundant damage field.

---

## Deferred notes from the Slice 2 (D5 combat) Architecture Review (non-blocking)

Architecture verdict PASS; no Critical/High. Distinct from TS1-TS4.

- **AS1. Terminal modeled as two parallel Options** (`src/game_state.rs`, Medium, domain). `objective:
  Option<Hex>` and `destruction_target: Option<u32>` with an implicit objective-wins precedence in
  `refresh_status`. A single `Terminal` enum (ReachHex / DestroyShip) would collapse the branch and
  make the both-set state unrepresentable. ▶ Fold into a schema pass (Medium blast radius: touches
  GameState public fields + `new()` + snapshot access).
- **AS2. Fire geometry recomputed** (`src/movement.rs`, `src/combat.rs`, Low/Medium). range +
  relative_bearing computed in `movement::declare` and recomputed in `combat::resolve_fire`.
  Behavior-preserving; consolidating intersects the still-unfirmed D2 simultaneous-resolution seam, so
  premature. ▶ Consolidate when D2 lands.
- **AS3. `FireOutcome` discarded** (`src/movement.rs`, Low). `resolve_fire` returns it but `resolve`
  drops it. Deliberate hook for future combat logging, not a defect. ▶ Consume when combat logging /
  replay is added.
- **AS4. Combat indexes the ships Vec and clones the attacker** (`src/combat.rs`, Low). Uses public
  index accessors + a clone to sidestep the borrow checker; consistent with the crate's "GameState as
  public data bag" idiom. Acceptable at this size. ▶ Revisit if GameState encapsulation tightens.

---

*Maintenance: when a deferred item is picked up, move it under its realized slice with the commit/PR
that landed it, and check the 🪝 hook actually held (if a rewrite was needed, note why for next time).*
