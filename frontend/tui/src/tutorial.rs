//! Tutorial mode — TUI-native, step-gated walkthrough.
//!
//! Race past the escort, inspect the tactical map, turn onto its stern, and
//! destroy it with beam + torp + plasma on turn 1
//! (`scenarios/tutorial_rear_attack.toml`, seed 4).
//!
//! Yellow bar = short **why + key** line (no "DO NOW" prefix). Bottom panel
//! holds the longer coach text.

use crate::protocol::Snapshot;

/// What kind of keypress/action a tutorial step expects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpectedAction {
    /// Down until the allocate cursor is on this field index.
    /// 0 = movement, 1..=n = weapons (**ship order**), then shields 0..5.
    /// (Tab is disabled during the lesson — use ↓/↑.)
    NavField(usize),
    /// Adjust the current allocate field until it equals `target`.
    ReachValue {
        field: usize,
        target: u32,
    },
    CommitAllocate,
    /// v4 path editor: lay this many total `move_f` steps into the draft path.
    PathForward(u32),
    /// v4 path editor: turn the draft path's projected facing to this value.
    PathFace(u32),
    /// v4 path editor: submit the drafted `commit_path`.
    PathCommit,
    EnterMap,
    PanMap,
    ZoomOut,
    ZoomIn,
    RecenterMap,
    ExitMap,
    EnterFire,
    ShieldFacing(u32),
    /// v4 volley builder: queue the selected weapon into the draft volley.
    FireWeapon,
    TabWeapon,
    /// v4 volley builder: submit the drafted `commit_volley`.
    ReadyFire,
    Dismiss,
}

/// One step in the tutorial sequence.
#[derive(Debug, Clone)]
pub struct TutorialStep {
    pub title: &'static str,
    /// Longer coach text (bottom panel) — systems and intent.
    pub text: &'static str,
    /// Short reason for the yellow bar (what system + why). No prefix.
    pub why: &'static str,
    pub expected: ExpectedAction,
    /// Keys only (also echoed in the coach panel).
    pub hint: &'static str,
}

/// The tutorial controller.
pub struct Tutorial {
    #[allow(dead_code)]
    pub name: &'static str,
    #[allow(dead_code)]
    pub objective: &'static str,
    pub steps: &'static [TutorialStep],
    pub current: usize,
    pub error_msg: Option<String>,
}

impl Tutorial {
    pub fn new() -> Self {
        Tutorial {
            name: "rear-attack",
            objective: "Race past the escort, inspect the map, and destroy it \
                        from behind with all weapons.",
            steps: REAR_ATTACK_STEPS,
            current: 0,
            error_msg: None,
        }
    }

    pub fn current_step(&self) -> Option<&TutorialStep> {
        self.steps.get(self.current)
    }

    pub fn is_complete(&self) -> bool {
        self.current >= self.steps.len()
    }

    pub fn advance(&mut self) {
        self.current += 1;
        self.error_msg = None;
    }

    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.error_msg = Some(msg.into());
    }

    pub fn check_action(&mut self, action: &ExpectedAction) -> bool {
        if let Some(step) = self.current_step() {
            if let ExpectedAction::NavField(target) = step.expected {
                if let ExpectedAction::NavField(next) = action {
                    // Allow stepping toward the target field (↓ from above).
                    if *next <= target {
                        self.error_msg = None;
                        if *next == target {
                            self.advance();
                        }
                        return true;
                    }
                    self.set_error(format!("Go to field {target} (↓). {}", step.hint));
                    return false;
                }
            }
            if let ExpectedAction::ShieldFacing(target) = step.expected {
                if let ExpectedAction::ShieldFacing(next) = action {
                    if *next <= target {
                        self.error_msg = None;
                        if *next == target {
                            self.advance();
                        }
                        return true;
                    }
                    self.set_error(format!("Select shield face {target} with →. {}", step.hint));
                    return false;
                }
            }
            if action_matches(&step.expected, action) {
                self.advance();
                return true;
            }
            self.set_error(format!("Expected: {}. {}", step.title, step.hint));
        }
        false
    }

    /// Validate an order-backed action without advancing the lesson. The
    /// caller advances only after the engine returns an accepted snapshot.
    pub fn validate_action(&mut self, action: &ExpectedAction) -> bool {
        if let Some(step) = self.current_step() {
            if action_matches(&step.expected, action) {
                return true;
            }
            self.set_error(format!("Expected: {}. {}", step.title, step.hint));
        }
        false
    }

    /// Free ←/→ on the correct field; advance only when value == target.
    pub fn check_reach_value(
        &mut self,
        field: usize,
        old_value: u32,
        new_value: u32,
    ) -> (bool, bool) {
        let Some(step) = self.current_step() else {
            return (true, false);
        };
        let ExpectedAction::ReachValue {
            field: exp_field,
            target,
        } = step.expected
        else {
            self.set_error(format!("Expected: {}. {}", step.title, step.hint));
            return (false, false);
        };
        if field != exp_field {
            self.set_error(format!(
                "Wrong field (▶ slot {field}; need {}). Press ↓/↑. {}",
                field_label(exp_field),
                step.hint
            ));
            return (false, false);
        }
        if new_value == old_value {
            self.set_error(format!(
                "Value is {old_value}; need {target}. Use → / ← (or digits)."
            ));
            return (false, false);
        }
        if new_value == target {
            self.advance();
            return (true, true);
        }
        if new_value > target {
            self.error_msg = Some(format!(
                "Too high ({new_value} > {target}). Press ← to come back down."
            ));
        } else {
            self.error_msg = Some(format!(
                "Now {new_value} / need {target}. Press → to raise (← lowers)."
            ));
        }
        (true, false)
    }

    /// Yellow bar line: **why** first, then the key action / live value.
    pub fn do_now_line(&self, cursor: Option<usize>, field_value: Option<u32>) -> String {
        if self.is_complete() {
            return "Tutorial complete — press q to quit.".to_string();
        }
        let Some(step) = self.current_step() else {
            return "Tutorial complete — press q to quit.".to_string();
        };
        let why = step.why;
        match step.expected {
            ExpectedAction::ReachValue { field, target } => {
                let cur = field_value.unwrap_or(0);
                let on = cursor == Some(field);
                let name = field_label(field);
                if !on {
                    format!("{why} · ↓/↑ until ▶ is on {name}, then set to {target}")
                } else if cur < target {
                    format!("{why} · {name} {cur}→{target}  (arrows or type {target})")
                } else if cur > target {
                    format!("{why} · {name} {cur}→{target}  (← back down · overshot)")
                } else {
                    format!("{why} · {name} is {target} — should advance")
                }
            }
            ExpectedAction::NavField(target) => {
                let cur = cursor.unwrap_or(0);
                let name = field_label(target);
                format!("{why} · ↓ to ▶ {name}  (now on {})", field_label(cur))
            }
            ExpectedAction::CommitAllocate => {
                format!("{why} · Enter (lock plan, open movement)")
            }
            ExpectedAction::PathForward(n) => {
                format!("{why} · press w to add forward steps (need {n})")
            }
            ExpectedAction::PathFace(f) => {
                format!("{why} · press {f} to turn the path's nose to facing {f}")
            }
            ExpectedAction::PathCommit => format!("{why} · Enter (commit the path)"),
            ExpectedAction::EnterMap => format!("{why} · v (focus the map)"),
            ExpectedAction::PanMap => format!("{why} · a (pan west / left)"),
            ExpectedAction::ZoomOut => format!("{why} · - (zoom out)"),
            ExpectedAction::ZoomIn => format!("{why} · + (zoom in)"),
            ExpectedAction::RecenterMap => format!("{why} · c (auto-fit contacts)"),
            ExpectedAction::ExitMap => format!("{why} · v (return to fire controls)"),
            ExpectedAction::EnterFire => format!("{why} · f or Enter (fire mode)"),
            ExpectedAction::ShieldFacing(f) => {
                format!("{why} · → until target shield face = {f}")
            }
            ExpectedAction::FireWeapon => format!("{why} · Enter (queue shot)"),
            ExpectedAction::TabWeapon => format!("{why} · ↓ (next weapon)"),
            ExpectedAction::ReadyFire => {
                format!("{why} · Space (fire the volley)")
            }
            ExpectedAction::Dismiss => format!("{why} · Enter or q"),
        }
    }

    pub fn narration(&self) -> String {
        if self.is_complete() {
            return "Tutorial complete! The rear-arc alpha strike secured the win. \
                    Press q to quit."
                .to_string();
        }
        match self.current_step() {
            Some(step) => {
                // The panel title already carries step/title and the yellow
                // prompt already carries the key. Spend the scarce coach rows
                // on explanation and pinned error feedback instead.
                let mut text = String::new();
                if let Some(ref err) = self.error_msg {
                    text.push_str(&format!("⚠ {err}\n"));
                }
                // Source narration uses Markdown markers; this terminal
                // client renders plain text, so keep the lesson readable.
                text.push_str(&step.text.replace("**", "").replace('`', ""));
                text
            }
            None => "Tutorial complete!".to_string(),
        }
    }

    #[allow(dead_code)]
    pub fn state_error(&self, snap: &Snapshot) -> Option<String> {
        if self.is_complete() {
            return None;
        }
        let step = self.current_step()?;
        if snap.is_over() && !matches!(step.expected, ExpectedAction::Dismiss) {
            if snap.status == "Won" {
                return None;
            }
            return Some(format!("Game ended unexpectedly: {}", snap.status));
        }
        None
    }
}

/// Human labels for allocate cursor slots (heavy cruiser ship order).
fn field_label(field: usize) -> String {
    match field {
        0 => "Engine (Movement)".into(),
        1 => "beam_1".into(),
        2 => "torp_1".into(),
        3 => "plasma_1".into(),
        4 => "shield F (forward)".into(),
        5 => "shield FR (fwd-right)".into(),
        6 => "shield RR (rear-right)".into(),
        7 => "shield R (rear)".into(),
        8 => "shield RL (rear-left)".into(),
        9 => "shield FL (fwd-left)".into(),
        n => format!("field {n}"),
    }
}

fn action_matches(expected: &ExpectedAction, actual: &ExpectedAction) -> bool {
    match (expected, actual) {
        (ExpectedAction::NavField(e), ExpectedAction::NavField(a)) => e == a,
        (ExpectedAction::CommitAllocate, ExpectedAction::CommitAllocate) => true,
        (ExpectedAction::PathForward(e), ExpectedAction::PathForward(a)) => e == a,
        (ExpectedAction::PathFace(e), ExpectedAction::PathFace(a)) => e == a,
        (ExpectedAction::PathCommit, ExpectedAction::PathCommit) => true,
        (ExpectedAction::EnterMap, ExpectedAction::EnterMap) => true,
        (ExpectedAction::PanMap, ExpectedAction::PanMap) => true,
        (ExpectedAction::ZoomOut, ExpectedAction::ZoomOut) => true,
        (ExpectedAction::ZoomIn, ExpectedAction::ZoomIn) => true,
        (ExpectedAction::RecenterMap, ExpectedAction::RecenterMap) => true,
        (ExpectedAction::ExitMap, ExpectedAction::ExitMap) => true,
        (ExpectedAction::EnterFire, ExpectedAction::EnterFire) => true,
        (ExpectedAction::ShieldFacing(e), ExpectedAction::ShieldFacing(a)) => e == a,
        (ExpectedAction::FireWeapon, ExpectedAction::FireWeapon) => true,
        (ExpectedAction::TabWeapon, ExpectedAction::TabWeapon) => true,
        (ExpectedAction::ReadyFire, ExpectedAction::ReadyFire) => true,
        (ExpectedAction::Dismiss, ExpectedAction::Dismiss) => true,
        _ => false,
    }
}

// ── Rear-attack sequence (matches REPL tutorial / seed 4) ─────────────────
//
// Allocate cursor (heavy cruiser, ship/TOML order):
//   0 Movement · 1 beam_1 · 2 torp_1 · 3 plasma_1
//   4 F · 5 FR · 6 RR · 7 R · 8 RL · 9 FL

static REAR_ATTACK_STEPS: &[TutorialStep] = &[
    // ── Turn 1 allocate ────────────────────────────────────────────────
    TutorialStep {
        title: "Engine power (Movement)",
        text: "Each turn you split a power pool. Movement buys **motion points** \
               for this turn only — one point per path step. There is no \
               inertia: your ship ends exactly where the path you draw ends, \
               then motion resets next turn.\n\n\
               Yellow bar shows why + keys. ▶ marks the selected allocate field. \
               Set Movement to 8 (this hull's cap) so we can lay a long path \
               past the escort.",
        why: "Movement = motion points for this turn's path",
        hint: "→ until Movement = 8, or type 8",
        expected: ExpectedAction::ReachValue {
            field: 0,
            target: 8,
        },
    },
    TutorialStep {
        title: "Charge the beam",
        text: "Weapons are separate power sinks. **beam_1** is your main gun: \
               multi-charge, solid damage, long range. Charge **carries** across \
               turns if you don't fire — we load it now and hold for the stern shot.\n\n\
               The form auto-selects beam_1 (▶). ↓/↑ move between fields; →/← set \
               the value. Charge to 4 (max) — more charge = more beam damage. We \
               will not shoot until we are behind the escort.",
        why: "beam_1 charge = damage budget for later volley",
        hint: "→ until beam charge = 4, or type 4",
        expected: ExpectedAction::ReachValue {
            field: 1,
            target: 4,
        },
    },
    TutorialStep {
        title: "Charge torpedo",
        text: "torp_1 is a single-charge, fixed-damage shot. It fires in the \
               same volley as beam and plasma. Weapon rows follow ship order \
               (same list you will see in fire mode). Charge to 1 (max) and leave \
               it loaded for the rear volley.",
        why: "Arm torp for the same volley as beam + plasma",
        hint: "→ once, or type 1",
        expected: ExpectedAction::ReachValue {
            field: 2,
            target: 1,
        },
    },
    TutorialStep {
        title: "Charge plasma",
        text: "plasma_1 is a short-range hammer (max charge 1). Huge damage at \
               close range — the finisher of the rear-arc dump. One point arms \
               it; the charge stays until you fire.",
        why: "Arm plasma for the close rear-arc dump",
        hint: "→ once, or type 1",
        expected: ExpectedAction::ReachValue {
            field: 3,
            target: 1,
        },
    },
    TutorialStep {
        title: "Select forward shield",
        text: "Shields are **six faces** around the ship (F, FR, RR, R, RL, FL). \
               They always start at 0 each allocate — no leftover armor. Power on \
               a face absorbs hits that land there.\n\n\
               **F (forward)** faces your nose. The escort will shoot your bow \
               while you close, so we armor F first.",
        why: "Select shield F — nose armor vs their approach fire",
        hint: "↓ to shield F",
        expected: ExpectedAction::NavField(4),
    },
    TutorialStep {
        title: "Power forward shield",
        text: "Put 6 on F (max per face). Hits on your forward arc spend this \
               before hull. Budget: 8 engine + 4+1+1 weapons + 6 F = 20 of 22.",
        why: "Shield F=6 so bow hits soak before hull",
        hint: "→ until F = 6, or type 6",
        expected: ExpectedAction::ReachValue {
            field: 4,
            target: 6,
        },
    },
    TutorialStep {
        title: "Commit allocate",
        text: "Nothing is spent in the engine until you commit. Enter sends the \
               allocate order and opens the movement stage, where you draw one \
               path for the whole turn.",
        why: "Commit power plan — draft becomes real",
        hint: "Enter",
        expected: ExpectedAction::CommitAllocate,
    },
    // ── Turn 1 movement — draw one path ─────────────────────────────────
    TutorialStep {
        title: "Draw the run east",
        text: "Movement is a single path you draw, then commit. Each step spends \
               one motion point (you have 8). Press w to add a forward step \
               (move_f). The escort will rush west toward you; lay **five** \
               forward steps so you shoot past and end up on its eastern side.",
        why: "Lay 5 forward steps to cross the escort",
        hint: "press w five times",
        expected: ExpectedAction::PathForward(5),
    },
    TutorialStep {
        title: "Turn the nose onto its stern",
        text: "Your guns are forward-arc, so the path's final facing decides \
               where they point. After crossing, the escort is west of you and \
               its unshielded stern faces east — toward you. Press 3 to append \
               the turns that leave your nose facing west (3). That spends your \
               last 3 motion points (5 forward + 3 turn = 8).",
        why: "End the path facing west — guns onto the stern",
        hint: "press 3",
        expected: ExpectedAction::PathFace(3),
    },
    TutorialStep {
        title: "Commit the path",
        text: "The path is [w w w w w, turn to 3]. Enter submits it as one \
               commit_path. Both ships move simultaneously; then the firing \
               stage opens with everyone in their final positions.",
        why: "Commit the path — movement resolves",
        hint: "Enter",
        expected: ExpectedAction::PathCommit,
    },
    TutorialStep {
        title: "Focus the tactical map",
        text: "You crossed the escort and now have its unshielded stern in front \
               of your guns. Before firing, press v to focus the map. Map focus \
               is read-only: it never spends motion or advances the phase.",
        why: "Inspect the pass without issuing an order",
        hint: "v",
        expected: ExpectedAction::EnterMap,
    },
    TutorialStep {
        title: "Pan toward the escort",
        text: "WASD pans the camera. The escort is west (left) of you, so press a. \
               Panning changes only what you can see; ships keep their coordinates.",
        why: "Move the camera west to inspect the target",
        hint: "a",
        expected: ExpectedAction::PanMap,
    },
    TutorialStep {
        title: "Zoom out",
        text: "- zooms out to cover more space. Use it when contacts or projected \
               movement spread beyond the current view.",
        why: "Fit more of the battle into the map",
        hint: "-",
        expected: ExpectedAction::ZoomOut,
    },
    TutorialStep {
        title: "Zoom in",
        text: "+ zooms back in for readable local geometry. Zoom and pan remain \
               manual until you ask the camera to auto-fit again.",
        why: "Return to a closer tactical view",
        hint: "+",
        expected: ExpectedAction::ZoomIn,
    },
    TutorialStep {
        title: "Auto-fit contacts",
        text: "c clears manual pan and zoom. The map automatically frames all \
               living ships and, during allocation, your movement preview.",
        why: "Let the camera frame the battle again",
        hint: "c",
        expected: ExpectedAction::RecenterMap,
    },
    TutorialStep {
        title: "Return to fire controls",
        text: "Press v again to leave map focus. Your firing window is still \
               waiting exactly where you left it.",
        why: "Return without changing the game state",
        hint: "v",
        expected: ExpectedAction::ExitMap,
    },
    TutorialStep {
        title: "Aim at the rear shield face",
        text: "Shots must name the target face they enter. The escort faces west, \
               so your attack from its east side hits face 3: R (rear). Press → \
               until the fire panel shows target shield R. The engine validates \
               this against the actual geometry.",
        why: "Aim the volley through the unshielded rear face",
        hint: "→ until target shield = 3:R",
        expected: ExpectedAction::ShieldFacing(3),
    },
    TutorialStep {
        title: "Fire the beam",
        text: "Fire mode is open. Enter queues **beam_1** at the escort (does not \
               resolve yet). Charge drops when everyone readies.",
        why: "Queue beam into their unshielded stern",
        hint: "Enter",
        expected: ExpectedAction::FireWeapon,
    },
    TutorialStep {
        title: "Select torpedo",
        text: "↓ cycles the selected weapon to torp_1 (ship order: beam, torp, plasma).",
        why: "Select torp for the same volley",
        hint: "↓",
        expected: ExpectedAction::TabWeapon,
    },
    TutorialStep {
        title: "Fire the torpedo",
        text: "Queue torp_1. It does not resolve until every living ship readies.",
        why: "Queue torp into the simultaneous volley",
        hint: "Enter",
        expected: ExpectedAction::FireWeapon,
    },
    TutorialStep {
        title: "Select plasma",
        text: "↓ to plasma_1 — the short-range finisher.",
        why: "Select plasma finisher",
        hint: "↓",
        expected: ExpectedAction::TabWeapon,
    },
    TutorialStep {
        title: "Fire the plasma",
        text: "Queue plasma. All three resolve together when you press Space.",
        why: "Queue plasma — complete the full volley",
        hint: "Enter",
        expected: ExpectedAction::FireWeapon,
    },
    TutorialStep {
        title: "Resolve the kill",
        text: "Space marks you ready. Hits/misses resolve; escort should die (Won).",
        why: "Resolve the triple volley",
        hint: "Space",
        expected: ExpectedAction::ReadyFire,
    },
    TutorialStep {
        title: "Victory",
        text: "Turn-one rear-arc volley complete. Yellow bar can rest.",
        why: "Won — Enter dismisses or q quits",
        hint: "Enter or q",
        expected: ExpectedAction::Dismiss,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tutorial_starts_at_step_zero() {
        let t = Tutorial::new();
        assert_eq!(t.current, 0);
        assert!(!t.is_complete());
        assert_eq!(t.name, "rear-attack");
    }

    #[test]
    fn reach_value_requires_target() {
        let mut t = Tutorial::new();
        // Step 0 targets movement = 8 (protocol v4 motion pool).
        let (ok, adv) = t.check_reach_value(0, 0, 1);
        assert!(ok);
        assert!(!adv);
        let (ok, adv) = t.check_reach_value(0, 7, 8);
        assert!(ok);
        assert!(adv);
        assert_eq!(t.current, 1);
    }

    #[test]
    fn reach_value_allows_left_to_correct_overshoot() {
        let mut t = Tutorial::new();
        let (ok, adv) = t.check_reach_value(0, 8, 9);
        assert!(ok);
        assert!(!adv);
        let (ok, adv) = t.check_reach_value(0, 9, 8);
        assert!(ok);
        assert!(adv);
    }

    #[test]
    fn reach_value_allows_left_while_below_target() {
        let mut t = Tutorial::new();
        assert!(t.check_reach_value(0, 0, 3).0);
        let (ok, adv) = t.check_reach_value(0, 3, 2);
        assert!(ok);
        assert!(!adv);
    }

    #[test]
    fn yellow_line_is_why_then_keys_no_do_now_prefix() {
        let t = Tutorial::new();
        let line = t.do_now_line(Some(0), Some(3));
        assert!(!line.starts_with("DO NOW"));
        assert!(line.contains("Engine") || line.contains("thrust"));
        assert!(line.contains('→') || line.contains("10"));
        // Shield step names the face
        let mut t = Tutorial::new();
        while t
            .current_step()
            .map(|s| !matches!(s.expected, ExpectedAction::ReachValue { field: 4, .. }))
            .unwrap_or(false)
        {
            t.advance();
        }
        let line = t.do_now_line(Some(4), Some(2));
        assert!(
            line.contains("F")
                || line.contains("forward")
                || line.contains("bow")
                || line.contains("Shield")
        );
    }

    #[test]
    fn wrong_action_blocked() {
        let mut t = Tutorial::new();
        assert!(!t.check_action(&ExpectedAction::PathCommit));
        assert_eq!(t.current, 0);
        assert!(t.error_msg.is_some());
    }

    #[test]
    fn sequence_completes() {
        let mut t = Tutorial::new();
        while !t.is_complete() {
            let step = t.current_step().unwrap().expected.clone();
            match step {
                ExpectedAction::ReachValue { field, target } => {
                    let (ok, _) =
                        t.check_reach_value(field, target.saturating_sub(1).min(target), target);
                    assert!(ok, "reach failed at step {}", t.current);
                }
                other => {
                    assert!(
                        t.check_action(&other),
                        "action {:?} failed at step {}",
                        other,
                        t.current
                    );
                }
            }
        }
        assert!(t.is_complete());
        assert!(REAR_ATTACK_STEPS
            .iter()
            .any(|s| matches!(s.expected, ExpectedAction::PathFace(3))));
        assert!(
            REAR_ATTACK_STEPS
                .iter()
                .filter(|s| matches!(s.expected, ExpectedAction::FireWeapon))
                .count()
                >= 3
        );
    }

    #[test]
    fn narration_explains_systems() {
        let t = Tutorial::new();
        let n = t.narration();
        assert!(n.contains("thrust") || n.contains("Movement") || n.contains("Engine"));
    }
}
