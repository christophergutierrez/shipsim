# TUI blind-playtest protocol

How to run a first-contact usability test on the TUI using cheap models as
players. Judging criteria live in `docs/UI-RUBRIC.md`; this file is the
*procedure*.

**Invocation.** "Test the UI for the TUI only. Use `docs/UI-PLAYTEST-PROTOCOL.md`."

---

## The hypothesis under test

> A competent adult can sit down at `scenarios/battle.toml`, having read
> nothing, and reach a decisive outcome using only what the screen tells them.

Cheap models are the instrument precisely because they are not clever. A strong
model will *infer* a missing affordance from context and paper over the defect;
a weak one gets stuck exactly where a distracted human gets stuck. **Testers
being confused is the measurement, not a failure of the test.**

**Scope: TUI only.** Not the REPL, not Love2D, not the engine, not balance.
A tester dying because they ignored shields is a rules outcome, not a UI defect.

---

## Roles

| Role | Who | Does |
|---|---|---|
| **Cohort** | 10 cheap models, independent, no shared memory | Play blind. Report. Never touch files except their own report. |
| **Analyst** | The orchestrating session | Builds/validates the harness, runs the cohort, **verifies every claim**, writes the findings and plan. |

The analyst never accepts a cohort claim at face value. See *Falsification gate*.

---

## Harness requirements

The TUI needs a PTY; it exits `rc=1` with a message if run without one. Testers
therefore drive it through a screen-reader script (reference implementation:
`tmp/haiku/play.py`; promote to `tools/tui_playtest.py` when it stabilises).

**Mandatory before every cohort — the harness self-check.** A harness artifact
is indistinguishable from a game defect in a tester's report, and has produced
false findings in every round so far. Confirm all five:

| # | Check | Why |
|---|---|---|
| H1 | Renders the opening screen with box borders intact | Baseline sanity |
| H2 | **Selection is visible without colour.** If the game marks selection with a background, the harness must surface it as text — and must mark *only* the selected row | A prior round stripped colour, hiding the game's primary signifier; the fix then over-triggered on the power gauge and marked two rows |
| H3 | **No escape fragments in output.** `grep -E '\[[0-9]+;[0-9]+H'` returns nothing | CSI sequences split across reads once leaked as literal `[22;7` into rendered screens |
| H4 | Long key sequences work (script-from-file), so deep play is affordable | Restart-per-invocation under-samples late-game friction |
| H5 | Renders at **80×24** and at 120×40 | 80×24 is where clipping defects appear and is the assertion size in the rubric |

Record the harness commit/hash in the findings. If the harness changes
mid-cohort, the cohort is void — a split cohort cannot be compared.

---

## Cohort composition

Ten testers, each given **one persona**. Personas produce different failure
modes; a uniform cohort finds one defect ten times.

| # | Persona | Instruction to add |
|---|---|---|
| 1–2 | On-screen keys only | "Only press keys that are printed on the screen." |
| 3 | Arrows only | "Prefer arrow keys and Enter/Space." |
| 4 | Letter keys | "Prefer letter keys." |
| 5 | Help-seeker | "Look for help before experimenting." |
| 6 | Defaults only | "Change as little as possible; commit defaults." |
| 7 | Fire ASAP | "Your goal is to shoot the enemy as early as possible." |
| 8 | Close the distance | "Your goal is to reach the enemy." |
| 9 | Glossary-minded | "Try to understand every number on screen." |
| 10 | Quit and restart | "Quit and restart at least once." |

**Capacity warning.** A previous round lost 9 of 10 testers mid-run to an
account session limit and produced no usable comparison. Confirm capacity first;
stagger launches if unsure. **A cohort below 8 completed reports is reported as
inconclusive, not as a result.**

---

## The tester prompt (copy verbatim, substitute `NN` and the persona line)

```text
You are a FIRST-TIME PLAYER of a terminal starship combat game. You have never
seen it before.

HARD RULE — NO DOCUMENTATION OR SOURCE. Do not read any .md file, any source
file (.rs/.lua/.py), any test, or git history. Do not grep them. Everything you
learn must come off the game screen. If you read docs or code, this exercise is
worthless.

The game is started with:
  cargo run --manifest-path frontend/tui/Cargo.toml -- scenarios/battle.toml

It needs a real terminal, so use the viewer from the repo root:
  python3 <HARNESS> [KEYS...]
Each argument is one keypress, in order; it prints the resulting screen.
  python3 <HARNESS>                       # opening screen
  python3 <HARNESS> Right Right Enter
  python3 <HARNESS> --all w w Enter       # screen after EVERY key
Key names: Enter Esc Tab Space Backspace Up Down Left Right PageUp PageDown
Home End F1-F4. Any single character is sent literally.
A row marked "<== HIGHLIGHTED" is the currently selected row.
The game RESTARTS FRESH each invocation: to get deep, replay the whole sequence
in one command, or put keys in a file and use --script FILE.
Use timeout 240 on your commands.

PERSONA: <persona line>

TASK: play properly. Allocate power, move, fire, complete turns, and try to
destroy the enemy. Reach turn 5+ at least once. Do at least 12 runs.

Write a report to tmp/playtest/<RUN>/agent-NN.md with these sections:
1. WHAT I UNDERSTOOD — and how you worked it out.
2. WHAT CONFUSED ME — every stuck/guessing moment. Quote the EXACT screen text.
3. WHAT I COULD NOT DO — tried and failed, or never figured out.
4. KEYS THAT DID NOTHING — any key with no visible effect.
5. HOW FAR I GOT — highest turn, shots fired, damage dealt/taken, win/loss.
6. TOP 5 UX PROBLEMS — worst first, each with exact screen text and why.
7. WAS ANYTHING MISLEADING — did any number or label make you believe something
   false? Quote it.
8. SCORE — usability 1-10, and would you play again (yes/maybe/no)?
9. WHAT I WISH I HAD BEEN TOLD — the smallest set of facts that would have
   helped most. Be specific and brief.

Be brutally honest. Confusion and failure are the valuable output. Never pretend
you understood something you guessed. If something worked well, say so briefly.
```

Section 9 is what feeds the quick-start guide. Everything else feeds defects.

---

## What to track

A fixed schema, so runs are comparable over time. Record every row every time,
even when zero — a metric that only appears when it is interesting cannot show a
trend.

### Cohort metrics

| Metric | Type |
|---|---|
| Launched successfully | n/10 |
| Completed ≥1 Allocate → Movement → Fire loop | n/10 |
| Queued a legal shot by turn 2 | n/10 |
| Fired ≥1 shot in any run | n/10 |
| Funded shields, or explicitly noticed they were zero | n/10 |
| Reached the engine's Game Over | n/10 |
| **Won** | n/10 |
| Saw `ENGINE: order_illegal` | n/10 |
| Trapped in any state (help, menu, phase) | n/10 |
| Reported input that appeared ignored/hung | n/10 |
| Median / max turn depth | turns |
| Mean usability score | x/10 |
| Would play again | yes / maybe / no |
| Confused at all | n/10 |

`Won` and `Confused` are expected to be 0/10 and 10/10 early; keep them anyway —
they are the headline trend.

### Per-defect record

| Field | Rule |
|---|---|
| Id | `Xn`, stable across the run |
| Frequency | `n =` how many testers hit it |
| Severity | Critical / Major / Minor / Polish, per `docs/UI-RUBRIC.md` |
| Rubric criterion | e.g. `V2`, `E1`, `G2` |
| Evidence | Exact quoted screen text |
| Status | **Reproduced** / Plausible-unverified / **Falsified** |

---

## The falsification gate (mandatory)

**No finding is filed until the analyst reproduces it against the running game.**

This is not ceremony. In the first cohort, **three of nine testers' top-ranked
findings were false** — "the budget header never updates", "arrow keys don't
work", "the power bar misrepresents fill". All three were correct *observations*
of confusion and wrong *diagnoses*, and all three traced to one real defect
elsewhere. A plan built from the raw reports would have spent its first phase on
bugs that did not exist.

Classify every claim into exactly one of:

1. **Reproduced defect** — the analyst made it happen. File it.
2. **Reproduced comprehension failure** — the software is correct, the tester
   drew a false conclusion. **File this too**, against whatever made the correct
   state unreadable. These are often the most valuable findings.
3. **Not reproduced** — record in a *Falsified* table with the correct
   behaviour. Never delete: the confusion was real even where the diagnosis was
   not.
4. **Engine / scenario / balance** — out of scope for a TUI test. Route
   elsewhere.

Track **filed vs falsified** as a protocol-health number. A rising falsification
rate means testers are misreading the screen — which is itself a UI signal.

Also produce, every run:

- **"Stayed fixed — do not reopen."** Items a prior run flagged that this cohort
  did not hit. Prevents re-litigating closed work.
- **"Not defects — do not implement."** The falsified table, stated as
  instructions.

---

## The quick-start guide is a residual, not an input

Testers are never given a quick start. It is derived *afterwards* from section 9
of the reports, and only for facts that survive this test:

> **Irreducibility test.** Could this fact be shown at the point of use — on the
> row, in the footer, in a label — by a reasonable UI change?
> **If yes, it is a UI defect. Fix the UI; it does not go in the guide.**

Only three kinds of fact are irreducible:

1. **Genre priors** the player must arrive with (this is a hex grid; ships have
   facing; arcs matter).
2. **The goal**, if the screen cannot state it in one line.
3. **A rule with no on-screen surface** that no placement could fix.

**Hard cap: 10 lines.** The cap is the point. If the residue exceeds ten lines,
that is evidence the UI is under-explaining, and the overflow is filed as
defects — not appended to the guide. A growing quick start is a failing UI.

Write it as `docs/TUI-QUICKSTART.md`, and record its line count as a tracked
metric. **Target: the line count falls every run.**

---

## Output artifacts

Per run, under `tmp/playtest/<date-or-tag>/`:

| File | Contents |
|---|---|
| `agent-01.md` … `agent-10.md` | Raw tester reports, unedited |
| `findings.md` | Cohort metrics, per-defect records, Falsified table, Stayed-fixed table, priority order |
| `plan.md` | Phased remediation: each milestone has Goal / Files / Work / numbered pass conditions / Commit |
| `harness-check.md` | H1–H5 results and the harness hash |

`findings.md` must carry a **confounds** section that names any harness artifact
or cohort loss. Declare your own tooling bugs there; they have caused false
findings in every round so far.

---

## Plan quality bar

A milestone is only done when every pass condition passes. Pass conditions must
be **falsifiable against a rendered screen or a test assertion** — an exit
criterion nobody can fail is decoration.

Good: *"At 80×24 the Fire buffer contains a weapon-nav token (`↑`/`Up`/`↓`) and
the word `face`."*
Bad: *"Fire is easier to use."*

Order milestones by **causal impact, not corroboration count**. A defect that
makes testers file false reports about *other* parts of the UI outranks a
more-reported cosmetic one, because it contaminates every future run until
fixed.

---

## Known failure modes of this protocol

| Failure | Mitigation |
|---|---|
| Cohort dies mid-run (session limits) | Check capacity; stagger; <8 reports ⇒ inconclusive |
| Harness artifact reported as a game defect | H1–H5 before every cohort; declare artifacts in confounds |
| Harness changed mid-cohort | Cohort void; rerun |
| Uniform cohort finds one defect ten times | Assign the ten personas |
| Testers cheat by reading docs | State the rule twice, in prompt and persona; spot-check reports for knowledge that is not on screen |
| Restart-per-invocation hides late-game friction | Require `--script`; require one tester to reach turn 5+ |
| Findings ranked by popularity | Rank by severity × causality, per the rubric |
