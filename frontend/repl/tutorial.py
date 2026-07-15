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


# Verified UI play (pipelined into repl.py --scroll) against
# scenarios/tutorial_rear_attack.toml + protocol 3 (seed 4):
# race past the escort, brake/revector west, point-blank dump all three weapons → Won turn 3.
_REAR_ATTACK_STEPS = (
    # ── Turn 1: arm everything and fly past ─────────────────────────────
    TutorialStep(
        "mov 10",
        "Buy a big thrust pool",
        "You start stopped at (0,4), nose east (face 0→). The escort is at (8,4) "
        "looking west. Engine power is this turn's thrust pool only — velocity "
        "persists after end-turn, thrust does not. Spend 10 on the engine so you "
        "can accel hard and still afford a 180° nose turn later.",
        1, "allocate",
    ),
    TutorialStep(
        "w b1 4",
        "Charge the beam now",
        "Put 4 on beam_1 (max). Charge carries across turns if you do not fire — "
        "we hold the shot until point-blank behind the escort. You cannot strip "
        "this charge later to spend elsewhere.",
        1, "allocate",
    ),
    TutorialStep(
        "w t1 1",
        "Arm the torp",
        "Charge torp_1 to 1. One-shot weapon; it rides with the beam into the "
        "same volley when geometry opens.",
        1, "allocate",
    ),
    TutorialStep(
        "w p1 1",
        "Arm plasma too",
        "Plasma_1 to 1. Full package this turn so the kill shot needs no re-arm "
        "mid-fight. Budget so far: 10 engine + 4+1+1 weapons = 16 of 22.",
        1, "allocate",
    ),
    TutorialStep(
        "sh 0 6",
        "Shield the nose",
        "Shields always start at 0 each allocate. Put the last 6 on face 0:F "
        "(forward). The escort will shoot your nose on the approach. Total 22.",
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
        "Accel spends 1 thrust along your nose. From a stop that sets course = "
        "facing and speed 1, then you slide 1 hex east. Each cycle you slide "
        "`speed` hexes on course — constant rate, every cycle.",
        1, "movement", 1,
    ),
    TutorialStep(
        "ready",
        "Do not shoot the bow",
        "You can bear on the escort, but forward shields would eat the volley. "
        "Type ready (not e) to leave the fire window without spending charge. "
        "e would end the whole turn.",
        1, "firing", 1,
    ),
    TutorialStep(
        "accel",
        "Speed 2 — bigger slide",
        "Accel again along course: speed 1→2. This cycle you slide 2 hexes east. "
        "Range collapses fast under constant-rate motion.",
        1, "movement", 2,
    ),
    TutorialStep(
        "ready",
        "Still not the rear",
        "Hold fire. We need higher q than the escort so stern geometry opens.",
        1, "firing", 2,
    ),
    TutorialStep(
        "accel",
        "Speed 3 — punch past",
        "One more accel to speed 3 (slide 3 hexes). You cross their track this "
        "cycle or the next.",
        1, "movement", 3,
    ),
    TutorialStep(
        "ready",
        "Patience",
        "Again: ready, do not fire. Charge is for the point-blank volley.",
        1, "firing", 3,
    ),
    TutorialStep(
        "turn 3",
        "Nose west while still flying east",
        "turn changes facing only (0→3 costs 3 thrust). Course stays east — you "
        "keep sliding that way and finish past their stern. Nose 3← points the "
        "guns back along the track. That is how you shoot 'backward' while "
        "flying past.",
        1, "movement", 4,
    ),
    TutorialStep(
        "ready",
        "Close the setup turn",
        "Range is still long. Ready out of fire, then end-turn: velocity and "
        "course persist, shields zero, unfired weapon charge stays.",
        1, "firing", 4,
    ),
    TutorialStep(
        "e",
        "Advance the clock",
        "e / end advances the whole turn. Velocity stays; thrust and shields "
        "reset; beam/torp/plasma charge remain because you never fired.",
        1, "turn_end",
    ),
    # ── Turn 2: brake the eastbound slide, reverse west ────────────────
    TutorialStep(
        "mov 10",
        "Thrust to brake — leave weapons alone",
        "Speed 3 course 0 still carries you east, but thrust is gone. Buy 10 "
        "engine power. Weapons still hold turn-1 charge — do not re-enter them.",
        2, "allocate",
    ),
    TutorialStep(
        "sh 0 6",
        "Rebuild shields from zero",
        "Shields do not carry. Rebuy 0:F fully — the AI still has teeth.",
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
        "3 on 5:FL. Budget: 10 engine + 6+3+3 shields = 22. Commit.",
        2, "allocate",
    ),
    TutorialStep(
        "commit",
        "Enter the brake turn",
        "Commit. Carried weapons are still loaded. Next: thrust against the "
        "slide until you stop, then push west.",
        2, "allocate",
    ),
    TutorialStep(
        "accel",
        "Brake: thrust opposite course",
        "Nose is west (3) while course is still east (0). Accel along facing "
        "against the vector: speed drops 3→2. You still slide 2 east this cycle.",
        2, "movement", 1,
    ),
    TutorialStep(
        "ready",
        "Hold the charge",
        "Still out of kill range. Ready without firing.",
        2, "firing", 1,
    ),
    TutorialStep(
        "accel",
        "Keep braking",
        "Accel again: speed 2→1. Slide 1 hex. Same reverse-thrust idea.",
        2, "movement", 2,
    ),
    TutorialStep(
        "ready",
        "Still holding",
        "Ready through this window.",
        2, "firing", 2,
    ),
    TutorialStep(
        "accel",
        "Kill the eastbound vector",
        "Accel once more: speed 1→0. Course becomes west (3) at rest — ready "
        "to push back toward the escort.",
        2, "movement", 3,
    ),
    TutorialStep(
        "ready",
        "Clear fire",
        "Ready.",
        2, "firing", 3,
    ),
    TutorialStep(
        "accel",
        "Push west",
        "From a stop, accel along face 3 sets course west and speed 1. You "
        "slide 1 hex toward the escort.",
        2, "movement", 4,
    ),
    TutorialStep(
        "ready",
        "Into turn end",
        "Ready, then end-turn. Weapons still charged.",
        2, "firing", 4,
    ),
    TutorialStep(
        "e",
        "Next turn — the kill run",
        "End turn. Rebuild shields; keep the full weapon load for point blank.",
        2, "turn_end",
    ),
    # ── Turn 3: close to range 1 and dump everything ───────────────────
    TutorialStep(
        "mov 10",
        "Thrust for the slam",
        "Buy 10 engine again. Weapons are still full from turn 1 — skip the "
        "w commands. Only shields need rebuy.",
        3, "allocate",
    ),
    TutorialStep(
        "sh 0 6",
        "Shields F",
        "0:F = 6 from zero.",
        3, "allocate",
    ),
    TutorialStep(
        "sh 1 3",
        "Shields FR",
        "1:FR = 3.",
        3, "allocate",
    ),
    TutorialStep(
        "sh 5 3",
        "Shields FL",
        "5:FL = 3 (22 total). Commit into the attack run.",
        3, "allocate",
    ),
    TutorialStep(
        "commit",
        "Commit",
        "Movement opens. Close the gap hard.",
        3, "allocate",
    ),
    TutorialStep(
        "accel",
        "Speed 2 west",
        "Accel along course: speed 1→2, slide 2 hexes toward the escort.",
        3, "movement", 1,
    ),
    TutorialStep(
        "ready",
        "Not yet",
        "Range is still medium. Hold the full volley for point blank.",
        3, "firing", 1,
    ),
    TutorialStep(
        "accel",
        "Speed 3 — still closing",
        "Accel to speed 3, slide 3. Geometry collapses.",
        3, "movement", 2,
    ),
    TutorialStep(
        "ready",
        "Almost",
        "One more accel after this window puts you at range 1.",
        3, "firing", 2,
    ),
    TutorialStep(
        "accel",
        "Point blank",
        "Accel to speed 4 and slide into range 1, nose on the escort from "
        "behind (higher q, face west). All three weapons should show FIRE READY.",
        3, "movement", 3,
    ),
    TutorialStep(
        "fire b1 B2",
        "Queue the beam",
        "One-line fire: weapon + target callsign. Queues commit_fire; does not "
        "resolve yet. Charge drops only when everyone is ready.",
        3, "firing", 3,
    ),
    TutorialStep(
        "fire t1 B2",
        "Queue the torp",
        "Add torp_1 to the same simultaneous volley.",
        3, "firing", 3,
    ),
    TutorialStep(
        "fire p1 B2",
        "Queue plasma",
        "Third commit. Beam + torp + plasma resolve together at range 1.",
        3, "firing", 3,
    ),
    TutorialStep(
        "ready",
        "Resolve the kill shot",
        "ready_fire marks you done. Hits spend charge and should destroy the "
        "escort (scenario Won). Type quit when the banner shows victory.",
        3, "firing", 3,
    ),
)


class Tutorial:
    """A strict command sequence layered over the normal REPL and engine."""

    name = "rear-attack"
    scenario = "scenarios/tutorial_rear_attack.toml"
    objective = (
        "Under protocol 3 motion: race east past the escort, turn the nose west, "
        "brake and revector, then destroy it with a point-blank all-weapons dump."
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
                "Rear-attack lesson complete. You used protocol 3 motion: accel "
                f"along the nose, turn for facing only, brake by reverse thrust, "
                f"and a point-blank triple volley. Status: {status}. "
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
