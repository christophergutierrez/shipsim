//! Tutorial mode — TUI-native, step-gated walkthrough.
//!
//! Same fight as the REPL `--tutorial rear-attack` lesson: race past the
//! escort, reverse-thrust brake, revector west, point-blank dump of beam +
//! torp + plasma on turn 3 (`scenarios/tutorial_rear_attack.toml`, seed 4).
//!
//! Yellow bar = short **why + key** line (no "DO NOW" prefix). Bottom panel
//! holds the longer coach text.

use crate::protocol::Snapshot;

/// What kind of keypress/action a tutorial step expects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpectedAction {
    /// Tab/Down until the allocate cursor is on this field index.
    /// 0 = movement, 1..=n = weapons (BTreeMap order), then shields 0..5.
    NavField(usize),
    /// Adjust the current allocate field until it equals `target`.
    ReachValue { field: usize, target: u32 },
    CommitAllocate,
    Accel,
    TurnTo(u32),
    Coast,
    EnterFire,
    FireWeapon,
    TabWeapon,
    ReadyFire,
    EndTurn,
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
            objective: "Race past the escort, brake and revector, destroy it at \
                        point blank with all weapons.",
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
            if action_matches(&step.expected, action) {
                self.advance();
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
                    format!("{why} · {name} {cur}→{target}  (→ raise · ← lower)")
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
            ExpectedAction::Accel => format!("{why} · t (accel along nose)"),
            ExpectedAction::TurnTo(f) => {
                format!("{why} · press {f} (face {f} only — course unchanged)")
            }
            ExpectedAction::Coast => format!("{why} · c (coast / free slide)"),
            ExpectedAction::EnterFire => format!("{why} · f or Enter (fire mode)"),
            ExpectedAction::FireWeapon => format!("{why} · Enter (queue shot)"),
            ExpectedAction::TabWeapon => format!("{why} · ↓ (next weapon)"),
            ExpectedAction::ReadyFire => {
                format!("{why} · Space (ready — resolve or skip fire)")
            }
            ExpectedAction::EndTurn => format!("{why} · e (end turn)"),
            ExpectedAction::Dismiss => format!("{why} · Enter or q"),
        }
    }

    pub fn narration(&self) -> String {
        if self.is_complete() {
            return "Tutorial complete! Point-blank alpha-strike secured the win. \
                    Press q to quit."
                .to_string();
        }
        match self.current_step() {
            Some(step) => {
                let mut text = format!(
                    "Step {}/{} — {}\n\n",
                    self.current + 1,
                    self.steps.len(),
                    step.title
                );
                text.push_str(step.text);
                text.push_str("\n\nKeys: ");
                text.push_str(step.hint);
                text.push_str("  ·  other keys blocked until this step finishes.");
                if let Some(ref err) = self.error_msg {
                    text.push_str(&format!("\n\n⚠ {err}"));
                }
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

/// Human labels for allocate cursor slots (heavy cruiser: 3 weapons A–Z).
fn field_label(field: usize) -> String {
    match field {
        0 => "Movement (engine)".into(),
        1 => "beam_1".into(),
        2 => "plasma_1".into(),
        3 => "torp_1".into(),
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
        (ExpectedAction::Accel, ExpectedAction::Accel) => true,
        (ExpectedAction::TurnTo(e), ExpectedAction::TurnTo(a)) => e == a,
        (ExpectedAction::Coast, ExpectedAction::Coast) => true,
        (ExpectedAction::EnterFire, ExpectedAction::EnterFire) => true,
        (ExpectedAction::FireWeapon, ExpectedAction::FireWeapon) => true,
        (ExpectedAction::TabWeapon, ExpectedAction::TabWeapon) => true,
        (ExpectedAction::ReadyFire, ExpectedAction::ReadyFire) => true,
        (ExpectedAction::EndTurn, ExpectedAction::EndTurn) => true,
        (ExpectedAction::Dismiss, ExpectedAction::Dismiss) => true,
        _ => false,
    }
}

// ── Rear-attack sequence (matches REPL tutorial / seed 4) ─────────────────
//
// Allocate cursor (heavy cruiser, weapons alphabetical):
//   0 Movement · 1 beam_1 · 2 plasma_1 · 3 torp_1
//   4 F · 5 FR · 6 RR · 7 R · 8 RL · 9 FL

static REAR_ATTACK_STEPS: &[TutorialStep] = &[
    // ── Turn 1 allocate ────────────────────────────────────────────────
    TutorialStep {
        title: "Engine power (Movement)",
        text: "Each turn you split a power pool. Movement is not distance — it \
               buys a **thrust pool** for this turn only. You will spend thrust \
               later to accel and turn. Velocity (speed/course) persists after \
               end-turn; thrust does not.\n\n\
               Yellow bar shows why + keys. ▶ marks the selected allocate field. \
               Set Movement to 10 so we can race past the escort.",
        why: "Engine = thrust this turn (not permanent speed)",
        hint: "→ until Movement = 10",
        expected: ExpectedAction::ReachValue {
            field: 0,
            target: 10,
        },
    },
    TutorialStep {
        title: "Select the beam",
        text: "Weapons are separate power sinks. **beam_1** is your main gun: \
               multi-charge, solid damage, long range. Charge **carries** across \
               turns if you don't fire — we load it now and hold for point blank.",
        why: "Move to beam_1 — main gun (charge carries until fired)",
        hint: "↓ to beam_1",
        expected: ExpectedAction::NavField(1),
    },
    TutorialStep {
        title: "Charge the beam",
        text: "Desired charge on beam_1 (max 4). More charge = more beam damage. \
               Full charge now; we will not shoot until we are behind the escort.",
        why: "beam_1 charge = damage budget for later volley",
        hint: "→ until beam charge = 4",
        expected: ExpectedAction::ReachValue {
            field: 1,
            target: 4,
        },
    },
    TutorialStep {
        title: "Select plasma",
        text: "plasma_1 is a short-range hammer (max charge 1). Huge damage at \
               range 1 — perfect for the kill shot. Weapons list alphabetically \
               (beam, plasma, torp).",
        why: "Move to plasma_1 — short-range heavy punch",
        hint: "↓ to plasma_1",
        expected: ExpectedAction::NavField(2),
    },
    TutorialStep {
        title: "Charge plasma",
        text: "One point arms the plasma. It stays charged until you fire it.",
        why: "Arm plasma for the point-blank dump",
        hint: "→ once (charge 1)",
        expected: ExpectedAction::ReachValue {
            field: 2,
            target: 1,
        },
    },
    TutorialStep {
        title: "Select torpedo",
        text: "torp_1 is a single-charge, fixed-damage shot. Third leg of the \
               alpha strike — fires with beam and plasma in one volley.",
        why: "Move to torp_1 — one-shot hull punch",
        hint: "↓ to torp_1",
        expected: ExpectedAction::NavField(3),
    },
    TutorialStep {
        title: "Charge torpedo",
        text: "Charge to 1 (max). Now all three weapons are loaded.",
        why: "Arm torp for the same volley as beam + plasma",
        hint: "→ once (charge 1)",
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
               before hull. Budget: 10 engine + 4+1+1 weapons + 6 F = 22 (full pool).",
        why: "Shield F=6 so bow hits soak before hull",
        hint: "→ until F = 6",
        expected: ExpectedAction::ReachValue {
            field: 4,
            target: 6,
        },
    },
    TutorialStep {
        title: "Commit allocate",
        text: "Nothing is spent in the engine until you commit. Enter sends the \
               allocate order and opens movement cycle 1 of 4.",
        why: "Commit power plan — draft becomes real",
        hint: "Enter",
        expected: ExpectedAction::CommitAllocate,
    },
    // ── Turn 1 movement ────────────────────────────────────────────────
    TutorialStep {
        title: "Accel — leave the pier",
        text: "t = accel: spend 1 thrust along your **facing** (nose). From a \
               stop, that sets course = facing and speed 1, then you slide 1 hex \
               on course. Each cycle you slide `speed` hexes.",
        why: "Build eastbound speed — race past the escort",
        hint: "t",
        expected: ExpectedAction::Accel,
    },
    TutorialStep {
        title: "Hold fire — cycle 1",
        text: "You can bear on them, but you would hit their forward shields. \
               Space = ready fire: leave the fire window **without** spending \
               weapon charge. (e would end the whole turn — wrong here.)",
        why: "Don't waste charged weapons on their bow",
        hint: "Space",
        expected: ExpectedAction::ReadyFire,
    },
    TutorialStep {
        title: "Accel — speed 2",
        text: "Accel again: speed 2, slide 2 hexes east. Range collapses fast.",
        why: "More speed = longer slides east each cycle",
        hint: "t",
        expected: ExpectedAction::Accel,
    },
    TutorialStep {
        title: "Hold fire — cycle 2",
        text: "Still not a stern shot. Hold charge.",
        why: "Still wrong geometry — hold the volley",
        hint: "Space",
        expected: ExpectedAction::ReadyFire,
    },
    TutorialStep {
        title: "Accel — speed 3",
        text: "Speed 3, slide 3. You cross / pass their track.",
        why: "Cross their track so we end up behind them",
        hint: "t",
        expected: ExpectedAction::Accel,
    },
    TutorialStep {
        title: "Hold fire — cycle 3",
        text: "Charge stays for point blank.",
        why: "Save the alpha strike",
        hint: "Space",
        expected: ExpectedAction::ReadyFire,
    },
    TutorialStep {
        title: "Turn nose west",
        text: "Facing and course are different. **Turn** only changes facing \
               (guns/nose); course/speed keep you sliding east. Face 3 = west. \
               Cost is hex-ring distance (0→3 costs 3 thrust). Guns now look back \
               along the track at their stern.",
        why: "Point guns west while still flying east (stern shot)",
        hint: "3",
        expected: ExpectedAction::TurnTo(3),
    },
    TutorialStep {
        title: "Close setup turn",
        text: "Ready out of fire. Next: end-turn keeps velocity/course and \
               unfired weapon charge; shields go to 0 next allocate.",
        why: "Finish fire window before ending the turn",
        hint: "Space",
        expected: ExpectedAction::ReadyFire,
    },
    TutorialStep {
        title: "End turn 1",
        text: "e advances the whole turn. Thrust pool resets; weapons stay charged.",
        why: "Next turn — brake and come back west",
        hint: "e",
        expected: ExpectedAction::EndTurn,
    },
    // ── Turn 2 allocate ────────────────────────────────────────────────
    TutorialStep {
        title: "Engine again (brake fuel)",
        text: "You still have speed 3 course east. Weapons are already charged — \
               leave them (carried charge costs nothing to keep). Buy Movement 10 \
               again so you can reverse-thrust to a stop and push west.",
        why: "Thrust to brake eastbound vector, then push west",
        hint: "→ Movement to 10",
        expected: ExpectedAction::ReachValue {
            field: 0,
            target: 10,
        },
    },
    TutorialStep {
        title: "Select shield F again",
        text: "Shields do not carry. Every allocate rebuilds from zero. Skip the \
               weapon rows (already full) and put ▶ on **F** again.",
        why: "Rebuild nose shield from zero (shields never carry)",
        hint: "↓ to shield F",
        expected: ExpectedAction::NavField(4),
    },
    TutorialStep {
        title: "Shield F = 6",
        text: "Full forward face again while you maneuver near them.",
        why: "F=6 — forward arc protection this turn",
        hint: "→ F to 6",
        expected: ExpectedAction::ReachValue {
            field: 4,
            target: 6,
        },
    },
    TutorialStep {
        title: "Select FR",
        text: "**FR** is forward-right (face 1) — a shoulder facing. Cover side \
               hits during the pass. Six faces total: F FR RR R RL FL around the hull.",
        why: "FR = forward-right shoulder armor",
        hint: "↓ to FR",
        expected: ExpectedAction::NavField(5),
    },
    TutorialStep {
        title: "Shield FR = 3",
        text: "Partial power on FR. Not max — we still need FL and engine budget.",
        why: "FR=3 — side cover without emptying the pool",
        hint: "→ FR to 3",
        expected: ExpectedAction::ReachValue {
            field: 5,
            target: 3,
        },
    },
    TutorialStep {
        title: "Select FL",
        text: "**FL** is forward-left (face 5) — the other shoulder. Skip RR/R/RL \
               (rear faces); we care about the bow hemisphere while re-engaging.",
        why: "FL = forward-left shoulder (mirror of FR)",
        hint: "↓ to shield FL",
        expected: ExpectedAction::NavField(9),
    },
    TutorialStep {
        title: "Shield FL = 3",
        text: "FL=3. Total: 10 engine + 6+3+3 shields = 22; weapons free (carried).",
        why: "FL=3 — other shoulder; full defensive budget",
        hint: "→ FL to 3",
        expected: ExpectedAction::ReachValue {
            field: 9,
            target: 3,
        },
    },
    TutorialStep {
        title: "Commit turn 2",
        text: "Lock the plan. Next: thrust against your course to kill east speed.",
        why: "Commit — then reverse-thrust brake",
        hint: "Enter",
        expected: ExpectedAction::CommitAllocate,
    },
    // ── Turn 2 movement ────────────────────────────────────────────────
    TutorialStep {
        title: "Brake (1/3)",
        text: "Nose is west, course still east. Accel along facing against the \
               vector reduces speed (3→2). Same key as speeding up — geometry decides.",
        why: "Reverse-thrust: nose opposite course slows you",
        hint: "t",
        expected: ExpectedAction::Accel,
    },
    TutorialStep {
        title: "Ready — brake 1",
        text: "Clear the fire window; keep weapons charged.",
        why: "Skip fire — still reloading geometry",
        hint: "Space",
        expected: ExpectedAction::ReadyFire,
    },
    TutorialStep {
        title: "Brake (2/3)",
        text: "Speed 2→1.",
        why: "Keep braking the eastbound slide",
        hint: "t",
        expected: ExpectedAction::Accel,
    },
    TutorialStep {
        title: "Ready — brake 2",
        text: "Space.",
        why: "Skip fire",
        hint: "Space",
        expected: ExpectedAction::ReadyFire,
    },
    TutorialStep {
        title: "Brake (3/3)",
        text: "Speed 1→0. At rest, course becomes west (your facing).",
        why: "Kill eastbound speed — ready to push west",
        hint: "t",
        expected: ExpectedAction::Accel,
    },
    TutorialStep {
        title: "Ready — stopped",
        text: "Space.",
        why: "Skip fire",
        hint: "Space",
        expected: ExpectedAction::ReadyFire,
    },
    TutorialStep {
        title: "Push west",
        text: "From a stop, accel along face 3 sets course west and speed 1. \
               You start closing the escort again.",
        why: "Revector west toward the escort",
        hint: "t",
        expected: ExpectedAction::Accel,
    },
    TutorialStep {
        title: "Ready — end motion",
        text: "Weapons still full. End turn after this window.",
        why: "Clear fire before end-turn",
        hint: "Space",
        expected: ExpectedAction::ReadyFire,
    },
    TutorialStep {
        title: "End turn 2",
        text: "Kill run next: close to range 1 and dump all weapons.",
        why: "Into the attack turn",
        hint: "e",
        expected: ExpectedAction::EndTurn,
    },
    // ── Turn 3 allocate ────────────────────────────────────────────────
    TutorialStep {
        title: "Engine for the slam",
        text: "Weapons still charged from turn 1. Only need thrust + shields.",
        why: "Thrust to close to point blank",
        hint: "→ Movement to 10",
        expected: ExpectedAction::ReachValue {
            field: 0,
            target: 10,
        },
    },
    TutorialStep {
        title: "Shield F",
        text: "Rebuild F from zero again (every allocate).",
        why: "Rebuild forward shield from zero",
        hint: "↓ to F",
        expected: ExpectedAction::NavField(4),
    },
    TutorialStep {
        title: "F = 6",
        text: "Full nose armor for the final approach.",
        why: "F=6 on the kill run",
        hint: "→ F to 6",
        expected: ExpectedAction::ReachValue {
            field: 4,
            target: 6,
        },
    },
    TutorialStep {
        title: "FR",
        text: "Shoulder armor again.",
        why: "FR shoulder for the pass",
        hint: "↓ to FR",
        expected: ExpectedAction::NavField(5),
    },
    TutorialStep {
        title: "FR = 3",
        text: "Partial FR.",
        why: "FR=3 side cover",
        hint: "→ FR to 3",
        expected: ExpectedAction::ReachValue {
            field: 5,
            target: 3,
        },
    },
    TutorialStep {
        title: "FL",
        text: "Other shoulder.",
        why: "FL other shoulder",
        hint: "↓ to FL",
        expected: ExpectedAction::NavField(9),
    },
    TutorialStep {
        title: "FL = 3",
        text: "Then commit and close.",
        why: "FL=3 — then lock and attack",
        hint: "→ FL to 3",
        expected: ExpectedAction::ReachValue {
            field: 9,
            target: 3,
        },
    },
    TutorialStep {
        title: "Commit kill run",
        text: "Movement opens. Close hard west.",
        why: "Commit — close to range 1",
        hint: "Enter",
        expected: ExpectedAction::CommitAllocate,
    },
    // ── Turn 3 movement + volley ───────────────────────────────────────
    TutorialStep {
        title: "Close — speed 2",
        text: "Accel west to collapse range.",
        why: "Close range for the dump",
        hint: "t",
        expected: ExpectedAction::Accel,
    },
    TutorialStep {
        title: "Hold — not yet",
        text: "Medium range — hold the full volley for point blank.",
        why: "Not yet — max damage at range 1",
        hint: "Space",
        expected: ExpectedAction::ReadyFire,
    },
    TutorialStep {
        title: "Close — speed 3",
        text: "Keep closing.",
        why: "Keep closing",
        hint: "t",
        expected: ExpectedAction::Accel,
    },
    TutorialStep {
        title: "Hold — almost",
        text: "One more accel after this window → range 1.",
        why: "Almost point blank",
        hint: "Space",
        expected: ExpectedAction::ReadyFire,
    },
    TutorialStep {
        title: "Point blank",
        text: "Accel into range 1, nose on them from behind (higher q, face west).",
        why: "Range 1 — best damage + hit chance",
        hint: "t",
        expected: ExpectedAction::Accel,
    },
    TutorialStep {
        title: "Fire the beam",
        text: "Fire mode is open. Enter queues **beam_1** at the escort (does not \
               resolve yet). Charge drops when everyone readies.",
        why: "Queue beam — main damage at PB",
        hint: "Enter",
        expected: ExpectedAction::FireWeapon,
    },
    TutorialStep {
        title: "Select torpedo",
        text: "↓ cycles the selected weapon to torp_1.",
        why: "Select torp for the same volley",
        hint: "↓",
        expected: ExpectedAction::TabWeapon,
    },
    TutorialStep {
        title: "Fire the torpedo",
        text: "Queue torp_1 into the simultaneous volley.",
        why: "Queue torp",
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
        text: "Queue plasma. All three resolve together when you ready.",
        why: "Queue plasma — full alpha strike",
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
        text: "Point-blank all-weapons dump complete.",
        why: "Lesson complete",
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
        let (ok, adv) = t.check_reach_value(0, 0, 1);
        assert!(ok);
        assert!(!adv);
        let (ok, adv) = t.check_reach_value(0, 9, 10);
        assert!(ok);
        assert!(adv);
        assert_eq!(t.current, 1);
    }

    #[test]
    fn reach_value_allows_left_to_correct_overshoot() {
        let mut t = Tutorial::new();
        let (ok, adv) = t.check_reach_value(0, 10, 11);
        assert!(ok);
        assert!(!adv);
        let (ok, adv) = t.check_reach_value(0, 11, 10);
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
        assert!(line.contains("F") || line.contains("forward") || line.contains("bow") || line.contains("Shield"));
    }

    #[test]
    fn wrong_action_blocked() {
        let mut t = Tutorial::new();
        assert!(!t.check_action(&ExpectedAction::Coast));
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
            .any(|s| matches!(s.expected, ExpectedAction::TurnTo(3))));
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
