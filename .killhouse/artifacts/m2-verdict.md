# M2 Verdict

verdict: COMPLETE

Milestone: M2 — Data model + Allocate order

Evidence:

- Red gate: `cargo test v2_allocate` initially failed because `Order::Allocate`,
  allocation snapshot fields, and phase/move-order state did not exist.
- Green gate: `cargo test --test v2_allocate` passes, 4 tests.
- Guard: `cargo test` passes.
- Guard: `cargo clippy --all-targets -- -D warnings` passes.
- Contract review: initial blockers repaired; re-review returned PASS.

Implemented:

- v2 `Allocate` order with validation and soft errors.
- v2 phase starts at `allocate`; advances to `movement` after all living ships allocate.
- per-ship movement allocation, move remaining, keel, powered shields,
  remaining shields, and weapon charges.
- move order sorted by movement allocation descending, with tied groups shuffled
  by PRNG once when order is built.
- snapshot fields for phase, move order, allocation state, and v2 weapon
  kind/mount/max charge.
- generic ship power budgets updated to escort 14 and cruiser 22, with v2
  weapon metadata added while preserving legacy weapons for pre-M7 tests.

Checkpoint:

- Next milestone is M3 — Movement phase + momentum.
- Stop requested by checkpoint autonomy.
