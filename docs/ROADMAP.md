# shipsim — Roadmap & Deferred Backlog

Durable tracker for everything intentionally cut from earlier slices, so we get back to it.
Each item notes **why deferred**, the **slice-1 hook** that keeps it non-disruptive (per ADR-0002),
and a **resume cue**. Ordering is indicative, not committed.

Legend: 🎯 target slice · 🪝 designed-for hook already in slice 1 · ▶ resume cue

---

## Slice 1 — Movement Skeleton (CURRENT)

Simplified N-hexes/turn sequential movement, 6-facing, player-plotted + scripted ship, objective-hex
win, CLI/JSON harness, headless test. See `CONTEXT.md`.

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

## Deferred — Combat (the next big system)

### D5. Direct-fire combat: phasers & disruptors  🎯 slice 3
- Range-based to-hit, 6-facing shields, damage allocation, ship destruction, combat win condition.
- ▶ Add `combat` module; new scenarios with hostile ships; extend snapshot with shield/damage state.

### D6. Ship systems / damage model (SSD-style)  🎯 with D5
- Internal systems, damage tracking, crippling.

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

*Maintenance: when a deferred item is picked up, move it under its realized slice with the commit/PR
that landed it, and check the 🪝 hook actually held (if a rewrite was needed, note why for next time).*
