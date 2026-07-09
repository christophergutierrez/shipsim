# shipsim — Roadmap & Deferred Backlog

Legend: REALIZED · 🎯 later · ▶ residual polish

---

## Core simulation (REALIZED)

| ID | Item | Notes |
|----|------|--------|
| Slice 1 | Movement skeleton | Superseded by Slice 3 |
| D5 | Direct-fire combat | Phaser/disruptor, shields, PRNG |
| D1/D2/D3 | Impulse movement fidelity | IMC, Plot/RunTurn, turn-mode |
| D1-fire | Impulse-gated fire | ADR-0010 |
| D2-fire | Simultaneous fire | ADR-0011 |
| D6 | SSD damage allocation | ADR-0012 |
| D7 | Multi-bucket energy | ADR-0009 + deepen; residual polish below |
| D5a | Seeking weapons | Drone + plasma (ADR-0014); arming residual |
| D9 | AI opponent | Greedy seek (ADR-0013) |
| D4 | Map edge policy | Hard vs floating recenter (ADR-0015) |
| D10 | Fleets + campaigns | fleet.toml, campaigns/, CLI `--campaign` (ADR-0016) |
| Hygiene | Multi-ship readiness | Fire ship id, Terminal, BTreeMap, load checks |
| Arch | Encapsulation, turn module, pure combat | ARCHITECTURE-slice3 + state plan |

## Stop line for this effort

### D8. Graphical frontend  🎯 **NEXT product phase** (ADR-0004)
- Love2D vs Godot vs web. Core emits JSON snapshots; no rendering in this crate.

## Residual polish (not blocking core)

- **D7+** EW / overload / full paper EA Form UX
- **D5a+** Multi-turn plasma arming, more seeking classes
- **D10+** Campaign persistence, branching campaigns, multi-fleet design tools
- **Combat logging** — turn combat_log in snapshot (done basic); richer replay UI later
- **PRNG resume** — `prng_state` + `set_prng_state` (done); full save/load document format later
- **AS2** geometry DRY optional
- **Impulse-by-impulse stepping** frontend concern (T4)

---

*When picking up residual polish, prefer small commits against the core API above.*
