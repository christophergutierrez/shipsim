# shipsim — Roadmap & Deferred Backlog

Durable tracker for everything intentionally cut from earlier slices, so we get back to it.
Each item notes **why deferred**, the **slice-1 hook** that keeps it non-disruptive (per ADR-0002),
and a **resume cue**. Ordering is indicative, not committed.

Legend: 🎯 target slice · 🪝 designed-for hook already in slice 1 · ▶ resume cue

---

## Slice 1 — Movement Skeleton (REALIZED)

Simplified N-hexes/turn sequential movement, 6-facing, player-plotted + scripted ship, objective-hex
win, CLI/JSON harness, headless test. See `CONTEXT.md`. Superseded for movement by Slice 3.

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

## Slice 3 - Movement fidelity (REALIZED)

32-impulse IMC, `Plot`/`RunTurn`, simultaneous per-impulse resolution, turn-mode at plot submit.
Fire deferred to turn end (mechanics unchanged). See `docs/CONTEXT-slice3.md`, ADR-0007, ADR-0008,
`implementation-plan-slice3.md`.

### D1. Full 32-impulse turn + Impulse Movement Chart  [REALIZED - slice 3]
- `src/impulse.rs`: `moves_on_impulse` / `move_count` pure IMC schedule (speeds 0..=31).
- `Order::RunTurn` drives impulses 1..=32 atomically.

### D2. Simultaneous plot-then-resolve movement  [REALIZED - slice 3]
- `Order::Plot { ship, path }` stores a path; `RunTurn` collects per-impulse intents and applies
  them atomically. Same-hex collision: both movers stop and clear remaining plot.

### D3. Turn-mode enforcement  [REALIZED - slice 3]
- Plot submission validates turn-mode (first step free; need `turn_mode` straight steps before a
  direction change). Whole plot rejected on violation.

## Deferred — Movement / map residual

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

## Fleet-readiness hygiene (REALIZED pre-D10)

Landed before fleets / multi-firer play (see multi-ship readiness commit):

- **T1** scripted plans use `BTreeMap` (deterministic id order).
- **T2** scenario load rejects overlapping ship placements.
- **T3** `ShipDef.id` optional/default; catalog key only.
- **TS2** `Order::Fire { ship, weapon, target }`; fired set keys `(ship_id, weapon_id)`.
- **TS3** snapshot includes `prng_state`.
- **TS4** unknown weapon kind/arc are typed `LoadError`s.
- **AS1** `Terminal` enum (`ReachHex` / `DestroyShip`); conflicting objective+destruction rejected at load.
- **A2** shared `Hex::is_valid_facing`.

Still deferred: **TS1** n/a (dead index already gone), **T4** turn container notes, **AS2–AS4** combat logging/geometry consolidation, mid-game PRNG resume (serialize only for now).

## Older Slice 1 tribunal notes (superseded where listed above)

- **T4. `Turn` is a thin counter** (design note). Impulse policy lives in `turn` + IMC. ▶ Revisit if
  impulse-by-impulse stepping is exposed.

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

- **TS1–TS4.** REALIZED or obsolete under fleet-readiness hygiene (see section above).

---

## Deferred notes from the Slice 2 (D5 combat) Architecture Review (non-blocking)

Architecture verdict PASS; no Critical/High. Distinct from TS1-TS4.

- **AS1.** REALIZED — `Terminal` enum (see fleet-readiness).
- **AS2. Fire geometry recomputed** — still deferred until D2-fire.
- **AS3. `FireOutcome` discarded** — still deferred until combat logging.
- **AS4.** Largely obsolete after pure `resolve_fire` + encapsulation; clone of attacker remains for
  borrow splitting.

---

*Maintenance: when a deferred item is picked up, move it under its realized slice with the commit/PR
that landed it, and check the 🪝 hook actually held (if a rewrite was needed, note why for next time).*
