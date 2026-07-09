# ADR-0013 -- AI opponent (D9)

Status: Accepted
Date: 2026-07-09

## Context

Non-player ships only followed authored waypoints (`scripted`). ROADMAP D9 asks for an AI
module (NorRust-style greedy seek) that can plot and fight without player input.

## Decision

- Scenario `controller = "ai"` (alias `"greedy"`) registers a ship as **GreedySeek** AI.
- `controller = "scripted"` (or non-empty waypoints) keeps waypoint scripts.
- `src/ai.rs` owns pure-ish helpers: seek target selection, greedy path building, fire choice.
- On `RunTurn`, before impulses: every NPC without a player-submitted plot auto-plots; AI also
  auto-issues legal `Fire` if a weapon can bear and energy allows.
- Seek target: nearest non-friendly living ship (lowest id on tie). If none, hold position.
- Path: same adjacency/occupancy/turn-mode rules via `movement::validate_plot` shortening.
- Fire: first operational weapon that passes `fire_legality` against the seek target.

## Consequences

- 1v1 AI duel scenarios work through the CLI without orders for the enemy.
- Stronger AI (search, energy strategy) can replace GreedySeek without touching the turn loop.
