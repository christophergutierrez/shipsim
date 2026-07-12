# Deferred to M5 (ADR-0022)

These test files are not compiled as part of the test suite (files in a
subdirectory of `tests/` are not auto-discovered by Cargo). They were moved
here, unmodified, when M4 deleted the single-active-mover legacy movement
model and its `apply_v2_move`/`can_any_move`/`move_remaining`/`keel` fields.

Every test in this directory depends on reaching `Phase::Firing`, which is
unreachable in M4: the four-phase maneuver/translation core has no fire
integration yet (that's M5's scope — "Fire Interleaving and Complete Turn
Loop"). Rather than hand-patch each file's legacy movement-driving helpers
(`MoveMode`, `move_order`, `active_v2_mover`, ...) to compile against APIs
that no longer exist, for logic that can't run until M5 rewires firing
anyway, they're parked here to preserve the still-valid fire/damage/turn-loop
assertions for revival then.

When implementing M5:
- Move these files back to `tests/`.
- Update their movement-driving helpers to commit maneuvers (`CommitManeuver`
  / `PassMove` == coast) across the four phases instead of the old
  single-active-mover `Order::Move`/`PassMove` loop.
- Delete this README.
