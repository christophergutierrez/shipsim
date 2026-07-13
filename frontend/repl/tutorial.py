"""Strict narrated tutorials built from verified UI-play sessions (protocol 3)."""

from __future__ import annotations

from dataclasses import dataclass
import shlex
from typing import Any


@dataclass(frozen=True)
class TutorialStep:
    command: str
    title: str
    text: str
    turn: int
    phase: str
    movement_phase: int | None = None


# Verified against scenarios/tutorial_rear_attack.toml + protocol 3 (seed 4):
# fly east past the escort, turn west, and pressure it until Won.
_REAR_ATTACK_STEPS = (
    # ── Turn 1: buy speed east ──────────────────────────────────────────
    TutorialStep(
        "mov 8",
        "Buy thrust, not distance",
        "You start stopped at (0,4), nose east (face 0→). The escort is at (8,4) "
        "looking west. Engine power becomes a thrust pool for this turn only — "
        "velocity will persist after end-turn, but thrust does not. Spend 8 power "
        "on the engine so you can accel and later turn the nose.",
        1, "allocate",
    ),
    TutorialStep(
        "w b1 4",
        "Charge the main battery",
        "Put 4 charge on beam_1 (its max). Charge carries across turns if you "
        "do not fire — we will hold this shot until we are behind the escort. "
        "You cannot later strip this charge to spend elsewhere.",
        1, "allocate",
    ),
    TutorialStep(
        "w t1 1",
        "Arm a one-shot",
        "Charge torp_1 to 1. Torpedoes are single-charge weapons; we will dump "
        "them with the beam when the rear arc opens.",
        1, "allocate",
    ),
    TutorialStep(
        "sh 0 6",
        "Shield the nose",
        "Shields always start at 0 each allocate — no leftover armor. Put 6 on "
        "face 0:F (forward). The escort will shoot your nose while you close.",
        1, "allocate",
    ),
    TutorialStep(
        "sh 1 3",
        "Spend the last power",
        "3 more on 1:FR. Total: 8 engine + 4 beam + 1 torp + 6 + 3 shields = 22, "
        "the whole pool. Nothing unspent.",
        1, "allocate",
    ),
    TutorialStep(
        "commit",
        "Lock the plan in",
        "The draft is local until commit. This sends allocate to the engine and "
        "opens movement cycle 1/4.",
        1, "allocate",
    ),
    TutorialStep(
        "accel",
        "Leave the pier",
        "Accel spends 1 thrust along your nose (face 0→). From a stop that sets "
        "course = facing and speed 1. You immediately slide 1 hex east. "
        "Remember: each cycle you slide `speed` hexes on course — not the old "
        "sparse schedule.",
        1, "movement", 1,
    ),
    TutorialStep(
        "ready",
        "Do not shoot the bow",
        "You can bear on the escort, but you would hit its forward shields. "
        "Type ready (not e) to leave the fire window without spending charge. "
        "e would end the whole turn.",
        1, "firing", 1,
    ),
    TutorialStep(
        "accel",
        "Speed 2 — bigger slide",
        "Accel again along course: speed 1→2. This cycle you slide 2 hexes east. "
        "The escort is also charging; range collapses fast under constant-rate motion.",
        1, "movement", 2,
    ),
    TutorialStep(
        "ready",
        "Still not the rear",
        "Hold fire. We need to get past the escort (higher q than B2) so our "
        "shots land on its stern, not its nose.",
        1, "firing", 2,
    ),
    TutorialStep(
        "coast",
        "Keep the vector free",
        "Coast costs 0 thrust and keeps speed/course. You still slide 2 hexes. "
        "Use coast when you already have the motion you want.",
        1, "movement", 3,
    ),
    TutorialStep(
        "ready",
        "Patience",
        "Again: ready, do not fire. Charge is for the rear volley.",
        1, "firing", 3,
    ),
    TutorialStep(
        "coast",
        "Cross their track",
        "One more coast. After this slide you should be east of the escort "
        "(higher q) while still facing east — flying past their stern line.",
        1, "movement", 4,
    ),
    TutorialStep(
        "ready",
        "Close the setup turn",
        "End fire for cycle 4. Next: end-turn keeps your velocity and course, "
        "zeros shields, and keeps unfired weapon charge.",
        1, "firing", 4,
    ),
    TutorialStep(
        "e",
        "Advance the clock",
        "e / end advances the whole turn (confirm if asked). Velocity stays; "
        "thrust and shields reset; beam/torp charge remain if unused.",
        1, "turn_end",
    ),
    # ── Turn 2: face west and open the stern ───────────────────────────
    TutorialStep(
        "mov 6",
        "Buy thrust; leave weapons alone",
        "Speed and course persist, but thrust is gone — buy 6 engine power for "
        "the big facing turn. Beam_1 and torp_1 still hold last turn's charge "
        "(you never fired). Do not re-enter them; carried charge stays unless "
        "you fire or the weapon is destroyed.",
        2, "allocate",
    ),
    TutorialStep(
        "w p1 1",
        "Only new charge costs power",
        "Plasma was never charged. Put 1 on plasma_1 — that spends 1 power. "
        "Beam and torp are already loaded from turn 1, so skip them.",
        2, "allocate",
    ),
    TutorialStep(
        "sh 0 6",
        "Rebuild shields from zero",
        "Shields do not carry. Every allocate starts faces at 0. Rebuy 0:F "
        "fully — the AI still has teeth.",
        2, "allocate",
    ),
    TutorialStep(
        "sh 1 3",
        "Quarter coverage",
        "3 on 1:FR for the pass geometry.",
        2, "allocate",
    ),
    TutorialStep(
        "sh 5 3",
        "Other shoulder",
        "3 on 5:FL. Budget: 6 engine + 1 plasma + 6+3+3 shields = 19 of 22. "
        "Commit when the draft matches.",
        2, "allocate",
    ),
    TutorialStep(
        "commit",
        "Enter the attack turn",
        "Commit. Carried beam/torp charge is still on the ship. Next: swing "
        "the nose, then dump the volley.",
        2, "allocate",
    ),
    TutorialStep(
        "turn 3",
        "Point the guns aft of them",
        "turn changes facing only (cost = hex ring distance: 0→3 costs 3 thrust). "
        "Course is still east — you keep sliding that way. Nose now 3← (west), "
        "so weapons look back along the track toward the escort. That is how you "
        "shoot 'backward' while flying past.",
        2, "movement", 1,
    ),
    TutorialStep(
        "fire b1 B2",
        "Queue the beam",
        "One-line fire: weapon + target callsign. This queues commit_fire; it "
        "does not resolve yet. Charge drops only when everyone is ready.",
        2, "firing", 1,
    ),
    TutorialStep(
        "fire t1 B2",
        "Queue the torp",
        "Add torp_1 to the same simultaneous volley.",
        2, "firing", 1,
    ),
    TutorialStep(
        "fire p1 B2",
        "Queue plasma",
        "Third commit. All three resolve together when the last ship readies.",
        2, "firing", 1,
    ),
    TutorialStep(
        "ready",
        "Resolve the volley",
        "ready_fire marks you done. When the AI readies too, hits and misses "
        "both spend charge. Watch the combat log for shield vs hull split.",
        2, "firing", 1,
    ),
    TutorialStep(
        "coast",
        "Hold the vector",
        "Keep sliding east on course 0 without spending thrust. Nose stays west "
        "for more stern shots if geometry allows.",
        2, "movement", 2,
    ),
    TutorialStep(
        "ready",
        "Skip empty windows",
        "If nothing legal is left charged, ready cleanly. Do not use e here.",
        2, "firing", 2,
    ),
    TutorialStep(
        "coast",
        "Still coasting",
        "Same idea: free slide, preserve thrust.",
        2, "movement", 3,
    ),
    TutorialStep(
        "ready",
        "Clear fire",
        "Ready through this window.",
        2, "firing", 3,
    ),
    TutorialStep(
        "coast",
        "Finish the turn’s motion",
        "Last movement cycle of the turn — coast again.",
        2, "movement", 4,
    ),
    TutorialStep(
        "ready",
        "Into turn end",
        "Ready, then end-turn when the phase allows.",
        2, "firing", 4,
    ),
    TutorialStep(
        "e",
        "Next turn",
        "End turn. Reload shields; top weapons; keep hunting from behind.",
        2, "turn_end",
    ),
    # ── Turns 3–5: repeat pressure until destruction ───────────────────
    TutorialStep(
        "mov 6",
        "Thrust for the grind",
        "Buy thrust again. You may not need to accel — coasting on residual "
        "speed is fine while you re-arm.",
        3, "allocate",
    ),
    TutorialStep(
        "w b1 4",
        "Recharge the beam — you spent it",
        "Last turn's ready resolved the volley: hit or miss, charge went to 0. "
        "Buy beam_1 back to 4 (full cost this time).",
        3, "allocate",
    ),
    TutorialStep(
        "w t1 1",
        "Recharge the torp",
        "Torp was fired too — charge it to 1 again.",
        3, "allocate",
    ),
    TutorialStep(
        "w p1 1",
        "Recharge plasma",
        "Same for plasma_1.",
        3, "allocate",
    ),
    TutorialStep(
        "sh 0 6",
        "Shields from zero",
        "Rebuy forward shields every turn.",
        3, "allocate",
    ),
    TutorialStep(
        "sh 1 3",
        "FR cover",
        "3 on 1:FR.",
        3, "allocate",
    ),
    TutorialStep(
        "sh 5 1",
        "FL scrap",
        "Last point on 5:FL, then commit.",
        3, "allocate",
    ),
    TutorialStep(
        "commit",
        "Commit",
        "Send allocation.",
        3, "allocate",
    ),
    TutorialStep(
        "coast",
        "Stay on the stern line",
        "Coast. Nose should still look west at the escort.",
        3, "movement", 1,
    ),
    TutorialStep(
        "fire b1 B2",
        "Beam again",
        "Queue beam into the escort.",
        3, "firing", 1,
    ),
    TutorialStep(
        "fire t1 B2",
        "Torp again",
        "Queue torp.",
        3, "firing", 1,
    ),
    TutorialStep(
        "fire p1 B2",
        "Plasma again",
        "Queue plasma, then ready.",
        3, "firing", 1,
    ),
    TutorialStep(
        "ready",
        "Resolve",
        "Resolve the triple volley.",
        3, "firing", 1,
    ),
    TutorialStep(
        "coast", "Coast", "Free slide.", 3, "movement", 2,
    ),
    TutorialStep(
        "ready", "Ready", "Clear the window.", 3, "firing", 2,
    ),
    TutorialStep(
        "coast", "Coast", "Free slide.", 3, "movement", 3,
    ),
    TutorialStep(
        "ready", "Ready", "Clear the window.", 3, "firing", 3,
    ),
    TutorialStep(
        "coast", "Coast", "Last cycle this turn.", 3, "movement", 4,
    ),
    TutorialStep(
        "ready", "Ready", "Then end turn.", 3, "firing", 4,
    ),
    TutorialStep(
        "e", "End turn", "Continue the stern pressure.", 3, "turn_end",
    ),
    # Turn 4
    TutorialStep(
        "mov 6", "Thrust", "Same reload pattern.", 4, "allocate",
    ),
    TutorialStep(
        "w b1 4", "Recharge beam", "Spent last turn — buy beam_1 to 4 again.", 4, "allocate",
    ),
    TutorialStep(
        "w t1 1", "Recharge torp", "Torp_1 to 1.", 4, "allocate",
    ),
    TutorialStep(
        "w p1 1", "Recharge plasma", "Plasma_1 to 1.", 4, "allocate",
    ),
    TutorialStep(
        "sh 0 6", "Shields F", "0:F = 6.", 4, "allocate",
    ),
    TutorialStep(
        "sh 1 3", "Shields FR", "1:FR = 3.", 4, "allocate",
    ),
    TutorialStep(
        "sh 5 1", "Shields FL", "5:FL = 1, commit.", 4, "allocate",
    ),
    TutorialStep(
        "commit", "Commit", "Into movement.", 4, "allocate",
    ),
    TutorialStep(
        "coast", "Coast", "Hold geometry.", 4, "movement", 1,
    ),
    TutorialStep(
        "fire b1 B2", "Beam", "Queue beam.", 4, "firing", 1,
    ),
    TutorialStep(
        "fire t1 B2", "Torp", "Queue torp.", 4, "firing", 1,
    ),
    TutorialStep(
        "fire p1 B2", "Plasma", "Queue plasma.", 4, "firing", 1,
    ),
    TutorialStep(
        "ready", "Resolve", "Volley.", 4, "firing", 1,
    ),
    TutorialStep(
        "coast", "Coast", "Slide.", 4, "movement", 2,
    ),
    TutorialStep(
        "ready", "Ready", "Clear.", 4, "firing", 2,
    ),
    TutorialStep(
        "coast", "Coast", "Slide.", 4, "movement", 3,
    ),
    TutorialStep(
        "ready", "Ready", "Clear.", 4, "firing", 3,
    ),
    TutorialStep(
        "coast", "Coast", "Last cycle.", 4, "movement", 4,
    ),
    TutorialStep(
        "ready", "Ready", "Then end.", 4, "firing", 4,
    ),
    TutorialStep(
        "e", "End turn", "One more arming pass.", 4, "turn_end",
    ),
    # Turn 5 — finishing volley
    TutorialStep(
        "mov 6", "Thrust", "Final allocate.", 5, "allocate",
    ),
    TutorialStep(
        "w b1 4", "Recharge beam", "Spent — beam_1 to 4.", 5, "allocate",
    ),
    TutorialStep(
        "w t1 1", "Recharge torp", "Torp_1 to 1.", 5, "allocate",
    ),
    TutorialStep(
        "w p1 1", "Recharge plasma", "Plasma_1 to 1.", 5, "allocate",
    ),
    TutorialStep(
        "sh 0 6", "Shields", "0:F.", 5, "allocate",
    ),
    TutorialStep(
        "sh 1 3", "Shields", "1:FR.", 5, "allocate",
    ),
    TutorialStep(
        "sh 5 1", "Shields", "5:FL, commit.", 5, "allocate",
    ),
    TutorialStep(
        "commit", "Commit", "Finish them.", 5, "allocate",
    ),
    TutorialStep(
        "coast",
        "Hold and shoot",
        "Coast into the fire window with nose still on the escort.",
        5, "movement", 1,
    ),
    TutorialStep(
        "fire b1 B2", "Beam", "Final beam.", 5, "firing", 1,
    ),
    TutorialStep(
        "fire t1 B2", "Torp", "Final torp.", 5, "firing", 1,
    ),
    TutorialStep(
        "fire p1 B2", "Plasma", "Final plasma.", 5, "firing", 1,
    ),
    TutorialStep(
        "ready",
        "Kill shot",
        "Resolve. Escort should go down (scenario Won). Type quit when done.",
        5, "firing", 1,
    ),
)


class Tutorial:
    """A strict command sequence layered over the normal REPL and engine."""

    name = "rear-attack"
    scenario = "scenarios/tutorial_rear_attack.toml"
    objective = (
        "Under protocol 3 motion: accelerate east past the escort, turn the nose "
        "west (facing only — course keeps sliding you east), and destroy it with "
        "stern-side volleys."
    )
    safe_commands = {
        "board", "b", "cls", "redraw", "refresh", "help", "?", "h",
        "hint", "what", "log", "hist", "history", "ships", "status", "s",
        "tactical", "tac", "quit", "q", "exit", "motion", "m",
    }

    def __init__(self) -> None:
        self.steps = _REAR_ATTACK_STEPS
        self.index = 0

    @property
    def complete(self) -> bool:
        return self.index >= len(self.steps)

    @property
    def step(self) -> TutorialStep | None:
        return None if self.complete else self.steps[self.index]

    @staticmethod
    def normalize(line: str) -> str:
        try:
            return " ".join(shlex.split(line.strip().lower()))
        except ValueError:
            return line.strip().lower()

    def is_safe(self, line: str) -> bool:
        normalized = self.normalize(line)
        return bool(normalized) and normalized.split()[0] in self.safe_commands

    def accepts(self, line: str) -> bool:
        return self.complete or self.is_safe(line) or self.normalize(line) == self.step.command.lower()

    def advances_for(self, line: str) -> bool:
        return not self.complete and self.normalize(line) == self.step.command.lower()

    def advance(self) -> None:
        if not self.complete:
            self.index += 1

    def reject_text(self, line: str) -> str:
        entered = self.normalize(line) or "(blank)"
        return (
            f"TUTORIAL BLOCKED {entered!r}; no choice was applied. "
            f"This lesson requires: {self.step.command}"
        )

    def state_error(self, snap: dict[str, Any]) -> str | None:
        step = self.step
        if step is None:
            return None
        actual = (int(snap.get("turn") or 0), str(snap.get("phase") or ""))
        expected = (step.turn, step.phase)
        if actual != expected:
            return f"tutorial state drift: expected turn/phase {expected}, got {actual}"
        if step.movement_phase is not None and int(snap.get("movement_phase") or 0) != step.movement_phase:
            return (
                f"tutorial state drift: expected movement cycle {step.movement_phase}, "
                f"got {snap.get('movement_phase')}"
            )
        return None

    def panel_text(self, snap: dict[str, Any]) -> str:
        if self.complete:
            status = snap.get("status")
            return (
                "Rear-attack lesson complete. You used protocol 3 motion: accel along "
                f"the nose, turn for facing only, constant-rate slides. Status: {status}. "
                "Type quit when ready."
            )
        step = self.step
        return (
            f"MISSION: {self.objective}\n\n"
            f"Step {self.index + 1}/{len(self.steps)} — {step.title}\n"
            f"{step.text}\n\n"
            f"Type exactly: {step.command}"
        )

    def prompt_text(self) -> str:
        if self.complete:
            return "TUTORIAL COMPLETE — type quit to leave."
        step = self.step
        return (
            f"\nTUTORIAL {self.index + 1}/{len(self.steps)} — {step.title}\n"
            f"{step.text}\n"
            f">>> type: {step.command}"
        )


def load_tutorial(name: str) -> Tutorial:
    normalized = name.strip().lower().replace("_", "-")
    if normalized in ("rear", "rear-attack"):
        return Tutorial()
    raise ValueError(f"unknown tutorial {name!r}; available: rear-attack")
