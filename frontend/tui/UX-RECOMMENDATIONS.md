# TUI UX Recommendations

Date: 2026-07-14

## Scope and method

This recommendation is based on:

- Reading the current TUI implementation, README, handoff notes, and accepted input/layout ADR.
- Completing the live Ratatui tutorial through the TUI, winning on turn 3 at 110x40.
- Playing `scenarios/fleet.toml` through one complete turn and the next turn's allocation at 120x42, using only TUI controls.
- Reviewing current official Ratatui documentation and examples for layout, tabs, stateful collections, scrolling, canvas rendering, and text input.

This is an information-architecture recommendation, not an implementation plan. The main problem is not styling. The current screen models a duel reasonably well, but its selection and panel model does not scale to fleet play.

## Executive recommendation

Preserve the dedicated map as the main, persistent tactical surface. Rebuild the rest of the screen around two concepts:

1. A **fleet browser** that groups ships by side and clearly separates inspecting a ship from issuing orders to an owned ship.
2. A **phase workbench** that gives the current task enough space, with category tabs and a fixed summary rather than several shallow panels competing at once.

The target medium/large layout should be:

```text
+ Turn 2 | Allocate | Cycle 0/4 | 2 ships pending | notice ----------------------+
|                                   | [ Allies 2 ] [ Enemies 2 ]               |
|                                   | > A1 Heavy Cruiser  READY                 |
|          TACTICAL MAP             |   A2 Escort         PENDING               |
|       Fit / Tactical / Detail     |-------------------------------------------|
|                                   | A1 Overview | Systems | Orders             |
|                                   | Hull 40/40   Shields 18/24                 |
|                                   | Facing 0 ->  Velocity 2  Position (4,4)    |
+-----------------------------------+-------------------------------------------+
| ALLOCATION WORKBENCH: Overview | Engines | Weapons | Shields                 |
| Power 18/22 [################----]   Free 4   Draft saved for A1              |
| ...active category rows...                         [Reset] [Commit]            |
+ Events (latest result, expandable) --------------------------------------------+
| Tab: panel  [ ]: tab  arrows: select  Enter: edit  ?: help  +/-: map zoom      |
+-------------------------------------------------------------------------------+
```

This gives each region one job:

| Region | Job |
|---|---|
| Header | Turn, phase, cycle, pending work, urgent notice |
| Map | Spatial understanding and tactical previews |
| Fleet browser | Find, filter, inspect, and select ships |
| Workbench | Complete the current phase's orders |
| Event strip | Show the latest outcome; expand when history is needed |
| Footer | Contextual controls for the focused region |

Avoid adding more permanently visible boxes. Tabs should replace simultaneous panels, not create a hierarchy of nested chrome.

## What the playtests showed

### What is already working

- The bordered map is the strongest part of the layout. Keeping it visible through allocation, movement, fire, and game over preserves orientation.
- Callsigns and facing arrows make ships identifiable without relying only on color.
- The header's turn, phase, cycle, game status, and pending-action warning provide useful global context.
- Direct numeric allocation is much faster than incrementing values repeatedly.
- The tutorial teaches a real tactic and reaches a clear three-turn payoff.
- Combat results are useful when the combat log has enough height to show the complete volley.

### Fleet-scale blockers

These should be addressed before visual polish because they determine the correct UI state model.

1. **Inspection, command selection, and draft ownership are conflated.** During fleet allocation, changing focus from A1 to A2 left A1's heavy-cruiser allocation draft active. Committing it for the escort produced an engine error because the escort has no plasma weapon.
2. **`Tab` traverses enemy ships while the player is issuing orders.** Moving from A2 to A1 required cycling through B3 and B4. The movement form rendered for each enemy on the way, implying that the player could command it.
3. **The fire target list includes allies.** For A2, A1 appeared as the first target ahead of B3 and B4. Fleet play makes accidental friendly fire or accidental rejection much more likely.
4. **The contacts list is flat.** Owned ships and enemies differ only by callsign/color, which is insufficient once there are several factions, neutral units, objectives, or destroyed ships.
5. **The workflow has no automatic next-pending behavior.** Every movement/fire cycle requires manual focus traversal even when only one allied ship remains actionable.

The UI should therefore maintain separate state for:

- `inspected_ship`: any visible ship whose details may be viewed.
- `command_ship`: an owned, actionable ship receiving an order.
- `target_ship`: a legal target for the current weapon/order.
- `draft_by_ship`: independent allocation, maneuver, and fire drafts keyed by ship ID.

The engine remains authoritative. The TUI should never infer ownership or target legality from display order.

### Layout and legibility pressure

- The map has a fixed cell scale. A 10x10 or 12x10 board leaves unused space on a large terminal but may truncate on a small one.
- The allocation panel is only tall enough for the first part of the form. Shield rows become visible only as cursor scrolling changes the viewport, and there is no scrollbar to reveal position.
- Tutorial prose, phase controls, combat history, and command echoes compete in the same lower portion of the screen.
- The most important firing relationship is distributed across boxes: source ship in Ship Status, weapon and target in Fire, exposed shield information elsewhere or absent, and queued results in the log.
- Repeated static help consumes space while immediate errors are comparatively easy to miss.
- Weapon ordering differs between allocation and firing, increasing navigation cost.

## Recommended information architecture

### 1. Persistent tactical map

Keep the map in its own bordered region at every supported size. It should have explicit viewport state rather than deriving its rendering only from board dimensions.

Use three discrete zoom modes:

| Mode | Purpose | Behavior |
|---|---|---|
| Fit | Fleet overview | Entire board visible; compact callsigns and facing markers |
| Tactical | Normal play | Centered on selected formation or ship; enough room for range and course overlays |
| Detail | Close engagement | Larger labeled hexes, shield orientation, weapon arcs, and queued vectors |

Recommended map controls when the map has focus:

| Key | Action |
|---|---|
| `+` / `-` | Step between zoom modes |
| Arrows or `hjkl` | Pan viewport |
| `Home` | Fit entire board |
| `Enter` | Center inspected ship |
| `f` | Toggle follow-command-ship |
| `o` | Cycle overlays |

Discrete modes are preferable to continuous zoom. Terminal cells are not square, callsigns need minimum widths, and a hex map benefits from a few deliberately designed representations rather than arbitrary scaling.

#### Tactical overlays

Overlays should answer questions the player is deciding now:

- Selected ship and selected target connection.
- Current velocity/course and proposed maneuver path.
- Weapon maximum range and legal arc for the selected weapon.
- Valid targets distinguished from visible but invalid contacts.
- Shield face that an attack would strike.
- Queued fire lines and predicted impact face, without predicting random hit results.
- Objective zones or formation groups when those systems exist.

The TUI must not duplicate combat rules to produce these overlays. If legal arcs, exposed shield faces, or target validity are not already protocol data, expose them as engine-produced advisory fields. Basic viewport clipping, labels, and line drawing belong in the TUI; legality belongs in `shipsim_core`.

### 2. Fleet browser with side tabs

Replace the single focused-ship block plus flat contacts list with a stateful fleet browser.

Top-level tabs should be generated from engine-provided side/faction identity:

```text
[ Allies 2 ] [ Enemies 2 ]
```

If the game later supports more sides, use their scenario names rather than hard-coding Allies/Enemies or Team A/Team B. Counts should include living ships, with destroyed ships available through a filter or history view.

Within a side tab, use a compact table:

```text
  ID  Class          Hull   Shield   Range   Order
> A1  Heavy Cruiser  40/40  18/24    --      READY
  A2  Escort         20/20  10/12    2       PENDING
```

Recommended columns vary by phase. Allocation should emphasize power/order state; movement should emphasize velocity/course; firing should emphasize charge and range. Preserve row selection and scroll offset independently for each side tab.

Selecting an enemy changes only `inspected_ship` or `target_ship`. It must not change `command_ship`. Owned actionable ships should have a clear marker and an automatic "next pending" command.

Useful fleet filters:

- Actionable now.
- Damaged.
- In selected formation/group.
- In weapon range of the inspected target.
- Destroyed, hidden by default during active play.

Do not start with arbitrary fleet trees. Side tabs plus a sortable/filterable table cover the current fleet scenario and can later add formation rows without forcing all players through tree navigation.

### 3. Ship inspector

The selected ship's details should be compact and secondary to the fleet list. Suggested inspector tabs are:

| Tab | Content |
|---|---|
| Overview | Class, size, hull, position, facing, velocity, course, status |
| Systems | Weapons, charge, damage, shield faces, power |
| Orders | Current draft, committed maneuver, queued fire, completion state |

On a medium terminal, show the overview below the fleet list and let tabs replace it. On a small terminal, make Fleet and Inspector peer tabs in the lower workspace.

Use gauges sparingly for high-value ratios such as hull, total shield, and power. Keep exact values adjacent because color and bar length alone are not precise enough.

### 4. Phase workbench

The lower region should show one primary task at a time. Combat log, help, tutorial narration, and the active form should not all reserve permanent height.

#### Allocation

Use category tabs with a fixed summary:

```text
Allocation: [ Overview ] [ Engines ] [ Weapons ] [ Shields ]
Power 18/22  Free 4  | Engines 6 | Weapons 6 | Shields 6
```

Categories are better than one tab per individual system. Put all weapons in Weapons and all six shield faces in Shields; this keeps tab count stable as ship loadouts grow.

The summary must remain visible while changing categories and should include:

- Used, pool, and free power.
- Category totals.
- Over-budget/error state.
- Draft/committed state and ship callsign.
- Reset and commit actions.

The Overview tab should support fleet efficiency, not just repeat category rows. A useful fleet allocation matrix is:

```text
Ship  Engines  Weapons  Shields  Free  State
A1       6        6       10      0    DRAFT
A2       4        4        6      0    COMMITTED
```

This creates a natural place for later commands such as copy previous turn, copy from similar ship, or apply a named preset. Any preset should preview exact values before commit.

Allocation drafts must be stored per ship and survive inspecting another ship, changing tabs, or opening help.

#### Movement

Present maneuvers as a selectable list with an engine-supplied preview:

```text
> Coast       (10,4) -> (1,4)   velocity 3   cost 0
  Accelerate  (10,4) -> (2,4)   velocity 4   cost 1
  Turn 2      facing 0 -> 2     course 0      cost 1
```

The map should preview the selected path and endpoint. The list should distinguish absolute facing from speed and explain when movement resolves. After commit, automatically select the next allied ship that still needs a maneuver.

#### Fire

Keep source, weapon, target, impact face, and queue in one workbench:

```text
A1 -> beam_1 -> B3 | range 5/10 | front shield | legal arc
Queue: beam_1 -> B3 F   torp_1 -> B3 F
```

Use separate source and target selection. The target collection must contain only engine-declared legal targets by default; an optional "all contacts" inspection view may show why other ships are unavailable.

Queued weapons need persistent row markers. Ready should be a clearly separate action from queueing a shot. After ready, select the next allied ship that has not readied.

### 5. Events, log, help, and tutorial

Use a one-line or two-line event strip for the latest important result. Expand it into a full-height Combat Log tab or overlay when requested. This prevents an empty log from taking six rows during allocation while preserving the complete volley when combat resolves.

`Tab` should cycle major regions, matching the accepted ADR intent:

```text
Map -> Fleet -> Workbench -> Events
```

Use `[` and `]` for tabs inside the focused region, and arrows for rows. This avoids overloading `Tab` as both panel navigation and ship selection.

Help should be contextual. The footer shows the five or six currently valid controls; `?` opens complete help. A `:` command palette remains a useful fallback for discoverability and accessibility, particularly for commands that are infrequent or difficult to bind consistently.

Tutorial narration should become a coach drawer or overlay anchored to the workbench, not a permanently competing panel. Pin these elements so they never scroll away:

- Current objective.
- Why this choice matters.
- Exact expected action.
- Visible wrong-action feedback.
- Progress grouped as Turn / Phase / step within phase, rather than only a global 1-of-56 count.

The tutorial gate should validate accepted engine outcomes, not raw keypresses. Navigation to help or map inspection should be allowed without advancing the lesson.

## Responsive layouts

Define explicit layout tiers and test each with Ratatui's `TestBackend`.

| Tier | Suggested size | Layout |
|---|---:|---|
| Small | 80x24 minimum | Header, map, one tabbed workspace, footer; Fleet/Workbench/Log replace each other |
| Medium | About 110x35 | Map left, fleet/inspector right, workbench across bottom |
| Large | About 140x40+ | Larger map, persistent fleet rail, workbench, compact event strip |

These dimensions are starting points, not promises. Choose breakpoints from content minimums: callsign width, useful map viewport, allocation summary, and one complete action row. Below the supported floor, render only a clear resize message and quit/help controls.

Ratatui's layout constraints support nested responsive regions using `Length`, `Min`, `Max`, `Fill`, ratios, and flex behavior. See the official [Layout concepts](https://ratatui.rs/concepts/layout/) documentation.

## Ratatui capability mapping

The proposed design does not require a different TUI framework.

| Need | Ratatui support | Recommendation |
|---|---|---|
| Side and category tabs | [`Tabs`](https://docs.rs/ratatui/latest/ratatui/widgets/struct.Tabs.html) with selected index and styles | Use built-in tabs; render counts and active/focus styling |
| Fleet browser | Stateful [`Table` example](https://ratatui.rs/examples/widgets/table/) and `TableState` | Preserve selection/offset per side; use phase-specific columns |
| Long forms and logs | Stateful widgets plus [`Scrollbar`](https://ratatui.rs/examples/widgets/scrollbar/) | Show a scrollbar whenever content is clipped |
| Responsive tiers | [`Layout`](https://ratatui.rs/concepts/layout/) constraints and nested splits | Build explicit Small/Medium/Large compositions |
| Map bounds and shapes | [`Canvas`](https://ratatui.rs/examples/widgets/canvas/) supports coordinate bounds, labels, shapes, and multiple markers | Prototype overlays with Canvas, but retain a custom shipsim map widget for crisp labeled hexes |
| Custom persistent viewport | [`StatefulWidget`](https://docs.rs/ratatui/latest/ratatui/widgets/index.html) model | Store center, zoom mode, overlay mode, and selection in app-owned state |
| Typed fallback | Official [`user_input` example](https://ratatui.rs/examples/apps/user_input/) | Use a visible command-entry mode with cursor and explicit cancel/submit |

`Canvas` is useful for bounded coordinates and tactical lines, but it should not dictate the map design. Shipsim needs readable callsigns, hex adjacency, selection layers, and predictable terminal-cell layouts. A custom `MapViewport` stateful widget can use Ratatui's buffer directly and borrow Canvas concepts for bounds and markers. Canvas can be evaluated for overlays after the viewport behavior is proven.

Prefer Ratatui built-ins initially. Third-party widgets such as scroll views or logger integrations are available in the official [third-party widget showcase](https://ratatui.rs/showcase/third-party-widgets/), but adding dependencies is not necessary for the core redesign.

## Protocol data needed

The TUI boundary requires several engine-owned facts to support fleet UX safely:

| Data | Why |
|---|---|
| Side/faction ID and display name | Build dynamic Allies/Enemies/faction tabs without guessing from controller |
| Owned/actionable ship IDs by phase | Prevent enemy command focus and drive next-pending selection |
| Legal target IDs | Exclude allies and invalid contacts without reimplementing rules |
| Legal impact shield faces or resolved exposed face | Make fire selection and map shading understandable |
| Maneuver previews with cost and endpoint | Explain acceleration and movement timing without UI rule copies |
| Weapon range/arc advisory geometry | Render accurate tactical overlays |
| Per-ship committed/pending/ready state | Populate fleet badges and completion counts |

Some of this may already be derivable from snapshots and pending-order lists. Prefer explicit protocol fields where derivation would duplicate a game rule or rely on ship ordering.

## Visual direction

The current border language is useful; improve hierarchy rather than decorating every region.

- Use one strong accent for keyboard focus and a separate target accent. Do not make both yellow.
- Give sides distinct symbols as well as colors so monochrome and color-blind terminals remain usable.
- Use dim borders for inactive regions and a clear title treatment for the focused region.
- Use short status badges such as `PENDING`, `DRAFT`, `READY`, `FIRED`, and `HIT`, with consistent widths.
- Reserve red for damage, invalid state, and errors; reserve green for accepted/complete state.
- Keep exact values next to gauges and shading.
- Use whitespace inside the three major regions rather than adding borders around every subsection.
- Animate only meaningful state changes, if at all: a brief hit flash, new event highlight, or viewport recenter. Avoid terminal-wide motion.

The desired visual character is a tactical command console: dense but calm, with the selected relationship obvious at a glance.

## Priority order

### P0: Correct fleet interaction model

1. Separate inspected, commanded, and targeted ships.
2. Group the fleet by engine-provided side and prevent enemy command focus.
3. Store drafts per ship and preserve them across navigation.
4. Filter fire targets through engine-declared legality.
5. Add next-pending allied ship selection after every accepted order.
6. Surface engine rejection as a persistent header/workbench notice, not only a log line.

### P1: Recompose the screen

1. Introduce the fleet browser and phase workbench.
2. Make allocation category-tabbed with a fixed power summary.
3. Convert combat history and help into switchable views; keep only a latest-event strip visible.
4. Restore `Tab` to major-region navigation and use separate keys for rows and local tabs.
5. Add visible scrollbars to every clipped list/form.

### P2: Tactical map viewport

1. Implement Fit/Tactical/Detail viewport states.
2. Add pan, center, and follow behavior.
3. Add selection, course, range, arc, and shield-face overlays from engine advisory data.
4. Test labels and hex geometry at all responsive tiers before considering mouse support.

### P3: Tutorial and advanced fleet efficiency

1. Move tutorial narration into a pinned coach drawer/overlay.
2. Add fleet allocation overview, copy-previous, and previewable presets.
3. Add filters, formation/group selection, and batch navigation.
4. Add a typed `:` command palette.

## Explicit non-goals for the first redesign

- Do not create one tab per weapon or shield face.
- Do not use arbitrary continuous map zoom.
- Do not let the same selection state mean inspect, command, and target.
- Do not keep help, full combat log, tutorial prose, and phase controls visible simultaneously.
- Do not reimplement legal movement, range, arcs, hit chance, or shield-facing rules in the TUI.
- Do not add mouse-only interactions; every operation must remain efficient from the keyboard.
- Do not adopt Canvas solely because it can draw shapes. Use it only where it improves the shipsim map after labeled-hex requirements are met.

## Validation criteria

A redesign prototype should pass these UI-play checks before visual refinement:

1. Allocate two different allied ship classes, switch between them repeatedly, and commit both without draft leakage.
2. Inspect every enemy without ever making an enemy the command ship.
3. Complete all four movement/fire cycles for a two-ship fleet without cycling through enemies.
4. Queue fire against an enemy without allies appearing in the default legal-target list.
5. Switch map zoom modes, pan, center, and follow without losing selected ship/target context.
6. See all allocation categories and current scroll position at 80x24.
7. Resize through Small, Medium, and Large tiers without losing a draft or selection.
8. Read a complete multi-ship volley in the expanded log, then return to the same workbench state.
9. Complete the tutorial while freely opening help, inspecting the map, and recovering from a wrong action.
10. Verify each interaction in both a live Crossterm session and Ratatui `TestBackend` snapshots.

## Bottom line

The dedicated map should remain the TUI's anchor and become a real viewport. The more important change, however, is separating fleet browsing from command ownership and giving each phase one coherent workbench. Side tabs, stateful tables, category tabs, scrolling, responsive layouts, and a zoomable custom map are all supported by the current Ratatui stack. The first implementation milestone should fix fleet selection and per-ship state; map zoom and visual polish will then build on a UI model that can actually scale.
