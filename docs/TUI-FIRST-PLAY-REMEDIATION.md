# TUI first-play remediation

This closeout records the consolidated first-play remediation milestones.
The target terminal size is 80×24; the engine and NDJSON protocol remain the
source of truth for rules and legality.

## Completed milestones

- M0: pinned floor-size fixture and one-input transition tests.
- M1: exposed fire weapon and shield-face controls in the footer; legal fire
  previews select an approved face.
- M2: rendered all six shield faces and caps, marked the target-relative face,
  and added a direct shield-section jump.
- M3: bounded disabled-ship Space passing to the initiating turn, added a
  visible turn receipt, and made GameOver exit keys real.
- M4: added a one-line resolution receipt for hull, shield, system, and weapon
  changes; engine-loss movement capacity is already called out in Movement.
- M5: added accepted/cap/carry/unavailable feedback and phase-specific Enter /
  Space footer guidance.
- M6: shortened floor-size legends, clarified `alloc`, `engine boxes`, and
  movement-key copy, and added a minimal first-play orientation line.

## Verification

The final local gates passed:

```text
cargo test --manifest-path frontend/tui/Cargo.toml --quiet   # 279 passed
cargo clippy --manifest-path frontend/tui/Cargo.toml --all-targets -- -D warnings
cargo test --quiet                                           # repository suite passed
```

The M0 fixture suite also covers 80×24 rendering and bounded one-input phase
transitions. The focused regression tests are named `tui_m01` through `tui_m63`
(with additional focused checks for movement-cause and phase-footer behavior).

## Acceptance-cohort note

A fresh ten-agent blind cohort could not be started during this closeout because
the agent-thread capacity was already exhausted. No cohort success rate is
claimed here. The manual/fixture gates above are complete; rerun the blind
cohort when agent capacity is available using the battle command:

```bash
cargo run --manifest-path frontend/tui/Cargo.toml -- scenarios/battle.toml
```

