# UI evaluation rubric

How we judge whether a shipsim interface change is good. Applies to all three
clients (TUI, REPL, Love2D) and to the shipyard as much as to combat.

**This is a review instrument, not an essay.** Every criterion below is written
so that a reviewer can point at a rendered screen and say "this fails F2" and be
either right or wrong. A heuristic you cannot fail is decoration.

**Precedent to follow:** `frontend/repl/screen_audit.py` already carries numbered
invariants (I1–I4) under the rule *"each must be falsifiable against a rendered
screen."* This rubric extends that discipline to the rest of the UI surface. Where
a criterion below can be mechanized, it says so and names the harness.

---

## How to use this

1. **Reviewing a UI change:** walk the five lenses in order. Lenses 1 and 5 catch
   most real defects; 2–4 catch the ones that make a screen feel bad without any
   single line being wrong.
2. **Filing a defect:** cite the lens and criterion (`V1`, `E2`, …). "This is
   confusing" is not actionable; "the keybind is only in the help screen (A1)" is.
3. **Proposing a redesign:** state which criterion the current design fails
   *before* proposing the replacement. A redesign that cannot name a failed
   criterion is a preference, and should be treated as one.

**Severity.** Not all failures are equal:

| Severity | Meaning | Example from this repo |
|---|---|---|
| **Critical** | The UI causes irreversible loss the player did not consent to | Movement allocation offered the full power pool while the engine converted 6 and destroyed the rest |
| **Major** | The player is misled about game state, or cannot recover from a state | Esc discarding an unsaved shipyard draft with no warning |
| **Minor** | The player must work to obtain information they should have been given | `Movement: 6` rendered with no `/cap`, so the ceiling is discoverable only by hitting it |
| **Polish** | Aesthetic or consistency issue with no decision impact | Legend text saying "Dim ghost" when the code renders cyan |

Critical and Major block a merge. Minor and Polish are logged.

---

## Lens 1 — Nielsen: the four that carry weight in a turn-based game

Nielsen's ten heuristics are the closest thing the field has to a shared
vocabulary, which is their main value: they let two reviewers disagree precisely.
Four of the ten do nearly all the work in a game like this one; the rest
(aesthetic minimalism, help documentation, error messages in plain language) are
real but rarely the thing that is broken here.

### V — Visibility of system status

The player must be able to see what the game currently believes, without asking.

- **V1** Every resource the player spends shows both **current and maximum**
  (`4/22`, not `4`). A bare number is unfalsifiable to the player.
  *Mechanized:* `screen_audit.py` I1 (bar-label integrity),
  `frontend/repl/tests/test_bar_honesty.py`.
- **V2** Every limit that will be enforced at resolution is visible **before**
  commit. If the engine will clamp, truncate, or reject, the screen said so first.
- **V3** State that changed as a result of the last action is visibly
  attributable to it. A silent state change is a bug even when the new state is
  correct.
- **V4** Where the client shows a derived number, it derives it from the engine's
  value, not its own reimplementation. *(This is also an engineering rule — see
  the `hexutil.py` preview mirrors — but it is a UI criterion because the failure
  mode is a screen that confidently lies.)*

> **Worked failure.** The allocate panel offered up to 36 power for movement on a
> hull that converts at most 6, displayed "power 20/36 ok", and the engine then
> truncated to 6 and destroyed the other 14. V1 passed (the bar was honest about
> power), V2 failed completely (the binding constraint was never shown). This is
> why V2 exists as a separate criterion: an honest gauge on the wrong quantity
> still misleads.

### R — Recognition rather than recall

The player should not have to remember what the game could show.

- **R1** Choosing between options shows the values being chosen between. A
  weapon picker lists damage, space, and cost; it does not require the player to
  remember what `beam_potent` does.
- **R2** Identity is legible without a legend lookup. If distinguishing two
  things on screen requires consulting a key, they are not distinguishable.
- **R3** Modal state is labeled where the mode is active, not inferred from
  which keys stopped working.

### E — Error prevention

Prefer making the illegal unrepresentable over reporting it afterward.

- **E1** The UI does not offer a value the engine will reject or silently
  truncate. Clamp at the source of input.
- **E2** Clamping is **announced**, not silent. A key that appears to do nothing
  is indistinguishable from a broken binding.
- **E3** Destructive and irreversible actions require a confirming second step,
  and the confirmation names *what* is being destroyed.
- **E4** An armed confirmation is cancelled by any action that changes what it
  refers to. A `d`-to-delete armed on row 3 must not fire on row 5 after a
  cursor move.

> **Worked pass.** The shipyard's weapon delete: first `d` arms and the status
> reads `delete beam_2? d again to confirm, any other key cancels`; second `d` on
> the same row confirms; moving the cursor cancels. E2, E3, and E4 in one
> interaction.

### C — User control and freedom (a visible way out)

- **C1** Every state has a visible exit, and the key for it is on screen.
- **C2** Leaving a state never discards unsaved work without a warning naming
  the work.
- **C3** The exit key is consistent across states, or the difference is
  displayed. Esc meaning "back" in one panel and "quit" in another, unlabeled,
  fails this.

---

## Lens 2 — Hick's Law: simultaneous options, not total options

Decision time scales with the **log of the number of options presented at once**.
This is the real KISS analogue for interfaces.

**The distinction that matters:** cut options *presented simultaneously*, not
options that *exist*. A catalog of 17 weapon SKUs is not a Hick's Law problem. 17
weapon SKUs rendered as one flat list with no ordering, grouping, or filter is.
Deleting SKUs to "simplify the UI" solves a UI problem by removing game depth —
almost always the wrong trade, and the one this lens is most often misused to
justify.

- **H1** No screen presents more than roughly 7–9 peer choices without grouping,
  ordering, or filtering. Sub-grouping resets the count.
- **H2** Lists longer than that offer at least one of: a meaningful default sort,
  a filter, or a type-ahead.
- **H3** Options that are illegal in the current state are visibly marked as
  such (or omitted), so they do not consume decision budget.
- **H4** The most common choice is reachable without traversing the full set —
  a default, a recency ordering, or a dedicated key.

> **Applied.** The shipyard picker keeps all SKUs available but adds a filter and
> a column layout showing the deciding values, and marks entries that do not fit
> the current hull. The option set is untouched; the decision cost is cut. The
> standards list sorts by size and the user list defaults to recency, so the
> most-likely target is at a predictable end (H4).

---

## Lens 3 — Progressive disclosure (and how it gets misapplied)

Show what the current decision needs; keep the rest one keystroke away. This is
the YAGNI analogue — and, as with YAGNI, the failure mode is over-application.

**The correct form:** detail that is *not needed for the decision in progress* is
one clearly-signposted keystroke away.

**The misapplications, in order of how often they happen here:**

1. **Hiding a binding constraint.** Progressive disclosure never justifies hiding
   something that changes the current decision. A movement ceiling is not
   "advanced detail" — it is the constraint the decision is made against. If it
   affects the choice being made now, it is *not* eligible for disclosure. This
   is the collision with V2, and **V2 wins**.
2. **Disclosure without a signpost.** Hidden content with no visible affordance
   is not disclosed, it is removed. If nothing on screen indicates more exists,
   the player will not press the key (see A1).
3. **Using it to excuse a cramped layout.** "It scrolls" is a legitimate answer
   to *reachability* and not to *comparison*: if the player must hold two numbers
   in working memory because they cannot be on screen together, scrolling has not
   solved it.

- **P1** Everything required for the decision in progress is visible
  simultaneously without navigation.
- **P2** Hidden detail has a visible signpost (a `…`, a count, a named key).
- **P3** Disclosure costs at most one keystroke from the point of need.
- **P4** Values the player is asked to trade off against each other are visible
  at the same time.

> **Correct application.** The allocate panel scrolls, and the scroll offset
> follows the cursor, so weapons and shield faces are always reachable (P1 for
> the field under edit, P3 for the rest). An external review misread a static
> render at cursor 0 as "shields aren't on screen at all" and proposed a panel
> redesign. The genuine remaining complaint is narrower and is a **P4** issue:
> shields are re-bought from zero every turn and cannot be seen alongside the
> movement field they compete with for power.

---

## Lens 4 — Gestalt grouping: the TUI's entire typography budget

Spatial arrangement communicates relationship before a single character is read.
In a graphical UI this competes with weight, size, and font. **In a terminal
there is no weight, no size, and no font** — there is position, a handful of
colors, and box-drawing characters. Grouping is therefore not a polish concern
here; it is most of the available design surface.

- **G1 — Proximity.** Things that belong to one decision are adjacent. A value
  and its constraint, an action and its key, a weapon and its stats.
- **G2 — Similarity.** Things of the same *kind* look the same; things of
  different kinds look different. **Different roles must differ in shape, not
  only in color** — colour is unavailable to some players, unreliable across
  terminal themes, and absent from a monochrome capture.
- **G3 — Common region.** Borders and panels mean containment. A bordered box
  asserts that its contents belong together; do not put unrelated content in one
  box to save space.
- **G4 — Continuity.** Elements of one conceptual object share a visual family.
  A path's origin, route, and destination should read as one thing in three
  states.

> **Worked failure and fix.** The move preview drew the planned destination as
> `A1↗` — the *same* callsign-and-arrow shape as the real ship — distinguished
> only by `Color::Cyan`, while the route between them used `◇`, an unrelated
> shape. Two G2 failures (ship vs. ghost differ by color alone) and a G4 failure
> (route and destination share no family). The fix renders the destination as
> `◇A1↗`: the `◇` prefix ties it to the route (G4) and gives it a shape
> difference from the ship that survives a monochrome terminal (G2).

---

## Lens 5 — Norman: affordances and signifiers

An affordance is what can be done. A **signifier** is the perceivable cue that it
can be done. In a keyboard-driven TUI, almost nothing is self-evidently
clickable, so **every affordance needs an explicit signifier** — and the signifier
is the keybind, shown at the point of use.

**The rule: the key belongs next to the thing it acts on, not in a help screen.**
A help screen is a reference for the key you have forgotten; it is not a
substitute for the cue that the action exists at all. A player who does not know
an action exists will never open the help to look for it.

- **A1** Every available action has its key visible on the screen where it
  applies. Help screens supplement; they do not carry the only mention.
- **A2** The signifier sits with its target (this is G1 applied to keys):
  `a add   d delete` on the weapons header, not in a footer three panels away.
- **A3** Unavailable actions are visibly unavailable rather than silently inert.
  A key that does nothing, with no message, reads as broken software (see E2).
- **A4** The signifier states the *actual* interaction, including multi-step
  ones — `d delete, d again to confirm` rather than `d delete`.
- **A5** What is displayed matches what the code does. A legend describing an
  attribute the renderer no longer produces is worse than no legend.

> **Worked failure.** The map legend read `Dim ghost = planned end` while the
> renderer drew the ghost in cyan with no dim modifier — an A5 failure that also
> made R2 harder, because the one place explaining the distinction described a
> cue that was not present.

---

## Known tensions

Criteria conflict. Resolve them in this order, and say which you applied:

1. **V2 (show the binding constraint) beats P-anything.** A constraint on the
   current decision is never "advanced detail."
2. **E1 (prevent) beats E2 (announce).** Announcing a clamp is a fallback for
   when preventing the input is impossible or would be more confusing; it is not
   an alternative to clamping.
3. **H1 (few simultaneous options) loses to R1 (show the values).** Cramming a
   choice into fewer, less-informative rows trades a decision-*speed* win for a
   decision-*quality* loss. Group or filter instead.
4. **G2 (differ by shape) beats compactness.** A one-character saving is not
   worth a distinction that vanishes without color.

---

## Review checklist

For any UI-touching change:

- [ ] Does anything on screen show a number without its limit? (V1)
- [ ] Will the engine clamp, truncate, or reject something the UI presented as
      available? (V2, E1)
- [ ] Can any input silently do nothing? (E2, A3)
- [ ] Is anything destructive reachable without confirmation naming the target?
      (E3)
- [ ] Does every state show its exit key, and warn before discarding work?
      (C1, C2)
- [ ] Does any new list exceed ~9 peer options with no sort, filter, or grouping?
      (H1, H2)
- [ ] Is anything needed for the *current* decision hidden behind navigation?
      (P1)
- [ ] Do any two distinct roles differ only by color? (G2)
- [ ] Does every new action's key appear next to what it acts on? (A1, A2)
- [ ] Does any legend or hint describe behavior the code no longer has? (A5)

## Mechanizing

Prefer a falsifiable check over a review note wherever the harness allows:

| Client | Harness | Good for |
|---|---|---|
| REPL | `frontend/repl/screen_audit.py` (PTY + pyte grid) | V1–V3, P1, G3 — anything about *what is visible* |
| REPL | `frontend/repl/tests/test_bar_honesty.py` | V1 |
| TUI | `render_to_string` + `buffer_contains` in `frontend/tui/src/tests.rs` | V1, G2, G4, A1, A5 |
| TUI | `handle_key` state assertions | E1–E4, C1, C2 |
| Love | `frontend/love/tests/run_all.lua` | E1, and allocation-clamp parity with the other clients |

When adding a mechanized check, follow the `screen_audit.py` convention: give it
a number, state the invariant in one sentence, and make sure it can fail.
