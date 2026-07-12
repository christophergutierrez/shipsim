"""Terminal presentation helpers — restrained palette, panels, NO_COLOR.

Inspired by roguelike TUI practice (Cogmind / Brogue school): limited color,
glyph semantics, box-drawing panels. Stdlib only; no ratatui/tcod yet.

Respects NO_COLOR (https://no-color.org) and SHIPSIM_REPL_COLOR=0|1.
"""

from __future__ import annotations

import os
import sys
from typing import Optional

# ── palette (ANSI 16 + a few bright). Names are semantic, not decorative. ──
_RESET = "\033[0m"
_BOLD = "\033[1m"
_DIM = "\033[2m"

# Foreground
_FG = {
    "default": "",
    "black": "\033[30m",
    "red": "\033[31m",
    "green": "\033[32m",
    "yellow": "\033[33m",
    "blue": "\033[34m",
    "magenta": "\033[35m",
    "cyan": "\033[36m",
    "white": "\033[37m",
    "bright_red": "\033[91m",
    "bright_green": "\033[92m",
    "bright_yellow": "\033[93m",
    "bright_cyan": "\033[96m",
    "bright_white": "\033[97m",
    "gray": "\033[90m",
}


def color_enabled() -> bool:
    if os.environ.get("NO_COLOR") is not None:
        return False
    env = os.environ.get("SHIPSIM_REPL_COLOR", "").strip().lower()
    if env in ("0", "false", "no", "off"):
        return False
    if env in ("1", "true", "yes", "on"):
        return True
    return sys.stdout.isatty()


def paint(text: str, *styles: str) -> str:
    """Apply named styles: bold, dim, or palette keys (cyan, bright_red, …)."""
    if not text or not color_enabled() or not styles:
        return text
    codes: list[str] = []
    for s in styles:
        if s == "bold":
            codes.append(_BOLD)
        elif s == "dim":
            codes.append(_DIM)
        elif s in _FG and _FG[s]:
            codes.append(_FG[s])
    if not codes:
        return text
    return "".join(codes) + text + _RESET


def panel(title: str, body: str, *, width: int = 72) -> str:
    """Box-drawing panel. Width is a soft guide for the top rule."""
    title = title.strip()
    inner_w = max(width - 2, len(title) + 4, 24)
    top = "┌─ " + title + " " + "─" * max(1, inner_w - len(title) - 3) + "┐"
    bot = "└" + "─" * (len(top) - 2) + "┘"
    lines = [top]
    for raw in (body or "").splitlines() or [""]:
        # Don't pad colored lines to width (ANSI lengths lie); left-border only.
        lines.append("│ " + raw)
    lines.append(bot)
    return "\n".join(lines)


def rule(label: str = "", *, width: int = 56) -> str:
    if label:
        core = f"── {label} "
        return paint(core + "─" * max(4, width - len(core)), "dim")
    return paint("─" * width, "dim")


# Semantic shortcuts used by view.py
def hit(text: str) -> str:
    return paint(text, "bold", "bright_red")


def miss(text: str) -> str:
    return paint(text, "dim", "yellow")


def ok(text: str) -> str:
    return paint(text, "bright_green")


def warn(text: str) -> str:
    return paint(text, "bright_yellow")


def focus(text: str) -> str:
    return paint(text, "bold", "bright_cyan")


def enemy(text: str) -> str:
    return paint(text, "yellow")


def player(text: str) -> str:
    return paint(text, "cyan")


def active(text: str) -> str:
    return paint(text, "bold", "bright_white")


def muted(text: str) -> str:
    return paint(text, "dim", "gray")


def err(text: str) -> str:
    return paint(text, "bold", "bright_red")


def fired(text: str) -> str:
    """Resolved weapon state."""
    return paint(text, "bold", "yellow")


def queued(text: str) -> str:
    """Weapon committed and waiting for the firing phase to resolve."""
    return paint(text, "bold", "bright_yellow")


def available(text: str) -> str:
    """Charged weapon that remains available to commit."""
    return paint(text, "bright_cyan")


def dead(text: str) -> str:
    """Destroyed ship or inoperable weapon box."""
    return paint(text, "dim", "red")
