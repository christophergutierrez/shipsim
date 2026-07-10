# shipsim -- FASA / Bocchino STCS context

Source rules: `tmp/StarshipCombat.pdf` (Bocchino 2013, adapted from FASA STCS).
Plan: `implementation-plan-fasa-stcs.md`. Decision: ADR-0019.

## Why pivot

shipsim v1 core followed SFB-style 32-impulse play. The preferred game is FASA STCS-style:
**turns → 3 rounds → ships act in order → spend power to move or fire now.**

## Glossary (ubiquitous language)

| Term | Meaning |
|------|---------|
| **Turn** | Full cycle; engine power restored at start (less damage). Contains 3 rounds. |
| **Round** | One pass through action order; each ship gets an action window. |
| **Action order** | Sequence of ships this round (skill + roll or scenario). |
| **Action window** | Active ship may spend power on move/fire/etc. until end_action. |
| **Power** | Single pool from engines; spent on actions; remainder may soak shield hits. |
| **Movement Point Ratio** | Power cost of one basic move action. |
| **Shield Point Ratio** | Power cost per damage point absorbed by shields. |
| **Facing shields 1--6** | Armor arcs on hex sides (Fig. 1); not the same as "shields" power spend. |
| **Fire action** | Pay weapon power; resolve hit immediately at current range/arc. |
| **Weapon once per turn** | Each weapon may fire at most once per turn. |

## Non-goals (for MVP)

Full FASA photocopy tables, crew skills, cloaking, sensors, 32-impulse dual-mode.
