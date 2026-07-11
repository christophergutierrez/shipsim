# shipsim - Roadmap and Deferred Backlog

The current product definition is `docs/PRD.md`; system structure is documented in
`docs/ARCHITECTURE.md`. Earlier rulesets remain recorded in ADRs but are not active product modes.

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
| Arch | Encapsulation, turn module, pure combat | `docs/ARCHITECTURE.md` |

## Rules pivot (next major effort)

### Combat model v2  REALIZED MVP (ADR-0020)
- Momentum moves; allocate move/weapons/shields; move-phase ↔ fire-phase loop; d20 range tables.
- Supersedes Bocchino 3-round MVP (ADR-0019) and SFB impulse (ADR-0002) as product target.
- PRD: `docs/PRD.md`.
- Milestones M0–M9 complete: pure rules modules, data model + Allocate, movement + momentum, firing + simultaneous resolve, turn loop + win, AI + multi-ship, FASA/legacy deletion, Love client v2, acceptance package.
- Acceptance: `tests/acceptance.rs` (two move cycles, deterministic seed 4242) + golden fixture `tests/fixtures/v2/duel.jsonl` (byte-lock).

### Historical: FASA / Bocchino MVP (ADR-0019)
- 3-round action loop briefly implemented; superseded by ADR-0020.

### Historical: SFB impulse core (ADR-0002 path)
- Superseded; legacy tests removed.

## Stop line for prior effort

### D8. Graphical frontend - REALIZED (ADR-0017, ADR-0018)
- **Love2D** at `frontend/love/` over **JSON orders-file replay**; no rules in Lua.
- Soft-error NDJSON + `controller` on snapshot. Scenario picker, phases, multi-ship, win/defeat.
- Launch: `cargo build && love frontend/love`. Headless: `luajit frontend/love/tests/run_all.lua`.
- Golden replay: `tests/fixtures/v2/duel.jsonl`. FFI deferred.

## Residual polish (not blocking core)

- **D7+** EW / overload / full paper EA Form UX
- **D5a+** Multi-turn plasma arming, more seeking classes
- **D10+** Campaign persistence, branching campaigns, multi-fleet design tools
- **Combat logging** — turn combat_log in snapshot (done basic); richer replay UI later
- **PRNG resume** — `prng_state` + `set_prng_state` (done); full save/load document format later
- **AS2** geometry DRY optional
- **Impulse-by-impulse stepping** frontend concern (T4)
- **Core `Lost` status** — first-class defeat in snapshot (D8 uses client-derived defeat for now)

---

*When picking up residual polish, prefer small commits against the core API above.*
