# Implementation Plan: GameState Encapsulation

## Verdict
- verdict: READY
- task_tier: full (public surface + combat API migration)
- classification: major (approved RFC from ARCHITECTURE-slice3)
- execution_policy: cost_optimized
- model_routing: current-model-only

## Request
Encapsulate `GameState`: private fields; narrow public API; purify combat so it does not take
`&mut GameState`; damage via `Ship` methods. Callers mutate only through intentional APIs
(orders + explicit setup mutators), never by poking `game.ships` / `game.board`.

## Outcomes
| id | outcome |
|----|---------|
| O1 | All `GameState` storage fields private (not `pub`) |
| O2 | Public read API: status, impulse, turn_number, seed, board, objective, ship, ships |
| O3 | No public `ship_mut` / `ship_index` / raw `ships` / `board` field access outside crate |
| O4 | Explicit setup mutators for tests/scenario tweaks (pos, facing, shields, structure, weapon config) |
| O5 | `combat::resolve_fire` takes ships + `&mut Prng`, not `GameState` |
| O6 | `Ship::apply_shield_and_structure_damage` (or equivalent) owns damage application |
| O7 | `turn` / `movement` / `snapshot` use accessors only |
| O8 | Full suite + clippy green; no behavior change (seeded combat still identical) |

## Non-goals
- Full `Ship` field privacy (can remain readable; state bag is the target)
- Plot domain type extraction (optional follow-up)
- ImcSpeed/Facing newtypes
- Wire/JSON order format changes

## Milestones

### M1 — Pure combat + Ship damage
- `resolve_weapon_damage(weapon, range, prng)`
- `resolve_fire(attacker, weapon_id, target: &mut Ship, prng) -> Option<FireOutcome>`
- `Ship::apply_hit(shield_facing, damage)`
- `GameState::apply_fire(weapon_id, target_id)` crate-internal wrapper for turn
- Gate: existing combat tests still pass (via ship_mut temporarily if needed)

### M2 — Private GameState fields + accessors
- Fields private; add getters
- Internal `pub(crate)` helpers for turn/movement
- Update snapshot, turn, movement to accessors
- Gate: compile + tests (ship_mut may still be pub temporarily)

### M3 — Remove public ship_mut; setup API + migrate tests
- Public setup: `set_ship_pos`, `set_ship_facing`, `set_ship_shields`, `set_ship_structure`, `configure_weapon_exact_damage` (or similar)
- Migrate all integration tests off `ship_mut` and field pokes (`game.status` -> `game.status()`)
- Gate: `cargo test` + `cargo clippy -D warnings`; absence of `.ships` / `.ship_mut` in tests/

### M4 — Docs
- Update ARCHITECTURE-slice3 residual RFC to REALIZED; HANDOFF note

## Acceptance
```
cargo test
cargo clippy --all-targets -- -D warnings
rg -n 'game\.(ships|board|status|impulse|seed|turn)\b' tests/ src/bin  # expect none (use accessors)
rg -n 'ship_mut' tests/  # expect none
rg -n 'fn resolve_fire\([^)]*GameState' src/  # expect none
```
