# Plan: Movement allocation cap, and map/allocate legibility

## Purpose

An external report ("allocate the move doesn't work") identified one real data-loss
bug and three UI legibility problems. This plan lands the bug fix already in
flight, extends it to the clients the report didn't examine, and sequences the
UI work by blast radius.

**Status: partially implemented.** The engine + TUI fix is written and working in
the tree, uncommitted. Phase 1 is "finish and land what exists", not "start".

---

## Verification of the report

Every load-bearing claim was checked against the code before planning. Recorded
because two claims were wrong in ways that change what should be built.

### Confirmed exactly

- `yard_heavy_cruiser` in `scenarios/battle.toml`: `power = 36`,
  `thrust_per_power = 1`, `power_per_thrust = 1`, `max_maneuver_actions = 6`.
  At most 6 power buys motion; the other 30 cannot.
- `input.rs::allocation_field_bounds`, movement branch (`cursor == 0`), returned
  `(0, power_available - power_cost)` — the entire pool. It consulted neither the
  maneuver cap nor the thrust ratio.
- **The asymmetry is real and is the sharpest part of the report.** The weapons
  branch immediately below clamps with `.min(max_charge)`; the shields branch
  clamps with `.min(ship.shield_cap(index))`. Movement was the *only* field
  offering an unreachable ceiling — so the panel taught a mental model with two
  fields and violated it with the third.
- The surplus is **destroyed, not refunded**: `game_state.rs:584` does
  `ship.motion_available = path::usable_motion(converted, cap)` where
  `usable_motion` is `converted.min(effective_cap)`, and the line above records
  `ship.movement_allocated = alloc.movement` — the full spend. The conversion
  remainder is bound to `_rem` and dropped.
- `map_metrics` uses `cell_width = 6 * (zoom + 1)` — a 6-column base cell.
- The move preview reuses the ship's own glyph vocabulary: the ghost renders
  `format!("{cs}{arrow}")`, the same callsign+arrow shape as a real ship.

### Wrong, or materially overstated

- **"differing only in a `Dim` modifier"** — the ghost is `Color::Cyan`, not a
  dim modifier. Substance survives (same glyph shape, distinguished only by
  color, so it degrades to identical on monochrome or for some color-vision
  deficiencies) but the mechanism is different. Separately: the on-screen legend
  literally reads `"Dim ghost = planned end"`, which does not describe what the
  code draws. That legend/code mismatch is its own small bug, cheap to fix.
- **"the allocate panel is unusable at 80×24 … zero weapons, zero shields"** —
  the panel **already scrolls**. `ui.rs::allocate_scroll` computes an offset that
  follows the cursor, so weapons and shield faces scroll into view as you
  navigate to them. The report appears to have rendered a static frame at
  cursor 0 and concluded the fields were unreachable. They are reachable. This
  downgrades the finding from "unusable" to "cannot see movement and shields at
  once", which is a real ergonomics complaint but a much smaller one, and it
  changes the fix from "redesign the panel" to "consider a denser layout."

### Missed by the report

- **The bug is cross-client.** The report examined only the TUI. Love2D's
  `allocation.available_for_movement` (`frontend/love/allocation.lua:59`) returns
  `power - (spent - movement)` — the remaining *power budget*, with no reference
  to `max_maneuver_actions` or the thrust ratio. Love has the same defect. (An
  earlier fix in this repo added a cap there, but capped to affordability, not to
  convertibility — the same mistake in a different file.) The REPL uses
  `max_maneuver_actions` only in display strings
  (`hexutil.py:36`, `commands.py:928`, `responsive.py:183`) and needs an audit to
  determine whether it clamps allocation at all.
- **The inverse conversion already existed.** `power_for_motion` in
  `src/simulation/policies/mod.rs:89` was already computing
  `ceil(motion × ppt / tpp)` — exactly the formula the report proposed writing.
  It was private and snapshot-typed. Implementing the fix as suggested would have
  created a third copy of a rule that has to agree across engine, AI, and UI.
- **`max_maneuver_actions` is not the enforced cap.** The engine enforces
  `effective_max_maneuver_actions()`, which is the design cap scaled by surviving
  engine boxes (`ssd::effective_max_speed`). The snapshot exposed only the raw
  design value and `engine`, never `engine_max`, so no client could derive the
  real cap. The report's "no protocol change is needed" is therefore true only
  for undamaged ships; clamping to the design cap would have left the identical
  silent-waste bug alive in every game where engines take damage.

---

## Phase 1 — Land the movement cap (mostly written)

### Already implemented in the tree (uncommitted, building, clippy-clean)

| File | Change |
|---|---|
| `src/thrust.rs` | `power_for_thrust(thrust, tpp, ppt)` free fn + `ThrustConversion::power_for_thrust` method — one definition of the inverse |
| `src/simulation/policies/mod.rs` | private `power_for_motion` now delegates to the shared helper instead of restating it |
| `src/snapshot.rs` | new `effective_max_maneuver_actions`, emitted from `ship.effective_max_maneuver_actions()` |
| `src/simulation/runner.rs`, `tests/simulation_metrics.rs` | fixture constructors updated |
| `frontend/tui/src/protocol.rs` | receives the field; adds `motion_cap()` (prefers effective, falls back to design) and `movement_power_cap()` |
| `frontend/tui/src/input.rs` | movement branch clamps to `residual.min(cap)`; `set_allocate_field` logs a refused raise instead of silently clamping |
| `frontend/tui/src/tests.rs` | `movement_allocation_is_capped_at_convertible_power` — drives the real engine subprocess, asserts the clamp and that the engine records no waste. **Passing.** |

Current gate state: core `cargo test` 16/16 binaries green; TUI 190 pass, **1
fail**; clippy clean on both crates.

### Remaining work

1. **Fix the one failing test.** `allocation_input_clamps_to_affordable_power`
   asserts `draft.movement == ship.power_available` (22). Its fixture is
   `power_available: 22, max_maneuver_actions: 8, 1:1 ratio`, so the correct
   ceiling is now 8 and it fails 8 ≠ 22. **This test encodes the bug as expected
   behavior.** Its actual intent — "input cannot exceed what you can afford" —
   remains valid; the bound is simply tighter now. Update it to assert the
   convertible ceiling and keep the affordability invariant
   (`power_cost <= power_available`). Do not delete it; renaming it to name both
   bounds would be clearer.
2. **Document the new snapshot field** in `docs/PROTOCOL.md`: additive, optional,
   old clients may ignore it; state plainly that clients must clamp movement
   against it and that surplus is truncated on resolve rather than refunded.
3. Consider whether `effective_max_maneuver_actions` should also appear in the
   REPL/Love snapshot docs, or whether Phase 2 covers it.

### Exit

| Check | Pass condition |
|---|---|
| `cargo test` (root) | green |
| `cargo test --manifest-path frontend/tui/Cargo.toml` | green, 191/191 |
| `cargo clippy --all-targets -- -D warnings`, both crates | 0 errors |
| Regression test | End-to-end test passes against the real engine binary |
| No third copy | `power_for_thrust` is the only definition of the inverse conversion |
| Docs | `docs/PROTOCOL.md` documents the new field and the clamp obligation |

### Commit

`fix(tui): cap movement allocation at convertible power`

---

## Phase 2 — Close the same bug in the other clients

### Goal

The defect is a client-side omission, and there are three clients. Fixing one
leaves two shipping the same silent data loss.

### Work

1. **Love2D**: `allocation.available_for_movement` caps to affordability only.
   Extend it to also cap at the power that buys `motion_cap`, reading the new
   snapshot field with a fallback to `max_maneuver_actions`. Love shells out to
   the engine for rules elsewhere and must not grow its own thrust math beyond
   this one clamp — mirror the TUI's `movement_power_cap` shape.
2. **REPL**: audit whether the movement allocation path clamps at all. Grep shows
   `max_maneuver_actions` used only for display strings, which suggests either no
   clamp or an affordability-only clamp. Fix to match, and extend
   `test_rules_parity.py` if any arithmetic lands in `hexutil.py`.
3. Add one test per client asserting the clamp, so this cannot regress in a
   client that nobody happened to be playing.

### Exit

| Check | Pass condition |
|---|---|
| Love | Movement bar cannot exceed convertible power; Love test suite green |
| REPL | Audit recorded; clamp present or its absence justified; unittest green |
| Parity | No client computes thrust conversion independently of `power_for_thrust` |

### Commit

`fix(love,repl): cap movement allocation at convertible power`

---

## Phase 3 — Preview vocabulary (cheap, self-contained)

### Goal

Origin, route, and destination read as one path in three distinct visual roles,
and the legend describes what is actually drawn.

### Work

1. Fix the legend/code mismatch: the legend says "Dim ghost", the code draws
   `Color::Cyan`. Make them agree — whichever way, they must not disagree.
2. Give the destination a distinct shape rather than a recolored duplicate of the
   ship: bracketed callsign (`[A1↗]`) or a hollow glyph. The requirement is that
   the player can tell ship from projection **without relying on color**.
3. Make the route continuous with the destination — currently the route is `◇`
   (`DarkGray`) and the ghost is a callsign, which share no vocabulary.

Deliberately excluded here: reachable-set shading and threat tinting. Both are
good ideas from the report, but both are materially more valuable at 1 hex/cell
than at 4, so they belong after Phase 4, not before it.

### Exit

| Check | Pass condition |
|---|---|
| Distinguishable | Ship and projected endpoint differ by glyph shape, not color alone |
| Legend accuracy | Legend text matches the rendered attributes |
| Continuity | Route and destination share a visual family |
| Rendering tests | Existing `render_to_string` map tests updated and green |

---

## Phase 4 — Map cell density (high blast radius — decide before starting)

### Goal

Fit enough hexes on screen that a 6-hex move and a 10-hex weapon range are
representable, which is a precondition for the preview work above being worth
much.

### The case for it

`cell_width = 6 * (zoom + 1)`. At 6 columns per hex a ~51-column map panel shows
8 hexes; `battle.toml` starts ships 15 apart, so the zoom heuristic falls back to
4 hexes/cell, at which a full-speed move shifts the icon by one or two cells.
The report's diagnosis — that the fix is the cell width, not the zoom
heuristic — is sound.

### Why this is not scheduled with the others

This touches every map render path and a large body of existing rendering tests
(`render_to_string` assertions, `pad_cell` widths, coordinate headers, the
off-map footer, the zoom heuristic itself). It is the highest-risk change in this
plan by a wide margin, and unlike Phase 1 it fixes an ergonomics problem rather
than data loss.

**Decision needed before work starts:** a 2-column cell drops the facing arrow
or the callsign or both at larger fleet counts. Ships are currently labeled
`A1`–`Zn`; two columns fits the callsign *or* `glyph + arrow`, not `callsign +
arrow`. Resolve what a cell shows at 2 columns — and what it shows when two
ships stack in one hex after squads — before touching the renderer.

### Exit (provisional)

| Check | Pass condition |
|---|---|
| Density | `battle.toml` renders at 1 hex/cell at 80×24 |
| Legibility | Ship identity and facing both recoverable at the new width |
| Squad stacking | A multi-ship hex renders unambiguously |
| Test suite | All map rendering tests updated, green |

---

## Phase 5 — Allocate panel density (severity corrected)

### Goal

See movement and shields without navigating between them.

### Scope correction

The report called the panel unusable at 80×24 with weapons and shields "not on
the screen at all." `allocate_scroll` already follows the cursor, so those fields
scroll into view. The genuine complaint is narrower: shields are re-bought from
zero every allocate, so they are the most-edited group, and you cannot see them
and the movement field simultaneously. That is a density problem, not a
reachability one — and it is the weakest-justified item in this plan.

### Work

Evaluate a two-column body (movement/hull left, weapons/shields right) at the
80×24 floor before committing to it. If it does not clearly beat the scrolling
single column, **do not change it** — the current layout is not broken, and a
redesign justified by a mis-read render is not worth the regression surface.

---

## Sequencing

```text
Phase 1 (finish movement cap — data loss)
  → Phase 2 (same bug, Love + REPL)
      → Phase 3 (preview vocabulary — cheap, independent)
          → Phase 4 (map density — gated on the 2-column decision)
              → Phase 5 (allocate density — only if it measurably wins)
```

Phases 1 and 2 are the only ones fixing silent data loss and should land
regardless of what happens to 3–5. Phase 3 is independently shippable. Phases 4
and 5 are ergonomics work that should be re-justified before starting; Phase 5
in particular may correctly end in "no change."

## Out of scope

- Reachable-set shading during allocate, and threat/arc tinting on the projected
  endpoint. Both are genuinely good and both are worth more after Phase 4; noted
  so they are not lost.
- Changing `usable_motion` truncation on the engine side. The engine is right to
  be permissive — clients are the layer responsible for not offering an illegal
  spend, and every other allocate field already behaves that way.
- Refunding surplus movement power at resolve. That would be a rules change, and
  it would paper over client bugs rather than surface them.
