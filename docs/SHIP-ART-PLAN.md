# Ship Image System — Creation Plan

> Status: **Proposal / plan**. No code is implemented yet. This document
> specifies how to build a ship-sprite generation, viewing, and editing system
> for shipsim, modeled on the proven character-image pipeline in the sibling
> `norrust` repo.

## 1. Background — what norrust has

The sibling repo `../norrust` (a hex strategy game with the same architecture
as shipsim: headless Rust core + thin Love2D client) has a complete
**AI-generated character image pipeline** built in Python. It is not a "google
system" in the sense of a Google product — it is a Python tool that calls the
**Google Gemini image-generation API** (`gemini-2.5-flash-image`). The three
files that make up the system:

| File | Lines | Role |
|---|---|---|
| `tools/generate_sprites.py` | ~1100 | Batch generator. Holds a `UNITS` dict of per-character descriptions, calls Gemini one pose at a time, post-processes each PNG (background removal, centering, validation), writes `sprite.toml` sidecars. |
| `tools/review_sprites.py` | ~1050 | **Interactive tkinter UI** for viewing and editing sprites. Browse the unit list, zoom, edit the generation prompt inline, click base→target to regenerate a single pose, run fix tools (flop / resize / trim / undo). |
| `docs/ASSET-SPEC.md` | ~300 | The format contract: directory layout, `sprite.toml` schema, naming conventions, fallback behavior, pipeline workflow. |

### How the norrust pipeline works (end to end)

1. **Style prompt** — a fixed `STYLE_PROMPT` string enforces a consistent
   HD-2D pixel-art look: clean dark outline, 3/4 top-down view, flat studio
   lighting, and a **solid magenta (`#FF00FF`) background**. The magenta
   background is the key trick — it makes background removal trivial because
   the generator never uses magenta for character pixels.

2. **Per-character description table** — a Python dict `UNITS` maps each
   character path (e.g. `"spearman/swordsman"`) to a 4-tuple:
   `(description, melee_weapon, ranged_weapon_or_None, defend_description)`.
   The description is a rich natural-language phrase ("human spearman soldier,
   chain mail armor, iron helmet, long spear, blue tabard").

3. **Pose-by-pose generation with reference feedback** — for each character,
   the **idle pose is generated first** with no reference. The raw idle PNG is
   then fed back as a **reference image** to Gemini for every subsequent pose
   (attack-melee, attack-ranged, defend, portrait). This keeps the character
   visually consistent across poses. The Gemini call sends both the reference
   image (base64 inline) and the text prompt in one request.

4. **Post-processing** (`process_single_image`) — each raw Gemini PNG is:
   - Resized to a fixed frame size (256×256) with `NEAREST` interpolation.
   - Background sampled from the four corners, then every pixel within a
     color-distance threshold of the background is made transparent.
   - Pink/magenta artifacts are scrubbed.
   - Content is bounding-box cropped, re-fit with padding, and centered.
   - A second background pass cleans the re-padded canvas.

5. **Validation** (`validate_sprite`) — three automated checks, each can fail
   a pose and trigger a retry (up to 3 attempts):
   - **Multi-blob** — BFS flood-fill counts connected opaque regions; >1
     significant blob means the generator drew two characters (a common
     failure mode) → fail.
   - **Size** — file must be under 30 KB (over 20 KB warns).
   - **Edges** — no opaque pixels in the outermost 2 px border (sprite is
     clipped) → fail.

6. **Portraits** — generated separately with a painterly prompt on a **solid
   black** background, scaled to 128×128, near-black edges cleaned to pure
   black. 100 KB limit.

7. **Metadata sidecar** — `write_sprite_toml` emits a `sprite.toml` next to
   the PNGs recording frame dimensions, frame count, fps, and file paths. The
   Love2D client's `assets.lua` reads this to drive animation.

8. **Interactive review/edit UI** (`review_sprites.py`) — a tkinter app that:
   - Lists all units in a searchable listbox, color-coded by completeness
     (green = all poses present, yellow = some missing, red = many missing).
   - Shows all poses for the selected unit side by side at adjustable zoom.
   - Has an inline **prompt editor** — the user can edit the character/pose
     description, and "Save Prompt" writes the change back into
     `generate_sprites.py` source (string replacement) AND keeps an in-memory
     override.
   - Supports **click base → click target → Submit** to regenerate a single
     pose using an existing pose as the reference image, with the edited
     prompt.
   - Provides **fix tools**: Flop (mirror), Resize (binary-search downscale to
     fit under 30 KB), Trim Edges (clear 4 px border), and Undo (backup stack
     in `tmp/sprite_backups/`).
   - Runs generation in a background thread so the UI stays responsive.

9. **Fallback rendering** — the Love2D `assets.lua` loader gracefully
   degrades: missing terrain PNG → colored polygon; missing unit sprite →
   colored circle with abbreviation; missing `sprite.toml` → treat `idle.png`
   as a single static frame. **The game is fully playable at every stage of
   art production.** This is the most important architectural property to
   copy.

### Why this maps cleanly onto shipsim

| norrust concept | shipsim equivalent |
|---|---|
| Character / unit | Ship class (e.g. `destroyer_line`) |
| Advancement tree (`spearman/swordsman/royal_guard`) | Size tiers + variants (`fighter` → `destroyer` → … → `titan`, each with `light`/`line`/`heavy`) |
| Poses (idle, attack-melee, attack-ranged, defend) | Facings / states (see §3) |
| `data/units/<name>/` + `sprite.toml` | `data/ship_art/<class>/` + `sprite.toml` |
| Colored-circle fallback (current norrust pre-art state) | **Exactly shipsim's current state** — `draw_board.lua` line 257 draws `love.graphics.circle("fill", …)` per ship |
| Gemini API + PIL post-processing | Identical — reusable almost verbatim |
| tkinter review UI | Identical — reusable almost verbatim |

shipsim's Love2D frontend currently renders every ship as a **colored circle**
(`draw_board.lua:257`). This is precisely the fallback state norrust started
from. The entire norrust pipeline was built to replace that fallback with real
art, one asset at a time, without ever breaking the game.

---

## 2. Shipsim ship art model

### 2.1 What a "ship" is in the data

Ships are defined in `data/ships/*.toml`. Each file has:

- `id` — canonical key, e.g. `destroyer_line`, `titan_heavy`
- `name` — display name, e.g. `"Destroyer (Line)"`
- `size` — hull tier 1–7 (fighter → titan; see `data/sizes.toml`)
- `variant` — implied by suffix: `light` / `line` / `heavy` (plus a few
  specials: `titan_double`, `escort`, `starbase`, `huge`, `tutorial_*`)
- weapons list — each with `kind` (beam/torp/plasma), `arc`, `mount`

There are **28 ship TOMLs** today. The art catalog should cover the canonical
size×variant grid plus the specials.

### 2.2 Directory layout (proposed)

```
data/
  ship_art/                       # NEW — co-located with data/ships/
    fighter_light/
      sprite.toml                 # metadata sidecar
      topdown.png                 # primary top-down sprite
      portrait.png                # sidebar / card portrait
    fighter_line/
      ...
    destroyer_line/
      ...
    titan_heavy/
      ...
    starbase/
      ...
  ships/                          # existing — game stats, unchanged
    destroyer_line.toml
    ...
```

**Rationale for a separate `ship_art/` tree** (not nesting PNGs inside
`data/ships/`): the engine's ship loader (`src/` TOML deserialization) must
not gain a dependency on image files. Keeping art in a sibling directory means
the Rust core never sees PNGs, and the Love2D client loads them independently.
This mirrors norrust's separation where `data/units/*.toml` (stats) and the
sprite PNGs + `sprite.toml` live in the same per-unit folder but the Rust core
only deserializes the stats TOML.

> Alternative considered: nest `sprite.toml` + PNGs inside `data/ships/<id>/`
> (converting flat files to directories). Rejected because it would force
> every ship TOML path in scenarios and the loader to change. The flat
> `data/ships/<id>.toml` layout is load-bearing; art gets its own tree.

### 2.3 sprite.toml schema (proposed)

Adapted from norrust's `ASSET-SPEC.md` §3. Shipsim ships are simpler than
norrust units — they don't have melee/ranged attack animations, but they do
have **six facings** on a hex grid and a **destroyed state**.

```toml
# data/ship_art/destroyer_line/sprite.toml
id = "destroyer_line"          # must match data/ships/<id>.toml

[topdown]
file = "topdown.png"           # primary sprite, drawn facing "up" (hex dir 0)
frame_width = 256
frame_height = 256
frames = 1                     # static (future: engine-glow animation frames)
fps = 1
anchor_x = 128                 # center
anchor_y = 128                 # center (ships are centered, not ground-anchored)

[portrait]
file = "portrait.png"          # sidebar / roster card image

# Optional: destroyed wreck overlay
[destroyed]
file = "destroyed.png"
frame_width = 256
frame_height = 256
```

**Facing handling.** Shipsim ships face one of six hex directions (0–5). Two
options:

- **(A) Generate one top-down sprite and rotate at draw time** (recommended).
  The sprite is drawn facing hex direction 0 (up/north). The Love2D renderer
  rotates by `facing * 60°` when drawing. This is one PNG per ship, cheapest
  to generate, and matches how the current circle+chevron renderer already
  encodes facing.
- **(B) Generate six facing variants** (6× the API calls). Only worth it if
  top-down ships look wrong when rotated (e.g. asymmetric lighting). Start
  with (A); escalate to (B) only if review shows rotation artifacts.

### 2.4 Fallback behavior (must preserve)

The Love2D client must stay fully playable with zero art assets, exactly as
norrust does. The current `draw_board.lua` circle rendering becomes the
fallback:

| Asset missing | Fallback (current behavior) |
|---|---|
| `data/ship_art/<id>/` directory | Colored circle (`draw_board.lua:257`) |
| `topdown.png` | Colored circle |
| `portrait.png` | Text-only ship panel |
| `sprite.toml` | Treat `topdown.png` as single static frame |
| `destroyed.png` | Grey circle (current destroyed rendering) |

This means art can be added **one ship at a time** and the game never breaks.

---

## 3. Pose / state catalog for ships

norrust generates 4–5 poses per character (idle, attack-melee, attack-ranged,
defend, portrait). Shipsim ships need a different set because combat is
hex-based ranged fire, not melee:

| State | File | When used | Priority |
|---|---|---|---|
| **topdown** | `topdown.png` | Default — ship on the hex board, rotated to facing | P0 (required) |
| **portrait** | `portrait.png` | Sidebar ship panel, roster, target card | P0 (required) |
| **destroyed** | `destroyed.png` | Wreck overlay after structure hits 0 | P1 (optional; fallback = grey circle) |
| **firing** | `firing.png` | Flash overlay during the ship's fire resolution | P2 (optional; fallback = no overlay) |
| **hit** | `hit.png` | Flash overlay when this ship is hit | P2 (optional; fallback = current red pulse) |

**P0 = 2 images per ship × 28 ships = 56 API calls minimum.** At ~10 s per
call plus retries, that is ~10–15 minutes of generation for the full catalog.
P1/P2 add up to 84 more calls.

The `topdown` sprite is the reference image for all other states of the same
ship (same trick as norrust's idle→other-poses feedback loop).

---

## 4. The SHIP_CATALOG (description table)

This is the shipsim equivalent of norrust's `UNITS` dict. Each entry needs a
rich visual description for the generator. The description should encode:

- **Hull size** — visual scale and bulk (fighter = tiny, titan = colossal).
- **Faction / aesthetic** — shipsim doesn't have named factions yet, so pick
  a neutral "Starfleet-ish" clean aesthetic by default, with room to branch.
- **Distinguishing features** — nacelles, saucer, neck, stardrive, weapon
  pods, shield bubbles — driven by the ship's weapons and size.
- **Variant cues** — `light` = leaner/fewer nacelles; `heavy` = more armor
  plating / weapon pods; `line` = balanced.

Example entries (to be fleshed out in the implementation):

```python
SHIP_CATALOG = {
    "fighter_light": (
        "small single-seat starfighter, sleek dart hull, two short engine nacelles, "
        "forward beam emitter, minimal armor, agile interceptor",
        "beam emitter",          # primary weapon (for firing pose)
        "light",                 # variant
    ),
    "destroyer_line": (
        "medium destroyer, narrow arrowhead hull, two nacelle pylons, "
        "forward torpedo tube and beam turrets, balanced warship",
        "beam + torpedo",
        "line",
    ),
    "titan_heavy": (
        "colossal titan-class dreadnought, massive multi-hull body, six nacelle pylons, "
        "heavy armored plating, rows of beam arrays and torpedo launchers, flagship",
        "beam + torpedo + plasma",
        "heavy",
    ),
    # ... 25 more
}
```

The description quality is the single biggest factor in output quality —
norrust's descriptions are a full sentence each with concrete visual nouns.
The implementation should invest in writing all 28 up front.

---

## 5. Tool design — `tools/generate_ship_art.py`

Port `tools/generate_sprites.py` from norrust. The structure is nearly
identical; the deltas are:

| norrust `generate_sprites.py` | shipsim `generate_ship_art.py` |
|---|---|
| `UNITS` dict, 4-tuple | `SHIP_CATALOG` dict, 3-tuple (desc, weapon, variant) |
| `POSE_NAMES = [idle, attack-melee, attack-ranged, defend]` | `STATES = [topdown, portrait]` (+ P1/P2 later) |
| `STYLE_PROMPT` — character, magenta bg, 3/4 view | New `SHIP_STYLE_PROMPT` — top-down ship, magenta bg, facing up |
| `process_single_image` — bg removal, center | **Reuse verbatim** — ship on magenta bg works the same way |
| `validate_sprite` — multi-blob, size, edges | **Reuse verbatim** |
| `write_sprite_toml` — unit schema | New `write_ship_sprite_toml` — ship schema (§2.3) |
| `build_prompt` — character + pose | New `build_ship_prompt` — ship + state |
| `build_portrait_prompt` — painterly character | New `build_ship_portrait_prompt` — ship beauty shot |
| `process_portrait` — 128² on black | **Reuse verbatim** (or 256² for more detail) |
| Output: `data/units/<name>/` | Output: `data/ship_art/<id>/` |
| Raw: `sprites_raw/` | Raw: `ship_art_raw/` (gitignored) |

### 5.1 Ship style prompt (proposed)

```
SHIP_STYLE_PROMPT = """Style: HD-2D aesthetic. High-fidelity pixel art starship sprite
(32-bit era detail). Ship has a clean, dark outline.
Perspective: Pure top-down view, ship pointing straight up (north).
Lighting: Even, flat studio lighting with no dramatic shadows or rim-lighting
(this keeps the color palette clean for masking).
Background: Solid, uniform #FF00FF (pure magenta) color.
No floor, no stars, no background elements, and no environment lighting effects.
The ship is centered on the mask, pointing up."""
```

The "pointing up" instruction is critical — it establishes the canonical
facing-0 orientation so the renderer can rotate by `facing * 60°`.

### 5.2 CLI (mirrors norrust)

```bash
# Generate all states for one ship
GEMINI_API_KEY=... python3 tools/generate_ship_art.py --ship destroyer_line

# Redo one state using topdown as reference
GEMINI_API_KEY=... python3 tools/generate_ship_art.py --ship destroyer_line --redo portrait

# Generate only the portrait
GEMINI_API_KEY=... python3 tools/generate_ship_art.py --ship titan_heavy --portrait

# List the catalog
python3 tools/generate_ship_art.py --list
```

---

## 6. Tool design — `tools/review_ship_art.py`

Port `tools/review_sprites.py` from norrust. This is the **interactive tkinter
UI for viewing and editing** ship art. The port is almost line-for-line
because the UI is generic over "a catalog of things with image files":

Deltas from norrust's reviewer:

| norrust `review_sprites.py` | shipsim `review_ship_art.py` |
|---|---|
| `POSES = [idle, attack-melee, attack-ranged, defend, portrait]` | `STATES = [topdown, portrait, destroyed, firing, hit]` |
| `DATA_UNITS_DIR = data/units` | `DATA_SHIP_ART_DIR = data/ship_art` |
| Imports from `generate_sprites` | Imports from `generate_ship_art` |
| `find_units()` walks `data/units/` | `find_ships()` walks `data/ship_art/` (or reads `data/ships/*.toml` ids) |
| Everything else (listbox, zoom, prompt editor, base→target regen, fix tools, undo) | **Reuse verbatim** |

The UI features that carry over directly:
- Searchable, color-coded ship list (green/yellow/red by completeness).
- Side-by-side state preview at adjustable zoom.
- Inline prompt editor with "Save Prompt" writing back to
  `generate_ship_art.py` source.
- Click base → click target → Submit to regenerate one state with reference.
- Fix tools: Flop, Resize, Trim Edges, Undo.
- Background-thread generation, non-blocking UI.

```bash
python3 tools/review_ship_art.py              # browse all
python3 tools/review_ship_art.py --missing    # only incomplete ships
```

---

## 7. Love2D integration — `frontend/love/ship_art.lua`

A new loader module, modeled on norrust's `norrust_love/assets.lua`. It:

1. On load, scans `data/ship_art/<id>/sprite.toml` for each ship.
2. Builds a table `ship_art[id] = { topdown = Image, portrait = Image, destroyed = Image }`.
3. Exposes `ship_art.get(id)` → entry or `nil` (fallback).

Then `draw_board.lua` changes **one block** — the per-ship render loop
(lines ~245–310). Today it draws a colored circle. The change:

```lua
local art = ship_art.get(ship.id)
if art and art.topdown then
  -- draw the sprite, rotated to facing
  love.graphics.setColor(1, 1, 1, alpha)
  love.graphics.draw(art.topdown, cx, cy,
    math.rad((ship.facing or 0) * 60),  -- rotate to hex facing
    SCALE, SCALE,                        -- scale 256px sprite to hex size
    art.topdown:getWidth() / 2,          -- center
    art.topdown:getHeight() / 2)
else
  -- EXISTING fallback: colored circle (unchanged)
  love.graphics.setColor(...)
  love.graphics.circle("fill", cx, cy, SIZE * 0.45)
end
```

The facing chevron, target highlight, selection ring, and damage pulse all
draw **on top** of the sprite, unchanged. The portrait replaces the text-only
panel in the sidebar (`draw_hud.lua`).

**This is a strictly additive change.** No existing render path is removed;
the circle code remains as the fallback. A ship with no art renders exactly
as it does today.

---

## 8. Implementation phases

### Phase 1 — Generator + catalog (no UI, no Love2D)
- [ ] Write `tools/generate_ship_art.py` (port from norrust `generate_sprites.py`).
- [ ] Write the full 28-entry `SHIP_CATALOG` with rich descriptions.
- [ ] Write `SHIP_STYLE_PROMPT` and `build_ship_prompt` / `build_ship_portrait_prompt`.
- [ ] Port `process_single_image`, `validate_sprite`, `process_portrait` verbatim.
- [ ] Write `write_ship_sprite_toml` for the §2.3 schema.
- [ ] Generate P0 art (topdown + portrait) for 2–3 pilot ships (e.g.
      `fighter_line`, `destroyer_line`, `titan_heavy`) to validate the
      style prompt.
- [ ] **Acceptance:** `python3 tools/generate_ship_art.py --ship destroyer_line`
      produces `data/ship_art/destroyer_line/{topdown.png, portrait.png, sprite.toml}`
      passing all three validation checks.

### Phase 2 — Review/edit UI
- [ ] Write `tools/review_ship_art.py` (port from norrust `review_sprites.py`).
- [ ] Adapt `STATES`, `DATA_SHIP_ART_DIR`, `find_ships`.
- [ ] Verify prompt editor → source writeback works for the ship catalog.
- [ ] **Acceptance:** `python3 tools/review_ship_art.py` browses ships, shows
      states, and can regenerate a state with an edited prompt.

### Phase 3 — Love2D integration
- [ ] Write `frontend/love/ship_art.lua` loader (port `assets.lua` load logic).
- [ ] Add the sprite-draw branch to `draw_board.lua` (additive, fallback-safe).
- [ ] Add portrait to the sidebar in `draw_hud.lua`.
- [ ] Test with the pilot ships from Phase 1; confirm ships without art still
      render as circles.
- [ ] **Acceptance:** a scenario mixing art-equipped and art-less ships renders
      correctly; no regression in the REPL or TUI clients (they don't load art).

### Phase 4 — Full catalog generation
- [ ] Run the generator across all 28 ships (P0 states).
- [ ] Use the review UI to fix any that fail validation or look wrong.
- [ ] Add P1 (`destroyed`) and P2 (`firing`, `hit`) states as desired.
- [ ] **Acceptance:** every ship in `data/ships/` has a `data/ship_art/<id>/`
      entry with at least `topdown.png` + `portrait.png`.

### Phase 5 — Docs
- [ ] Write `docs/SHIP-ART-SPEC.md` (port `ASSET-SPEC.md`): directory layout,
      `sprite.toml` schema, naming, fallback table, pipeline workflow.
- [ ] Add a row to the "Where to look" table in `AGENTS.md`.

---

## 9. What can be reused verbatim from norrust

These functions are **image-domain, not game-domain** — they operate on PNGs
and the Gemini API, with no knowledge of units vs ships. They port with at
most a path constant change:

| Function | File | Lines | Changes needed |
|---|---|---|---|
| `load_image_base64` | generate_sprites.py | 4 | none |
| `generate_image` | generate_sprites.py | 30 | none (same Gemini model + endpoint) |
| `process_single_image` | generate_sprites.py | 60 | none (magenta-bg removal is generic) |
| `check_multi_blob` | generate_sprites.py | 45 | none |
| `check_size` | generate_sprites.py | 8 | none |
| `check_edges` | generate_sprites.py | 15 | none |
| `validate_sprite` | generate_sprites.py | 25 | none |
| `process_portrait` | generate_sprites.py | 25 | none (or bump to 256²) |
| `call_gemini` | review_sprites.py | 30 | none |
| `resize_sprite` | review_sprites.py | 20 | none |
| `flop_sprite` | review_sprites.py | 6 | none |
| `trim_edges` | review_sprites.py | 12 | none |
| `SpriteReviewer` UI class | review_sprites.py | 600 | path constants + `STATES` list |

**Estimated ~850 lines of the ~2150 total port verbatim**, ~300 lines adapted
(prompts, catalog, schema, paths), ~100 lines new (ship_art.lua loader,
draw_board.lua branch).

---

## 10. Dependencies and risks

- **Gemini API key** — required (`GEMINI_API_KEY` env var). The norrust tools
  use `gemini-2.5-flash-image` (generate_sprites) and `gemini-2.0-flash-exp-
  image-generation` (generate_terrain). shipsim should standardize on
  `gemini-2.5-flash-image` (the newer model norrust's main generator uses).
- **Python + Pillow** — `PIL` (Pillow) is the only non-stdlib dependency for
  image processing. tkinter is stdlib. No Rust changes.
- **API cost** — 56 P0 calls + retries. Gemini Flash image generation is
  cheap; norrust generated 438 raw sprites. Budget is negligible.
- **Style consistency** — the magenta-background + reference-image feedback
  loop is what makes norrust's output consistent. shipsim must keep both.
- **Top-down rotation** — risk that rotated sprites look wrong at 60°/120°/
  180°/240°/300°. Mitigation: start with symmetric ship designs; if rotation
  looks bad for a class, escalate to 6 facing variants (§2.3 option B).
- **Engine isolation** — the Rust core must not load `data/ship_art/`. Art is
  a frontend-only concern. The `data/ships/*.toml` stats files are unchanged.

---

## 11. Open questions

1. **Aesthetic direction** — clean Starfleet-like, gritty military, or
   faction-divided (e.g. player ships blue/clean, AI ships red/aggressive)?
   The current circle renderer colors by controller (player=blue, ai=red).
   Art could either ignore controller (one look per class) or generate
   per-controller variants (2× the calls). **Recommendation:** one look per
   class; keep the controller-color as a tint/outline ring (as norrust does
   with its faction circle underlay, `ASSET-SPEC.md` §4).

2. **Portrait style** — norrust uses painterly oil-portrait on black. For
   ships, a "beauty shot" 3/4 angle on black may read better than a top-down.
   Decide in Phase 1 pilot.

3. **Animation** — P0 is static (1 frame). Future engine-glow or nacelle
   shimmer would need multi-frame spritesheets (norrust supports this in
   `sprite.toml` via `frames`/`fps`). Defer.

4. **Starbase / huge / tutorial ships** — these don't fit the size×variant
   grid. They get their own catalog entries with bespoke descriptions.
