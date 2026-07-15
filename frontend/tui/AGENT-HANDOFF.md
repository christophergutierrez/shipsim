# Agent handoff — build the shipsim TUI (Small tier)

You are implementing a new terminal UI client for shipsim. This file is your
entry point: it tells you what to read, where everything lives, how to work,
and how to prove each step done. Follow it exactly. When this file and your
own judgment disagree, this file wins; when this file and the PRD disagree,
the PRD wins.

## What you are building

A Rust ratatui client in `frontend/tui/` that plays shipsim by spawning the
existing engine binary as a subprocess and exchanging NDJSON lines. First
deliverable: the **Small tier** — a complete playable game (allocate → move →
fire → end turn, to a WON/LOST conclusion) with keypress-primary input and a
map that is always visible. No Medium/Large tiers yet.

## Required reading, in this order

Read these BEFORE writing any code. Do not skim; the answers to most design
questions you will have are already written down here, and inventing a
different answer is the main way this project fails.

1. `frontend/tui/PRD.md` — **the spec.** Every requirement, every decision,
   every accepted downgrade, every assumption. This is the document you are
   implementing. (It is the tracked copy of the Killhouse pipeline artifact
   `.killhouse/artifacts/PRD-tui-small-tier.md`; if both exist and differ,
   ask — do not pick one silently.)
2. `docs/adr/0023-tui-input-layout-and-verification.md` — the accepted
   decisions the PRD builds on (input model, layout, tiers, verification,
   packaging).
3. `frontend/tui/README.md` — the implementation handoff: the six build
   slices, protocol notes, anti-goals, status checklist.
4. `docs/PROTOCOL.md` — the NDJSON wire contract (protocol v3): order shapes,
   snapshot fields, soft errors, the phase loop.
5. `frontend/repl/ASCII-UI.md` §1-2 — presentation vocabulary you must reuse
   (callsigns, facing arrows, shield-face semantics) and the model/view rules.
6. `frontend/README.md` — frontend isolation rules. Short; binding.

Reference, as needed while building:
- `src/snapshot.rs` — exact snapshot field names/types (source of truth).
- `src/movement.rs` — exact order shapes accepted by the engine.
- `frontend/repl/GAMEPLAY.md` — how the game is actually played, phase by
  phase, from a client's point of view.
- `frontend/repl/hexutil.py` — REPL's display-geometry mirror. Read for
  understanding only. **Do not port it** (the PRD rejects a Rust copy).

## Where everything lives

| Thing | Location |
|---|---|
| Repo root | `/mnt/storage/git_home/shipsim` |
| Your entire work area | `frontend/tui/` — you create `Cargo.toml`, `src/`, `tests/` here |
| Engine binary | `target/debug/shipsim` after `cargo build -q` at repo root |
| Scenarios to play | `scenarios/ai.toml` (primary dev/test scenario), `scenarios/combat.toml` |
| Scenario that must be REJECTED by the engine | `scenarios/v2_duel.toml` (no player ship; engine exits with an error — do not "fix" this) |
| Your scratch/session junk | `frontend/tui/local/` (gitignored) — NEVER repo root, never `/tmp` |
| The spec (PRD) | `frontend/tui/PRD.md` |

## How to work

### The slices

Build in this order, one slice at a time, from `frontend/tui/README.md`
("Suggested first implementation slices"), with the PRD's additions:

1. **Skeleton** — package builds; spawns harness; reads post-load snapshot;
   draws turn/phase + ship names; `q` quits.
2. **Map + status** — map panel, callsigns, facing arrows, the status bar
   (turn, phase, cycle N/4, focus ship, queued-shots indicator, notice slot).
   **This slice also measures and sets `dim0`** (the width×height floor) and
   implements below-floor pause / above-floor slack absorption.
3. **Allocate** — the allocate form (engine / per-weapon / six shield faces /
   free-power readout) as a local draft → `allocate` order on commit.
4. **Movement** — maneuver pick (coast / accel / turn + facing) →
   `commit_maneuver`.
5. **Fire** — weapon → target → shield-facing selection → `commit_fire`;
   `ready_fire`; end-of-scenario handling (`status` WON/LOST); combat results
   in notice slot + Combat Log. No to-hit previews (PRD: deliberate).
6. **Polish** — Help panel, `Tab` cycling of reference panels, resize
   hardening. **The deliverable ships at the end of slice 6, not 5.**

One slice = one commit (or a few small ones). Do not start slice N+1 until
slice N's verification passes. Do not reorder.

### Rules that are non-negotiable

- **No `shipsim_core` dependency.** Your crate stands alone. No workspace
  membership in the root `Cargo.toml`. No `include!` of engine source.
- **No rules logic.** Never compute legality, to-hit, damage, or AI moves.
  If an order might be illegal, send it; render the engine's soft error.
- **No imports from `frontend/repl/` or `frontend/love/`.** Pattern reuse
  yes, code reuse no.
- **Touch nothing outside `frontend/tui/`.** No engine changes, no protocol
  changes, no REPL changes. If you believe you need one, STOP and report why
  instead of making it.
- **Every interaction gets a `TestBackend` test.** Key event(s) in →
  assert on the rendered `Buffer` text and resulting app state. A slice
  without its tests is not done. This is the required verification path
  (ADR-0023); a real-pty test is optional and never blocks you.
- **Reuse the game's vocabulary** (from `ASCII-UI.md` / PRD): callsigns
  `A#`/`B#`/`C#`, facing 0..5 board-aligned arrows, ship-relative shield
  faces 0..5, HIT/MISS labels. Do not invent new terms for existing concepts.
- **Color:** honor `NO_COLOR` and `SHIPSIM_TUI_COLOR=0`; monochrome must be
  fully playable.

### The architecture seam (from the PRD — do not deviate)

One `App` type holding snapshot + local UI state, with two entry points:

- key handling: `(KeyEvent, App) -> (App, Option<Order>)` — pure; at most
  one order out per key.
- render: `(&App, &mut Frame)` — pure function of state; rebuilt from the
  latest snapshot every frame; no cached display state that can go stale.

The subprocess adapter (spawn/write/read loop) is a thin shell around this
seam and is the ONLY untested-by-TestBackend code. Terminal input/resize
handling must stay responsive while a subprocess read is in flight (PRD:
"Application structure" — a naive sequential block-on-read loop is a spec
violation because the below-floor message must repaint during a read).

Keep tier-scoped layout constants (`dim0`, region minimums, column widths)
in one place; render reads them, key handling never does.

### Verify every slice

```bash
# engine present and healthy (run once per session, from repo root):
cargo build -q && cargo test -q

# your crate (this is YOUR test command; root cargo does NOT cover you):
cd frontend/tui && cargo build && cargo test

# live protocol sanity (slice 1+): the engine speaks first
echo "" | ./target/debug/shipsim --scenario scenarios/ai.toml --stdin | head -1
# → one JSON snapshot line, protocol_version 3

# manual play (slice 2+): run your binary in a real terminal
cd frontend/tui && cargo run -- --scenario ../../scenarios/ai.toml
```

Every slice's definition-of-done: crate builds, all its `TestBackend` tests
pass, root `cargo test` still passes (proves you didn't touch the engine),
and the slice's feature works in a live run against `scenarios/ai.toml`.

For slice 5's full-loop smoke: play a complete game (the REPL tutorial's
command sequence in `frontend/repl/tutorial.py` `_REAR_ATTACK_STEPS` shows a
known-good order flow you can translate to your input model).

### When you are unsure

- Spec question → the answer is in the PRD (including its "Assumptions and
  Open Questions" section — several were deliberately resolved; respect
  them).
- Protocol question → `docs/PROTOCOL.md`, then `src/snapshot.rs` /
  `src/movement.rs` as ground truth. You may also run the engine binary and
  inspect real snapshots.
- Game-behavior question → play the REPL yourself:
  `python3 frontend/repl/repl.py scenarios/ai.toml` (or with
  `--tutorial rear-attack` for a guided walkthrough).
- Still stuck, or the spec seems wrong/contradictory → STOP, write down the
  contradiction (quote both passages), and report it. Do not silently pick
  an interpretation for anything the PRD marks as a decision.

### Report format (end of each slice)

State plainly: what slice, what was built, test count and result, the exact
commands you ran to verify, anything out of scope you noticed (note it, do
NOT fix it), and any spec ambiguity you hit and how you resolved it.

## Anti-goals (from the ADRs — repeated because agents keep doing them)

- Do not reimplement game rules "just for display."
- Do not add to-hit/damage previews (deferred until a protocol field exists).
- Do not make the engine a dependency to "share types" — parse JSON.
- Do not write scratch files outside `frontend/tui/local/`.
- Do not add your crate to the root workspace "for convenience."
- Do not build Medium/Large layouts, tier switching, mouse support, tutorial
  mode, or campaign UI. Small only.
