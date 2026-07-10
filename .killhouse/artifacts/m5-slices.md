# M5 Slice Plan

- id: turn-loop-decision
  advances_gate: G1 (loop back to movement when move remains), G2 (turn_end when no actions)
  seam: `GameState::resolve_fire_phase_v2` loop decision + `can_any_move` / `can_any_legal_fire`
  red_command: `cargo test --test v2_turn_loop`
  green_condition: G1/G2 pass; resolving a batch returns to a fresh movement phase iff any
    living ship can still move or fire legally, otherwise `turn_end`
  scope_files: `src/game_state.rs`, `tests/v2_turn_loop.rs`
  contracts_used: M5 + "Turn state machine" in `implementation-plan-combat-v2.md`
  contract_review: batch_standard
  invariants_touched: legacy tests green; replaces the M4 `Phase::TurnEnd` placeholder

- id: end-turn-and-warning
  advances_gate: G3 (warning true + EndTurn advances turn), G4 (warning false at turn end)
  seam: `Order::EndTurn` -> `GameState::end_turn_v2`; `end_turn_warning` on `StateSnapshot`
  red_command: `cargo test --test v2_turn_loop`
  green_condition: G3/G4 pass; EndTurn always advances (counter + full v2 reset), warning is
    the derived advisory `can_any_move() || can_any_legal_fire()`
  scope_files: `src/movement.rs`, `src/game_state.rs`, `src/snapshot.rs`, `tests/v2_turn_loop.rs`
  contracts_used: locked product decisions #1/#2 in the M5 task brief
  contract_review: batch_standard
  invariants_touched: EndTurn advances via `advance_turn_counter` + `reset_all_power` (NOT the
    FASA 3-round `advance_round_or_turn`)

- id: destruction-win-and-reset
  advances_gate: G5 (destroy terminal target -> Won), G6 (new turn clears allocation/keel/
    shields/charges/fired flags)
  seam: existing `refresh_status` + `Terminal::DestroyShip`; `reset_all_power` /
    `reset_v2_allocation` reached via `end_turn_v2`
  red_command: `cargo test --test v2_turn_loop`
  green_condition: G5/G6 pass; win asserted (no new win logic), reset verified after EndTurn
  scope_files: `tests/v2_turn_loop.rs` (asserts existing reset behavior)
  contracts_used: M5 gates 3-4 in `implementation-plan-combat-v2.md`
  contract_review: batch_standard
  invariants_touched: none new; reuses M4 destruction path

## Notes / Repair

The M4 firing resolution parked every resolved batch at `Phase::TurnEnd` as a documented
placeholder (see `.killhouse/artifacts/m4-slices.md`). M5 replaces that single line with the
frozen loop decision. One M4 test encoded the placeholder
(`tests/v2_fire.rs::test_v2_ready_fire_resolves_and_consumes_weapon`, asserting `turn_end`
and a rejected second commit). Under the correct loop decision that scenario has charged,
legal weapons left on both ships, so it loops back to a fresh firing phase; the test was
updated to assert the loop-back (`phase == "firing"`, commits/ready cleared) and that the
still-charged torp is committable in the reopened firing phase. No FASA orders/tests touched.

`end_turn_warning == true` iff any living ship has `move_remaining > 0` OR a charged, unfired
v2 weapon with at least one currently-legal shot (`can_any_legal_fire` reuses the M4 commit
legality primitives: charge, fired flag, range, beam no-damage, arc, legal shield facing).
</content>
