# Plan: Catalog, combat kit, squads, budget fleets

Finish `docs/TODO.md` and the locked combat additions. Size is a fleet
tradeoff; the yard is the only construction model; v4 allocate / path /
volley stays.

**Implementation status:** M1–M9 engine work is on the current branch. The
remaining M10–M13 balance work is governed by
`docs/plans/catalog-review-remediation.md`; this file remains the locked design
record, not a statement that implementation has not started.

**Client-surface gate (do not mark a phase shipped without this).**
Engine unit tests are not enough. For every new SKU, system, or order field:

1. A player can **install** it in TUI `--yard` (or the phase explicitly
   leaves Love/TUI out).
2. The **reference client** (REPL) can **issue** the matching allocate /
   path / volley field.
3. TUI combat can **send** the same field and **display** the snapshot
   result (cloak flag, squad, intercept, computer bonus).
4. The exit table names the client test, not only the engine test.

Missed once: computers existed in combat and compile, but the yard could
not mount them. Same class of hole: cloak/repair/follow/unsquad were
engine-only until the client-surface pass.
Each milestone has a **Goal**, **Work**, and an **Exit** table. A phase is
done only when every exit check passes.

`.killhouse/todo-completion-plan.md` is a pointer at this file.

---

## Locked

### Catalog and loop

- **Seven standard classes**, one per size (Fighter … Titan). No light/heavy
  catalog. Players author variants in `--yard`.
- **Nuke the old stock when nothing live needs it.** Live = scenarios,
  fixtures, campaigns, sim suites, Rust/Python tests, generators. Historical
  prose and ship-art inventory pages may still *name* deleted ids.
- **Yard cost is the only budget unit.** Delete `data/ship_costs.toml` in
  M1c; do not keep a second index.
- **v4 path stays.** No `movement_cost`, initiative, or momentum.
- **No exact FASA / old-SSD reproduction.**
- **TUI `--yard` is authoring.** Love yard is not a milestone.
- **sqlite / tech tree:** out.
- **Deflectors:** out. **Traveling missile tokens:** out. **Invisible cloak:**
  out. **Multiple gravitons:** out. **Real armor damage layer:** out.

### Data model (implement in M2, used by every later phase)

Do not grow free-form string tags or a new `WeaponKind` arm per SKU.

**Weapons** — `WeaponKind` is delivery; modifiers are orthogonal flags:

| Kind | Resolve |
|---|---|
| `Beam` | existing charge × range-factor |
| `Plasma` | existing range table |
| `Torp` | existing flat + magazine |
| `Missile` | M9; instant flat 2 + larger magazine |
| `Pd` | M4; not a ship-to-ship shot |
| `Graviton` | M7; hex slap |

| Flag | Legal on | Effect |
|---|---|---|
| `repeat` | `Beam` only | each current charge point is one packet (own roll, damage = `1 ×` range-factor, half_up) |
| `pierce` | `Beam` only | one roll; skip face shields; see Pierce math |

`repeat` and `pierce` together are **illegal** (yard + loader reject).

**Systems** — own typed collection, not flattened bonuses:

```text
ShipDef.systems: Vec<SystemDef>
SystemKind: Computer { mk: 1|2|3 } | Cloak | Repair | Ecm
```

Yard `Design.systems` compiles 1:1 into that vec. Loader rejects a second
computer, cloak, repair, or ECM. Multiplicity is proven by the vec, not by
inferring a scalar.

`ShipDef.attack_accuracy_bonus` remains **hull fire-control** (size-2 only).
Computers are a different field. Do not reuse the fire-control path.

**Repair needs `hull_max`.** `Ssd` has `engine_max` / `power_sys_max` but no
hull cap. Add `hull_max` (immutable, seeded from `ShipDef.structure` at
load). Repair restores `ssd.hull` toward `hull_max` only.

### To-hit order (every attack that uses a d20 vs a ship)

```text
base table(kind, range)
→ size-adjust (target / baseline, range ceiling)
+ hull fire-control          # size-2 targets only; attack_accuracy_bonus
+ mount precision            # weapon_accuracy_bonus; all sizes
+ computer                   # Mk N; all sizes; all such attacks
− defender cloak             # 4, if cloaked this turn
− evasion × evasion_per_point
− natural-defense delta      # vs baseline size 2
→ clamp [1, min(ceiling_max, die_sides − 1)]
```

**PD does not use that formula.**

```text
PD threshold = rules.combat.pd.to_hit[kind]
             + attacker computer
             → clamp [1, die_sides − 1]
```

Default `pd.to_hit.torp = 8` until M9, then `torp = 5`, `missile = 10`.
No size, range, fire-control, cloak, evasion, or natural defense.

**ECM** (M9, decided now): always-on, space/cost, no allocate, one per ship,
**−2** on incoming **missile** to-hit only, applied with the other
subtractions in the ship-to-ship formula. Does not affect torps, beams,
PD, or graviton.

### Pierce math

`ceil(table_damage / 2) + damage_bonus`. Bonus is after the half so a
potent pierce is still a bonus, not a halved bonus. Then skip the face
shield; remaining structure (including armor wrap) takes it.

### Repeat log and display

`max_charge` for Repeat SKUs is **4** (no charge-6 gun). Each packet is
its own `CombatLogEntry` with the same `weapon` id and `packet: u8`
(0-based). Engine traces keep every packet. **REPL/TUI display
aggregates** per weapon per turn (`3/4 hit, 10 dmg`). Do not dump 16
lines on the player.

### Graviton and armor

Armor is extra `structure`, not a layer. Graviton **bypasses shields** and
writes Δ to structure. Armor still helps because the ship has more
structure.

`Δ = max(0, attacker.size − victim.size)` per ship in the hex.

A **Δ = 0** shot is legal, cannot miss, **consumes the charge**, logs
`kind = graviton` with `damage = 0`. You wasted the slot.

### Cloak cost (confirmed)

`4 + size` is **intentional**. A fighter spending ~60% of a small plant to
cloak almost cannot shoot; a titan spending ~10% can cloak and still
fight. The −4 is flat on the d20. Do not “fix” the percent curve.

### Point defense resolve

1. After volleys lock, partition shots into **ordinance** (`Torp`, later
   `Missile`) and **other**.
2. Incoming bearing: relative facing **defender → attacker** (same mount
   table as if the defender were shooting that attacker). A PD mount must
   bear on that facing.
3. Incoming order: attacker id ascending, then weapon id lexicographic.
4. PD order: defender weapon id lexicographic, charge ≥ 1, in-arc for
   that attacker.
5. Greedy assign: walk incoming; for each still-live shot, walk unused
   eligible PD; roll; **first hit kills the shot** (0 damage from it).
   Miss: try the next PD. Charge is spent **only if that PD rolled**.
   Leftover PD move to later incoming. Unused PD **keep** charge.
6. Then all remaining shots (non-ordinance + surviving ordinance)
   resolve as today.

Combat log: `kind = pd_hit` | `pd_miss`. `attacker` = PD ship, `target` =
the ship that fired the ordinance, `weapon` = PD id, `vs_weapon` = the
incoming weapon id, `damage = 0`.

### Squads

- **Form:** scenario/fleet lists `squad` + `leader`, or mid-fight by
  ending a path in a friendly hex (join **is** the declare).
- **Split:** `unsquad: true` on allocate. Until then they share facing and
  the leader’s path.
- **Leader:** player-picked. `squad_leader` on allocate re-picks (must be
  a current member).
- **Follow:** `CommitPath { follow: true, actions: [], evasive }`.
  Non-empty `actions` while `follow` is an order error. A member who
  submits a real path while still squaded is an order error.
- **Motion:** each member still allocates movement independently. The
  path budget is **min** of members’ converted motion (and hull caps).
  Mixed sizes allowed. The unpowered member holds the brick unless they
  unsquad.
- **Evasive:** followers **may** declare `evasive`. It spends from **that
  ship’s own** motion pool (they did not spend path actions). Not from
  the leftover squad budget.
- **Join facing:** on successful join, joiner facing snaps to the
  existing leader’s facing at resolve end.
- **Cap:** 12 ships per hex / squad.
- **Occupancy:** only a declared squad shares a hex. Enemies never stack.
  Path into an enemy-occupied hex is illegal (order error at commit if
  known, else fallback — implementation picks one; tests lock it).
- **Simultaneous empty-hex meetup:** form a squad; **lowest ship id** is
  temporary leader; player may re-pick next allocate. An existing squad
  in that hex keeps its leader.
- **Fire:** pick **one** ship. Normal guns hit only that ship.
- **Death:** remaining stay a squad. Leader death (volley): **immediate**
  temporary leader = lowest remaining id, in effect for the next path
  stage. Player may re-pick next allocate.
- **Save/replay:** new fields default off / absent so old saves load.
  Snapshot lists `squad_id`, `leader`, `members`.

### Path resolver (the actual M6b job)

`src/path_resolve.rs` and ADR-0025 today guarantee **one unique final hex
per living ship** (module doc, “Final uniqueness invariant” assert,
tests). Squads **replace** that invariant:

```text
unique final hex per GROUP
group = a squad, or a singleton ship
```

- Resolver input is **groups**, not raw ships. A moving squad is **one
  claim** using the **leader’s** trace and the **leader’s** submitted
  cost.
- Fallback moves the **whole group** along the leader’s chain. Members
  stay stacked. **No silent split** on a lost contest.
- Joiners this turn are **not** in the group until the endpoint succeeds.
  Failed join → that ship falls back on **its own** chain.
- Stationary groups reserve their hex. Eviction/cascade is per group.
- The uniqueness assert becomes “no two groups share a hex” plus “all
  members of a group share a hex.”

This is a redesign of the most delicate engine code. It is **M6b**, not
a side note inside a feature dump.

### Protocol additions (backward-compatible defaults)

```text
Allocate {
  ...existing...
  cloak: bool = false          # spend 4+size if true
  repair: u32 = 0              # boxes; 2 power each
  unsquad: bool = false
  squad_leader: Option<u32>    # re-pick
}
CommitPath {
  ...existing...
  follow: bool = false
}
CombatLogEntry {
  ...existing...
  packet: Option<u8>
  vs_weapon: Option<String>
}
```

Protocol stays **v4** (additive defaults). ADR-0025 uniqueness sentence
is amended in M6b.

### Firepower (budget AI — deliberately dumb)

Not an effectiveness model. Name it **raw alpha**.

```text
alpha(weapon) =
  Pd or Graviton → 0
  Beam (repeat or not) → table damage at range 1 with charge = max_charge
                         + damage_bonus
  Beam+pierce → ceil(that table/2) + damage_bonus
  Plasma / Torp / Missile → table@1 + damage_bonus
firepower(class) = Σ alpha(weapons)
reject any class with cost == 0   # no infinite buy loop
tie-break: higher cost, then higher size, then id
```

Support systems (computer, cloak, repair, ECM, PD, graviton) do **not**
change this ranking. Largest is “biggest advertised punch,” not “best
ship.”

### Swarm and graviton (policy, not a weapon nerf)

Do **not** auto-pack Swarm into 12-fighter bricks. That forces the AI
into the one formation a titan graviton deletes, while a human declines.

- **Swarm** buys size ≤ 2 and leaves them **unsquadded**.
- Humans **may** squad (the reason squads exist: one path for many hulls).
- Optional later opponent type `brick` (not required for M8) if we want
  an AI that packs 12s.
- M10 includes a **mechanism fixture** (titan graviton vs a 12-fighter
  brick) that is **not** the 80% win-rate gate. The gate is Largest vs
  **spread** Swarm.

Do **not** split graviton Δ across the hex. The brick dying is the point.

### Torpedo potent

Do **not** decide drop-vs-reprice in M1. Revisit at the end of **M4**
(PD exists, 40% intercept). One-line reason in this plan when decided.

### Seed pools

| Pool | Seeds | Use |
|---|---|---|
| Tune | 1–191 | Lever cycles. Reusable. These are training data. |
| Eval | **328–391** | After tune, before sign-off. Pass/fail only. If fail, retune on 1–191. Do not fit levers to these outcomes. |
| Sign-off | 264–327 | **Once.** Burned if seen. |
| Burned | 200–263 | 2026-07 campaign. Do not reuse. |

Ledger: `docs/BALANCE.md` (or a short table in this file’s M13) records
every eval/sign-off run: date, commit, fingerprint, result, and the
replacement range if 264–327 is burned.

Budgets of 200 / 800 / 2000 are **placeholders**. Pick real budgets in
M8 after M1a publishes the seven costs.

### Client surface (every phase that adds state)

| Client | Gate |
|---|---|
| REPL | **Must** show the new snapshot/log fields (reference client). |
| TUI combat | Must not panic; path/follow must be legal. Pretty display is required for squads (M6c), encouraged earlier. |
| Love | Must not panic. Pretty display is not a gate. |
| TUI `--yard` | Must mount any new SKU the phase adds. |

`frontend/repl/hexutil.py` + `tests/test_rules_parity.py` **must** stay
in lockstep with `combat_tables.rs` whenever the to-hit formula changes
(M5, and M9 for ECM). This has been missed twice.

---

## Already shipped (do not rebuild)

Yard construction, size 1–7, to-hit + natural defense, compile-to-`ShipDef`,
TUI `--yard`, per-face shields, exterior armor, discrete engines. See
`docs/SHIPYARD.md`.

`Design` is engines + shields + weapons only. `ShipDef` has no `systems`.
`WeaponKind` is Beam / Plasma / Torp. `path_resolve` asserts unique hex
per ship. `Ssd` has no `hull_max`. REPL `hit_preview` mirrors fire-control
+ precision + evasion + natural defense only.

---

## Phase 0 — Scoreboard (optional)

**Goal:** TODO metrics as report fields. Skip old 21-ship suites as a gate.

**Work:** motion power / translated hex by size; first weapon-range
contact; stalemate / zero-translation by size.

**Exit (M0):**

| Check | Pass |
|---|---|
| Report JSON has those three fields | present |
| `data/` unchanged | `git diff data/` empty |
| `cargo test` | green |

May merge into M10. Do not block M1a.

---

## Phase 1a — Add the seven standards

**Goal:** one playable class per size exists. **Delete nothing.**

**Work:** author and compile seven designs. Ids stable (`fighter` …
`titan`, or keep `yard_*` and rename in the same PR). Minimum batteries:

| Size | Plant | Weapons (at least) |
|---:|---|---|
| 1 | fission `s` | 2× `beam_compact` |
| 2 | fission `m` | `beam` + `torpedo` |
| 3 | fission `m`/`l` | 2× `beam` + `torpedo` |
| 4 | fission `l` | 2× `beam` + `torpedo` + `plasma` |
| 5 | fission `l` | 3× `beam` + 2× `torpedo` |
| 6 | fusion `l` | 4× `beam` + 2× `torpedo` + `plasma` |
| 7 | fission `h` | 4× `beam` + 2× `torpedo` + 2× `plasma` |

Spread mounts. Must `validate`. Record each class’s yard `cost` in the
PR (M8 will pick budgets from these numbers).

**Exit (M1a):**

| Check | Pass |
|---|---|
| 7 designs validate + compile | 7/7 |
| Each meets the battery table | audit |
| Yard `cost` printed for all 7 | in the PR |
| `cargo test`, TUI `yard_` | green |
| No catalog files deleted | `git diff --name-status data/ships` has no `D` |

---

## Phase 1b — Migrate every live consumer

**Goal:** nothing that *loads* a ship still names a class we will delete.

**Work:** repository-wide audit, then retarget.

```text
rg -n 'class = ' scenarios/ fixtures/ campaigns/ simulation/
rg -n 'escort|heavy_cruiser|huge|starbase|titan_double|titan_light|_light|_line|_heavy' \
  --glob '*.rs' --glob '*.py' --glob '*.toml' --glob '*.md'
```

Live (must migrate or stay on the explicit allowlist):

- `scenarios/`
- `fixtures/` (`v4_conflict`, `combat`, `simulation_duel`,
  `class_id_duplicates`, `m7_hull_efficiency`, …)
- `campaigns/`
- `simulation/suites/` and simulation impl tests
- Rust unit/integration tests that load those TOMLs
- TUI scripted-pump / REPL tests
- `data/ship_costs.toml` (leave in place until M1c delete)
- Ship-art **runtime** manifests that a client resolves by class id

Not live (may keep the old names):

- `docs/history/`
- ship-art `PHASE0-INVENTORY.md` and similar inventories
- This plan

**Tutorial allowlist** (nongenerated ships still permitted after M1c).
Name every row; do not use a vague “tiny allowlist”:

| Id | Why it may remain |
|---|---|
| `tutorial_escort` | scripted tutorial HP / lesson |
| `tutorial_heavy_cruiser` | scripted tutorial HP / lesson |
| `starbase` | only if a committed scenario still needs an immobile |
| `titan_double` | only if a committed scenario still needs it |

If a tutorial can run on a standard, drop that allowlist row in M1b.

**Exit (M1b):**

| Check | Pass |
|---|---|
| Audit output checked into the PR (or the plan) | yes |
| `rg 'class = '` on scenarios/fixtures/campaigns/suites only
  names remaining ships or allowlist ids | 0 dangling live refs |
| `battle` / `fleet` / tutorials / fixture pumps load | smoke + existing tests |
| `cargo test` + REPL unittest + TUI tests | green |

---

## Phase 1c — Delete the old catalog

**Goal:** unused stock gone. Yard cost is the only price list.

**Work:** delete `*_light` / `*_line` / `*_heavy`, leftover `escort` /
`huge` / unused `heavy_cruiser` once M1b is green. Delete
`data/ship_costs.toml`. Point any leftover docs at yard `cost`.

**Exit (M1c):**

| Check | Pass |
|---|---|
| No `*_light` / `*_heavy` / `*_line` in `data/ships/` | unless allowlisted |
| `data/ship_costs.toml` gone | yes |
| Every remaining `data/ships/*.toml` is yard-generated **or**
  on the M1b allowlist table | audit test |
| `cargo test` + REPL + TUI | green |

---

## Phase 2 — Typed combat-kit model

**Goal:** Design → validate → ShipDef → Ship → snapshot can carry the
kit **before** any new resolve code. Later phases fill behavior.

**Work:**

- Extend `components.toml` with a `systems` table.
- `Design.systems`, `WeaponComponent` flags `repeat` / `pierce`, new
  kinds `pd` / `graviton` / `missile` (compile/load only).
- `ShipDef.systems`, `WeaponDef` kind + flags.
- Runtime `Ship` + snapshot expose systems and flags.
- Yard rejects: second computer/cloak/repair/ECM; graviton count > 1;
  `repeat`+`pierce`; `repeat`/`pierce` on a non-beam; unknown kind.
- `Ssd.hull_max`.
- Docs: `ARCHITECTURE.md` + `SHIPYARD.md`. No combat behavior change.
- Combat tests that exist today stay green (new fields default empty).

**Exit (M2):**

| Check | Pass |
|---|---|
| Round-trip: design with computer+cloak+repair+ECM+repeat+pierce+pd+graviton
  compiles, loads, snapshot lists them | unit test |
| Second computer / two gravitons / repeat+pierce rejected at yard **and** load | unit test |
| `hull_max == structure` at load | unit test |
| Existing volley fixtures unchanged | golden |
| `cargo test` | green |

M2 may overlap M1b/M1c. **M3+ wait for M2.**

---

## Phase 3 — Repeat and Pierce

**Goal:** two beam identities on the existing volley.

**Work:** SKUs `beam_repeat`, `beam_pierce`. Resolve per Locked. Docs:
combat tables + shipyard. REPL aggregates Repeat packets. No PD, cloak,
or squads.

**Exit (M3):**

| Check | Pass |
|---|---|
| Charge-4 Repeat vs size-2 range 1 → **4** log rows, `packet` 0..3 | unit test, seeded |
| Repeat miss zeros that packet; later packets still fire | unit test |
| Pierce vs 6 shields + 10 hull: shields **unchanged**, hull − pierce | unit test |
| Pierce vs 0 shields deals **less** than same-charge normal beam | unit test |
| Pierce can miss | unit test |
| `damage_bonus` added **after** the half | unit test |
| REPL display for a 4-packet volley is **one** aggregated line | REPL test |
| Yard mounts both SKUs | `shipsim-yard` |
| `cargo test` + REPL unittest | green |

---

## Phase 4 — Point defense

**Goal:** powered auto-beams vs incoming ordinance. Torps stay useful.

**Work:** PD SKU. Resolve per Locked (`to_hit.torp = 8`). No new player
order. Snapshot/log: `pd_hit` / `pd_miss` + `vs_weapon`. REPL shows
intercepts. Revisit `torpedo_potent` (drop or reprice, one line here).

**Exit (M4):**

| Check | Pass |
|---|---|
| 0 PD: torp fixture **identical** to pre-M4 | golden |
| 1 in-arc PD, roll ≤ 8: torp does 0, PD charge 0, log `pd_hit` | unit test |
| Same, roll ≥ 9: torp resolves, PD charge 0, log `pd_miss` | unit test |
| Out-of-arc: no roll, torp resolves, PD charge **kept** | unit test |
| 2 PD vs 1 incoming: second does not roll if the first hit; second
  **keeps** charge | unit test |
| Beam / plasma: PD does not roll | unit test |
| Charge 0: no roll | unit test |
| Rules fingerprint changes when `pd.to_hit` changes | fingerprint test |
| REPL shows the intercept | REPL test |
| `torpedo_potent` gone **or** this plan states the new identity | one of the two |
| `cargo test` + REPL | green |

---

## Phase 5 — Computer, cloak, repair

**Goal:** three systems. Accuracy, hard-to-hit, hull recover.

**Work:** SKUs already loadable from M2. Wire allocate (`cloak`,
`repair`) and the to-hit order. Snapshot `cloaked`. TUI yard can add
them. **REPL `hexutil.hit_preview` and `test_rules_parity.py` grow
`computer` and `cloaked` arguments** and stay equal to
`combat_tables.rs`.

**Exit (M5):**

| Check | Pass |
|---|---|
| Mk3 vs Mk0: same shot +3, including a PD roll | unit test |
| Destroyer cloak costs **6**; titan **11** | unit test |
| Cloaked ship: incoming −4, still in snapshot, still targetable | unit test |
| Cloak + evasion + defense still floor at 1 | unit test |
| Size-2 repair: 2 power → +1 hull, cap 1; second box rejected | unit test |
| Size-5 repair: cap 2 | unit test |
| Repair cannot exceed `hull_max` | unit test |
| Small plant cannot cloak + 6 faces + charge-4 beam | allocate reject
  or leftover-math test |
| `hexutil.hit_preview` matches engine on computer **and** cloak | `test_rules_parity.py` |
| TUI yard can mount all three | `yard_` test |
| REPL shows cloaked + repaired hull | REPL test |
| `cargo test` + REPL | green |

---

## Phase 6a — Squads: state and protocol

**Goal:** membership exists. Path resolver **still** unique-per-ship.
Nobody stacks yet.

**Work:** scenario `squad` / `leader`. Allocate `unsquad`,
`squad_leader`. Snapshot membership. Load-time: same-side, cap 12, one
leader, leader is a member. Mid-fight join/split flags accepted but
**do not change occupancy**. Tests for the schema and rejects. Docs:
`PROTOCOL.md`, `SAVE-FORMAT.md` (defaults).

**Exit (M6a):**

| Check | Pass |
|---|---|
| Fleet TOML with a 3-ship squad loads; snapshot lists id/leader/members | test |
| Cap 13 at load rejected | test |
| Enemy listed in one squad rejected | test |
| Old saves / orders without new fields still replay | golden |
| Path resolver uniqueness assert **still** fires if two ships share a hex | existing tests green |
| `cargo test` | green |

---

## Phase 6b — Squads: path resolution

**Goal:** groups share a hex. The invariant change lives here.

**Work:** rewrite `path_resolve` per Locked. `CommitPath.follow`.
ADR-0025 uniqueness sentence updated. Join-by-endpoint. Failed join =
own fallback. Lost contest = **whole group** on the leader chain.
Enemy hex illegal. Mixed-size min motion. Tests named in the exit table
**and** a comment in `path_resolve.rs` that states the new invariant.

**Exit (M6b):**

| Check | Pass |
|---|---|
| Two squaded ships end in the **same** hex after one follow path | unit test |
| 13th join rejected; 12th allowed | unit test |
| Path into an enemy hex fails / falls back; hex still one side | unit test |
| Mixed size-1 + size-3: usable motion = size-3 cap | unit test |
| Unsquad → independent paths this turn | unit test |
| Member non-follow path while squaded → order error | unit test |
| Lost contest: all members fall back to the **same** hex (no split) | unit test |
| Failed joiner falls back on **own** chain; existing squad stays put | unit test |
| Leader destroyed: temp leader = lowest id | unit test |
| Follower `evasive: 2` applies to **that** ship only | unit test |
| Uniqueness assert: two **groups** cannot share; members of one can | unit test |
| Beam at A in a 2-ship stack: A damaged, B not | unit test |
| `cargo test` | green |

---

## Phase 6c — Squads: clients

**Goal:** the reference client can drive a brick. Other UIs do not break.

**Work:** REPL: show stack, pick leader, follow, unsquad. TUI combat:
follow is legal; show membership on the unit / hex. Love: no panic on
stacked hexes.

**Exit (M6c):**

| Check | Pass |
|---|---|
| REPL moves a 3-ship squad with one plotted path | API or UI-play script |
| REPL shows leader + members on the hex | test or script |
| TUI combat test: follow commit accepted | TUI test |
| Love fixture with a 2-stack loads without error | smoke if cheap; else
  “no panic” note in the PR |
| `cargo test` + REPL unittest | green |

---

## Phase 7 — Graviton

**Goal:** one short-range hex slap. Capitals delete a brick; same-size
is a wasted slot.

**Work:** wire the M2 SKU. Charge 1, range 1–2, in-arc. Pick one ship;
every ship in that hex takes its own Δ. Bypass shields; armor stays
structure. Cloak/PD irrelevant.

**Exit (M7):**

| Check | Pass |
|---|---|
| Size 7 vs size 1, 2 HP: destroyed, no miss roll | unit test |
| Size 7 vs size 7: 0 hull, charge spent, log `graviton` | unit test |
| Size 1 vs size 1: 0 hull, charge spent | unit test |
| Size 7 vs armored size 2 (9 HP): hull −5, shields unchanged | unit test |
| Range 3: order error | unit test |
| Two gravitons: already rejected in M2; still rejected | regression |
| 12 size-1 squad, titan graviton on one: **all 12** destroyed | unit test |
| Cloaked fighter in range 2: still full Δ | unit test |
| Beam in the same hex still hits only the picked ship | M6b regression |
| REPL shows one line per victim or one aggregated graviton line
  (pick one in the PR, document it) | REPL test |
| `cargo test` + REPL | green |

---

## Phase 8 — Budget fleet scenario

**Goal:** a scenario kind. Swarm does **not** auto-squad.

**Inputs:** `budget`, `opponent` = `largest` | `swarm` | `balance`,
`roster`. Same budget both sides.

Policies use **raw alpha** (Locked). Zero-cost classes rejected.

**Largest:** loop buy max-alpha affordable.

**Swarm:** same, `allowed = size ≤ 2`, **unsquadded**.

**Balance:**

```text
1. If any size-4 affordable: buy max-alpha size-4 once.
2. While any size-3 affordable: buy max-alpha size-3.
3. Then greedy size-2, then greedy size-1.
```

Skip a step you cannot afford. No size 5–7. No auto-squad.

**Human:** any multiset with `Σ cost ≤ budget`, optional squads (cap 12).

Pick budgets from the M1a cost table (do not assume 200/800/2000).

Implementation: scenario TOML or a tiny pre-combat TUI. Deterministic.
Not an NDJSON `request`.

**Exit (M8):**

| Check | Pass |
|---|---|
| Three policies + human pick, one budget | golden fleets |
| Swarm never buys size ≥ 3 and never writes a squad | test |
| Balance never buys size ≥ 5 | test |
| Cost-0 class in roster: ignored / rejected, loop terminates | test |
| Tie on alpha: higher cost then size then id | test |
| Fleet `Σ cost ≤ budget` always | test |
| Two-player pick, same budget | test |
| TUI or documented CLI starts a fight from budget + type | one path |
| Docs if the wire changed | yes |
| `cargo test` | green |

---

## Phase 9 — Missiles and ECM

**Goal:** second ordinance identity. Skip only if we accept certifying a
**torps-only** ruleset; adding this later **burns** that certification.

**Work:** `missile` SKU (flat 2, larger magazine, cheaper, instant).
PD: missile 10, torp **5**. ECM always-on −2 vs missiles. Yard + docs.
`hexutil` + parity tests if missile to-hit is previewed.

**Exit (M9):**

| Check | Pass |
|---|---|
| Missile resolves in the same volley (no token) | unit test |
| PD vs missile 10; vs torp 5; vs beam no roll | unit test |
| ECM −2 missile to-hit; torp fixture unchanged | unit test |
| Magazine(missile) > magazine(torp) on the same size | unit test |
| Parity test updated if previews include missiles | `test_rules_parity.py` |
| `cargo test` + REPL | green |

---

## Phase 10 — Evidence

**Goal:** TODO metrics as a rubric on the **seven + kit + squads +
budget** ruleset.

**Suite:** mirrors of each of the seven; adjacent sizes at equal budget;
Largest vs Swarm vs Balance at the M8 budgets. **Mechanism fixture:**
size-7 graviton vs a 12-fighter brick (not a win-rate gate).

**Exit (M10):**

| Check | Pass |
|---|---|
| Suite runs seeds 1–32 forward and reversed | exit 0 |
| TODO evidence metrics in the report | checklist |
| Numeric gates written **before** pooled tuning | rubric file |
| Seeds 264–327 and 328–391 unused | comments |
| Graviton-vs-brick fixture present | yes |

**Starting gates** (change only with a written reason):

| Signal | Gate |
|---|---|
| Same-class mirror | 35–65% player |
| Equal-budget adjacent sizes | neither >75% |
| Equal-budget Largest vs **spread** Swarm | neither >80% |
| Size-7 zero-translation among scheduled paths | <40% |
| In-progress / stalemate | <15% |

---

## Phase 11 — Tune (pooled, 1–191)

One lever per cycle (`docs/BALANCE-PROTOCOL.md`). Not a new movement
system. Do not fit to 328–391 or 264–327.

**Exit (M11):** blocking rubric on seeds 1–191; destroyer mirror 35–65%;
fingerprint in `docs/BALANCE.md`.

---

## Phase 12 — Eval (328–391)

Run the M11 candidate **once** on 328–391. Pass/fail against the same
gates. If fail: back to M11 on 1–191 only. Record the run in the ledger.

**Exit (M12):** eval report checked in or linked; ledger row written;
264–327 still unused.

---

## Phase 13 — Sign-off (264–327)

Spend 264–327 **once**. Same gates. `BALANCE.md` may say certified **for
the named claims only**. If it fails, burn the range, allocate a new
tracked one, ledger it. Do not tune after seeing these seeds.

---

## Out of scope

- 21-ship light/line/heavy catalog.
- sqlite, tech tree.
- Initiative / momentum / `movement_cost`.
- Deflectors; traveling missiles; invisible cloak.
- A separate armor absorb layer.
- Multiple gravitons.
- Squads as a merged reactor or a single volley.
- Auto-squad on Swarm (human bricks only, unless a later `brick` policy).
- Love yard; Love pretty-print as a gate.
- “All sizes fair on all maps.”
- Sign-off or eval seeds before their phases.

---

## Order

```text
M0 metrics (can merge into M10)
  → M1a seven standards (add only)
    → M1b migrate live consumers          ──┐
    → M2 typed model (may overlap M1b/c)    ├── then M3+
    → M1c delete old catalog              ──┘
      → M3 Repeat + Pierce
        → M4 point defense (+ torp_potent decision)
          → M5 computer + cloak + repair (+ REPL parity)
            → M6a squad schema/protocol
              → M6b path_resolve groups
                → M6c REPL/TUI surface
                  → M7 graviton
                    → M8 budget fleets (no auto-squad)
                      → M9 missiles + ECM
                        → M10 evidence
                          → M11 tune 1–191
                            → M12 eval 328–391
                              → M13 sign-off 264–327
```

Do not run M10–M13 on a ruleset that is about to gain M6–M9.

---

## Old TODO sequence

| Old | Fate |
|---|---|
| 1 Movement design | Done (v4). |
| 2 sizes.toml + align TOMLs | Ladder done. Align → **M1a–c**. |
| 3–4 movement_cost / buy motion | Done as thrust + `mv`. |
| 5 Initiative | Dropped. |
| 6 Component TOMLs + load validate | TOMLs done. Systems model → **M2**. |
| 7 Generic hulls | **M1a** (7, user-made variants). |
| 8 Mirrored suites | **M10–M13**. |
| 9 Construction money | Design-time cost done. In-play spend → **M8**. |

---

## Commands

```bash
cargo test --lib shipyard::
cargo test --manifest-path frontend/tui/Cargo.toml -- yard_
cargo run --bin shipsim-yard -- validate data/designs/<id>.toml
cargo run --bin shipsim-yard -- compile data/designs/<id>.toml
cargo test
(cd frontend/repl && python3 -m unittest discover -s tests)
```
