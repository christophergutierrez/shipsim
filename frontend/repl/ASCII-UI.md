# ASCII / terminal UI notes (shipsim REPL)

Working notes for making the **text client** readable and playable. Terminal
starship combat is uncommon; roguelike and classic TUI craft transfer well, but
this file is **shipsim-specific** — not a generic roguelike tutorial.

Scope: `frontend/repl/` only. The Rust engine remains the rules authority
(`docs/PROTOCOL.md`, `docs/ARCHITECTURE.md`). Isolation rules: `frontend/README.md`.

---

## 1. Architecture first (non-negotiable)

### Model / view split

```
shipsim binary  --NDJSON snapshot/order-->  REPL (view + input only)
```

- **Model**: `StateSnapshot` and soft errors from the harness. Never recompute
  hit chance, damage, legality, or AI in the client.
- **View**: `view.py`, `style.py`, map glyphs, panels, bars, combat banners.
- **Input**: `commands.py` / `repl.py` build orders; the engine accepts or soft-rejects.

If a future client (Love, ratatui, tcod, web) appears, it should consume the
**same** snapshot stream. Improving ASCII presentation must not fork rules.

### Display-geometry boundary

`hexutil.py` mirrors core hex/arc **geometry** (distance, bearings, legal
shield facings) for display hints only — the engine still validates every
order. That mirror is the ceiling for client-side computation:

- **Allowed in the client:** pure geometry (range, bearing, which shield faces
  are geometry-legal), and *reprinting* the frozen tables from
  `docs/combat-v2-tables.md` as reference text.
- **Not allowed:** computing to-hit odds, expected damage, or order legality
  and presenting them as authoritative. If the fire UI should show odds or a
  "this shot would round to 0 damage" pre-warning, that data must arrive in
  the snapshot or soft error — extend the protocol, don't fork the math into
  Python.

### Coordinate truth

- Core uses **axial** hexes (`src/hex.rs`) — same family as
  [Red Blob Games — Hexagonal Grids](https://www.redblobgames.com/grids/hexagons/).
- Display may stagger rows/columns for legibility; it must not invent a second
  rules coordinate system.
- Facing is always **0..5** with the same direction table as the core.
- Shield faces **0..5** are **ship-relative** (0 = that ship’s forward), not map
  absolute. Do not conflate map facing with shield index without converting.

### Freshness

Always paint from the **latest** snapshot after each accepted order.

- Prefer a short **Δ line** (phase, hull, shield rem, weapon FIRED) so change is
  obvious even when the full panel is long.
- Combat log is cleared on `EndTurn`. Any “new events” cursor must **reset when
  the log shrinks**, or later turns will silently drop HIT/MISS banners.
- Charge after fire is **0** and `fired` is true until turn reset. Never show a
  spent weapon as charged because of draft state or stale UI.

---

## 2. What “good ASCII” means here

Borrowing from the Cogmind / Brogue school (Josh Ge / gridsagegames, Brogue
minimalism), not from dumping every color on every line.

### Restraint in color

- Palette size: about **8–16** semantic roles, not rainbow decoration.
- Color means information: player vs enemy, focus, HIT vs MISS, warnings, dead.
- Background changes (if used later) should mean highlight/threat/terrain — not
  chrome.
- Honor **`NO_COLOR`** and `SHIPSIM_REPL_COLOR=0`. Monochrome must remain fully
  playable (glyphs and bars carry the load).

Implementation today: `style.py` (`paint`, `panel`, hit/miss/player/enemy helpers).

Current semantic roles — the enforceable version of "8–16 roles". Keep this
table in sync with `style.py` when adding a helper:

| Role | Helper | Style | Means |
|---|---|---|---|
| player | `player` | cyan | player callsigns/ships |
| enemy | `enemy` | yellow | AI / scripted callsigns |
| focus | `focus` | bold bright_cyan | UI focus ship |
| active | `active` | bold bright_white | Next pending maneuver; phase name |
| hit | `hit` | bold bright_red | HIT banners, hull damage |
| miss | `miss` | dim yellow | MISS results |
| fired | `fired` | bold yellow | resolved weapon marker |
| queued | `queued` | bold bright_yellow | committed weapon awaiting resolution |
| available | `available` | bright_cyan | charged weapon available to commit |
| dead | `dead` | dim red | destroyed ship / inoperable weapon box |
| ok | `ok` | bright_green | accepted orders; `Won` |
| warn | `warn` | bright_yellow | leftover-power ⚠, confirms |
| err | `err` | bold bright_red | soft errors; `Lost` |
| muted | `muted` | dim gray | empty hexes, debug/queue lines |

`hit` and `err` deliberately share bold bright red — both mean "something bad
just happened." Do not extend that style to a third meaning, and do not add a
new color for decoration; a new entry here needs a new *meaning*.

### Glyph semantics

| Glyph / pattern | Meaning (keep stable) |
|---|---|
| Map cell `A1→` | Callsign (side letter + id) + facing arrow (= forward) |
| Map cell `····` | Empty hex (muted; four printable columns) |
| Map cell ` x  ` | Destroyed ship wreck (muted; living ship takes precedence) |
| `@` / `*` in lists | Focus / next pending maneuver |
| `[####....]` | Quantity bar (power, charge, hull, shield rem) |
| `CHG n/m (available)` | Charged and available for the current volley |
| `QUEUED →#t` | Local volley draft contains this shot; submit with `ready` |
| `FIRED HIT/MISS` | Phase resolved; charge spent (`chg=0`) |
| `DEAD` | Weapon box gone |
| `←` on a shield row | That face is relevant to the observer (e.g. facing you) |
| `arc=… rng≤…` on a weapon line | Mount arc + max range (engine data, frozen tables) |
| `rel bearing: N` | Direction to focus target relative to your nose (0 = F) |
| `status=Won` / `status=Lost` | Endgame; painted ok / err in the header |
| `committed=[…] pending=[…]` | Path or volley commitments for the current phase (muted) |

Prompt fragments (`*1`, `draft11/22`, `/ready`, …) have their canonical table
in `GAMEPLAY.md` §7 — update both together when the prompt language changes.

One glyph family → one meaning. Do not reuse bright red for both “cosmetic
header” and “you just took hull damage.”

### Bars

- Bars are **the** power/health language of this client.
- Prefer a fixed scale that still moves when values change:
  - Hull: remaining / session max (or class max if known).
  - Shields: **remaining vs max_shield_per_facing**, and also print `pwr=` so
    allocation is visible when rem is 0.
  - Weapons: charge vs max_charge; after fire, empty bar + **FIRED**, not a full
    charge bar.
- During allocate **draft**, bars are local until `commit`; label the UI so the
  player knows nothing hit the engine yet.

### Panels and whitespace

- Box-drawing panels (`┌ │ └`) for YOUR SHIP, CONTACTS, MAP, FIRE RESOLUTION.
- Cramped walls of numbers feel like a debug dump; panels + short headers feel
  like a game.
- Prefer one tactical “page” after an order over streaming unframed dumps.

### Animation / “juice” (optional, view-only)

Cheap and high-impact when we add them later:

- Loud HIT/MISS banner and one-line Δ after every order — implemented in
  `view.py` (banner + Δ helpers); status lives in code, not this sentence.
- Later: brief color flash on the map cell that was hit; multi-step projectile
  trace on the ASCII map; message-log panel with last N events.

Never require animation for correctness. Never put timing into the engine.

### Terminal geometry

Design target is a **plain 80×24 terminal**: panels use a soft ~72-col rule
width (`style.panel`), and the default play frame (header + YOUR SHIP +
CONTACTS + MAP + RECENT + prompt) should fit without scrolling for current
scenarios.

- Never let a line hard-wrap mid-bar or mid-map-row; shorten labels or
  truncate before that happens (ANSI codes make wrapped lines lie about
  length).
- Maps wider than the terminal, or frames taller than it, have **no viewport
  strategy yet**. Treat that as a known limit of the current design: when a
  scenario outgrows 80×24, the answer is a deliberate viewport/scroll design,
  not per-panel improvisation.

---

## 3. Hex map on a character grid

Three practical options (community standard):

1. **Offset trick** (what we use) — one cell per hex, odd rows indented, often
   double-width (`id` + facing). Cheap, readable, good for tactical maps.
2. **Drawn hex borders** (`/ \ _`) — pretty, expensive in screen space; fine for
   maps ≲15 hexes across if we ever want a “pretty board” mode.
3. **Braille / half-block subpixels** — usually overkill; hurts ASCII clarity.

**shipsim choice:** (1) odd-r stagger + double-width cells in `view.format_board`.
Keep axial labels (q, r) available for debugging; do not force players to think
in cube coords.

Map legend must match **screen axes** (q right, r down). Facing 0 is **+q (→)**,
not “up”. Wrong arrows train the player that forward is north — never reintroduce
that. Legend: **0→ 1↗ 2↖ 3← 4↙ 5↘**. On this map, port turns toward ↗ and
starboard turns toward ↘.

### Callsigns / sides

Until scenarios carry real fleet/side ids, group by controller:

| Letter | Side |
|---|---|
| **A** | player (controllable) |
| **B** | ai |
| **C** | scripted |

Callsign = letter + ship id (`A1`, `B2`). Same letter = same side. When fleets
land, replace letters with scenario-provided side codes — do not invent rules in
the client beyond presentation.

### Absolute move vs engine orders

The engine only accepts relative orders: forward / reverse / turn port /
starboard. Each is **one decision** per ship per movement phase.

Absolute `m 0..5` is a client convenience that emits **at most one** order:
turn toward the dir, **or** forward/reverse if already aligned. Never batch
turn+step in one command — that ends the ship’s movement after the first order
while the client keeps sending moves into the fire phase (regression).

---

## 4. Interaction design (text client)

### Ship-centric flow

- Select a **focus ship** once; subsequent allocate/fire defaults to it.
- Movement commitments are simultaneous. If focus differs from the next pending ship, say so and
  move the active ship.
- Allocate: **pick ship → local draft → commit**. Do not require retyping the
  ship id on every `mov` / `w` / `sh`.

### Grouped commands with shortcuts

Enter a group, then short tokens:

```
w          # list weapons with shortcuts
t1 1       # torp_1
b1 2       # beam_1
done
```

Also allow one-shot forms: `w t1 1`, bare `t1 1` when unambiguous.

Shields: `sh` then `0 3` / `F 2`. Facing numbers are universal 0..5.

### Targeting decision-support (firing)

Firing is the range-math phase: to-hit falls steeply with range and beam
damage is `half_up(charge × factor)` (`docs/combat-v2-tables.md`), so "is this
shot worth committing?" is *the* decision the UI must support. Carry as much
of that as the §1 boundary allows:

- Always show **range** next to targets and each weapon's `rng≤` cap in the
  weapon menu (both exist today). Out-of-range and out-of-arc targets are now
  marked up front with `[OUT OF RANGE]` / `[OUT OF ARC]` / `[in arc]` flags, so
  the soft error is no longer the only feedback.
- Geometry-legal shield facings come from `hexutil.legal_shield_facings` — a
  display mirror; the engine still validates the commit.
- To-hit odds and expected damage are **engine data**, not client math. Until
  the snapshot carries them, the sanctioned fallback is reprinting the frozen
  tables as reference text (e.g. a future `tables` command) — presentation of
  docs, not recomputation.

### Threat display (allocate)

Contacts show which of *their* shields face you — that serves offense. The
defensive question during allocate is the inverse: **which of my faces do
enemies currently bear on?** `hexutil.threats_to_ship` now answers this from
snapshot fields + pure geometry: the tactical view shows a **THREATS** panel
listing each enemy ship + weapon that can bear on the focus ship, with range.
The ship card also prints `rel bearing` vs the focus contact. Future work:
mark threatened faces directly in the shield **draft** bars so the player
knows where to stack power without doing hex math in their head.

### Maneuver commitments

Movement and firing are simultaneous: each living ship commits one full path,
then one full volley. The display shows short callsigns in muted
`committed=[…] pending=[…]` lists for whichever collection stage is active.

### Endgame

Snapshot `status` is `InProgress` / `Won` / `Lost`; the three interactive
phases are `allocate`, `movement`, and `firing`.

- `Won` paints ok-green, `Lost` paints err-red in the header
  (`view.format_header`). On the transition, be at least as loud as FIRE
  RESOLUTION — the game is over; don't let the player keep typing orders into
  soft errors without noticing.
- After endgame the hint should offer `quit` (and, later, rematch/scenario
  select) and repeat the session-log path.
- `turn_end` is a real protocol phase; the prompt and hint must name it rather
  than showing a stale `firing`.

### Phase prompts

- Prompt should encode turn, phase, focus, active (movement), draft used/free.
- On phase change, print a **one-line hint** (what legal verbs are).
- `end` ends the **whole turn**, not the fire phase — warn in firing.
  Leaving fire without shots is `ready` / `nofire`.

### History

Readline history under `local/history` is not optional polish; typos are
constant in tactical UIs. Keep it.

---

## 5. What not to do

- **Do not** reimplement combat tables, LOS, or AI in Python “for nicer UX.”
- **Do not** write session junk outside `frontend/repl/local/`.
- **Do not** couple Love and REPL; share protocol docs only.
- **Do not** treat golden engine fixtures as free to break for UI experiments
  without regenerating fixtures intentionally.
- **Do not** hide soft errors; illegal orders are part of learning the game.
- **Do not** make color required for legibility.
- **Do not** grow a second rules coordinate system “because ASCII looks better.”

---

## 6. Libraries (when / if)

| Stack | Role for shipsim |
|---|---|
| **stdlib Python** (current) | Playable REPL, no deps, easy for agents |
| **ratatui** (Rust) | Full-screen TUI later under e.g. `frontend/tui/` |
| **tcod** / BearLibTerminal | Tileset swap path; still a thin client |
| **drawille** etc. | Generally skip unless we deliberately want a pixel-ish mode |

A stack change is a **new directory tree**, not a rewrite of the core. Same
snapshot contract.

---

## 7. External references (for humans, not dependencies)

- [Red Blob Games — Hexagonal Grids](https://www.redblobgames.com/grids/hexagons/) — axial/cube, distance, neighbors, rotation.
- Josh Ge / Cogmind design writing (gridsagegames.com/blog) — ASCII hierarchy, color restraint, terminal “juice.”
- Brogue — minimalism + gradients as an aesthetic ceiling for sparse UIs.
- r/roguelikedev FAQ Fridays / tutorial archives — message logs, targeting, map UI patterns.
- [NO_COLOR](https://no-color.org/) — disable ANSI when requested.

---

## 8. Checklist for REPL UI changes

Before merging presentation work:

1. Still **view-only**? (no new rules in Python — geometry mirror ceiling per §1)
2. Still works with **`NO_COLOR=1`**?
3. Snapshot fields painted **after** the order that changed them?
4. Combat log cursor correct across **EndTurn**?
5. Weapons after fire show **FIRED**, not leftover charge?
6. Shield/hull bars **move** when rem/structure change?
7. Scratch files only under **`frontend/repl/local/`**?
8. README / this file updated if commands or glyph meanings changed?
9. Scaled bars use `format_bar` (always `filled/total`), not bare `bar` + lone number?
10. Play mode does not double-paint at launch; alternate screen avoids scrollback stack?
11. New visual bugs become invariants in `screen_audit.py` (not only rubric prose)?
9. Absolute `m N` still emits exactly **one** engine order (past regression)?
10. Fire menu still opens **once per phase entry** — no auto-reopen after `r`?
11. New markers/colors added to the **glyph and palette tables** in §2?
12. Play frame still fits **80×24** with no mid-bar / mid-map wrapping?

---

## 9. File map (presentation)

| File | Responsibility |
|---|---|
| `view.py` | Snapshot → tactical text, map, combat banner, Δ lines |
| `style.py` | Palette, panels, NO_COLOR |
| `hexutil.py` | Display geometry (bearings, bars, aliases for facing) |
| `commands.py` | Input language, allocate draft, order construction |
| `repl.py` | Session loop, readline, order send, when to repaint |
| `screen.py` | Play-frame clear/redraw, RECENT/log panel, `--debug` transcript |
| `client.py` | Harness subprocess only |

### Play frame vs scroll log

- **Default play mode:** fixed frame — snapshot (map, callsigns, shield/weapon bars)
  redraws from the latest engine state after every order so damage and depletion
  show without scrolling the world away. Short **RECENT** strip; `log` toggles
  longer history.
- **Session file (default on):** tee history + frames to
  `frontend/repl/local/session-*.log`. Override with `--log-file`; off with
  `--no-session-log`.
- **`--debug`:** verbose file transcript (timestamps + ORDER JSON), not a different UI.
- **`--scroll`:** classic append-only on-screen log.

When in doubt: **make the snapshot obvious**, then make typing shorter — never
the reverse.
