# PRD  -  shipsim D8: Love2D Thin Frontend

Source alignment: `CONTEXT.md` (D8 grilling), `docs/D8-FRONTEND-BRIEF.md`, ADR-0017 (Love + JSON
subprocess), ADR-0018 (soft errors + controller on snapshot), ADR-0001/0004 (core/frontend split,
harness-first). Core through D10 is shipped; this PRD is the graphical client phase.

Pipeline stage: post-grill **to-prd** complete.  
Autonomy: **Checkpoint**.  
Execution policy: **cost_optimized**.  
Budget defaults: `max_milestones_unattended=8`, `max_pipeline_reentries=3`.

## Problem Statement

The simulation core is playable only through a CLI/JSON harness. There is no desktop UI to pick a
scenario, see the hex board, allocate energy, plot movement, fire weapons, and play multi-turn
fights to a win or loss. Prior Love + AI attempts failed operationally: agents asked humans to open
the window, could not falsify the client headlessly, and could only land tiny batches. The product
need is a **thin, maximally decoupled** Love2D frontend that agents can build and verify mostly
without human assistance, while still delivering a full play-to-end experience.

## Solution

Ship a Love2D client under `frontend/love/` that:

1. Lists scenarios and lets the player pick one.
2. Spawns the existing `shipsim` harness as a **JSON subprocess** (not FFI).
3. Renders **debug-geometry** hexes and ships from `StateSnapshot` only.
4. Guides the player through **Energy -> Plot -> Weapons -> Resolve** phases.
5. Sends `Order` NDJSON; displays soft-error NDJSON on the shared status strip without killing the
   session (after harness soft-reject lands).
6. Supports multi player ships via selection; AI/scripted ships act inside `run_turn` in the core.
7. Ends on core `Won` or **client-derived defeat** (all player ships destroyed), then returns to the
   picker.

All game rules stay in `shipsim_core`. The client never reimplements legality. Headless gates
(`cargo test`, golden NDJSON fixtures, pure Lua tests) are the primary verification path; human
checks are for visual readability only.

## User Stories

1. As a player, I want to launch a Love desktop client, so that I can play without using the CLI.
2. As a player, I want to see a list of available scenarios from the scenarios directory, so that I
   can choose what to play.
3. As a player, I want to select a scenario and enter play, so that the simulation loads that fight.
4. As a player, I want the board to appear after load without issuing an order, so that I see the
   initial setup immediately (post-load snapshot).
5. As a player, I want a flat-top hex grid matching the map size and mode from the snapshot, so that
   I understand the battlefield layout.
6. As a player, I want ships drawn at their hex positions with facing, so that I can read geometry
   at a glance.
7. As a player, I want ship class, id, and basic status visible, so that I can tell ships apart
   without art assets.
8. As a player, I want a phase bar for Energy, Plot, Weapons, and Resolve, so that I know what to do
   next in the turn.
9. As a player, I want the Energy phase to let me set movement, weapons, and shields for the
   selected ship within power, so that allocation is intentional.
10. As a player, I want a default-fill control for energy, so that I can start a turn quickly.
11. As a player, I want confirming energy to send an allocate order and refresh the snapshot, so that
    the core records my allocation.
12. As a player, I want the Plot phase to let me build a path by clicking hexes, so that I plan
    movement visually.
13. As a player, I want keyboard shortcuts for path building and ship cycling, so that I can play
    without only the mouse.
14. As a player, I want a ghost path preview before submit, so that I see what I will send.
15. As a player, I want confirming the plot to send a plot order for the selected ship, so that
    movement is declared for the turn.
16. As a player, I want the Weapons phase to let me pick a weapon and a target ship, so that I can
    declare fire.
17. As a player, I want to declare multiple fires before resolve when the core allows, so that a
    volley is possible.
18. As a player, I want Resolve to send run_turn, so that movement, seeking, AI, and fire resolution
    play out in the core.
19. As a player, I want the board and HUD to update from the post-resolve snapshot, so that I see
    outcomes without reloading.
20. As a player, I want AI and scripted ships to act without my input during resolve, so that I only
    command player ships.
21. As a player, I want to select among multiple player-controlled ships, so that fleets are
    playable.
22. As a player, I want allocate, plot, and fire to apply to the selected player ship, so that
    multi-ship turns are controllable.
23. As a player, I want only player ships to be selectable as the active commander ship, so that I
    do not try to pilot the AI.
24. As a player, I want illegal orders to show a clear message without crashing the client, so that
    I can correct mistakes and continue the fight.
25. As a player, I want all errors (bad order, missing binary, parse failure) to use the same status
    strip pattern, so that feedback is consistent.
26. As a player, I want the HUD to show turn, impulse, status, power buckets, shields, structure,
    and weapons, so that I can make informed decisions.
27. As a player, I want recent combat log entries visible, so that I understand what hit whom.
28. As a player, I want seeking munitions shown on the board when present, so that drones/plasma are
    visible.
29. As a player, I want pan and zoom on the map, so that large scenarios remain usable.
30. As a player, I want victory when the snapshot status is Won, so that objective/destruction wins
    are celebrated.
31. As a player, I want defeat when all my player ships are destroyed even if status is still
    InProgress, so that a lost fight ends the session cleanly.
32. As a player, I want an end screen that returns me to the scenario picker, so that I can play
    again without restarting Love.
33. As a player, I want to abandon a fight back to the picker, so that I am not stuck in a bad game.
34. As a developer, I want the client under frontend/love with room for other frontends later, so
    that presentation stacks stay separated by directory.
35. As a developer, I want all rules to remain in shipsim_core, so that the Love tree stays thin
    (ADR-0001, ADR-0017).
36. As a developer, I want integration via JSON subprocess only for D8, so that FFI complexity is
    deferred.
37. As a developer, I want Order and StateSnapshot shapes to match the existing serde contracts, so
    that the harness remains the single protocol surface.
38. As a developer, I want the harness to soft-reject illegal orders with a structured error line,
    so that multi-turn UI sessions survive mistakes (ADR-0018).
39. As a developer, I want ShipSnapshot to include controller, so that the client never parses
    scenario TOML for ownership (ADR-0018).
40. As a developer, I want post-load snapshot emission preserved, so that initial paint needs no
    dummy order.
41. As a developer, I want binary discovery via target/debug/shipsim with SHIPSIM_BIN override, so
    that local and agent runs are reliable.
42. As a developer, I want the harness working directory to be the repo root, so that scenario paths
    match cargo tests.
43. As a developer, I want pure Lua unit tests runnable with luajit without opening a window, so
    that agents verify hex math, order builders, phase transitions, and defeat derivation headlessly.
44. As a developer, I want golden NDJSON fixtures locked in cargo tests, so that protocol drift is
    caught immediately.
45. As a developer, I want agents never to ask a human to open Love for correctness, so that
    batches stay large and autonomous (D8 brief ladder).
46. As a developer, I want debug geometry only for D8 visuals, so that art taste does not block
    play-to-end.
47. As a developer, I want client-derived defeat without a core Lost status for D8, so that end
    flow ships without a rules expansion.
48. As a maintainer, I want core Lost, full EA form UX, impulse-step animation, campaign UI, FFI,
    and themed art deferred on the roadmap, so that D8 stays shippable.
49. As a maintainer, I want Godot out of scope for this phase, so that stack churn does not reopen.
50. As an agent implementer, I want milestones with falsifiable headless gates, so that each slice
    can complete without human visual sign-off except optional polish.

## Implementation Decisions

### Architecture and boundaries

- Love2D 11.x client lives under `frontend/love/`; other frontends may appear later as siblings.
- Simulation authority is exclusively `shipsim_core` via the `shipsim` CLI harness.
- Protocol: NDJSON over stdin/stdout. One snapshot line after load; one snapshot or error line per
  submitted order line.
- No game rules, range/arc/energy validation, or AI in Lua. Phase UI may *suggest* sequence only.
- FFI / cdylib bridge out of scope for D8.

### Core / harness changes (same product effort as the client)

- Soft-reject illegal orders: print structured error envelope; do not exit; do not mutate state.
- Error envelope fields: `type=error`, `ok=false`, `code`, `message`, optional `order`, `source`.
- Success snapshots remain unwrapped `StateSnapshot` JSON objects.
- Add `controller` string to each ship in the snapshot (values aligned with scenario controllers:
  player, ai, scripted, ...).
- Regenerate and lock D8 golden fixtures after snapshot/protocol changes.
- Existing order types unchanged: allocate, plot, fire, run_turn.

### Client structure (conceptual modules)

- App screens: scenario picker, play, end (win/defeat).
- Harness adapter: spawn process, line-buffered IO, parse snapshot vs error lines.
- View model: last snapshot, phase, selected ship id, ghost path, pending weapon/target, status
  strip messages.
- Flat-top hex layout math (pure, testable).
- Board draw (debug geometry), HUD draw, shared status strip.
- Input: mouse primary path/target; keyboard shortcuts for cycle/confirm/cancel/phase.
- Phase controller: Energy -> Plot -> Weapons -> Resolve; Resolve sends run_turn and advances turn UX.
- End condition evaluator: Won from snapshot; defeat if every player-controller ship is destroyed.
- Binary locator: prefer built shipsim under target; env override; repo-root cwd.

### UX decisions

- Visual bar: debug geometry + text; no sprite pipeline.
- Energy: minimal steppers + remaining power + default-fill + confirm.
- Multi-ship: one active player ship; orders target selection; single Resolve per turn.
- Camera: pan and zoom sufficient for large maps.
- run_turn is atomic in the UI (no impulse animation).

### Seams (prefer existing)

Primary seam: **harness NDJSON** (already exercised by integration tests). Extend with error lines
and controller field; keep one protocol surface.

Secondary seam: **pure Lua modules** tested via luajit (new but isolated; no Love runtime).

Do not introduce a second simulation API.

## Testing Decisions

Good tests assert **external behavior**: process output, pure function results, order JSON shape,
end-condition derivation - not Love draw call counts or private UI state.

### Rust / harness

- Soft-reject: illegal order yields error line, process can accept a following legal order, state
  unchanged for the illegal attempt.
- Snapshot includes `controller` per ship; golden fixture updated.
- Existing post-load + per-order snapshot counts remain correct.
- Reproducibility of scripted order files preserved where applicable.

Prior art: `tests/harness.rs`, `tests/fixtures/d8/`, combat/movement acceptance tests.

### Pure Lua (luajit, no window)

- Flat-top hex pixel conversion and neighbor layout consistency.
- Order builders produce correct JSON tables/strings for allocate/plot/fire/run_turn.
- Phase transition rules (allowed actions per phase) as pure state machine tests.
- Defeat derivation from fixture-like ship tables.
- Scenario path listing helpers if pure (filesystem tests as appropriate).

### Love window

- Not required for milestone gates. Optional smoke only if it asserts exit code / no Lua error
  without human observation.
- Human visual check is residual polish after play-to-end gates pass.

### Agent policy

- Never gate correctness on "please open Love and check."
- Prefer logs of last N order/error/snapshot lines for debugging.

## Out of Scope

- Godot, web, or other frontend implementations (directory reserved only).
- FFI / in-process cdylib bridge.
- Core `Lost` status (deferred residual; client-derived defeat only).
- Full paper Energy Allocation form, EW, overload UX.
- Impulse-by-impulse animation or stepping UI (roadmap T4).
- Campaign picker / multi-scenario campaign flow in Love (harness campaign flag may exist; D8 UI is
  single-scenario pick from scenarios list).
- Themed art, sound, animation polish.
- Save/load document format beyond what the harness already supports for play sessions.
- Reimplementing combat, movement, or AI rules in Lua.
- Trademarked SFB names or published stats (ADR-0003 still applies to content).

## Further Notes

- Headline acceptance: from Love (or headless client logic + harness), load a destruction scenario
  with an AI or scripted opponent, complete multi-turn play, reach Won or client-derived defeat,
  return to picker - with illegal-order soft reject demonstrated in automated tests.
- Default binary path and repo-root cwd must be documented for `love frontend/love` (or equivalent)
  launches.
- JSON codec: vendored pure Lua or Love-compatible library; no FFI for JSON (assumption).
- D8 brief remains the agent workflow doc; this PRD is the product/spec handoff to PLAN.

## Assumptions and Open Questions

### Assumptions (proceed unless PLAN or review blocks)

1. Vendored pure-Lua JSON is acceptable.
2. Scenario picker lists `*.toml` under `scenarios/` at repo root; no recursive campaign nesting in UI.
3. "Scripted" and "ai" controllers are both non-selectable as active player ships.
4. Windowed Love is installed in the agent/dev environment (already true on maintainer machine);
   CI may run only cargo + luajit gates.
5. Phase UI may disable out-of-order actions client-side for UX; core remains final authority.
6. cost_optimized execution; Checkpoint autonomy for the rest of the pipeline.

### Open questions (non-blocking defaults if unset)

1. Exact `code` string vocabulary for errors beyond `order_illegal` (e.g. `spawn_failed`,
   `parse_error`)  -  PLAN may define a small enum.
2. Whether fleet multi-ship is required in the headline acceptance scenario or only supported when
   present  -  default: support in UI; headline gate may use 1v1 `ai.toml`.
3. Precise keyboard binding map  -  PLAN/implementer may choose and document in the Love tree README.
