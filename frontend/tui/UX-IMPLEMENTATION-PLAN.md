# TUI UX Implementation Plan

Date: 2026-07-14

## Purpose

This plan turns [UX-RECOMMENDATIONS.md](UX-RECOMMENDATIONS.md) into
incremental, testable milestones. It preserves the TUI's core boundary:
`shipsim_core` determines rules and legality; the TUI renders engine-provided
state, maintains local interaction state, and submits orders.

The plan is intentionally phased. Fleet correctness must land before a new
layout, and the new layout must be stable before map overlays and advanced
workflow features are added.

## Scope decision required before implementation

The current Small-tier PRD says that the first TUI release makes no engine or
protocol changes and defers Medium/Large layouts. The UX recommendations call
for fleet-scale behavior and engine-provided advisory data that the current
snapshot does not guarantee.

Before Phase 1 starts, approve an ADR/PRD amendment that permits:

1. A fleet-capable TUI roadmap, including Medium and Large layout tiers.
2. Additive protocol fields for UI advisory data where derivation would copy a
   game rule.
3. Engine and protocol tests for those new fields, reviewed separately from
   TUI rendering work.

Do not compensate for missing protocol data by recreating targeting, arcs,
shield-facing, or movement rules in `frontend/tui/`.

## Verification model

Every completed phase must pass all applicable checks below.

| Layer | Purpose | Required evidence |
|---|---|---|
| Engine/protocol | Contract data is correct and backward-safe | Engine tests plus NDJSON fixture/snapshot tests |
| App/input | Local state and emitted orders are correct | Synthetic `Snapshot` + `KeyEvent` tests |
| Render | The player can see the needed state | Ratatui `TestBackend` buffer assertions |
| Harness | Subprocess boundary remains healthy | One scripted real-engine smoke per changed phase |
| UI play | Controls are efficient in the actual terminal | Manual TUI run using the named scenario and terminal size |

Required commands for a TUI implementation phase:

```bash
cargo test -q
cargo test --manifest-path frontend/tui/Cargo.toml
cargo clippy --manifest-path frontend/tui/Cargo.toml -- -D warnings
```

The manual UI-play commands used by this plan are:

```bash
cargo run --manifest-path frontend/tui/Cargo.toml -- scenarios/fleet.toml
cargo run --manifest-path frontend/tui/Cargo.toml -- --tutorial
```

Use a real terminal for UI play. `TestBackend` is required for deterministic
coverage, but it does not replace keyboard navigation through the compiled
Ratatui application.

## Phase 0: Contract and baseline

### Goal

Establish an approved, testable API and an executable baseline before changing
interaction or layout.

### Deliverables

1. Amend the TUI PRD/ADR to record the expanded fleet and responsive-layout
   scope.
2. Inventory current snapshot fields and classify each recommendation as one
   of: already available, safely derived presentation data, or new engine
   advisory data.
3. Define additive protocol fields, with stable IDs rather than display order,
   for:
   - Side/faction ID and display label.
   - Owned/actionable ships for the current phase.
   - Legal fire target IDs and legal impact shield faces.
   - Maneuver endpoints/costs and weapon range/arc advisory geometry.
   - Per-ship allocation, maneuver, fire, and ready completion state.
4. Add a `fleet` snapshot fixture containing at least two owned ships, two
   enemies, distinct loadouts, and a targetable enemy.
5. Record baseline screenshots/buffer assertions at 80x24, 110x35, and
   140x40. These are comparison artifacts, not golden visual designs.

### Exit criteria

- The protocol owns all legality and side information needed in later phases.
- Existing v3 clients continue to parse snapshots or the protocol versioning
  and migration path is explicitly approved.
- The new fleet fixture parses in the TUI without client-side rules logic.
- Existing engine and TUI test suites remain green before feature work starts.

### Tests

- Engine tests prove advisory fields agree with accepted/rejected orders for
  representative movement and fire cases.
- Protocol parsing tests verify absent optional advisory fields are handled
  intentionally during migration.
- TUI fixture test verifies IDs, sides, and actionable state are not inferred
  from callsigns or list order.

## Phase 1: Fleet interaction safety

### Goal

Make multi-ship play correct before changing visual composition.

### Deliverables

1. Split local state into `inspected_ship`, `command_ship`, `target_ship`, and
   per-ship drafts.
2. Restrict command selection to owned/actionable ships. Enemy inspection must
   never make an enemy eligible to receive an order.
3. Store `AllocDraft`, movement selection, fire selection, and queued-shot
   draft state by source ship ID as applicable.
4. Use protocol-provided legal targets and shield faces for fire selection.
5. After an accepted allocate, movement, or ready-fire order, select the next
   owned pending ship. If none remain, preserve the current inspected ship and
   let the engine phase transition drive the next mode.
6. Promote engine rejections to the persistent notice slot while retaining the
   draft and source ship that produced the rejected order.

### Exit criteria

- Switching A1 -> A2 -> A1 never applies A1's allocation fields to A2.
- Inspecting B3 changes its details only; no order emitted from that state uses
  `ship_id = B3`.
- The default fire list contains only engine-declared legal targets. If
  friendly fire is a rule-supported option, it is opt-in and visually distinct.
- A two-ship player side completes a movement and firing cycle without cycling
  through enemies.

### Tests

- App/input test: create different drafts for A1 and A2, switch repeatedly,
  and assert each emitted `allocate` order has the matching loadout and ID.
- App/input test: select an enemy for inspection, press each order-producing
  key, and assert no enemy order is emitted.
- App/input test: accepted movement/ready action selects the next owned pending
  ship; rejected action keeps the same command ship and draft.
- Render test: header and active form show the command ship; inspector can show
  a different enemy ship.
- Render test: target list excludes non-legal contacts and labels the selected
  target/impact face.
- Live UI play: run `scenarios/fleet.toml` through all four movement/fire
  cycles at 120x42 without `Tab` traversing B3/B4 to reach A1/A2.

## Phase 2: Fleet browser and workspace navigation

### Goal

Replace the flat contact list and overloaded `Tab` behavior with a clear,
keyboard-first information architecture.

### Deliverables

1. Add explicit region focus: Map, Fleet, Workbench, and Events.
2. Make `Tab`/`Shift+Tab` move only among those regions.
3. Add dynamic side tabs generated from protocol side labels, with counts and
   independent selected-row/scroll state.
4. Render a stateful fleet table with phase-relevant columns:
   - Allocate: power and allocation state.
   - Movement: position, velocity, course, and maneuver state.
   - Fire: charge, range, queued state, and ready state.
5. Add Ship Inspector tabs: Overview, Systems, and Orders.
6. Add an explicit next-pending command and clear `PENDING`, `DRAFT`, `READY`,
   and `FIRED` badges.

### Exit criteria

- The user can inspect an enemy, return to owned ships, and resume the same
  command draft without losing selection or state.
- Fleet navigation remains useful with two sides and is data-driven for a third
  side.
- `Tab` no longer changes command ship ownership or commits/cancels a draft.
- Table clipping is visible through a scrollbar or explicit position indicator.

### Tests

- `TestBackend` buffer test at 110x35: side tabs, table columns, selected row,
  and ship inspector title are visible.
- State test: selected row and scroll offset survive switching side tabs.
- State test: region navigation returns from Events to Workbench with the same
  allocation/fire selection.
- Render test: monochrome mode preserves side identity through glyphs/labels,
  not only color.
- Live UI play: inspect both enemies and issue every order for A1/A2 without
  accidental enemy focus or loss of a local draft.

## Phase 3: Phase workbench and information hierarchy

### Goal

Give the active decision enough space while preserving immediate combat
feedback and access to reference views.

### Deliverables

1. Replace permanently stacked lower panels with one active Workbench and a
   compact latest-event strip.
2. Add Workbench tabs for allocation: Overview, Engines, Weapons, Shields.
3. Keep a fixed allocation summary containing source ship, used/pool/free
   power, category totals, draft state, reset, and commit controls.
4. Add a fleet allocation overview matrix for comparing owned ships.
5. Render movement as a maneuver list with engine-supplied cost/endpoint
   preview.
6. Render firing as one compact source -> weapon -> target -> impact-face
   relationship, including persistent queued-shot markers and a distinct Ready
   action.
7. Move full Combat Log and Help to switchable Events/reference views; retain
   the newest outcome or error in the always-visible event strip.
8. Use consistent ordering for weapons in allocation, fire, and inspector
   views.

### Exit criteria

- At 80x24, all allocation categories are reachable, the selected field is
  visible, and clipping has an explicit scroll position.
- At 110x35, the active phase surface, source ship, selected target, and latest
  result are visible simultaneously.
- A multi-weapon volley remains readable after resolution in the expanded log.
- No interaction requires reading a control hint that is hidden below a panel.

### Tests

- Render tests for every allocation category at 80x24, including last shield
  face and scrollbar/position indicator.
- Input tests verify category changes do not mutate other fields or discard the
  active ship's draft.
- Render test verifies power summary updates from a draft and exposes an
  over-budget/rejection state.
- Render test verifies queued fire is visible beside the source/target choice.
- Render test verifies the latest combat result remains in the event strip
  while the full ordered volley appears in the expanded log.
- Live UI play: allocate A1 and A2 with different loadouts, play one full
  fleet turn, and read the complete volley without opening unrelated panels.

## Phase 4: Responsive composition and tactical map viewport

### Goal

Turn the permanent map into a usable tactical viewport and make the new
information architecture work across explicit terminal-size tiers.

### Deliverables

1. Define and implement tested Small (80x24), Medium (about 110x35), and Large
   (about 140x40) layouts.
2. Below the Small floor, preserve session state and render a clear resize
   message with quit/help controls.
3. Add map viewport state: `Fit`, `Tactical`, and `Detail` zoom; pan; center
   inspected ship; follow command ship; overlay mode.
4. Render the map with a custom stateful widget so hex labels and callsigns
   remain crisp at each discrete zoom mode.
5. Add non-rule overlays first: selected ship, inspected ship, command ship,
   target line, proposed maneuver endpoint, and queued-shot vectors.
6. Add range, arc, legal-target, and shield-face overlays only from Phase 0
   advisory fields.

### Exit criteria

- Viewport zoom, pan, center, and follow never change a draft, command ship,
  target, or phase selection.
- Fit renders all board objects; Tactical and Detail make close engagement
  legible without hiding the selected relationship.
- Small mode shows one tabbed workspace; Medium and Large show the intended
  simultaneous map/fleet/workbench composition.
- A resize below the floor and back restores the prior UI state exactly.

### Tests

- Snapshot/buffer tests at 79x24, 80x24, 110x35, and 140x40.
- Viewport state tests for zoom clamping, pan bounds, center, follow, and
  persistence across snapshots.
- Render tests assert all ships appear in Fit and selected labels/overlays
  appear in Tactical/Detail.
- Render tests assert range/arc/shield overlays disappear when advisory data is
  unavailable rather than using a client-side approximation.
- Live UI play: play `scenarios/fleet.toml` at Medium and Large sizes; zoom to
  a close engagement, center A1, inspect B3, and return to Fit without losing
  an active order.

## Phase 5: Tutorial and advanced fleet efficiency

### Goal

Make the learning path and larger-fleet workflow use the stable interaction
model rather than creating special-case UI paths.

### Deliverables

1. Move tutorial narration into a pinned Coach drawer/overlay associated with
   the Workbench.
2. Show objective, rationale, exact expected action, visible wrong-action
   feedback, and Turn/Phase/step-within-phase progress.
3. Allow map and help navigation during the tutorial without advancing a step.
4. Advance tutorial progress only after an accepted engine outcome.
5. Add allocation overview actions: copy previous allocation, copy a selected
   owned ship, and previewable presets. Each action must show the exact changed
   fields before commit.
6. Add fleet filters: actionable, damaged, formation/group, in-range, and
   destroyed-history.
7. Add a visible `:` command palette as a keyboard fallback; it must invoke the
   same App actions as dedicated bindings.

### Exit criteria

- The rear-attack tutorial completes to WON while help/map inspection and a
  wrong action remain recoverable.
- Coach instructions and feedback remain visible at the Small floor.
- Bulk allocation actions never alter an unselected ship without preview and
  explicit confirmation.
- Command-palette and dedicated-key actions produce the same orders.

### Tests

- Tutorial state tests: opening help/map, wrong key, rejected order, and
  accepted order do not desynchronize the scripted step.
- `TestBackend` test at 80x24: objective, expected action, and wrong-action
  feedback are all visible without relying on color.
- Allocation action tests: copy/preset modifies only the selected draft and
  can be reset before commit.
- Command-palette tests: parse valid command, reject invalid command visibly,
  and compare emitted order with the dedicated key path.
- Live UI play: complete `--tutorial`, then play a two-ship fleet turn using a
  copied/preset allocation and next-pending navigation.

## Release gates

### Internal alpha: after Phase 1

Fleet play is safe: no draft leakage, no enemy command focus, and no default
ally targeting. Keep the current presentation if necessary; correctness is the
release criterion.

### Fleet UX beta: after Phase 3

The fleet browser and workbench are usable at Small and Medium sizes. A player
can complete a two-ship turn without panel starvation or ambiguous selection.

### Tactical viewport beta: after Phase 4

All three responsive tiers and discrete map viewport controls are available.
Only engine-provided advisory overlays are rendered.

### Feature complete: after Phase 5

Tutorial, command palette, and advanced fleet workflows sit on the same state
model and test seam as free play.

## Deferred work

Keep these out of the milestones unless a later product decision promotes them:

- Mouse-first controls.
- Continuous/freeform map zoom.
- A tab per weapon or shield face.
- Client-side to-hit, damage, arc, movement, or shield calculations.
- Campaign, save/resume, multiplayer, spectator, or mission-planning UI.
- Visual animation beyond a small, non-blocking state-change highlight.

## First implementation slice

After the Phase 0 scope decision is approved, start with Phase 1 only. It is
the smallest slice that eliminates the demonstrated fleet failures and it
requires no layout rewrite. Do not begin the fleet browser, category tabs, or
map viewport until the per-ship state and command/inspection separation have
their App/input and `TestBackend` coverage.
