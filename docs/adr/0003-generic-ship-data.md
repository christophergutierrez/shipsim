# ADR-0003 — Generic, SFB-inspired ship data (no trademarked content)

Status: Accepted
Date: 2026-07-04

## Context

"Star Fleet Battles", its ship names, and its published stat values are the intellectual property
of Amarillo Design Bureau. shipsim models SFB-*style mechanics*, not SFB itself.

## Decision

All shipped content uses **generic ship classes and our own stat values** — e.g.
`data/ships/heavy_cruiser.toml` with a name like "Heavy Cruiser" and shipsim-chosen numbers. We do
**not** ship ADB-trademarked ship names or reproduce SFB's published stat blocks.

## Consequences

- The project can be shared without reproducing trademarked/copyrighted content.
- Balance is ours to define and tune (and to test — see ADR-0001's headless test surface).
- Anyone wanting SFB-authentic values can supply their own private TOML; the engine is data-driven,
  so that is a personal-use data choice, not a code change.
