# Love2D client upgrade plan — TUI parity + graphical advantage

**Written:** 2026-07-17, after two full TUI playtests and the TUI multi-ship fix
round. The ratatui client (`frontend/tui/`) is the current UX benchmark; this
plan brings `frontend/love/` to parity on *guidance* (previews, call-to-action,
event feedback) and then past it on *visualization* (things a terminal cannot
draw). A phase = one PR-sized unit with pass/fail milestones a weaker model can
self-check. Fable reviews at the end of each phase before the next begins.

## Ground rules (repeat in every PR description)

1. **No rules in the client.** Hit odds, arcs, ranges, costs, legality all come
   from the engine — via snapshot fields or the read-only requests in
   `docs/PROTOCOL.md` (`movement_preview`, `maneuver_options`, `fire_preview`).
   The client may do *geometry for pixels* (where to draw), never *geometry for
   legality* (whether a shot/move is allowed).
2. **Isolation.** All code, tests, and scratch stay under `frontend/love/`.
   Never edit `src/`, `frontend/tui/`, or `frontend/repl/` from this plan.
3. **Verification stack.** After every phase, all of:
   ```bash
   cargo build -q
   luajit frontend/love/tests/run_all.lua      # all checks pass, incl. new ones
   love frontend/love                          # manual visual checklist below
   ```
   Each phase adds named checks to `tests/run_all.lua`; a phase is NOT done
   until its listed check names print `OK`.
4. **Session junk** goes to `frontend/love/local/` only.
5. When a milestone says "grep gate", run the exact command; it must output
   nothing (or exactly what is stated). These are cheap machine-checkable
   stand-ins for judgment.

## Reference: what the TUI has that Love lacks (verified 2026-07-17)

| Capability | TUI | Love today |
|---|---|---|
| `movement_preview` request (reachable endpoints, `clamp:true` live drag) | yes | **no** |
| `maneuver_options` request (authoritative thrust costs, NO markers) | yes | **no** |
| `fire_preview` request (hit %, damage, legal faces) | yes | **no** — client computes arcs locally |
| `fire_opportunity` call-to-action (player-scoped, attacker-attributed) | yes | **no** |
| `translation_results` ("Moved 3/8; blocked by B2") | yes | **no** |
| Destroyed-weapon phrasing ("destroyed and cannot fire") | yes | no |
| Multi-ship focus handoff + "Tab to switch" hint + dead-focus recovery | yes | unknown/partial |
| Game-over summary (turns, shots, hits, damage dealt/taken) | yes | minimal ("won"/"defeat") |
| Rules provenance (`rules_id`/`rules_fingerprint`) shown | header | no |

Love's structural advantages to exploit: real color/alpha/shape, animation
time, mouse hover, tooltips, smooth pan/zoom. Phases 4–5 spend them.

---

## Phase 0 — Protocol catch-up and test scaffolding

**Files:** `harness.lua`, `main.lua`, `tests/run_all.lua`, new `events.lua`.

The snapshot grew fields the Love client ignores. Parse them, expose them, and
build the request/response plumbing every later phase uses.

Tasks:
1. In `harness.lua`, add `harness.request(session, tbl)` that sends a JSON line
   with a `request` field and returns the decoded response envelope (these are
   read-only: they produce a typed response, **not** a snapshot, and must not
   enter the orders log — see `docs/PROTOCOL.md` "Read-only requests").
2. Surface on the app state, from every accepted snapshot:
   `fire_opportunity`, `translation_results`, `end_turn_warning`, `rules_id`,
   `rules_fingerprint`, per-ship `attack_accuracy_bonus` (absent = 0).
3. New module `events.lua`: a pure ring buffer (cap 50) of structured events
   built by diffing consecutive snapshots' `combat_log` (new entries only) plus
   `translation_results`. Each event: `{turn, kind, text}`. No Love APIs in
   this module — it must run under plain luajit.
4. Show `rules_id` + first 12 chars of `rules_fingerprint` in the status strip.

Milestones (all must pass):
- [ ] `luajit frontend/love/tests/run_all.lua` gains checks named
      `request envelope round-trip` (send `movement_preview` for ship 1 against
      a live engine, assert `type == "movement_preview"`, `ok == true`,
      `endpoints` non-empty), `events ring buffer caps and orders`, and
      `snapshot exposes fire_opportunity fields`; all pass.
- [ ] Grep gate — requests never pollute the order log:
      `grep -rn '"request"' frontend/love/orders.lua` → no output.
- [ ] Manual: status strip shows `rules: default fnv1a-…` on load.

## Phase 1 — Engine-authoritative previews (delete local rules math)

**Files:** `draw_board.lua`, `draw_hud.lua`, `main.lua`, `tests/run_all.lua`.

Replace the client's homegrown arc/bearing legality with `fire_preview` and
`maneuver_options`. This is the parity core AND removes an anti-goal violation
(`draw_board.lua` currently reimplements `arc_ok`/`relative_bearing` to decide
what is shootable).

Tasks:
1. Firing phase: when a weapon+target pair is selected (or hovered — cheap in
   Love), issue `fire_preview`; render its `hit_percent`, `projected_damage`,
   `legal_shield_facings`, and `legal`/`reason` verbatim. Attribute the
   attacker by callsign exactly like the TUI: `A2 beam_1 → B4 45% dmg≈8`.
2. If the previewed weapon exists on the ship but `operational == false`, show
   `"<id> is destroyed and cannot fire"` — never the engine's raw
   "was not found" lookup text (TUI parity; see `frontend/tui/src/ui.rs`
   `fire_preview_line`).
3. Movement phase: on active-ship selection, issue `maneuver_options`; the
   action buttons show engine costs and disable unaffordable/illegal entries
   with the engine's `reason` as tooltip text. Delete any client-side cost
   guessing.
4. Demote `arc_ok`/`bearing_to`/`relative_bearing` in `draw_board.lua` to
   display-only shading helpers, or delete them if Phase 4's arc rendering
   replaces their use. Add a header comment: "display only — legality comes
   from fire_preview".

Milestones:
- [ ] New checks pass: `fire_preview drives target legality` (against live
      engine: an out-of-arc pairing renders the engine's reason, not a local
      verdict) and `maneuver options disable unaffordable turns`.
- [ ] Grep gate — no legality words in board math:
      `grep -n "legal\|can_fire\|affordable" frontend/love/draw_board.lua` → no output.
- [ ] Manual: hovering an enemy in firing phase shows hit % + damage within
      150 ms; selecting an uncharged/destroyed weapon explains why in words.

## Phase 2 — Call-to-action and multi-ship command flow (TUI learnings)

**Files:** `main.lua`, `draw_hud.lua`, `ui_status.lua`, `tests/run_all.lua`.

Port the exact semantics the TUI converged on after the 2v2 playtests — these
were bugs there; do not rediscover them here:

1. Banner line (top of HUD) driven by phase, always naming the ship that will
   receive the next input:
   - allocate/movement: `"A2 needs power allocation"` / `"A1 needs a maneuver"`
     for the *selected* pending ship; if the selected ship is done but a
     fleetmate is pending: `"A2 needs a maneuver — click A2 or press Tab"`.
   - firing: `fire_opportunity` verbatim with attacker callsign
     (`"Shot available: A2 beam_1 → B4"`), **skipping ships already in
     `ships_ready_fire`** (the TUI still has this gap — do it right here);
     otherwise `"No legal shot; Ready passes fire"`.
   - turn_end: `"Unused legal shot will be forfeited"` when
     `end_turn_warning`, else `"Turn complete"`.
   - status `Won`/`Lost`: `"Game over"` — never stale next-turn advice.
2. Selection auto-advance: after a ship completes its phase action, select the
   next pending player ship. If the selected ship is destroyed in any snapshot,
   reselect the first living player ship and rebuild any drafts (dead-focus
   recovery — blocking bug found in TUI 2v2).
3. Never issue orders or read-only requests for destroyed ships (grep gate
   below), and never show enemy shots as opportunities (engine now scopes
   `fire_opportunity`, but the banner must not invent its own).

Milestones:
- [ ] New pure-Lua checks pass: `banner names pending fleetmate`,
      `banner skips ready-locked ships`, `dead selection recovers to survivor`
      (feed synthetic snapshots; banner/selection logic must live in a
      requireable module, not inline in `love.draw`).
- [ ] Grep gate: `grep -n "destroyed" frontend/love/main.lua` shows a guard in
      every order-submit path (list the line numbers in the PR).
- [ ] Manual (fleet.toml): kill A1 via console cheat or long play — selection
      jumps to A2 and allocate works without an engine rejection.

## Phase 3 — Event feedback: ticker, floaters, translation callouts

**Files:** `events.lua` (from P0), new `fx.lua`, `draw_board.lua`, `draw_hud.lua`.

The #1 playtest complaint on both clients: damage happens between your
glances and only a 4-line log records it. Love can fix this properly.

1. Recent-events ticker: last 6 events from `events.lua` rendered above the
   log, color-coded (player hit dealt = green, damage taken = red, miss =
   gray, blocked slide = yellow), fading after ~5 s of no change.
2. Damage floaters: on a new combat_log entry, spawn drifting text at the
   target's hex (`-8`, `MISS`, `shield 6`) rising and fading over 1.2 s.
   `fx.lua` owns particles/tweens: pure state + `update(dt)` + `draw()`;
   `update` must be testable headless (no Love calls in state math).
3. Translation callouts: when `translation_results` reports a block for any
   visible ship, draw a small `⊘` marker on the blocked hex edge and add a
   ticker line using the same wording as the TUI ("Moved 3/8; blocked by B2").
4. Ship damage pulse: hull loss flashes the ship marker red for 0.4 s;
   destroyed ships render as a wreck glyph immediately (no animation needed).

Milestones:
- [ ] Headless checks pass: `fx tweens complete and free themselves` (spawn
      100 floaters, step `update(60×2s)`, assert pool empty),
      `ticker colors by event kind`, `blocked translation becomes an event`.
- [ ] Manual: in a duel, every enemy hit on the player is visible without
      reading the log (floater + red pulse + ticker), and a same-course
      tailgate shows the ⊘ marker.
- [ ] Frame budget: with 20 active floaters, `love.timer.getFPS()` ≥ 60 on the
      dev machine (print it in a debug corner while testing).

## Phase 4 — Board visualization: make the graphics earn their keep

**Files:** `draw_board.lua`, `main.lua`, small additions to `harness.lua` use.

This is the phase a terminal cannot follow. Everything here renders
engine-provided data — no new rules.

1. **Reachable-endpoint cloud.** During allocate, as the movement slider/keys
   change, issue `movement_preview` with `clamp:true` (built for live drags —
   see PROTOCOL "clamp") and render `endpoints` as translucent diamonds,
   `coast` as a distinct outline, `occupied` endpoints in warning color.
   Debounce to ≤ 5 requests/s.
2. **Weapon arc fans.** For the selected ship, draw each weapon's arc as a
   translucent fan (radius = `max_range` in hexes, arc span from the weapon's
   `arc`/`mount` fields), colored by charge state (charged / uncharged /
   destroyed). Use snapshot fields only; this replaces the old shading helpers
   for good.
3. **Shield ring.** Around the selected ship (and hovered contacts), draw six
   arc segments sized/colored by `shields_remaining` vs `max_shield_per_facing`
   — bare faces visibly missing. This answers "why did that torp hit sh-0"
   at a glance (playtest finding: the AI flanks to bare faces invisibly).
4. **Threat bearing.** For each enemy with a charged weapon that the engine
   says can reach the selected ship (reuse `fire_preview` with roles reversed,
   throttled to selection changes), draw a thin red bearing line. Cache per
   snapshot; never per frame.
5. **Velocity vectors.** Arrow from each ship along its course, length ∝
   velocity, so head-on pass-throughs and kiting are legible pre-slide.

Milestones:
- [ ] Headless checks: `preview debounce coalesces bursts` (pure timer logic),
      `arc fan geometry spans correct hex count` (pixel-math only — assert
      hex→pixel fan endpoints for a known weapon).
- [ ] Manual checklist (screenshot each into `frontend/love/local/p4/`):
      endpoint cloud updates while dragging the movement slider; coast marker
      distinct; a bare shield face is visually obvious from 2 m away; arc fans
      change color when a weapon fires/dies; velocity arrows present.
- [ ] Grep gate: `grep -n "movement_preview" frontend/love/draw_board.lua` →
      no output (requests live in main/harness layer, board only draws state).

## Phase 5 — Resolution theater and game over

**Files:** `fx.lua`, `main.lua`, `draw_hud.lua`, `end_condition.lua`.

1. **Fire animation on resolve:** when a volley resolves (new combat_log
   entries), play ≤ 0.8 s of tracers: beam = instant line flash, torp = moving
   dot, plasma = expanding bolt; impact spark on hit, "whiff" puff past the
   hull on miss. Input stays live (animations are cosmetic; never block
   orders). Skippable via a settings flag `fx_enabled=false`.
2. **Slide interpolation:** ships lerp between snapshot positions over 0.3 s
   instead of teleporting (store previous position per ship id).
3. **Game-over panel:** replicate TUI stats — VICTORY/DEFEAT, turns, player
   shots/hits, internal damage dealt/taken — computed from the `events.lua`
   history (already structured; do not parse log strings). Plus a "quit"
   button and the session log path.
4. Session log on quit (orders + snapshots already dumped to `local/`): print
   the path on exit like the TUI does.

Milestones:
- [ ] Headless: `game over stats match event history` (synthetic events in →
      exact counts out), `lerp reaches target within duration`.
- [ ] Manual: play `scenarios/ai.toml` to a win — tracers visible, no input
      lag during animation, game-over panel shows non-zero stats matching the
      log, `fx_enabled=false` disables all of it.
- [ ] `luajit frontend/love/tests/run_all.lua` total check count ≥ 30 (13
      today + phases' additions) and all pass.

## Phase 6 (stretch, needs explicit go-ahead) — tutorial and replay

- Port the rear-attack tutorial (step gate machine mirrors
  `frontend/tui/src/tutorial.rs`; same scenario, same step order).
- Replay viewer: load a save's order stream and scrub turn-by-turn with the
  Phase 5 animations. Read-only; reuses `--resume` replay semantics.

---

## Execution notes for the implementing model

- Work phase by phase, one commit per phase, message `love: phase N — <title>`.
  Do not start phase N+1 before phase N's milestones all pass; paste milestone
  evidence (test output, grep output, screenshot paths) into the PR/handoff.
- When a milestone fails and the fix is unclear, STOP and hand off with the
  failing output — do not improvise around engine behavior.
- The TUI source is the semantic reference: `frontend/tui/src/ui.rs`
  (`phase_call_to_action`, `fire_preview_line`), `app.rs` (dead-focus
  recovery, preview guards). Copy behavior, not code.
- Protocol contract: `docs/PROTOCOL.md`. If a needed field seems missing,
  re-read that file before touching anything outside `frontend/love/`; if it
  is genuinely missing, stop and report — engine changes are out of scope.
- Keep every new module luajit-clean (`luajit -e "require('<mod>')"` must not
  error) so it is testable headless; only `main.lua`, `draw_*.lua`, `fx.lua`'s
  draw half may touch `love.*` APIs.
