# ADR-0010 -- Impulse-gated fire (D1-fire)

Status: Accepted
Date: 2026-07-09

## Context

Slice 3 deferred impulse-gated fire: all Fire orders resolved at turn end after movement.
With a 32-impulse turn, combat should discharge only on weapon fire windows so timing matters.

## Decision

- `combat::fires_on_impulse(kind, impulse)` defines the simplified IFF:
  - Phaser: impulses 4, 8, 12, ..., 32
  - Disruptor: impulses 8, 16, 24, 32
- `Fire` still declares and queues (and spends weapon energy) at declare/resolve time.
- During `RunTurn`, each impulse: movement steps first, then any queued fires whose weapon
  class may fire on that impulse resolve with **current** geometry.
- A given queued shot resolves at most once (first matching window).
- Unmatched pending fires after impulse 32 are discarded (shipped windows always include 32).

## Consequences

- Mid-turn movement can change range/arc before a shot lands.
- Simultaneous fire (D2-fire) remains deferred; fire order is declaration order within an impulse.
