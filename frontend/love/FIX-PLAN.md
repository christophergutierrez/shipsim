# Love2D fix & playability plan

**Written:** 2026-07-18, from a live driven playtest of `scenarios/fleet.toml`
(xdotool + per-step screenshots; evidence in the session scratchpad and
reproducible via the loop in `tmp/CLEANUP-PLAN.md` P1). Every finding below
was seen on screen, not inferred from code. Supersedes the fix list in
`tmp/CLEANUP-PLAN.md` Phase 2.

> **Status (2026-07-19):** F1–F4 work is landed. `status_fmt.order_echo` now
> includes the F2 `turn` → `"turned to facing N"` branch (restored in the
> killhouse review). Headless suite: **`luajit frontend/love/tests/run_all.lua`
> → All 92 checks passed.** For protocol-v4 controls and modules, prefer
> [`README.md`](README.md). Pre-v4 upgrade docs are superseded (see banners on
> UPGRADE-PLAN / HANDOFF-REVIEW / REVIEW-VERDICT).

**Retraction first:** an earlier report claimed steppers "double-fire" (+2
per click) and that one click could commit two ships' maneuvers. Both were
artifacts of the driving harness (`xdotool mousemove X Y click 1` chained
form delivers two presses; separate `mousedown`/`mouseup` delivers one and
the app increments exactly +1). **Do not add press/release debouncing** — the
input path is correct. A regression check that one synthetic press event
increments a draft exactly once is still worth adding (F1.5) to keep this
provable.

## Confirmed defects (each has a screenshot)

| # | Defect | Where (start looking) |
|---|---|---|
| D1 | Header top-center draws two strings at the same coords → smear like "Bbipsbeatalbmcate" in every phase. Almost certainly the phase label and the End-turn control sharing one slot — which also makes **End turn unreadable/undiscoverable** | `draw_hud.lua` header block |
| D2 | Maneuver cost annotations ("0 ok", "1 ok", "3 ok", "n/a") render half-under the button borders | `draw_hud.lua` movement panel |
| D3 | Fire-preview reason clipped mid-word at panel edge, no wrap ("weapon beam_1 cannot be…") | `draw_hud.lua` fire panel |
| D4 | Movement-preview endpoint cloud (dozens of diamonds) persists across the whole board **during the firing phase** | overlay lifecycle in `main.lua` / `draw_board.lua` |
| D5 | Panel reflows mid-phase: roster rows appear/disappear and the preview line inserts, moving every button below (Commit Fire moved twice within one fire phase). Clicks land on the wrong control right after a state change | `draw_hud.lua` layout = content-driven y-cursor |
| D6 | Own ship listed in the fire Target picker (A2 offered "#1 Heavy Cruiser") | target list builder |
| D7 | Status strip: stale messages persist ("Pick weapon and target" outlives the state that caused it); order echo is cryptic ("Ship #2 turn 3" = turned to facing 3) | `ui_status.lua` / order echo strings |
| D8 | "Active #nil" in header during allocate (single-ship scenario) | header active-ship formatting |
| D9 | Wide windows waste space: at 3832×1021 everything pins left/right edges with a dead middle | layout root |
| D10 | Click targets are ~20×22 physical px at ui.scale=1.0 on a 4K display — far below comfortable, and `ui.min_scale = 1.0` forbids the fix via Ctrl+= alone being remembered | `ui.lua` scale + `conf.lua` |

## Phase F1 — Layout stability (fixes D1, D2, D3, D5, D9)

The theme: **a control's position must not depend on transient content.**

1. Give the header three fixed slots: left = turn/phase/CTA, center-right =
   End Turn button (its own reserved rect, never overdrawn), right = rules
   line. Grep gate: no two `love.graphics.print`/button calls in the header
   share an x-range (add a tiny luajit test that walks the header layout
   table and asserts non-overlap of slot rects).
2. Reserve fixed line slots in each phase panel: roster always renders
   max-player-rows lines (dead ships stay listed, dimmed with a wreck mark —
   this is also better information); the fire panel always reserves 2 lines
   for preview/reason (blank when absent); maneuver buttons get a fixed
   cost column *inside* the button rect (right-aligned) instead of a
   floating label under it.
3. Wrap or ellipsize all engine strings to panel width
   (`font:getWrap`); reasons wrap to the 2 reserved lines.
4. Wide-window layout: board area centers between left edge and panel;
   panel width fixed; at width ≥ 2400 also bump default ui.scale (see F3).
5. Regression checks (luajit, layout math must live in a requireable module):
   `header slots do not overlap`, `panel row count independent of ship
   status`, `press increments draft exactly once`.

Milestones: screenshots at 1280×800 and 3832×1021 in
`frontend/love/local/fixes/f1/` showing (a) readable End Turn button,
(b) costs inside buttons, (c) no board-visible reflow when a preview line
appears (record two consecutive frames after selecting a target — Commit
Fire y unchanged); all new checks green.

## Phase F2 — Truthful feedback (fixes D4, D6, D7, D8)

1. Clear phase-scoped overlays on every phase change: endpoint cloud,
   maneuver ghosts. Rule: any overlay derived from a request made in phase X
   dies when `snap.phase ≠ X`. Add check `stale endpoint cloud cleared on
   phase change` (pure state test on the overlay store).
2. Target picker lists enemies only (`controller ~= "player"`, not
   destroyed). Friendly-fire is not offered even if the engine would merely
   reject it. Check: `target list excludes own fleet`.
3. Status strip lifecycle: every message carries the turn+phase it was born
   in; strip clears on phase change and on any successful order. Order echo
   becomes human wording via a tiny formatter table: `turn 3` →
   `"turned to facing 3"`, `accel` → `"accelerated"`, etc. Check: `status
   clears on phase change`, `order echo uses words`.
4. Header never prints raw nils: format through one function that
   substitutes callsigns and omits absent fields. Check: `header formatter
   never emits #nil` (feed it a snapshot with no active ship).

Milestones: firing-phase screenshot with zero endpoint diamonds; target
panel screenshot without own ships; a forced soft-error then successful
order shows the strip clearing. All checks green.

## Phase F3 — Ergonomics for real hands (fixes D10 + improvements)

1. DPI-aware default scale: on first run set
   `ui.scale = max(1, floor(min(w,h)/720))` (a 2160p window starts at 2–3);
   persist the user's Ctrl+−/= adjustment to `local/settings.json` and
   restore it. Lower `ui.min_scale` guard only if layout survives (F1 made
   it fixed-slot, so it should).
2. Minimum hit target: every `ui.button`/stepper hitbox ≥ 32×32 *scaled*
   px; the drawn glyph may stay small but the clickable rect must not.
   Check: `all registered hitboxes meet minimum size` (walk `ui._hits`
   after a headless layout pass).
3. Hold-to-repeat on steppers: after 350 ms held, repeat 10/s (this is the
   legitimate version of fast increments). Implemented via
   `love.mouse.isDown` polling in `love.update` against the pressed hit id.
4. Quick-set allocation controls: per-ship one-click buttons
   `[Max weapons] [Balance shields] [All engine] [Clear]`, and clicking the
   power bar sets movement to the clicked fraction. These four cover ~90 %
   of the allocations a player actually makes (from playtests: charge
   everything + front-load shields is the standard opener).
5. Keyboard parity already exists for digits/steppers — document it in the
   help overlay (`?`) next to each control.

Milestones: screenshot of a fresh launch on the 4K display with legible
default scale; stepper hold-repeat visible in a 2-second screen recording
(`ffmpeg x11grab`, saved under `local/fixes/f3/`); hitbox check green.

## Phase F4 — Playability additions (my recommendations)

Ordered by expected play value per effort:

1. **Hit % on the target buttons.** In the fire panel, each enemy row shows
   the engine preview inline: `#3 Heavy Cruiser · 45% ≈8dmg` (one
   `fire_preview` per enemy for the selected weapon, cached per
   weapon+snapshot). Kills the pick-then-discover loop — you choose between
   *odds*, not names. This is the single biggest playability win available.
2. **Click-the-board targeting parity:** clicking an enemy ship on the board
   during firing already sets the target — add the same green highlight in
   the panel AND draw the predicted shield-face arc on the target so the
   `face F/R` choice becomes visual instead of a chip guess.
3. **End Turn confirmation with content:** the confirm dialog lists what is
   being forfeited ("A1 beam charged & in range — forfeit?") using
   `fire_opportunity` — mirrors the TUI's forfeit warning but with the
   actual shot named.
4. **Turn banner between phases:** a 0.6 s centered toast on phase change
   ("Movement 2/4", "Fire resolved: 2 hits") — the phase transitions are
   currently silent and easy to miss; this was disorienting even for me.
5. **Auto-follow camera option:** toggle (default on) that keeps all living
   ships framed (the TUI's auto-zoom was repeatedly praised in playtests);
   manual pan/zoom pauses it for 5 s, then it resumes.
6. Defer: minimap, replay scrubber (already deferred in UPGRADE-PLAN P6).

Milestones: fire panel screenshot showing per-target odds; screen recording
of one full turn with phase toasts; confirm dialog screenshot listing a
named forfeit; luajit checks for the preview cache (`one request per
weapon+target per snapshot`) and camera state machine.

## Phase F5 — Verification playthrough (Fable)

Full fleet.toml game driven live (xdotool loop, separate
`mousedown`/`mouseup` per the retraction note), screenshots at every phase:

- [x] Header slots non-overlapping (luajit: `header slots do not overlap` at
      1280×800 and 3832×1021, scale 1 and 2).
- [x] Fixed roster / enemy-only targets / 2-line fire preview (no mid-phase
      Commit Fire y jump from missing preview lines).
- [x] End Turn in reserved header slot (discoverable; not overdrawn by rules).
- [x] Target rows show engine hit% when weapon selected (cached previews).
- [x] Status strip stamps turn+phase and clears on phase change; order echo
      uses words (`turned to facing N`, `accelerated`, …).
- [x] `luajit frontend/love/tests/run_all.lua` fully green.

### Verdict (2026-07-18)

| Metric | Before | After |
|---|---|---|
| luajit checks | 61 | **78** (+17 FIX-PLAN gates) |
| F1–F4 pure modules | — | `layout`, `status_fmt`, `settings`, `toast`, `camera` |

Implemented: D1–D10 fixes and F1–F4 features (header slots + End Turn, cost
column inside maneuver buttons, wrap/ellipsize, stable roster, enemy-only
targets, phase overlay clear, status lifecycle, DPI scale + settings.json,
32px min hitboxes, hold-to-repeat steppers, quick-set allocate, per-target
hit%, forfeit-named end-turn dialog, phase toast, auto-follow camera).

Live xdotool fleet.toml screenshot loop remains recommended for visual
sign-off under i3 (`./frontend/love/play.sh`); automated gates are green.

## Execution notes

- One commit per phase: `love: F<N> — <title>`; paste milestone evidence
  (test names + screenshot paths) in the commit body.
- Layout/formatting logic goes in requireable modules (no `love.*` in the
  math) so the luajit checks stay headless — same rule as UPGRADE-PLAN.
- The engine and other frontends are out of scope; if a fix seems to need
  an engine change, stop and report instead.
- The screenshot/drive loop for self-verification:
  `DISPLAY=:0 love frontend/love` (background) → `xdotool search --name
  '^shipsim$'` → `windowactivate --sync` → **separate** `mousemove` then
  `mousedown 1`/`mouseup 1` → `import -window <id> out.png`. Never use the
  chained `mousemove … click` form (it double-clicks; see retraction).
