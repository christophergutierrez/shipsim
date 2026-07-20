# Love2D upgrade review verdict

> **SUPERSEDED (2026-07-19).** Verdict on the 2026-07-17/18 Love upgrade cycle.
> Cited line numbers and some FAIL items were fixed later (e.g. fan basis,
> `love.resize`, Active-ship nil). Do **not** re-open work from this file.
>
> **Current client:** [`README.md`](README.md) · **Later findings:**
> [`REVIEW-FINDINGS.md`](REVIEW-FINDINGS.md)

**Reviewed:** 2026-07-18

## Verdict

**Conditional pass.** The upgrade satisfies its automated protocol,
integration, and tutorial milestones. Phase 4 is **failed** pending the
coordinate-basis correction listed below: its rendered weapon fans and shield
segments do not align with the axial board directions or with a ship's facing.

The required host-display review could not run in this sandbox: `DISPLAY=:0`
is present but rejects `xdotool` and ImageMagick `import`, and no `Xvfb` is
installed. No screenshot evidence is claimed. The Phase 2 visual screenshots
must be captured and spot-checked in a display-capable session after the fixes.

## Milestones

| Phase | Result | Evidence |
|---|---|---|
| −1, interactive harness | PASS | `cargo build -q`; all 61 `run_all.lua` checks pass; every new module requires under plain LuaJIT. |
| 0, protocol and event data | PASS | `fire_opportunity`, rules provenance, and event-ring checks pass; `orders.lua` has no request envelope. |
| 1, authoritative previews | PASS | fire-preview and maneuver-options checks pass; display legality is not used to submit orders; `draw_board.lua` makes no requests. |
| 2, CTA and focus recovery | PASS | selection and CTA tests pass; destroyed-ship guards cover all order paths in `main.lua`. |
| 3, event feedback | PASS (automated) | FX, ticker, and block-callout checks pass. Host visual/FPS check is unverified. |
| 4, board visualization | FAIL | Arc-fan and shield-ring angles are rotated relative to `hex.to_pixel`; see F1. Other pure checks pass. |
| 5, resolution and game over | PASS (automated) | structured-event game-over stats, slide, and FX tracer checks pass. Host visual check is unverified. |
| 6, tutorial | PASS (automated) | all 26 tutorial titles exactly match the TUI; full walkthrough completes. Host visual/gate check is unverified. |

## Findings

1. **P1 - Correctness: rendered arcs and shields do not face the same direction as the board.**
   [`geom.lua`](geom.lua) (near the facing→angle helper) treats facing 0 as 0
   degrees and increases it by 60 degrees. On the actual board, `hex.to_pixel`
   maps the six core facing vectors to 30, -30, -90, -150, 150, and 90 degrees.
   Thus a facing-0 fan points right while the facing tick points down-right;
   each other fan is similarly displaced or mirrored.
   [`draw_board.lua`](draw_board.lua) independently repeats the wrong angle
   basis for shield segments and also fails to rotate those relative
   F/FR/RR/R/RL/FL faces by the ship's facing. This is display-only, so engine
   legality remains authoritative, but the visual is actively misleading.
   **Fix in P2.**

2. **P2 - UX: allocate header prints `Active #nil`.**
   [`draw_hud.lua`](draw_hud.lua) obtains an active ship only in movement,
   then formats it in every phase. **Fix in
   P2.**

3. **P2 - Layout: the header and rules-provenance label own the same top-right
   pixels.** [`draw_hud.lua`](draw_hud.lua) prints an unbounded header while
   independently painting the same row from the right edge. Long call-to-action
   text overlaps provenance. **Fix in P2.**

4. **P2 - Layout: resizing does not re-center the board.**
   [`main.lua`](main.lua) computes the appropriate centered camera only on
   scenario load; there is no `love.resize` hook. A maximized/wide window thus
   retains its 1280-pixel camera coordinates and leaves content pinned to the
   upper-left. **Fix in P2.**

## Existing uncommitted Love changes

The four existing modifications add coherent Exit/Q controls and documentation:
they separate "Scenarios" from application exit, allow quitting from the
tutorial, picker, and game-over screen, and update `play.sh` guidance. They do
not look like accidental edits, but they have no dedicated tests and touch
`draw_hud.lua` and `main.lua`, which P2 also changes. Preserve them as
user-owned work; do not silently fold them into a cleanup commit.

## Automated evidence

```
cargo build -q                              PASS
luajit frontend/love/tests/run_all.lua      PASS (61 checks)
tutorial title diff                         PASS (no output)
grep '"request"' frontend/love/orders.lua   PASS (no output)
grep movement_preview frontend/love/draw_board.lua  PASS (no output)
```
