# Document lifecycle: what gets tracked

Most working documents are **ephemeral** — true only until the work is done.
Tracking them rots the repo: a plan that says "M3 in progress" is wrong the day
M3 lands, and every later reader has to guess whether it still applies.

This file defines what belongs in git, what does not, and the check that
enforces it.

## The test

> **Will this file still be true after the work it describes is finished?**

- **Yes** → durable. Track it.
- **No** → ephemeral. Keep it out of git.

A second, sharper form for the ambiguous cases:

> **Does it describe *what the system is*, or *what someone intends to do*?**
> The first is durable. The second is ephemeral.

## Durable — track these

| Kind | Examples |
|---|---|
| Decisions | `docs/adr/*` — an ADR stays true; it records a decision *and its date* |
| Contracts & formats | `docs/PROTOCOL.md`, `docs/SAVE-FORMAT.md`, `docs/ARCHITECTURE.md` |
| Requirements / rationale | `docs/PRD.md`, `frontend/tui/PRD.md` — *why* the thing exists |
| Reference tables | `docs/combat-v2-tables.md`, `docs/SIZE-VARIANTS.md` |
| Rubrics & protocols | `docs/UI-RUBRIC.md`, `docs/UI-PLAYTEST-PROTOCOL.md`, `docs/BALANCE-PROTOCOL.md` |
| How to use / run | `README.md`, `AGENTS.md`, `docs/AGENT-PLAY.md`, per-client `README.md` |
| Living status | `docs/TODO.md` — the single open-work list (durable *because* it is maintained) |
| Deliberate archives | `docs/history/*` — explicitly labelled historical records |

## Ephemeral — do not track

| Kind | Signals |
|---|---|
| Plans | `*-PLAN.md`, `docs/plans/*`, anything with milestones `M0…Mn` |
| Handoffs | `*HANDOFF*` |
| Reviews / findings | `*REVIEW*`, `*FINDINGS*`, `*RECOMMENDATIONS*`, `*VERDICT*` |
| Logs / inventories | `*-LOG.md`, `*INVENTORY*` |
| Dated working files | anything with `-20260714`-style stamps |
| Playtest output | cohort reports, per-run findings |

Put these under `tmp/` (git-ignored). Nothing is lost — they stay on disk and
in git history if they were ever committed.

## Mostly ephemeral: split it

A plan often carries one or two durable facts — a field contract, a formula, an
invariant. **Move the fact, drop the plan.** Do not track the plan to preserve
the fact.

Worked example from the 2026-08-15 cleanup: `docs/SHIP-ART-IMPLEMENTATION-PLAN.md`
was cited from `src/ship.rs`, `src/snapshot.rs`, and five tools. The durable
content was the `class_id` contract, which now lives in `docs/PROTOCOL.md`'s
snapshot-field table and `frontend/love/assets/ship_art/README.md`. The
citations were repointed there and the plan was archived.

**Rule: before deleting, grep for inbound references.** A removed doc that is
still cited leaves a dangling pointer, which is worse than the stale doc.

```bash
git grep -l "THE-DOC.md" -- '*.md' '*.rs' '*.py' '*.lua'
```

## Where plans live instead

- Active plan for work in progress: `tmp/` (or any ignored scratch dir).
- Finished plan worth remembering: it isn't. The *outcome* belongs in an ADR,
  the contract docs, or `docs/TODO.md`. The task list does not.
- Genuinely notable campaign records: `docs/history/`, explicitly labelled, and
  only by deliberate choice.

## The check

`tests/doc_lifecycle.rs` enforces this in three ways:

1. **Tracked ephemeral docs** — a plan/handoff/review/log/etc. committed to git.
2. **Date-stamped working files** — `-YYYYMMDD` in the name.
3. **Ephemeral scratch in a durable directory, tracked or not** — an untracked
   working file can sit in `docs/` forever and no git-based check will see it.
   That is how a stale `docs/HANDOFF.md` survived the first cleanup pass:
   untracked, invisible to the guard, and still the first thing a reader saw.

There is a small allowlist for durable files whose names would otherwise trip a
marker (e.g. `docs/history/*`).

Run it with the normal suite:

```bash
cargo test --test doc_lifecycle
```

If it fails on a file you believe is durable, either rename it to describe what
the system *is* rather than what someone will *do*, or add it to the allowlist
in that test with a one-line justification. Adding to the allowlist without a
reason is how the rot comes back.
