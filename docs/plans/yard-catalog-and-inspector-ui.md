# Plan: Yard browse ordering, protected standards, and a weapon inspector

## Purpose

Three usability problems reported after using the TUI shipyard (`--yard`):

1. The 7 standard classes sort alphabetically by id, not by size, and nothing
   stops a player from editing or deleting one out from under every suite that
   depends on it.
2. Adding an over-space weapon and hitting Esc silently discarded the whole
   draft (fixed separately — see `is_dirty`/`request_exit` in `yard.rs`). What
   remains from that report: once a weapon is on a ship, there is no way to
   tell what it *does* — no damage number, no indication a SKU is Precise,
   Potent, Repeat, Pierce, or anything else.
3. User-authored designs have no stated sort order and no way to change one.

**Implementation status:** Phases 1–5 shipped in `da078f7` / `89ec78e`.
This file is the locked design record. Headline math and read-only
navigation were later corrected to match the Phase 3/1 exits (full-charge
range-1 damage, `damage_bonus`, missile flat 2, inspect-only keys still
move the cursor).

Each phase has a **Goal**, **Work**, and an **Exit** table; a phase is done
only when every exit check passes.

---

## Grounding (verified against the current code)

- `shipyard::list_designs` sorts by `design.id.cmp` — alphabetical, not size.
  `yard_capital` (size 7) sorts before `yard_dreadnought` (size 6); `battleship`
  (size 5) sorts before both. This is exactly the disorder reported.
- There is already one precedent for "hide some designs from the interactive
  picker": `shipyard::QUALITY_FIXTURE_IDS`, added to keep the four weapon-SKU
  balance controls (`yard_baseline`/`compact`/`potent`/`precise`) out of the
  browser. This plan follows the same pattern for a second, distinct
  concern — protecting the 7 standards from edits — rather than inventing a
  new mechanism.
- The 7 standard ids are currently duplicated as a literal list in exactly one
  place: `tests/catalog_contract.rs::yard_catalog_roles_and_costs_are_locked`.
  That test is the closest thing to a canonical list today; it is not
  reusable from `yard.rs`. This plan promotes it to a real constant.
- `DesignPreview` (`shipyard::preview_design`) returns ship-level aggregates
  only — cost, space, power, structure, shields. No per-weapon numbers exist
  anywhere in the shipyard module today.
- Per-weapon stats live in `data/components.toml` under `[weapons.<sku>]`:
  `kind`, `space`, `cost`, `max_charge`, `max_range`, optional `max_ammo`,
  `accuracy_bonus`, `damage_bonus`, `repeat`, `pierce`. All parsed today by a
  **private** `WeaponComponent` struct — nothing public exposes them, matching
  the private `EngineComponent`/public `EngineSpec` split the module already
  uses for engines (`shipyard::engine_spec`). This plan adds the weapon
  equivalent rather than a new shape.
- Actual damage numbers require the combat rules, not just the component
  catalog: `combat_tables::beam_damage/plasma_damage/torp_damage` each take a
  loaded `Ruleset` (`crate::rules::Ruleset::load(root)`) and compute
  charge/range-dependent damage. The shipyard module does not load rules
  today — this plan is the first thing that makes it do so.
- Range dependence is real and asymmetric by kind (`data/rules/default.toml`):
  beam damage = `charge × range_factor[range]`, factor falls from 2.0 at range
  1 to 1.0 at range 9-10; plasma damage comes from a flat range-indexed table
  (8 at range 1, down to 1 at long range); torpedo damage is flat within its
  range band, no falloff. PD, graviton, and missile do not fit "damage number
  that falls off with range" at all — graviton is `max(0, size_diff)`, PD
  never damages a ship, missile is flat like a torpedo. A single formula
  cannot describe all six kinds; the plan treats this explicitly.
- Filesystem mtime is already available for free and needs no new persisted
  field: `fs::metadata(&path).modified()`. Using it for "recency" means no
  schema change and no drift risk between a stored timestamp and reality.
- The yard's own minimum terminal size is `60×16`, distinct from the general
  game's `80×24` (`frontend/tui/src/ui.rs`). Layout work in this plan targets
  the smaller floor.
- `browse_cursor = yard.listings.len()` is the existing idiom (used by 5+
  current tests) for "select the new-ship row." Every phase here preserves
  that invariant — the new-row index stays `listings.len()` regardless of how
  the rows above it are grouped or sorted.

---

## Locked decisions

Three places where the request needs a specific, statable choice rather than
being left implicit. Stated with reasoning so a reviewer can push back on the
reasoning, not just the outcome.

**Sort standards by `size`, not by cost.** The request says "cost is likely an
indirect way to do this" — correctly hedged. Cost only approximates size
ordering because of how the current cost model happens to be tuned; an earlier
review of this same catalog flagged the size-3→4 cost step as unusually
shallow (43% vs 130-260% at other tiers). Sorting by cost would silently
reorder the list the day that gap closes or a tier gets re-priced. Sorting by
the `size` field is direct, stable under re-tuning, and is data the ship
already carries. Cost stays in the row as a second column; it does not decide
row order.

**Default sort for user ships is Recency, not Size/Cost.** The request's first
sentence proposes size-or-cost as the default and then gives the actual reason
a mode matters: *"they can find the ships they just edited to improve them."*
That reasoning argues for Recency, not for whichever mode is listed first. Size
and Cost remain one keypress away (see Phase 2); Recency is the default because
the request's own justification is a Recency justification.

**Standards are protected at two layers, not one.** "Uneditable" is enforced
by disabling the input-layer keys (nudge/add/delete/type/save/compile) *and*
by making the state-mutating methods themselves (`save`, `compile`,
`delete_weapon`, `nudge`, …) refuse to act when the open draft is a standard.
Single-layer (input-only) protection is exactly the shape of bug this session
has hit three times already (evasion, weapon modifiers, `systems=[]` drift) —
two independent paths quietly disagreeing. Testing the state layer directly
(call `yard.save()` on a standard, bypassing keys entirely) is what makes this
a real guarantee instead of a UI convention.

**Explicitly deferred, not built here:** a "clone this standard into an
editable custom ship" flow. Real and probably wanted eventually, but it is a
new user-facing action the request didn't ask for; adding it speculatively is
exactly the kind of scope creep this repo's plans have been trimmed for
before. Noted so it isn't silently forgotten, not silently built.

---

## Phase 1 — Standards: size order, always first, protected

### Goal

Opening the yard shows the 7 standards sorted by size, ahead of every user
design, and no key or method can mutate one.

### Work

1. Promote the 7-id list out of `tests/catalog_contract.rs` into a real,
   public constant in `src/shipyard/mod.rs` — `STANDARD_CLASS_IDS`, sibling to
   the existing `QUALITY_FIXTURE_IDS` — and have that test import it instead
   of restating it. One list, one place it can go stale.
2. Add a way to ask "is this id protected" (standards; fixtures are already
   excluded entirely, see Phase 0 grounding — this is a second, distinct
   check, not a merge of the two lists, since fixtures are *hidden* and
   standards are *shown but locked*).
3. `YardState::refresh_listings`: partition into standards (sorted by
   `design.size`, ties broken by id for determinism) and user designs (sorted
   per Phase 2, defaulting to Recency); standards render first, unchanged
   order regardless of `data/designs/` file-system read order.
4. `YardState::open_selected` on a standard row: load it into `draft` as
   today (needed so Phase 3/4's inspector can show its stats), but set a
   `viewing_readonly: bool` flag and label the edit header "(read-only —
   standard class)".
5. Gate every mutating method — `nudge`, `add_weapon`,
   `request_delete_weapon`, `type_name`, `backspace_name`, `save`,
   `compile` — on `!self.viewing_readonly`, returning early with a status
   message ("standard classes are reference-only") rather than acting. This
   is the state-layer half of the two-layer requirement above; `input.rs`
   additionally skips dispatching those keys when read-only, purely so the
   status line doesn't flicker "reference-only" on every keypress.

### Tests

- 7 fixed-size fake designs (`data/designs/`-shaped fixtures, sizes 1-7 in
  scrambled id order) load in ascending size order, ahead of any user design,
  regardless of on-disk read order.
- The new-row index is still `listings.len()` after the standards section is
  inserted (regression guard for the existing `browse_cursor = listings.len()`
  idiom used elsewhere).
- Opening a standard sets `viewing_readonly`; opening a user design does not.
- Direct state-layer test, no keys involved: with a standard open, call
  `save()`, `compile()`, `nudge(1)`, `add_weapon()`, `request_delete_weapon()`,
  `type_name('x')` each in turn — draft is byte-identical before/after every
  call, and `status` reports the reference-only message.
- Key-layer test: press `s`/`c`/`d`/`a`/Left/Right/typing while viewing a
  standard — same result, exercised through `input.rs` this time.
- `QUALITY_FIXTURE_IDS` behavior is unchanged (still hidden, not merely
  read-only) — explicit regression test, since Phase 1 adds a second
  id-based rule next to it and the two must not merge or shadow each other.

### Exit criteria

| Check | Pass condition |
|---|---|
| Order | Standards render size-ascending, always above user designs |
| Single source | `STANDARD_CLASS_IDS` has exactly one definition; the test imports it |
| Input-layer lock | Every mutating key no-ops on a standard, with a status message |
| State-layer lock | Every mutating method no-ops on a standard when called directly |
| New-row invariant | `browse_cursor = listings.len()` still opens a fresh draft |
| Fixture regression | `QUALITY_FIXTURE_IDS` still hidden, unaffected by this phase |
| `cargo test` (root + tui), `cargo clippy --all-targets -D warnings` | green |

### Commit

`feat(yard): sort standards by size and make them read-only`

---

## Phase 2 — User-ship sorting: Size / Cost / Recency, switchable

### Goal

The user section of the browse list defaults to Recency and can be cycled to
Size or Cost with one key, without touching the standards section above it.

### Work

1. `SortMode { Recency, Size, Cost }` on `YardState`, default `Recency`.
   Recency uses `fs::metadata(&listing.path).modified()`, descending (most
   recent first) — no schema or TOML change.
2. One key on the Browse screen (proposed: `o`, free today) cycles the mode
   and re-sorts the user section in place; the header/status line names the
   active mode ("sort: recency — o to change") so it's discoverable without
   a manual.
3. Ties within a mode break by id, for determinism in tests and in the UI
   (no visible reordering on re-render with unrelated data unchanged).
4. Cursor position on the *design under the cursor* is preserved across a
   re-sort where possible (track the selected id, not the index, across the
   resort) — losing your place because you changed sort mode would defeat
   the point of adding modes.

### Tests

- Three fake user designs with distinct size, cost, and mtime order (chosen
  so all three metrics disagree) assert each mode produces the mode-specific
  order, and that cycling wraps back to Recency after Cost.
- Standards section order is unchanged by any sort-mode change (regression
  guard against the two sections' sort logic merging by accident).
- Selecting a design, cycling sort mode, and reading `browse_cursor` back
  still points at the same design id post-resort (not just the same numeric
  index).

### Exit criteria

| Check | Pass condition |
|---|---|
| Default | Fresh yard load sorts user designs by Recency |
| Cycle | One key visits Size, Cost, Recency and back, each producing the stated order |
| Determinism | Equal-key ties break by id, stable across repeated renders |
| Standards unaffected | Standards section order never changes with sort mode |
| Selection stability | The selected design stays selected across a re-sort |
| `cargo test`, `cargo clippy -D warnings` | green |

### Commit

`feat(yard): switchable sort for user-authored designs`

---

## Phase 3 — Weapon stat line: max damage and effect tags

### Goal

Every weapon row in the editor shows a damage number and any active effect
(Precise/Potent/Repeat/Pierce/…), computed from the same data the engine
fires with — not a second, hand-maintained description.

### Work

1. Add `shipyard::weapon_spec(root, sku) -> WeaponSpec`, mirroring the
   existing `engine_spec`/`EngineSpec` pair: promotes the already-parsed but
   private `WeaponComponent` fields (`kind`, `space`, `cost`, `max_charge`,
   `max_range`, `max_ammo`, `accuracy_bonus`, `damage_bonus`, `repeat`,
   `pierce`) to a public, read-only view. No parsing logic duplicated.
2. Add `shipyard::weapon_headline(root, sku) -> Result<String, Error>` that
   loads `Ruleset::load(root)` once and computes a kind-appropriate headline:
   - **Beam/Plasma**: best-case damage at range 1, full charge —
     `combat_tables::beam_damage`/`plasma_damage` plus `damage_bonus`. Shown
     as `dmg N @ r1` so it reads as a ceiling, not a promise (both fall off
     with range; Phase 4 explains the curve).
   - **Torp/Missile**: flat damage (`torp_damage`) plus `damage_bonus`, shown
     without a range qualifier since it doesn't fall off — `dmg N`.
   - **PD**: no damage headline (it never damages a ship) — shown as
     `intercept only`.
   - **Graviton**: no fixed number exists — shown as `dmg = your size − target
     size`, matching the actual rule instead of a misleading placeholder.
3. Effect tags derived from `WeaponSpec` fields directly, never from the SKU
   name string — robust to a future SKU that doesn't follow the
   `<kind>_<adjective>` naming convention:
   `accuracy_bonus > 0` → `Precise +N`; `damage_bonus > 0` → `Potent +N`;
   `repeat` → `Repeat`; `pierce` → `Pierce`; `max_ammo.is_some()` →
   `Ammo N`. Multiple tags can co-occur; render as a comma-joined suffix.
4. Weapon row becomes, e.g.: `beam_precise   mount forward   dmg 8 @ r1   Precise +2`.

### Tests

- For one representative SKU per kind (beam/plasma/torp/pd/graviton/missile),
  assert the headline text matches a value computed independently through
  `combat_tables` against the live `data/rules/default.toml` — not a literal
  pinned in the test, so the test still passes if the rules file is
  re-tuned and only breaks if the *shipyard's* computation diverges from the
  engine's.
- `beam_potent` shows `Potent +2` (or whatever `damage_bonus` is at the time,
  read from `data/components.toml`, not hardcoded); `beam_precise` shows
  `Precise +2`; `beam_repeat`/`beam_pierce` show their respective tag; plain
  `beam` shows no tag.
- PD and Graviton rows never render a bare number where the mechanic doesn't
  support one.

### Exit criteria

| Check | Pass condition |
|---|---|
| Public API | `weapon_spec`/`weapon_headline` exist, no private struct exposed |
| Correctness | Headline for each kind matches an independent `combat_tables` computation against live rules |
| Tags | Every nonzero modifier field produces its tag; zero fields produce none |
| No SKU-string coupling | Tags come from `WeaponSpec` fields, not from parsing `weapon.component` |
| Row rendering | Editor weapon list shows damage + tags for every weapon on the draft |
| `cargo test`, `cargo clippy -D warnings` | green |

### Commit

`feat(shipyard): expose weapon stats and surface them in the yard editor`

---

## Phase 4 — Description panel

### Goal

A panel that explains whatever field the cursor is currently on — hull, material,
engine, armor, shields, or a specific weapon and its mechanic — updating live
as the cursor moves, with weapon text pulling the Phase 3 numbers rather than
inventing separate prose.

### Work

1. Re-layout `render_yard_edit`: split the middle chunk horizontally
   (component list | description) instead of the current single column,
   sized to still fit the yard's own `60×16` floor (narrower than the
   general game's `80×24`) — verify the split degrades to something legible
   at exactly 60 columns, not just at a comfortable width.
2. One function mapping `EditField` → description text:
   - Hull fields (Size/Material/EngineKind/EngineSize/Armor/Shields): short,
     qualitative explanations of what the field trades off (e.g., material
     trades cost for structure; a bigger hull holds more but its frame cost
     grows faster than its capacity — this is exactly the balance note
     already written in `docs/plans/catalog-review-remediation.md`'s
     grounding section, reused here rather than re-derived).
   - `EditField::Weapon { index }`: kind-specific mechanic text (beam damage
     falls off with range, strongest at range 1; plasma follows a range
     table with similar falloff; torpedo/missile are flat but magazine-
     limited; PD auto-intercepts incoming ordnance and never targets ships;
     graviton ignores shields and armor and hits everyone else in the hex
     too) **plus** a one-line summary of that weapon's active tags from
     Phase 3, so the two panels agree by construction — the description
     never states a number, only the mechanic; Phase 3's line carries the
     number.
3. Explanatory text stays qualitative/formulaic ("damage = charge × range
   factor") rather than embedding specific figures from `data/rules/`, so it
   cannot drift out of sync with a future rules tune the way a literal number
   would; the adjacent Phase 3 line is the only place a live number appears.

### Tests

- Cursor visits every `EditField` variant in turn (Name through the last
  weapon); each produces non-empty text, and no two adjacent-but-distinct
  fields produce identical text (catches a copy-paste default that never
  got filled in).
- Weapon description text contains a kind-appropriate keyword per kind
  (`"range"` for beam/plasma, `"shield"` and `"armor"` for graviton,
  `"magazine"` or `"ammo"` for torp/missile, `"intercept"` for PD) —
  cheap guard that the mapping didn't fall through to a generic default.
- Rendered frame reflects the new cursor's description in the same
  `render_to_string` call that follows a cursor-move key (no stale-frame
  regression), using the existing test harness pattern already used
  elsewhere in `tests.rs`.
- 60×16 smoke render does not truncate/panic (minimum-size regression,
  matching the yard's own stated floor).

### Exit criteria

| Check | Pass condition |
|---|---|
| Coverage | Every `EditField` variant has non-empty, field-specific text |
| Distinctness | No two adjacent field types render identical text |
| Weapon agreement | Weapon description's tag summary matches Phase 3's tags for the same weapon |
| Live update | Description changes in the same frame as the cursor move |
| Minimum size | Renders without truncation/panic at 60×16 |
| `cargo test`, `cargo clippy -D warnings` | green |

### Commit

`feat(yard): add a live description panel for the highlighted field`

---

## Phase 5 — Docs and acceptance

### Goal

Close the loop: what changed is discoverable without reading source.

### Work

1. `docs/SHIPYARD.md`: document that standards sort by size and are
   reference-only; document the sort-mode key and its cycle order and
   default; document the weapon stat line format and where the tags come
   from (component fields, not SKU name parsing).
2. On-screen help text (`ui.rs` footer) names the sort-mode key and confirms
   the read-only indicator wording matches what Phase 1 actually renders.

### Full verification

```bash
cargo build -q
cargo test
cargo clippy --all-targets -- -D warnings
cargo test --manifest-path frontend/tui/Cargo.toml
cargo clippy --manifest-path frontend/tui/Cargo.toml --all-targets -- -D warnings
cargo run -q --bin shipsim-yard -- check-all
```

Manual smoke (UI play, per `docs/AGENT-PLAY.md`): launch `--yard`; confirm the
7 standards appear first, size-ascending; confirm Enter on one shows stats but
every edit key is refused with the reference-only message; confirm `o` cycles
Size/Cost/Recency on the user section only; confirm a `beam_precise` on a
draft shows both a damage number and `Precise +2`; confirm the description
panel changes as the cursor moves and stays legible at 60×16.

### Exit criteria

| Check | Pass condition |
|---|---|
| Docs | `docs/SHIPYARD.md` describes ordering, protection, sort key, stat line, tags |
| Help text | On-screen footer names the sort key |
| Full gate | All commands above pass |
| Manual smoke | Every item in the smoke list confirmed by hand |

### Commit

`docs(shipyard): document standard ordering, sort modes, and the inspector`

---

## Sequencing

```text
Phase 1 (standards: order + protect)
  → Phase 2 (user-ship sort modes)
      → Phase 3 (weapon stat line — needs Ruleset access, new to this module)
          → Phase 4 (description panel — reuses Phase 3's numbers/tags)
              → Phase 5 (docs + acceptance)
```

Phases 1 and 2 touch only `refresh_listings`/browse ordering and are
independently shippable. Phase 3 is a prerequisite for Phase 4 by design — the
description panel's weapon text must not duplicate Phase 3's number
computation, so Phase 4 cannot start first without creating exactly the kind
of two-sources-of-truth bug this plan's grounding section flags as a repeat
offender in this codebase.

## Out of scope

- Cloning a standard into an editable custom ship (noted above as a real,
  deferred follow-up).
- Persisting the chosen sort mode across sessions (resets to Recency on
  restart; add a settings file only if that turns out to matter in practice).
- Reorganizing `data/designs/` into subdirectories by origin — the id-based
  `STANDARD_CLASS_IDS` constant is cheaper and does not touch
  `load_ship_def`'s existing path assumptions.
- Love2D shipyard parity (consistent with every prior plan in this repo:
  Love yard is not a milestone).
- Any change to combat/damage formulas — this plan is read-only display work
  built on the existing `combat_tables` functions, not a rules change.
