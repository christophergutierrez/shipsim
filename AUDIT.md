# REPL interface audit

Audit date: 2026-07-12. The audited client is `frontend/repl/repl.py`; the Rust
binary is the NDJSON engine transport and is not itself a human command line.
No dependency was added.

## Phase 1 — cold play findings

### R-001 — launch path is not self-explanatory (blocks-discovery) — fixed

Reproduction:

```text
$ cargo run --bin shipsim
--scenario, --campaign, or --resume is required
$ cargo run --bin shipsim -- --scenario scenarios/v2_duel.toml --stdin
{"protocol_version":2,...}
```

Expected: a first-time player launching the game gets a human orientation.
Actual: the default binary requires undocumented flags and then emits JSON;
the human client is a separate Python entry point. The REPL now prints the
objective, `help`/`?`, and the state carried by the prompt on startup. The
REPL launch command is documented in the existing frontend docs.

### R-002 — help was context-trapped (blocks-discovery) — fixed

Before:

```text
t1/allocate@1 draft0/22> help
  draft: mov N | w [alias N] | sh [face N] | show | reset | commit | cancel
  ... for global help ... type 'help' again ...
t1/allocate@1 draft0/22> help
  draft: mov N | w [alias N] | sh [face N] | show | reset | commit | cancel
```

Expected: `help` and `?` always expose the command index; `help <command>`
includes syntax, an example, and purpose. Actual: draft help was printed and
the second help did the same. `render_help()` now generates the index from
`COMMAND_REGISTRY`, is globally dispatched before draft parsing, and supports
topics such as `help fire`.

After:

```text
t1/allocate@1 draft0/22> ?
shipsim REPL — objective: destroy the opposing fleet.
The prompt shows turn, phase, focus, and remaining actions. Type help <command> for details.
Commands:
  fire | attack | f                  choose a charged weapon and target
  ...
t1/allocate@1 draft0/22> help fire
  fire | attack | f
    choose a charged weapon and target
    example: attack (then choose a charged weapon and target)
```

### R-003 — invalid input did not teach the player (friction) — fixed

Before:

```text
t1/movement@1*1> attack
  unknown command; try help
t1/movement@1*1> <empty line>
[frame redraw only]
```

After:

```text
t1/allocate@1 draft0/22> 
  no command entered. Type help (or ?) to see commands; try hint for the next action.
t1/allocate@1 draft0/22> atack
  unknown command 'atack'. Did you mean 'attack'? Type help for commands.
```

The parser remains case-insensitive. `attack`/`a` are accepted in firing;
`a` remains allocation in allocation phase.

### R-004 — current action state was not persistent (friction) — fixed

Before:

```text
t1/movement@1*1> 
movement 1/4: ... thrust=4
```

Actual state existed in the frame, but the prompt did not expose remaining
actions. The prompt and header now show `actions=power:N`, `actions=thrust:N`,
or `actions=charged:N`, alongside turn, phase, and focus.

### R-005 — EOF in the fire picker could crash (crash) — fixed

Before:

```text
firing: pick weapons to fire, [-1] Done.
EOFError: EOF when reading a line
```

The picker now catches EOF/interrupt and leaves the picker safely without
sending a shot. The normal command loop already catches EOF at its prompt.

### R-006 — fire goal was not discoverable from the natural word (friction) — fixed

Before:

```text
t1/movement@1*1> attack
  unknown command; try help
```

After: `attack`, `fire`, `f`, and (in the firing phase) `a` open the same fire
flow. `help fire` documents the flow.

### R-007 — confirmation prompts crashed on EOF outside the fire picker (crash) — fixed

R-005 only wrapped the top-level `interactive_fire` call and the main command
prompt in `try/except (EOFError, KeyboardInterrupt)`. Every other bare
`input()` call in the codebase was still unguarded: the quit confirmation,
the end-turn-mid-firing confirmation, the commit-empty-allocate confirmation,
the scenario picker, and — critically — `_prompt_int`, which backs the
weapon-charge/shield-charge prompts inside the allocate draft *and* the
weapon/target/shield prompts inside the fire picker. Any of these hitting
EOF (piped/redirected stdin running dry, e.g. a scripted session or a closed
terminal) crashed with a raw traceback instead of a clean exit.

Reproduction (before fix):

```text
$ python3 repl.py scenarios/combat.toml <<< $'w\nb1'
...
t1/allocate@1 draft0/22/w>   charge b1 (0..4) [0]: Traceback (most recent call last):
  File "commands.py", line 511, in _prompt_int
    raw = input(f"{msg} [{default}]: ").strip()
EOFError: EOF when reading a line
```

Fixed by wrapping every remaining `input()` call site
(`_prompt_int`, quit confirm, end-turn confirm, commit-empty confirm, scenario
picker) in the same `try/except`. Confirmations default to their safe choice
on EOF (quit → yes, since stdin is gone and there is no way to keep playing;
end-turn/commit-empty → no/cancelled, the non-destructive choice); `_prompt_int`
returns its stated default. Verified no traceback on any of these paths after
the fix; added regression coverage in `test_interface_audit.py`
(`ConfirmPromptEOFSafety`).

### R-008 — ending the turn mid-firing silently discards queued shots (wrong-info / friction) — fixed

Found via an independent cold-play pass by a second, code-blind agent: after
queuing a shot with `f`, using `end` to leave the fire phase (rather than
`ready`/`nofire`) advances straight to the next turn with the weapon's charge
spent but **no HIT/MISS line, no hull change, and no `combat_log` entry at
all** — the engine drops any `fire_commits` that were never resolved via
`ready_fire`. The existing `end_turn` confirmation only warned that it ends
the whole turn, not that it forfeits the shot with zero record of what
happened to it — a player would reasonably read "shot queued — resolves at
end of fire phase" and expect their shot to still land.

Reproduction (before fix), confirmed against the raw NDJSON protocol
directly (bypassing the REPL) to rule out a client-only rendering gap:

```text
>>> commit_fire  ship=1 weapon=beam_1 target=2   (fire_commits now has 1 entry)
>>> end_turn                                      combat_log= []   (turn 1 → 2, shot vanished)
```

Fixed in two places:
- `commands.py`: the end-turn-mid-firing confirmation now inspects
  `snap["fire_commits"]` and, if the focused ship has unresolved shots,
  names them explicitly and states they will be **DISCARDED, not fired**.
- `repl.py`: the "shot queued" acknowledgment now says it resolves "once
  every ship readies" (not the vaguer "at end of fire phase", which reads as
  synonymous with the `end` command) and calls out that `end_turn` instead
  will discard it.

After:

```text
t1/firing@1/r=done actions=charged:1> end
  end_turn ends the WHOLE turn, not the fire phase.
  To leave firing without shots: ready / nofire
  warning: queued shot(s) not yet resolved will be DISCARDED, not fired: beam_1→#2.
  Use ready/nofire to resolve them first.
  type yes to end whole turn: no
  cancelled
t1/firing@1/r=done actions=charged:1> r
A1 beam_1 → C2  HIT for 8  on shield 0:F
```

Regression coverage added in `test_interface_audit.py`
(`EndTurnDiscardsQueuedShotsIsWarned`). This is a UI accuracy fix, not an
engine rule change: whether `end_turn` *should* resolve pending fire_commits
is an engine-rules question left untouched; the REPL now truthfully
describes the engine's actual (discard) behavior instead of implying the
shot will still land.

## Phase 2 — engine truth and drift review

### T-001 — help duplicated parser vocabulary (wrong-info) — fixed

The old `HELP` block and draft-help branch were maintained separately from
`build_action`. The command registry is now the source for generated global
help; `HELP` is only a compatibility alias to the generated result. The
golden test checks every registry syntax and description appears in help.

### T-002 — displayed ship values and combat geometry (wrong-info) — fixed/already safe

`view.py` reads turn/phase/status, ship power, velocity, thrust, hull, shield
arrays, weapon charge/range, positions, and combat outcomes directly from the
engine snapshot. `hexutil.py` computes range, arcs, and legal shield facings;
the fire picker uses those helpers rather than UI constants. Allocation uses
`power_available`, weapon `max_charge`, and `max_shield_per_facing` from the
snapshot. No new duplicated gameplay constants were introduced by this fix.

### T-003 — movement schedule is a derived explanatory table (deferred)

The REPL displays the velocity schedule `0:none, 1:4, 2:2,4, 3:1,2,4,
4:1,2,3,4`, matching `src/motion.rs::translation_phases` and its unit tests.
The engine snapshot currently exposes velocity but not the schedule itself.
This remains a small presentation debt: a future protocol snapshot field could
remove the duplicated explanatory mapping without changing rules.

## Regression and golden evidence

`frontend/repl/tests/test_interface_audit.py` covers generated help, topic
help, `?`, case-insensitive `ATTACK`, blank input, typo suggestions, EOF
safety on every confirmation prompt (`ConfirmPromptEOFSafety`), and the
end-turn discard warning (`EndTurnDiscardsQueuedShotsIsWarned`). The existing
tests cover the live move/ready/end order path and engine error rendering.
Full suite result after the R-007/R-008 fixes:

```text
Ran 96 tests ... OK
```

## Fuzz gate

256 lines of blank, typo, truncated, wrong-arity, malformed-quote, negative,
out-of-range, and oversized-integer command input were passed through
`build_action` across all four phases (allocate/movement/firing/turn_end),
plus a second pass of garbage and EOF-mid-picker input driven directly at
`interactive_fire` (weapon/target/shield-facing prompts).

```text
fuzz_lines=256 (build_action) + garbage/EOF sequences (interactive_fire)  crashes=0
```

Every line produced either an actionable parser message or a structured
action; there were no silent exceptions or unhandled tracebacks.

## Cold-start gate transcript

Two cold-start passes were run.

**Pass 1 (scripted, pre-existing):** an interface-only scripted cold session
completed repeated full turn cycles and reached game over using only prompts
and commands shown by the REPL:

```text
turn 1  phase=allocate  status=InProgress ... actions=power:22
turn 1  phase=movement ... actions=thrust:16
turn 1  phase=firing ... actions=charged:1
A1 beam_1 → B2 MISS on shield 0:F
turn 1  phase=turn_end status=InProgress
turn 2  phase=allocate status=InProgress ... actions=power:22
turn 3  phase=turn_end status=Lost
SCENARIO LOST
```

**Pass 2 (independent agent, no code/docs access):** a second agent, given
only the launch command and told not to read any source or documentation,
played `scenarios/combat.toml` cold and deliberately tried to break the
REPL (blank lines, typos, wrong-arity commands, out-of-range coordinates,
quitting mid-game). It completed the allocate → movement → firing →
ready/end loop for 4 turns using interface output alone, and surfaced two
real defects the scripted pass had not exercised:

1. Two reproducible EOF crashes (confirming turn-end and quit with no more
   input available) — R-007.
2. Ending the turn mid-firing after queuing a shot left both ships at
   `hull=12/12` with no hit/miss feedback across 4 turns — R-008. Root cause
   confirmed against the raw NDJSON protocol: `end_turn` discards
   `fire_commits` that were never resolved via `ready_fire`.

Both were fixed (see R-007, R-008 above) and re-verified by hand: the same
input sequences the agent used no longer crash, and ending the turn with a
pending shot now names the shot and warns it will be discarded, with `ready`
still resolving it and showing `A1 beam_1 → C2 HIT for 8 on shield 0:F` /
`hull 12→9` when the player chooses to resolve rather than discard.

The player could see the objective, command discovery, phase transitions,
remaining actions, weapon result, and terminal state without consulting docs.

## Truth gate

Three sampled snapshot states were checked against engine-derived values:

1. A1 Heavy Cruiser: power 22, beam max charge 4/range 10, shield max 6.
2. B2 Escort: power 14, beam max charge 4/range 10, shield max 6.
3. A1 after allocation: thrust 4, shield face 0 powered 3, beam charge 2.

The values matched the NDJSON snapshot and the corresponding source fields
(`allocate_v2`, ship definitions, `motion::translation_phases`, and
`hexutil` geometry helpers).

## Summary

Top player-impact issues, in order of severity: (1) confirmation prompts
crashing on EOF anywhere outside the fire picker (R-007) — a real crash class
in scripted/piped play, not just the one path R-005 covered; (2) ending the
turn mid-firing silently discarding a queued shot with zero feedback, which
made combat look broken ("no shot ever lands") for a player who reaches for
`end` instead of `ready` (R-008); (3) the undocumented launch boundary
(R-001); (4) trapped help (R-002); (5) non-actionable unknown/empty input
(R-003). The fixes add startup orientation, generated help, aliases and
suggestions, persistent action counts, EOF-safe confirmations everywhere
(not just the fire picker), an accurate discard warning before an
irreversible action, and golden/regression tests for all of the above.

R-007 and R-008 were found by a second, independently-run cold-play agent
with no access to the code, docs, or this audit — confirming the Phase 1
pass alone was not exhaustive; nested interactive prompts and multi-step
irreversible actions are exactly where a single audit pass tends to miss
gaps. Deliberately deferred debt is only exposing the engine's movement
schedule directly in the snapshot (T-003); the current table matches the
engine's tested rule.
