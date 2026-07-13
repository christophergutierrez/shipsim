"""Strict narrated tutorials built from verified UI-play sessions."""

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


_REAR_ATTACK_STEPS = (
    TutorialStep("mov 10", "Build a thrust reserve", "You begin stopped and need to circle below the escort. Put 10 power into the engine; this becomes thrust for acceleration and steering.", 1, "allocate"),
    TutorialStep("w b1 4", "Charge the finishing weapon", "Charge beam_1 to 4. We will hold this shot until the cruiser is directly behind the escort.", 1, "allocate"),
    TutorialStep("sh 0 6", "Protect the approach", "The escort is facing you, so fill shield 0:F. This forward shield will absorb the head-on fire during the approach.", 1, "allocate"),
    TutorialStep("sh 5 2", "Cover the lower turn", "Put the final 2 power on shield 5:FL. We are about to break toward the lower lane, so this covers the adjacent approach face.", 1, "allocate"),
    TutorialStep("commit", "Commit allocation", "The draft is only local until committed. Send this exact power plan to the engine.", 1, "allocate"),
    TutorialStep("accel 5", "Leave the head-on lane", "Accelerate on course 5 (down-right). Speed and course control travel; your facing remains 0 so the nose still points toward the escort.", 1, "movement", 1),
    TutorialStep("ready", "Hold fire", "The beam can reach, but the escort's forward shield faces you. Ready without firing so the charged beam is preserved for the rear attack.", 1, "firing", 1),
    TutorialStep("accel", "Increase to speed 2", "Accelerate again. At speed 2 this cycle includes a translation, moving the cruiser onto the lower lane.", 1, "movement", 2),
    TutorialStep("ready", "Keep the ambush concealed", "You are still approaching the protected front quarter. End this firing window without a shot.", 1, "firing", 2),
    TutorialStep("course port", "Turn travel east", "Turn the course one step to port, from 5 to 0. Course steers travel independently of the hull's weapon-facing direction.", 1, "movement", 3),
    TutorialStep("ready", "Do not waste the beam", "The escort's forward shields are still exposed. Ready and continue the pass.", 1, "firing", 3),
    TutorialStep("accel", "Carry momentum through the pass", "Accelerate to speed 3. Cycle 4 translates on the new eastward course, closing beneath the escort.", 1, "movement", 4),
    TutorialStep("ready", "Finish the setup turn", "Skip the last front-quarter shot. The next turn preserves velocity and course, but resets allocation.", 1, "firing", 4),
    TutorialStep("end", "Advance the turn", "The four movement/fire cycles are complete. End the turn to refresh power while keeping speed 3 and course 0.", 1, "turn_end"),
    TutorialStep("mov 4", "Reserve rotation thrust", "Momentum already supplies movement. Allocate 4 engine power for the hull rotations needed to aim backward during the pass.", 2, "allocate"),
    TutorialStep("w b1 4", "Recharge beam_1", "Weapons reset between turns. Recharge beam_1 to 4 for the range-1 rear strike.", 2, "allocate"),
    TutorialStep("sh 0 6", "Rebuild the forward shield", "Refill 0:F. The escort gets one final head-on firing opportunity before you cross behind it.", 2, "allocate"),
    TutorialStep("sh 1 6", "Cover the forward quarter", "Fill 1:FR as the relative bearing changes while the ships pass each other.", 2, "allocate"),
    TutorialStep("sh 5 2", "Spend the remaining power", "Place the last 2 power on 5:FL for adjacent-quarter coverage.", 2, "allocate"),
    TutorialStep("commit", "Commit the attack plan", "Send the second-turn allocation. It provides exactly three rotations plus a spare thrust point.", 2, "allocate"),
    TutorialStep("coast", "Close without changing course", "Coast keeps speed 3 and course 0 for no thrust. Both ships translate and the range drops to 3.", 2, "movement", 1),
    TutorialStep("ready", "Ignore the tempting front shot", "Range is excellent, but the target still presents forward shields. Ready without firing.", 2, "firing", 1),
    TutorialStep("rotate port", "Rotate the nose, not the course", "Rotate the hull one step to face 1 while continuing to travel east on course 0. This is the first of three aiming rotations.", 2, "movement", 2),
    TutorialStep("ready", "Continue through point-blank range", "The firing angle is not the rear yet. Preserve the beam and let the ships cross.", 2, "firing", 2),
    TutorialStep("rotate port", "Track the escort", "Rotate the hull to face 2. The cruiser keeps sliding east even though its nose now points up-left.", 2, "movement", 3),
    TutorialStep("ready", "The mount is still out of arc", "At range 1 the target is close but not on the beam's narrow forward mount. Ready instead of forcing an invalid shot.", 2, "firing", 3),
    TutorialStep("rotate port", "Complete the stern attack", "Rotate once more to face 3. Translation carries you east of the escort while your nose points west: range 1, directly behind shield 3:R.", 2, "movement", 4),
    TutorialStep("fire b1 B2", "Fire into the rear shield", "The engagement panel now shows bearing 0:F and shield exposed=3:R. Queue beam_1 against B2; the target and sole rear shield are selected by this command.", 2, "firing", 4),
    TutorialStep("ready", "Resolve the queued shot", "Committed fire resolves only after every ship is ready. Ready now to release the range-1 beam into the unpowered rear shield.", 2, "firing", 4),
)


class Tutorial:
    """A strict command sequence layered over the normal REPL and engine."""

    name = "rear-attack"
    scenario = "scenarios/ai.toml"
    safe_commands = {
        "board", "b", "cls", "redraw", "refresh", "help", "?", "h",
        "hint", "what", "log", "hist", "history", "ships", "status", "s",
        "tactical", "tac", "quit", "q", "exit",
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
                "Rear attack complete. The range-1 beam struck shield 3:R from directly "
                f"astern. Scenario status: {status}. Type quit when ready."
            )
        step = self.step
        return (
            f"Step {self.index + 1}/{len(self.steps)} - {step.title}\n"
            f"{step.text}\n\nRequired command: {step.command}"
        )


def load_tutorial(name: str) -> Tutorial:
    normalized = name.strip().lower().replace("_", "-")
    if normalized in ("rear", "rear-attack"):
        return Tutorial()
    raise ValueError(f"unknown tutorial {name!r}; available: rear-attack")
