# M7 Implementation Plan — Delete FASA/Legacy Orders + Rewrite Tests

## Spec (implementation-plan-combat-v2.md lines 435-444)
- Remove FASA `EndAction`/3-round fields as product API
- Delete/empty `tests/fasa.rs`
- Harness fixtures for v2
- `rg` for old order types in `src/`
- Gates: cargo test green, cargo clippy clean, no `Order::EndAction`/three-round FASA as primary path in `src/`

## Legacy Code to Remove

### src/movement.rs
1. **Order enum (lines 51-61):** Remove `Fire`, `EndAction`, `EndRound` variants
2. **apply_order dispatch (lines 176-185):** Remove `Order::Fire`, `Order::EndAction`, `Order::EndRound` match arms
3. **apply_order Move branch (lines 156-162):** Remove the legacy `else` branch — `Order::Move` always uses `apply_v2_move` (phase check moves into `apply_v2_move` or is already there via `require_v2_active_mover`)
4. **require_active (lines 190-202):** Delete — only used by legacy `apply_move` and `apply_fire`
5. **apply_move (lines 204-262):** Delete — legacy move path
6. **apply_fire (lines 360-437):** Delete — legacy fire path
7. **apply_end_action (lines 439-445):** Delete — legacy end-action path
8. **Module doc (line 1-2):** Update from "FASA / Bocchino STCS-style" to "Combat v2"

### src/game_state.rs
1. **round field (line ~128):** Remove `round: u8` field
2. **acted_this_round (line 129):** Remove `acted_this_round: HashSet<u32>` field
3. **action_order (line 127):** Remove `action_order: Vec<u32>` field
4. **round() (line 244):** Remove accessor
5. **action_order() (line 248-249):** Remove accessor
6. **active_ship() (line 256-259):** Remove — legacy concept
7. **has_acted() (line 262-263):** Remove
8. **rebuild_action_order() (line 316-326):** Remove — only used by legacy init + advance_round_or_turn
9. **end_ship_action() (line 976-981):** Remove
10. **force_end_round() (line 983-985):** Remove
11. **round_complete() (line 987-991):** Remove
12. **advance_round_or_turn() (line 993-1006):** Remove
13. **resolve_npc_actions() (line 1009-1088):** Remove — legacy NPC driver (v2 uses resolve_v2_npc_actions)
14. **Init (line ~186-204):** Remove `action_order`, `acted_this_round`, `round` init; remove `rebuild_action_order()` call
15. **spend_power() (line 270-282):** KEEP — still used? Check. Actually v2 uses `spend_v2_move_power`. But `spend_power` is used by `apply_fire` (being deleted) and tests/fasa.rs (being emptied). Check if anything else uses it. If only legacy, remove.

### src/snapshot.rs
1. **round field (line 83):** Remove or set to constant 0
2. **active_ship field (line 85):** Remove or repurpose for v2 active mover
3. **action_order field (line 86):** Remove
4. **from_game_state:** Remove corresponding assignments

### src/bin/shipsim.rs
- No explicit legacy references — uses serde to parse `Order` enum. Removing variants from `Order` automatically removes them from the CLI. No changes needed.

### tests/fasa.rs
- Empty the file (replace with comment like tests/movement.rs)

### tests/acceptance.rs
- Rewrite to use v2 orders (Allocate, Move, CommitFire, ReadyFire, EndTurn) instead of `Order::Fire`

### tests/fleet_campaign.rs
- Uses `game.active_ship()` — needs to be updated to use a v2 concept or removed

### tests/harness.rs
- `test_orders_file_emits_snapshots`: checks `snaps[0]["active_ship"]` — needs v2 fixture
- `test_soft_reject_illegal_fire`: sends `fire` and `end_action` orders via stdin — needs v2 rewrite
- `test_d8_fixture_regenerate_lock`: uses `d8_frontend_orders.jsonl` which has `end_action` — needs v2 orders

### scenarios/d8_frontend_orders.jsonl
- Currently: `{"type":"move","ship":1,"mode":"forward"}` + `{"type":"end_action","ship":1}`
- Rewrite to v2 orders

## Execution Order
1. movement.rs — remove legacy Order variants + functions
2. game_state.rs — remove legacy state + methods
3. snapshot.rs — remove legacy fields
4. tests/fasa.rs — empty
5. tests/acceptance.rs — rewrite for v2
6. tests/fleet_campaign.rs — fix
7. tests/harness.rs — rewrite for v2
8. scenarios/d8_frontend_orders.jsonl — rewrite for v2
9. tests/fixtures/d8/snapshots.jsonl — regenerate
10. cargo test + cargo clippy
