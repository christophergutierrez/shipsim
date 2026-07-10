# M4 Verdict: Firing Phase + Simultaneous Resolve

Verdict: PASS

Scope reviewed against `implementation-plan-combat-v2.md` M4:

- `CommitFire` / `ReadyFire` protocol implemented for v2 firing.
- Commit legality checks charged v2 weapon, target, range, arc, legal shield
  facing, and beam projected damage.
- Ready from all living ships auto-resolves the firing batch.
- Resolve uses deterministic commit order `(ship, weapon)`.
- Hit/miss uses v2 d20 threshold tables.
- Hit damage uses v2 beam/plasma/torp damage tables.
- Shield facing power absorbs first; overflow goes to hull.
- Misses still clear charge and mark the weapon fired.
- Multiple weapons from one ship can commit in the same phase.
- `fire_commits` and ready ships are visible in snapshots.
- Legacy FASA tests remain green.

Contract review: PASS

Verification:

- `cargo test --test v2_fire`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`

Notes:

- Initial review found that a resolved firing phase allowed a second batch and
  that resolve-time legality re-check was incomplete. Both were repaired before
  verdict.
- M4 currently transitions to `turn_end` after resolution to close the firing
  batch. M5 owns replacing that placeholder with the full move/fire loop and
  EndTurn behavior.
