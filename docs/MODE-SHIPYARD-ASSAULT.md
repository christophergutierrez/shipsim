# Game mode: Shipyard Assault

The first build-as-you-fight scenario. Two sides, two shipyards, one win
condition: **destroy the enemy shipyard.**

This document describes the mode's rules and the reasoning behind its numbers.
It is a design contract, not a task list — the implementation plan lives
outside the tracked tree (see `docs/DOC-LIFECYCLE.md`).

---

## 1. The core loop

Every turn each side receives income, may spend it on ships, and fights. New
ships appear at the owner's shipyard. The mode's whole tension is
**tempo versus efficiency**:

- Spend now on a fighter and it shoots this turn.
- Save for something bigger and it does nothing until it exists.

The larger ship is genuinely better per credit (see §5), so the cost of
teching up is paid in *time*, not in credits — the classic rush-versus-tech
decision, made once per turn.

## 2. Setup

Each side starts with:

| Unit | Count |
|---|---|
| Shipyard | 1 |
| Light cruiser | 1 |
| Destroyer | 1 |
| Fighter | 2 |

All starting ships are the standard catalog classes. Both sides are identical;
the scenario is symmetric.

## 3. The shipyard

A new, purpose-built entity. It is **not** the existing `starbase`, and it is
not purchasable from the yard — it is scenario furniture with deliberately
non-economic stats.

| Property | Value | Why |
|---|---|---|
| Structure | **100** | See §6. Roughly two turns of concentrated fire from a starting fleet. |
| Shields | **0** | Deliberate. See below. |
| Mobility | Immobile (`max_maneuver_actions = 0`) | It is a structure. |
| Armament | Light, all-arc, short range | Enough to punish a lone plinker, not enough to fight a fleet. |
| Purchasable | No | Its stats would be nonsense inside the cost model. |

**Zero shields is a rule, not a rounding of "minimal."** Shields re-power every
turn, so any attacker whose incoming damage falls below the target's shield
regeneration deals *exactly zero net damage indefinitely*. A shielded shipyard
would create a hard breakpoint where a lone fighter — or a mauled fleet —
plinks forever with no progress and no on-screen explanation of why. Zero
shields makes damage strictly monotonic: every hit counts, the clock always
advances, and the game cannot enter that stalemate.

## 4. Economy

| Rule | Value |
|---|---|
| Income | **100 per turn**, per side, flat |
| Accumulation | Unspent credits carry over |
| Purchase timing | Any turn, any amount affordable |
| Build delay | **None — cost *is* the delay** |
| Spawn point | Owner's shipyard |

**Cost-as-time.** There is no separate build timer. Saving up is the wait. This
is one dial (income) rather than two interacting ones, and it already produces
the intended tension: a battleship is ~5.5 turns of *not* buying fighters. A
second timer on top would push large hulls past the game's horizon entirely
and collapse the ladder.

Income of 100/turn is set so that a fighter (74) is roughly one turn's income —
the stated floor — while a battleship (546) is a real multi-turn commitment.

### Reachability at 100/turn

| Ship | Cost | Turns of saving |
|---|---:|---:|
| Fighter | 74 | 0.7 |
| Destroyer | 98 | 1.0 |
| Light cruiser | 216 | 2.2 |
| Heavy cruiser | 309 | 3.1 |
| Battleship | 546 | 5.5 |
| Dreadnought | 1,322 | 13.2 |
| Capital | 3,470 | 34.7 |

**Sizes 6–7 are intentionally out of reach in this mode.** A game targeting
~12–20 turns cannot fund them. They are not broken — they are simply this
scenario's unreachable top end, reserved for a future longer mode. Do not
rebalance the cost curve to force them in.

## 5. Why bigger ships are better value

Ship cost is dominated by the **frame** — the hull itself — which scales
steeply with size, while weapons and shields are flat-priced regardless of what
they are mounted on.

| Class | Total cost | Frame + plate | Weapons | Frame share |
|---|---:|---:|---:|---:|
| Fighter | 74 | 6 | 52 | 8% |
| Destroyer | 98 | 13 | 28 | 13% |
| Light cruiser | 216 | 30 | 44 | 14% |
| Heavy cruiser | 309 | 94 | 58 | 30% |
| Battleship | 546 | 300 | 104 | 55% |
| Dreadnought | 1,322 | 948 | 110 | 72% |
| Capital | 3,470 | 3,000 | 116 | 86% |

Because the frame is a sunk cost, **filling a hull is progressively cheaper the
bigger it is.** Adding weapons to a capital costs ~18% more for several times
the firepower; the same weapons on a fighter would more than double its price.
That is the property that makes teching up worth the wait.

## 6. Sizing the shipyard's 100 structure

Derived, not chosen. A starting fleet (light cruiser 20 + destroyer 12 + two
fighters 32 = 64 raw damage/turn, ~54 after to-hit against a large stationary
target) kills 100 structure in **under two turns of concentrated fire**.

Add ~2 turns of travel and a rush cannot resolve before roughly turn 4–5 — and
only if the defending fleet is beaten or bypassed first. That is the intended
floor. It keeps games short, which is the explicit goal.

## 7. The defender's advantage

Structural, and it compounds:

- **Reinforcement tempo** — a defender's new ship fights the turn it spawns; an
  attacker's spends ~2 turns travelling. At one ship per turn that is a
  *standing* deficit, not a one-time cost.
- **Concentration** — the defender is already massed at the objective; the
  attacker arrives piecemeal as reinforcements trickle in.
- **Proximity** — a damaged defender is already home.

The attacker therefore needs a material edge merely to break even, which is
what makes "fight until you have an advantage, then commit" a correct strategy
rather than a stalling tactic. The shipyard's low structure is the counterweight:
without a fast kill once superiority is achieved, attacking would never pay.

## 8. Ships are ~60–70% filled, on purpose

Standard classes are built to roughly **60–70% of their power ceiling** —
their real constraint is power, not space (see §9).

Not 100%, deliberately. At full efficiency there is exactly one optimal
loadout and every ship of a class is identical. Leaving slack means:

- **trimming** a weapon or system to afford a hull *now* is a real decision;
- **upgrading** when flush is a real decision;
- two players' battleships can differ.

That in-play design texture is a primary goal of the mode, and it only exists
if ships are not already optimal.

## 9. Power is the constraint, not space

A hull's real limit is how much it can **charge** per turn, not what it can
physically carry.

| Hull | Max guns by space | Max guns by power | Binds |
|---|---:|---:|---|
| Fighter | 1 | 3 | SPACE |
| Destroyer | 5 | 10 | SPACE |
| Light cruiser | 17 | 29 | SPACE |
| Heavy cruiser | 71 | 30 | POWER |
| Battleship | 242 | 26 | POWER |
| Dreadnought | 782 | 36 | POWER |
| Capital | 2,480 | 68 | POWER |

Small hulls are space-limited; large hulls are power-limited. **This crossover
is load-bearing and must be preserved.**

It is what keeps `*_compact` components (which buy space with cost) a
small-ship choice: a capital has space to spare, so paying a premium to save
space it does not need is irrational. **Do not tighten the large hulls' space
caps** — that would make them space-limited too and make compact universally
attractive, collapsing the distinction.

Weapon efficiency differs sharply on the axis that binds:

| Weapon | Space | Power | Damage @ r1 | Dmg/power | Range |
|---|---:|---:|---:|---:|---:|
| Plasma | 8 | 1 | 8 | **8.0** | 6 |
| Torpedo | 8 | 1 | 4 | 4.0 | 12 |
| Beam | 10 | 4 | 8 | 2.0 | 10 |

Beams are the most power-hungry option per point of damage. A power-limited
hull that fills up on beams wastes most of its potential — which is what the
pre-mode standard designs did.

## 10. Win condition

Destroy the enemy shipyard. Uses the existing `destruction` terminal keyed to
the enemy shipyard's ship id. Losing your entire fleet is not a loss; losing
your shipyard is.

## 11. Strategic space (design intent)

The mode should support at least these, none dominant:

- **Rush** — commit everything early, accept that a failed rush loses the game.
- **Turtle** — heavy defence, win on economy, advance late.
- **Attrition** — trade fleets until materially ahead, then commit.
- **Tech** — skip early ships, field something big, survive the window.

If one of these dominates in balance testing, that is a tuning signal. The
primary dials are **income rate** and **shipyard structure**, in that order —
both change tempo without perturbing the ship cost model.

---

## 12. Two players, both over the API

**Both sides play through the same API.** There is no privileged "host player"
and no UI-only capability. A client — human at a terminal, built-in algorithmic
AI, or an LLM — is just an API consumer.

This is a testing requirement first and a feature second: if the UI can do
anything the API cannot, then automated balance testing is not testing the real
game.

A player may be:

| Player type | Purpose |
|---|---|
| Human at a terminal | Real play, UX feedback |
| Built-in algorithmic AI (greedy, aggressive, defensive, …) | Fast, deterministic, seeded — for balance at volume |
| LLM over the API | Slow, clever — for corner cases and emergent strategy |

Any type may face any other type.

## 13. The four-tier testing ladder

Each tier answers a question the tier below it cannot, and costs more to run.

**Tier 1 — Algorithm vs algorithm, hundreds of games, no UI.**
The Rust engine plays itself with seeded policies. Answers: *is it balanced?*
Cheap, deterministic, reproducible. This is where tuning happens. The existing
`shipsim-sim` batch runner, policies, and seeded suites already provide this.

**Tier 2 — LLM vs algorithm.**
An LLM plays a built-in policy. Answers: *is it exploitable?* LLMs are slower
but cleverer, and will find degenerate strategies a fixed policy never explores
— "build only X and always win." Expect corner cases here, not balance data.

**Tier 3 — LLM vs LLM.**
Answers: *is it interesting?* Expected to produce long games and surface
missing mechanics — the "this mode needs Y to be fun" signal.

**Tier 4 — Human vs anything.**
Answers: *is it playable and fun?* The only tier that can.

Balance is settled at Tier 1 before spending Tier 2/3 effort. A degenerate
strategy found at Tier 2 is a Tier 1 rubric that was missing.

## 14. Balance discipline

This mode's tuning obeys the existing seed protocol
(`docs/BALANCE-PROTOCOL.md`): tune on pooled seeds, evaluate once on the
evaluation range, sign off once on the virgin range. A strategy that wins
>70% at equal income across seeds is a finding, not a feature.
