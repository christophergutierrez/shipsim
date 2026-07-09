# shipsim Killhouse Handoff

## Current State

- Root: `/mnt/storage/git_home/shipsim`, branch `master` (**not pushed**).
- **Core simulation complete through D10.** Graphical frontend (**D8**) is the intentional stop.
- `cargo test` green; `cargo clippy --all-targets -- -D warnings` clean.

## What the core can do

- Multi-ship fleets (player + AI/scripted), floating or hard maps
- Energy allocate → plot → 32-impulse turn (move, seeking munitions, simultaneous IFF fire)
- SSD internals, drones/plasma, greedy AI, campaign scenario lists (`--campaign`)

## Next phase

**D8 frontend** — choose Love2D / Godot / web; consume JSON snapshots over the existing harness.
See ADR-0004 and `docs/ROADMAP.md`.

## CLI

```text
shipsim --scenario scenarios/fleet.toml --orders …
shipsim --campaign campaigns/demo.toml --orders …
shipsim --scenario scenarios/ai.toml --stdin
```
