# shipsim — CONTEXT

> A hex-based tactical starship-combat game *in the spirit of* Star Fleet Battles, built as a
> pure-Rust headless simulation core with a thin, deferred frontend. Designed from day one for
> full-fidelity SFB-style rules, but delivered in minimal vertical slices.

Status: **Combat model v2 PLAN ready — Checkpoint**.
- ADR-0020 Accepted. PRD: `docs/PRD-combat-model-v2.md`.
- Plan: **`implementation-plan-combat-v2.md`** (READY, M0–M9).
- Autonomy: **checkpoint**. Policy: **cost_optimized**.
- Next: IMPLEMENT M0/M1 when user continues (fresh session: read plan bootstrap section first).
- FASA MVP still in tree until v2 cutover (plan M7).

### Combat model grill (live)

**Turn loop (decided — refined B):**
1. **Turn start:** allocate power to **movement**, **specific weapons**, and **specific shields**.
2. **Loop** until no ship has useful move or fire left (or all pass / end turn):
   - **Movement phase:** each ship with ≥1 movement power may spend **1** to perform one move (momentum rules later).
   - **Firing phase:** ships with powered weapons in arc/range may **commit** to fire; commitments lock when all have chosen; **resolve simultaneously**.
3. Player may **end turn early** while still having move/weapon power → **warn** about leftover power. No warning if nobody has a legal action left.

**Movement phase (decided — A):** Sort by allocated movement power (high first). Each ship with remaining move power may take **exactly one** step (or pass), then go to Firing phase. Repeat outer loop. (Pass allowed: spend 0, stay put.)

**Firing phase (decided — A):** Each ship may commit any number of legal powered weapons (powered, not yet fired this turn, arc+range OK). Simultaneous resolve from pre-fire state. No legal shot → commit nothing.

**Shields (decided — A, clarified):** Per-facing powered pools at turn start (facings 1–6). Hit facing absorbs until empty, then hull. Unpowered = 0. No carry-over. Next turn repower up to ship max per facing. Hull 0 = destroyed.
- **On fire commit:** attacker chooses **target ship and which facing shield** is struck. Geometry constrains legal facings (often two candidates; point-blank usually one). Only a legal facing for that geometry may be chosen.

**Weapon power (decided — A, clarified):**
- **Torpedo:** charge cost always **1**; damage **fixed** (not range-scaled).
- **Beam:** charge **1+** (ship/weapon max); damage scales with charged power (multiple TBD); **all-or-nothing** — when it fires, it spends the full charge in one shot (cannot split 3 into three 1-point shots).
- **Plasma:** charge **fixed >1** (value TBD); damage profile separate (high close, steep falloff); not variable like beams.
- Weapon fires **at most once per turn** after resolve.

**Momentum (decided — A):** Track last keel state: forward / stopped / reverse. Continuing forward (and forward±60° if allowed) costs **1** move power. Reversing keel direction costs **2** (illegal if only 1 left). Turn in place costs **1**, does not flip keel. Start of turn = stopped.

**Movement initiative (decided — A):** Sort **once per turn** after allocation by **movement power allocated** (high first). Ties: one coin flip, order frozen for the turn. Same order every movement phase.

**Weapon arcs (decided):** Each weapon has a **90°** fire arc (read as degrees, not percent). Mount points (relative to ship heading):
- forward, forward port, forward starboard
- aft, aft port, aft starboard
(Exact mapping onto 6 hex facings to be fixed at implement; 90° ≈ 1–2 hex sides.)

**Product target (decided — A):** This grilled model **replaces** Bocchino/FASA 3-round MVP (ADR-0019) as shipsim’s combat end-state. New ADR at grill convergence; keep hex board, generic ships, thin Love client, headless tests.

**Power allocation (decided — A):** Single pool `power`. At turn start: movement + weapon charges + six shield facings ≤ power. Unspent idle. Allocation locked for the turn (no mid-turn reallocation).

**Map scale (REVISED — double tight):** Typical engagement ~1–12 hexes; maps ~20–28. Max ranges: beam **10**, plasma **14**, torp **12**.

**Combat tables (REVISED — working MVP):**
- To-hit: **d20**, roll ≤ threshold (decline shape like prior d10 table, stretched over doubled ranges).
- **Torp:** charge **1**, damage **4** flat at all in-range hexes.
- **Plasma:** charge **1**, warhead **8** at r1 (~2× torp); falls to ~torp mid-range, **worse than torp** at long; 0 past max.
- **Beam:** charge **1..max**, all-or-nothing on fire; damage = `charge × beam_factor(range)` (factor **2** at PB → ~**1** at max range). Round **half-up** (≥0.5 → up). If rounded damage **< 1**, shot is **illegal** for that charge (effective OOR); more charge can make same geometry legal. 2 charges = 2× before round.
- To-hit **d20**; map/ranges doubled (beam 10 / plasma 14 / torp 12). Torp dmg 4 cost 1; plasma cost 1 warhead 8 at r1 with steep falloff.
- Numeric tables: grill working set (tune in play).

**Ship power MVP (decided — A):** Escort **14**, Cruiser **22**. Hard tradeoffs. Sample mounts later in data files.

**Win (decided — A):** Hull 0 = destroyed. Scenario win via destruction targets / wipe. Disengage/objectives later.

**Logged assumptions (implement defaults):**
1. Hex arc mounts map to 1–2 of 6 facings relative to heading (implementer picks consistent table).
2. Sideslip = continue-forward family at cost 1 if implemented; else forward + turn only for first slice.
3. Simultaneous fire: compute all hits from pre-fire snapshot, then apply (deterministic ship/weapon id order for PRNG).
4. AI uses same rules; greedy allocate/move/fire.

Origin: greenfield in `shipsim/`. Architectural template: sibling project **NorRust**
(`../norrust`) — pure-Rust headless core + thin client over a JSON/C-ABI bridge + TOML data +
heavy headless tests.

### D8 frontend grilling (live)

- **Stack / integration:** Love2D thin client; JSON subprocess to `shipsim` (ADR-0017). No rules in Lua.
- **Shippable end state:** **C if reachable** — full scenario play to win/loss in Love (not view-only;
  not campaign). Stretch past a single turn cycle: multi-turn until `status` is terminal.
- **Illegal orders:** **soft reject** — harness stays alive on illegal order (no process death).
- **Error envelope (wire + UI):** structured NDJSON error line:
  `{"type":"error","ok":false,"code":"…","message":"…","order":…?,"source":"harness"|"client"}`.
  Success remains bare `StateSnapshot` lines. UI: one shared status strip (error/warn/info) for
  harness rejects, spawn failures, parse errors — same pattern throughout.
- **Turn UX:** **guided phases** — Energy → Plot → Weapons → Resolve (`run_turn`). Phase bar;
  client enables suggested next actions; core remains authority. AI ships resolve inside `run_turn`.
- **Scenario start:** **in-game picker** listing `scenarios/*.toml` (menu before play). Not
  hardcoded single scenario; harness still launched with the chosen path.
- **Multi-ship:** active-ship selection for multi player ships; allocate/plot/fire target the
  selected ship; one Resolve (`run_turn`) for the turn. **ShipSnapshot gains `controller`** so the
  client does not parse scenario TOML for ownership.
- **Input:** **mouse primary + keyboard shortcuts (C)** — click hex path / weapon+target; keys
  for facing/step, ship cycle, phase confirm, etc.
- **Visuals (D8 bar):** **debug geometry (A)** — hex grid, simple ship markers, text HUD. Art
  polish deferred after play-to-end works.
- **Tree layout:** **`frontend/love/`** (not `shipsim_love/`). Room for other frontends later
  under `frontend/`; decoupling stays protocol-level (JSON), not a separate repo.
- **Binary discovery:** prefer repo `target/debug/shipsim` (or release) from Love client;
  override `SHIPSIM_BIN`; scenario paths relative to **repo root** as harness cwd.
- **Energy phase UI:** **minimal (A)** — steppers for movement/weapons/shields, power remaining,
  default-fill + confirm → `allocate`. No full EA form.
- **Scenario end:** victory on core `Won`; **client-derived defeat** when all player ships
  (`controller == "player"`) are destroyed → end screen → scenario picker. **Core `Lost` status
  deferred** (roadmap residual; not required for D8).
- **Hex drawing:** **flat-top** (frontend-only; core orientation-agnostic).
- **Agent autonomy:** batch work with headless gates; human only for visual polish.

### D8 logged assumptions (low-risk defaults)

1. JSON in Love via a vendored pure-Lua decoder/encoder (or Love-compatible lib); no FFI for JSON.
2. `run_turn` is atomic in the UI (no impulse-by-impulse animation in D8; T4 residual).
3. Camera: pan + zoom enough for large maps; no fancy follow-cam required.
4. Scenario picker reads filenames from `scenarios/` relative to repo root (same cwd as harness).
5. Human assistance only for visual readability after automated gates pass.

---

## Vision

A two-player, hex-grid tactical game of starship maneuver and combat, modeled closely on the
*mechanics* of Star Fleet Battles (impulse movement, ship facing, 6-shield-facing combat, energy
allocation) but using **our own generic ship classes and stat values** — not ADB's trademarked
names or published numbers. The end state is high-fidelity SFB-style play; we get there through
small, individually shippable slices, never by boiling the ocean.

## Architecture (decided)

Mirror NorRust's separation, **core-first with the frontend deferred**:

- `shipsim_core/` — pure-Rust headless simulation library. No rendering, no display deps.
  - Rust 2021, `crate-type = ["cdylib", "rlib"]`, `panic = "abort"`.
  - Deps: `serde` + `serde_json` + `toml` + `thiserror` (mirror NorRust).
  - Module shape (target, filled in as slices land): `hex`, `board`, `ship`, `movement`,
    `game_state`, `snapshot`, `save`, `scenario`, `loader`, `schema`, `ffi`, later `combat`,
    `energy`, `ai`.
- **CLI/JSON harness** (Phase 1 frontend) — a small `shipsim` binary that loads a scenario, applies
  orders (from a scripted orders file and/or stdin JSON), and emits state snapshots as JSON.
  Deterministic and test-friendly.
- **Graphical frontend (D8):** Love2D thin client over JSON harness (ADR-0017/0018). PRD ready.
- Content is **data-driven**: ships and scenarios loaded from TOML at startup (`data/ships/*.toml`,
  `scenarios/*.toml`).
- **Headless `cargo test` is the verification surface.** Every rule is provable without any
  frontend.

## First slice — Movement Skeleton (decided)

The smallest honest vertical slice. **No combat.**

In scope:
- Hex map + hex coordinate math + facing (6 directions).
- Turn structure with a ship "speed" (max hexes/turn).
- Two ships on the map: a **player-plotted** ship and a **scripted** ship.
- A scenario (TOML) defining map size, ship placements, the scripted ship's waypoints, and an
  **objective hex**.
- Terminal condition: **a ship entering the objective hex resolves the scenario as WON** (falsifiable).
- CLI/JSON harness to drive it; headless integration test that plots a path to the objective and
  asserts the scenario resolves `WON`.

Explicitly deferred (but designed-for — see ADR-0002):
- The full 32-impulse turn / Impulse Movement Chart (slice uses simplified N-hexes/turn).
- Simultaneous plot-then-resolve movement (slice uses sequential/alternating resolution).
- SFB turn-mode constraint (slice allows free turning; `turn_mode` field exists, unenforced).
- All combat: phasers, disruptors, shields, damage allocation.
- Energy Allocation.
- AI opponent (slice uses scripted waypoints).

## Decisions log (see docs/adr/ for detail)

- **ADR-0001** — Mirror NorRust: pure-Rust headless core + thin client + JSON/C-ABI bridge + TOML.
- **ADR-0002** — Design for full SFB fidelity, implement minimal slices. Turn-structure and
  move-resolution abstractions must accept the 32-impulse chart and simultaneous resolution later
  without rework.
- **ADR-0003** — Generic, SFB-inspired ship data. No ADB-trademarked names or published stats.
- **ADR-0004** — Frontend deferred; Phase 1 is a CLI/JSON harness. Graphical frontend chosen later.

## Logged assumptions (low-risk defaults; revisit if wrong)

1. **Hex coordinates**: axial storage with cube conversion (standard hex math); flat-top vs
   pointy-top is a frontend concern, core is orientation-agnostic.
2. **Map**: bounded rectangle, dimensions from the scenario TOML, **no wraparound**; moving off-map
   is an illegal order. (SFB's fixed floating map can be revisited when it matters.)
3. **Harness I/O**: core exposes an order-application API (`apply_order` / `step`); the CLI can run a
   scenario to completion from a scripted orders file or accept orders as JSON on stdin.
4. **Determinism**: no RNG in the movement skeleton — fully deterministic, so tests assert exact
   end-states.
5. **Ship "speed" in slice 1**: a fixed per-ship max hexes/turn from the ship TOML (energy-driven
   speed arrives with Energy Allocation).

## Open (later slices, not blocking)

- Impulse chart fidelity (ADR-0002 target), simultaneous movement, turn-mode enforcement.
- Combat model + Energy Allocation (the two big subsequent slices).
- Frontend selection (ADR-0004).
- Multi-ship / scenario & campaign structure beyond 1v1.
