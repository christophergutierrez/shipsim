"""Strict narrated tutorials built for protocol v4 path/volley turns."""

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


# Short guided sequence for scenarios/tutorial_rear_attack.toml (protocol 4).
# Path + single volley replace the old multi-cycle inertial lesson.
_REAR_ATTACK_STEPS = (
    TutorialStep(
        "mov 8",
        "Buy motion power",
        "You start at (0,4), nose east (face 0→). The escort is at (8,4). "
        "Engine power becomes this turn's motion pool for path actions. "
        "Spend 8 so you can walk several hexes and still turn.",
        1,
        "allocate",
    ),
    TutorialStep(
        "w b1 4",
        "Charge the beam",
        "Put 4 on beam_1. Charge carries across turns if you do not fire.",
        1,
        "allocate",
    ),
    TutorialStep(
        "w t1 1",
        "Arm the torp",
        "Charge torp_1 to 1 for the kill volley.",
        1,
        "allocate",
    ),
    TutorialStep(
        "w p1 1",
        "Arm plasma",
        "Plasma_1 to 1. Full package for one commit_volley later.",
        1,
        "allocate",
    ),
    TutorialStep(
        "sh 0 6",
        "Shield the nose",
        "Shields always start at 0 each allocate. Put 6 on face 0:F.",
        1,
        "allocate",
    ),
    TutorialStep(
        "commit",
        "Lock the allocate",
        "The draft is local until commit. After every ship allocates, "
        "the engine opens the movement stage for path commits.",
        1,
        "allocate",
    ),
    TutorialStep(
        "path f f f f f f f f",
        "Draft a straight path east",
        "Each f is move_f (1 motion). Eight steps walk you toward the escort. "
        "The draft is local until you commit. Use undo/clear if you mis-type.",
        1,
        "movement",
    ),
    TutorialStep(
        "preview",
        "Ask the engine if the path is legal",
        "preview sends path_preview — the engine owns legality. "
        "Check cost, remaining motion, and final hex/facing.",
        1,
        "movement",
    ),
    TutorialStep(
        "commit",
        "Commit the path once",
        "commit_path submits the whole path. Paths resolve when every living "
        "ship has committed. Empty path (hold/p) stays put.",
        1,
        "movement",
    ),
    TutorialStep(
        "fire b1 B2",
        "Draft the beam into the volley",
        "One-line fire adds a shot to the local volley draft — it does not "
        "resolve yet. Charge drops only when commit_volley resolves.",
        1,
        "firing",
    ),
    TutorialStep(
        "fire t1 B2",
        "Draft the torp",
        "Add torp_1 to the same volley.",
        1,
        "firing",
    ),
    TutorialStep(
        "fire p1 B2",
        "Draft plasma",
        "Third shot. All three resolve together when you submit.",
        1,
        "firing",
    ),
    TutorialStep(
        "ready",
        "Submit the volley",
        "ready / nofire / commit sends commit_volley (empty = hold fire). "
        "After every ship commits, fire resolves and the next turn's allocate "
        "begins automatically — there is no end_turn.",
        1,
        "firing",
    ),
)


class Tutorial:
    """A strict command sequence layered over the normal REPL and engine."""

    name = "rear-attack"
    scenario = "scenarios/tutorial_rear_attack.toml"
    objective = (
        "Under protocol 4: allocate power, draft a path east with f actions, "
        "commit_path once, draft a multi-weapon volley, then commit_volley."
    )
    safe_commands = {
        "board",
        "b",
        "cls",
        "redraw",
        "refresh",
        "help",
        "?",
        "h",
        "hint",
        "what",
        "log",
        "hist",
        "history",
        "ships",
        "status",
        "s",
        "tactical",
        "tac",
        "quit",
        "q",
        "exit",
        "motion",
        "m",
        "path",
        "preview",
        "undo",
        "clear",
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
        return (
            self.complete
            or self.is_safe(line)
            or self.normalize(line) == self.step.command.lower()
        )

    def advances_for(self, line: str) -> bool:
        return (
            not self.complete
            and self.normalize(line) == self.step.command.lower()
        )

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
        return None

    def panel_text(self, snap: dict[str, Any]) -> str:
        if self.complete:
            status = snap.get("status")
            return (
                "Rear-attack lesson complete. You used protocol 4: allocate, "
                f"path draft + commit_path, volley draft + commit_volley. "
                f"Status: {status}. Type quit when ready."
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
            return "tutorial complete — type quit"
        return f"tutorial next: {self.step.command}"


def load_tutorial(name: str) -> Tutorial:
    key = name.strip().lower().replace("_", "-")
    if key in ("rear-attack", "rear", "tutorial"):
        return Tutorial()
    raise ValueError(f"unknown tutorial {name!r}; available: rear-attack")
