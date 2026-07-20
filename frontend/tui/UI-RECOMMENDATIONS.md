# shipsim TUI — UI recommendations

> **Partially historical (2026-07-14).** Prefer [`README.md`](README.md) for
> current protocol-v4 keys and orders.

**Date:** 2026-07-14  
**Scope:** ratatui capabilities research, live rear-attack tutorial play
(`cargo run --manifest-path frontend/tui/Cargo.toml -- --tutorial`), and
code review of `frontend/tui/src/{ui,input,tutorial,app}.rs`.  
**Crate:** ratatui **0.29** (see `Cargo.toml`).

This is a recommendation / backlog note, not a PR plan. Priorities are
subjective; P0 items are correctness or “player stuck” issues.

---

## 1. What ratatui can do (relevant subset)

### 1.1 Core model

| Concept | Role |
|--------|------|
| **`Terminal` + `Frame`** | Each draw: `terminal.draw(\|f\| …)`. `Frame` is the only safe way to paint for that frame. |
| **`Buffer` / `Rect`** | Character grid; widgets paint into a `Rect`. Origin top-left. |
| **`Layout` + `Constraint`** | Split a `Rect` into children (vertical/horizontal). Cassowary solver; over-constrained layouts get a best-effort split (non-deterministic edge cases). |
| **`Flex`** | How leftover space is distributed: `Legacy`, `Start`, `End`, `Center`, `SpaceBetween`, `SpaceAround`. We only use default/Legacy-style splits. |
| **`Constraint::{Length,Min,Max,Percentage,Ratio,Fill}`** | Size policy. Prefer nested layouts over one flat constraint soup. |
| **`Block` + `Borders` + titles** | Panel chrome. Title can be multi-span / alignment (left/center/right on recent APIs). |
| **`Widget` / `StatefulWidget`** | Render traits. Stateful widgets keep selection/scroll outside the ephemeral widget (e.g. `ListState`, `TableState`). |
| **`Clear`** | Wipe an area before drawing a modal over stale content. |

Docs: [Widgets](https://ratatui.rs/concepts/widgets/), [Layout](https://ratatui.rs/concepts/layout/), [Showcase](https://ratatui.rs/showcase/widgets/).

### 1.2 Built-in widgets we underuse

| Widget | Typical use for shipsim |
|--------|-------------------------|
| **`Paragraph`** | ✅ Primary content (we use heavily). |
| **`List` + `ListState`** | ✅ Events/log (no selection state). Ideal for allocate fields, weapons, targets with **highlight + scroll**. |
| **`Table` + `TableState`** | Allocate form: rows = Movement / each weapon / each shield face; columns = name, value, max, note. |
| **`Gauge` / `LineGauge`** | Power spent vs pool; per-face shield fill; hull bar; charge bars. |
| **`Tabs`** | Ship focus or reference panes (Combat log / Help / Motion legend) without losing the map. |
| **`Scrollbar`** | Long allocate lists, combat history, coach text. |
| **`Canvas`** | Hex map with true per-cell styling (ships, range rings, course rays). |
| **`Sparkline`** | Optional: speed/thrust trend across cycles (low priority). |
| **`Chart` / `BarChart`** | Balance/debug only — not free-play. |
| **`Calendar` / `Logo`** | Not relevant. |

### 1.3 Patterns we are not using yet

1. **`StatefulWidget` for forms** — Allocate is a hand-rolled ▶ + scroll math (`allocate_scroll`). A `List`/`Table` with state would handle “keep selection visible” correctly.
2. **`Clear` + modal layer** — Confirm quit/end-turn is only a header line; a centered `Block` over a `Clear`ed area is the ratatui idiom for dialogs.
3. **`Layout::horizontal` / `vertical` helpers + `.flex()` + `.spacing()`** — Cleaner than always `Layout::default().direction(…)`.
4. **`Block::title` with styled spans** — Phase color in the title; “over budget” in red without eating content lines.
5. **Per-cell `Span` composition on the map** — See bug §3.1: we currently **drop** map colors.
6. **Root `impl Widget for &App`** — Compositional pattern from ratatui docs; optional refactor for testability, not player-facing.
7. **Third-party** (Awesome Ratatui): tui-textarea, popup helpers — only if typed `:` command line lands.

### 1.4 What we already do well

- Snapshot-driven pure render path (good seam).
- Soft engine errors surface in the header.
- Terminal floor with pause message (`MIN_WIDTH`/`MIN_HEIGHT`) instead of a broken partial layout.
- Tutorial yellow **why · keys** line (header) + longer coach text (side panel).
- Power budget free/over coloring on allocate.
- Confirmation for quit / end-turn with open work.

---

## 2. Playing the rear-attack tutorial — UX notes

Played the gated path conceptually via step list + harness smoke (`tutorial_rear_attack_wins_against_engine`). Live feel issues below are from that path and prior live sessions.

### 2.1 What works

- Yellow bar is scannable; “why first” is the right structure.
- Gate prevents random phase-breaking keys once the player understands yellow = law.
- Field labels in `do_now_line` (e.g. `shield F (forward)`) help when present.
- Three-turn victory is a good length vs the old five-turn REPL grind.

### 2.2 Friction (interface)

| # | Issue | Recommendation |
|---|--------|----------------|
| F1 | **Allocate looks like a text dump**, not a form. ▶ is easy to miss among raw strings. | `Table` or stateful `List`: selected row inverted; columns Name / Value / Max / Note. |
| F2 | **Shield faces are abstract** (F/FR/…) without a tiny diagram. | One-line compass in allocate: `  FL F FR` / `  RL · RR` / `  · R ·` with current values; highlight selected face. Or Canvas arc. |
| F3 | **Map does not show course vs facing** separately (velocity arrow vs nose). Critical for reverse-thrust lesson. | Two glyphs: face `→` and course `.` or second character; legend “nose / course”. |
| F4 | **No range ring / “behind” cue** on map when tutorial cares about stern geometry. | Dim highlight of target hex + optional line of bearing (Canvas or Span). |
| F5 | **Coach vs yellow bar compete** for attention; short coach steps (see wording) feel empty. | When coach `text` is short, expand with context or hide coach and only use yellow + form annotations. |
| F6 | **56 micro-steps** (nav field, then set value) feel like busywork on T2/T3 shield rebuilds. | Collapse “select F + set 6” into one step if already navigable; or auto-jump cursor to the field the step needs. |
| F7 | **Movement help lists both `0-5` and “digits: set facing”** — redundant / slightly contradictory. | One line: `0–5: turn to facing`. |
| F8 | **Fire panel** does not scream “queued, not resolved until Space”. | Pending-fire strip (like REPL RECENT) with `List` of commits. |
| F9 | **Header height fixed** while stacking ERROR + confirm + yellow + status can clip. | Dynamic header `Constraint::Length(n)` from line count, or modal for confirm. |
| F10 | **Events/log squeezed** in tutorial layout (`Length(5)`). | Tabs: Coach \| Log \| Events; or only show combat when non-empty. |

### 2.3 What ratatui features map cleanly onto fixes

| Friction | Ratatui tool |
|----------|----------------|
| F1 form | `Table` + `TableState` |
| F2 shield diagram | `Paragraph` mini-art or `Canvas` |
| F3–F4 map | `Canvas` or `Line` of styled `Span`s (one cell = one Span) |
| F8 queue | `List` |
| F9 confirm | `Clear` + centered `Block` |
| Power/hull | `Gauge` / `LineGauge` |
| Long coach | `Paragraph` + `Scrollbar` + scroll state |

---

## 3. Bugs and correctness issues

### 3.1 P0 — Map ship colors are discarded

In `render_map`, styled `Span`s are built for focused/player/enemy cells, then only `.content` is appended into a plain `String` row:

```text
row.push_str(&span.content);  // style thrown away
lines.push(Line::from(row));
```

**Effect:** Focused yellow / player green / enemy red never appear; map is monochrome text.  
**Fix:** Build each row as `Vec<Span>` (or `Line::from(vec![…])`), one span per cell.

### 3.2 P1 — Hull display shows `hull=N/N`

```text
hull={}/{}  // both ship.structure
```

Snapshot has current structure only; max is not shown (or is same field). Displays as e.g. `12/12` always equal — useless after damage if we only show current twice, or misleading as “full”.  
**Fix:** Show `hull={structure}` only, or track max from ship class if protocol adds it; after damage ensure current updates (it should) and don’t print fake max.

### 3.3 P1 — Accel “next thrust” line is wrong for reverse / revector

Movement panel:

```text
thrust={} (next accel: {}→{})
```

Always `thrust → thrust-1`. Reverse-thrust and revector costs are not always 1 (`speed+1` for oblique revector per protocol).  
**Fix:** Soften to “accel costs depend on alignment (often 1)” or omit predicted next until we read engine preview (we must not reimplement rules).

### 3.4 P2 — Allocate weapon order is alphabetical (`BTreeMap`)

Draft iteration order is `beam_1`, `plasma_1`, `torp_1`, while fire mode uses ship TOML order `beam_1`, `torp_1`, `plasma_1`. Tutorial accounts for this, but free-play players get two different orderings.  
**Fix:** Preserve ship weapon order in `AllocDraft` (`Vec` or `IndexMap`), not `BTreeMap`.

### 3.5 P2 — Tutorial field labels assume exactly 3 weapons

`field_label()` hard-codes heavy-cruiser slots. Other classes would mislabel. Fine for this tutorial scenario; document or key off live draft.

### 3.6 P2 — Digit entry multiplies by 10 while typing

Typing `1` then `0` → 10 is good; typing `1` when value is already `4` can become `41` capped at 30 → odd. Tutorial allows free adjust; still confusing. Prefer: digits replace unless “entry mode” with timeout, or Backspace-clear then type (partially documented as Backspace=clear).

### 3.7 P3 — `cycle=` shows `0` in allocate

Header forces cycle 0 in allocate. Harmless but slightly invents UI state. Prefer omit cycle outside movement/firing.

### 3.8 P3 — Tutorial error vs engine ERROR both red in header/panel

Easy to confuse soft tutorial blocks with engine rejects. Prefix tutorial with `TUTORIAL:` consistently (input panel uses `ERROR:` for tutorial messages today).

---

## 4. Wording improvements

### 4.1 Yellow bar / `why` strings

| Current | Issue | Suggested |
|---------|--------|-----------|
| `PB` in “Queue beam — main damage at PB” | Jargon | “point blank” or “range 1” |
| `Skip fire — still reloading geometry` | “Reloading” sounds like weapons | “Skip fire — still fixing course/position” |
| `Save the alpha strike` | Game-y | “Save the full volley for point blank” |
| `Lesson complete` | Fine | Keep; optional “Won — q to quit” |
| Repeated T3 shield whys (`F=6 on the kill run`) | Thin | Point back to “shields reset every turn” once |

### 4.2 Coach `text` that is too thin (steps ~4–6, 13–14, 16, 21–27, 29…)

Many mid-tutorial steps are a single sentence (`"Space."`, `"Speed 2→1."`). Yellow already carries the action; coach should either:

- repeat the **system rule** (why reverse-thrust works, why Space ≠ e), or  
- be omitted when empty.

**Suggested rule:** coach `text` ≥ 2 sentences for first occurrence of a concept; later repeats may be short.

### 4.3 Coach still says “Tab” in places

Early beam step historically said “↓ / Tab”; Tab **cycles ships**, not allocate fields. Audit for “Tab” in tutorial copy — only allow where ship-focus is intended.

### 4.4 Allocate chrome

- `Pwr cost/pool` is good; consider “Engine power” not only “Movement” in the row label (yellow already says engine).
- Footer `Enter commit · ↓/j next · ←/→ adjust` — add `↑/k prev` (supported in code).

### 4.5 Movement panel

- “digits: set absolute facing (not speed)” is defensive; if digits only work as facing in movement mode, say so once in the title: `Movement (facing 0–5 · t accel · c coast)`.

### 4.6 Fire panel

- Prefer “Queue shot (resolves when all ships Space/ready)” over bare weapon lists.
- Shield face for *target* armor hit: show face name (F/FR/…) not only index.

### 4.7 Header

- `phase=allocate` raw engine tokens — OK for dev client; product polish: `Allocate` / `Move 2/4` / `Fire`.
- `TUTORIAL` badge is good; optional step `3/56`.

### 4.8 Terminology consistency with REPL

| Prefer | Avoid |
|--------|--------|
| thrust / thrust pool | “movement power” alone |
| facing vs course | “direction” ambiguous |
| ready (Space) | “skip” alone |
| charge (carries) | “reload” for unfired carry |
| shield face F | “shield 0” in player text |

---

## 5. Recommended improvements (prioritized)

### P0 — Fix before more polish

1. **Map Span styling** (§3.1).  
2. **Hull line** honesty (§3.2).  
3. **Tutorial ERROR label** → `TUTORIAL:` (§3.8).

### P1 — High value UX

4. **Allocate as `Table`/`List` + selection highlight** (§F1).  
5. **Shield face mini-diagram** in allocate (§F2).  
6. **Map: facing vs course** (§F3).  
7. **Confirm dialogs with `Clear` + centered `Block`** (§F9).  
8. **Weapon order stable across allocate/fire** (§3.4).  
9. **Auto-move allocate cursor** to the field required by the current tutorial step (§F6).

### P2 — Tutorial / teachability

10. Expand thin coach steps; kill “Tab” mis-hints (§4.2–4.3).  
11. Replace jargon (PB, alpha strike) in yellow bar (§4.1).  
12. Collapse select+set pairs or sticky-nav with auto-cursor (§F6).  
13. Pending fire `List` during fire phase (§F8).  
14. `Gauge` for power pool and optional hull.

### P3 — Structural / Medium tier prep

15. `Tabs` for Coach | Combat | Help.  
16. `Scrollbar` on coach + combat history.  
17. `Canvas` hex map (range, course rays).  
18. Dynamic header height from stacked notices.  
19. Optional root `Widget` refactor for panels.  
20. `NO_COLOR` / monochrome already planned in PRD — verify yellow/black bar remains readable (bold + reverse video fallback).

---

## 6. Suggested layout evolution (sketch)

**Free play (Small, improved):**

```text
┌─ turn · phase · cycle · status · queued ─────────────────────┐
│ [modal confirm overlays here with Clear]                     │
├─ Map (Canvas/Spans) ──────────┬─ Ship (gauges + contacts) ──┤
├─ Phase form (Table/List) ─────┴─ optional side help ─────────┤
└─ Combat / Log (Tabs) ────────────────────────────────────────┘
```

**Tutorial:** keep yellow why·keys in header (works). Prefer:

```text
┌─ status + yellow why·keys ───────────────────────────────────┐
├─ Map ──────────────────────────┬─ Ship ─────────────────────┤
├─ Form (selected field jumps) ──┤ Coach (scroll) ────────────┤
└─ Combat (only if non-empty) ───┴────────────────────────────┘
```

Avoid three equal horizontal slivers of coach/events/form without hierarchy.

---

## 7. Out of scope / non-goals

- Reimplementing to-hit or damage in the client (PRD).  
- Medium/Large tier switching (ADR-0023) — but widgets above prepare for it.  
- Copying REPL hexutil into Rust — geometry display only; no rules.  
- Binding tutorial to typed REPL commands — TUI stays keypress-primary.

---

## 8. References

- Local: `frontend/tui/PRD.md`, `frontend/tui/README.md`, `frontend/tui/src/ui.rs`, `tutorial.rs`  
- Engine contract: `docs/PROTOCOL.md`  
- Ratatui: https://ratatui.rs/ · https://docs.rs/ratatui/0.29.0/ratatui/  
- REPL presentation vocabulary: `frontend/repl/ASCII-UI.md`, `GAMEPLAY.md`

---

## 9. Quick checklist for the next UI pass

Status after 2026-07-14 implementation pass:

- [x] Fix map cell `Span` composition
- [x] Fix hull display (current structure only)
- [x] Rename tutorial soft-block prefix (`TUTORIAL:`) / engine (`ENGINE:`)
- [x] Allocate selection highlight (inverted ▶ rows)
- [x] Shield face diagram
- [x] Facing vs course on map
- [x] Wording pass on yellow `why` + short coach steps
- [x] Stabilize weapon order in draft (ship order `Vec`)
- [x] Modal confirms via `Clear`
- [x] Auto-cursor to tutorial allocate field
- [x] Power `Gauge` on allocate
- [x] Pending fire queue summary
- [ ] Tabs Coach | Combat | Help (deferred)
- [ ] Canvas hex map + triangle ships + range rings (deferred)
- [ ] Scrollbars on coach / combat (deferred)
- [ ] Player color picker (fixed green/red for now)
