"""Pure responsive layout and compact REPL renderers.

The engine snapshot remains the only source of game state.  This module only
measures strings, chooses already-rendered blocks, and derives concise display
forms for a terminal that cannot show the full tactical frame.
"""

from __future__ import annotations

from dataclasses import dataclass
import re
from typing import Any, Iterable, Optional, Sequence

from hexutil import FACING_GLYPH, SHIELD_LABELS, distance, format_bar, ship_callsign
from style import panel
from view import (
    format_header,
    format_tactical_blocks,
    living_ships,
    movement_focus_id,
    ship_by_id,
)


_ANSI = re.compile(r"\x1b\[[0-9;]*m")


def visible_len(text: str) -> int:
    return len(_ANSI.sub("", text))


def line_count(text: str) -> int:
    return len(text.splitlines()) if text else 0


def fits(text: str, rows: int, cols: int) -> bool:
    """Return whether a rendered block fits without wrapping or scrolling."""
    return line_count(text) <= rows and all(visible_len(line) <= cols for line in text.splitlines())


def clamp_line(text: str, cols: int) -> str:
    """Return one unwrapped line, preserving no partial ANSI escape sequence."""
    plain = _ANSI.sub("", " ".join(str(text).splitlines()))
    if cols <= 0:
        return ""
    if len(plain) <= cols:
        return plain
    if cols == 1:
        return plain[:1]
    return plain[: cols - 1].rstrip() + "…"


@dataclass(frozen=True)
class FrameBlock:
    """A candidate panel supplied to the pure layout selector."""

    role: str
    text: str
    priority: int = 100
    required: bool = False


@dataclass(frozen=True)
class LayoutDecision:
    """The selected blocks and the roles intentionally omitted from the frame."""

    blocks: tuple[FrameBlock, ...]
    hidden_roles: tuple[str, ...] = ()
    compact: bool = False

    @property
    def text(self) -> str:
        return "\n".join(block.text for block in self.blocks if block.text)

    @property
    def height(self) -> int:
        return line_count(self.text)


def phase_priority(phase: str) -> tuple[str, ...]:
    """Stable compact ordering; lower index means more important."""
    common = ("terminal_banner", "banner", "action", "prompt")
    by_phase = {
        "allocate": ("player", "draft", "hint", "map", "contacts", "recent", "history", "tutorial"),
        "movement": ("map", "player", "hint", "contacts", "draft", "recent", "history", "tutorial"),
        "firing": ("contacts", "action", "player", "map", "draft", "recent", "history", "tutorial"),
    }
    return common + by_phase.get(phase, by_phase["allocate"])


def choose_layout(
    rows: int,
    cols: int,
    phase: str,
    full_blocks: Sequence[FrameBlock],
    compact_blocks: Sequence[FrameBlock],
    *,
    prompt_rows: int = 1,
) -> LayoutDecision:
    """Choose a frame without ever spending the reserved prompt row.

    Full blocks are tested as an ordered sequence first.  Compact blocks are
    then considered by phase-aware priority, with required blocks retained and
    optional blocks omitted once the budget is exhausted.
    """
    if rows < 1 or cols < 1:
        raise ValueError("terminal dimensions must be positive")
    full_text = "\n".join(block.text for block in full_blocks if block.text)
    if fits(full_text, max(0, rows - prompt_rows), cols):
        return LayoutDecision(tuple(full_blocks), compact=False)

    budget = rows - prompt_rows
    order = {role: i for i, role in enumerate(phase_priority(phase))}
    candidates = sorted(
        enumerate(compact_blocks),
        key=lambda item: (order.get(item[1].role, 1000), item[1].priority, item[0]),
    )
    selected: list[FrameBlock] = []
    hidden: list[str] = []
    used = 0
    for _, block in candidates:
        height = line_count(block.text)
        if not block.text:
            continue
        if height <= budget - used and fits(block.text, height, cols):
            selected.append(block)
            used += height
        elif block.required:
            raise ValueError(
                f"required responsive block {block.role!r} does not fit "
                f"in {rows}x{cols} after reserving {prompt_rows} prompt row"
            )
        else:
            hidden.append(block.role)

    # Display order follows the same phase priorities used for admission.
    selected.sort(
        key=lambda block: (
            order.get(block.role, 1000),
            compact_blocks.index(block),
        )
    )
    return LayoutDecision(tuple(selected), tuple(hidden), compact=True)


def class_abbreviation(value: Any, width: int = 4) -> str:
    """Make long catalog class names useful in a one-line contact display."""
    words = [w for w in re.split(r"[^A-Za-z0-9]+", str(value or "?")) if w]
    if not words:
        return "?"
    if len(words) > 1:
        short = "".join(word[0] for word in words)
    else:
        short = words[0]
    return short[: max(1, width)].upper()


def _facing(ship: dict[str, Any]) -> str:
    face = int(ship.get("facing") or 0) % 6
    return f"{face}{FACING_GLYPH.get(face, '?')}"


def _motion(ship: dict[str, Any]) -> str:
    motion = int(ship.get("motion_available") or 0)
    cap = int(ship.get("max_maneuver_actions") or 0)
    if cap:
        return f"mot={motion}/{cap}"
    return f"mot={motion}"


def render_compact_player(
    ship: dict[str, Any],
    *,
    hull_max: Optional[int] = None,
    selected: bool = False,
    active: bool = False,
    width: int = 80,
) -> str:
    """Three lines: identity/status, all six shields, and weapon summary."""
    marker = ("@" if selected else "") + ("*" if active else "")
    power = ship.get("power_available", ship.get("power", 0))
    hull = int(ship.get("structure") or 0)
    maximum = int(hull_max or ship.get("keel") or hull)
    identity = (
        f"{marker}{ship_callsign(ship)} {class_abbreviation(ship.get('class'))} "
        f"pwr={power} {_motion(ship)} face={_facing(ship)} "
        f"hull={format_bar(hull, maximum)}"
    )
    remaining = ship.get("shields_remaining") or [0] * 6
    shield_max = max(int(ship.get("max_shield_per_facing") or 0), 1)
    shields = "shields " + " ".join(
        f"{label}:{int(remaining[i]) if i < len(remaining) else 0}/{shield_max}"
        for i, label in enumerate(SHIELD_LABELS)
    )
    weapons = []
    for weapon in ship.get("weapons") or []:
        wid = str(weapon.get("id") or "?")
        charge = int(weapon.get("charge") or 0)
        maximum_charge = int(weapon.get("max_charge") or 0)
        state = "X" if not weapon.get("operational", True) else f"{charge}/{maximum_charge}"
        weapons.append(f"{wid}={state}")
    weapon_line = "weapons " + (" ".join(weapons) if weapons else "none")
    return "\n".join(clamp_line(line, width) for line in (identity, shields, weapon_line))


def render_compact_contacts(
    snap: dict[str, Any],
    *,
    selected: Optional[int] = None,
    hull_max: Optional[dict[int, int]] = None,
    width: int = 80,
) -> str:
    """One width-clamped line for each living non-player contact."""
    hull_max = hull_max or {}
    me = ship_by_id(snap, selected) if selected is not None else None
    contacts = [ship for ship in living_ships(snap) if ship.get("controller") != "player"]
    lines: list[str] = []
    for contact in contacts:
        rng = "?"
        if me is not None:
            rng = str(distance(int(me.get("q") or 0), int(me.get("r") or 0), int(contact.get("q") or 0), int(contact.get("r") or 0)))
        hull = int(contact.get("structure") or 0)
        maximum = int(hull_max.get(int(contact.get("id") or 0), contact.get("keel") or hull))
        lines.append(
            clamp_line(
                f"{ship_callsign(contact)} {class_abbreviation(contact.get('class'))}/s{int(contact.get('size') or 0)} "
                f"rng={rng} face={_facing(contact)} {_motion(contact)} "
                f"hull={format_bar(hull, maximum)}",
                width,
            )
        )
    return "\n".join(lines)


def _map_bounds(snap: dict[str, Any]) -> tuple[int, int, int, int]:
    living = [ship for ship in living_ships(snap)]
    if not living:
        return 0, 0, 0, 0
    qs = [int(ship.get("q") or 0) for ship in living]
    rs = [int(ship.get("r") or 0) for ship in living]
    m = snap.get("map") or {}
    mode = str(m.get("mode") or "unbounded").lower()
    min_q, max_q = min(qs) - 2, max(qs) + 2
    min_r, max_r = min(rs) - 2, max(rs) + 2
    if mode == "hard":
        width, height = int(m.get("width") or 0), int(m.get("height") or 0)
        if width > 0:
            min_q, max_q = max(0, min_q), min(width - 1, max_q)
        if height > 0:
            min_r, max_r = max(0, min_r), min(height - 1, max_r)
    return min_q, max_q, min_r, max_r


def render_compact_map(
    snap: dict[str, Any],
    *,
    selected: Optional[int] = None,
    active: Optional[int] = None,
    width: int = 80,
) -> str:
    """Crop the living-ship bounding box while retaining coordinate labels."""
    min_q, max_q, min_r, max_r = _map_bounds(snap)
    ships = {
        (int(ship.get("q") or 0), int(ship.get("r") or 0)): ship
        for ship in snap.get("ships") or []
        if not ship.get("destroyed")
    }
    lines = ["MAP compact", f"q={min_q}..{max_q} r={min_r}..{max_r}"]
    for r in range(min_r, max_r + 1):
        cells: list[str] = []
        for q in range(min_q, max_q + 1):
            ship = ships.get((q, r))
            if ship is None:
                cells.append("....")
                continue
            face = FACING_GLYPH.get(int(ship.get("facing") or 0) % 6, "?")
            marker = "@" if selected is not None and int(ship.get("id") or 0) == selected else "*" if active is not None and int(ship.get("id") or 0) == active else ""
            cells.append(f"{marker}{ship_callsign(ship)}{face}"[:4].ljust(4))
        lines.append(clamp_line(f"r{r:>3} " + " ".join(cells), width))
    return "\n".join(lines)


def render_compact_draft(draft_text: str, *, width: int = 80) -> str:
    """Keep allocation totals visible while collapsing per-system detail."""
    lines = [line.strip() for line in (draft_text or "").splitlines() if line.strip()]
    pool = next((line for line in lines if line.startswith("draft ")), "draft (unavailable)")
    movement = next((line for line in lines if line.startswith("engine ")), "engine [0] 0")
    weapons = [
        line
        for line in lines
        if not re.match(r"^[0-5]:", line)
        and line.startswith(("b", "t", "p", "w"))
    ]
    shields = [line for line in lines if re.match(r"^[0-5]:", line)]
    weapon_total = len(weapons)
    shield_total = sum(int(re.search(r"\[(?:[#.]+|—)\]\s*(\d+)", line).group(1)) for line in shields if re.search(r"\[(?:[#.]+|—)\]\s*(\d+)", line))
    weapon_summary = f"weapons={weapon_total} systems"
    shield_summary = f"shields={shield_total} allocated"
    return "\n".join(
        clamp_line(line, width)
        for line in (
            "ALLOCATE DRAFT",
            pool,
            f"{movement}  {weapon_summary}  {shield_summary}",
        )
    )


def render_compact_hint(hint: str, *, width: int = 80) -> str:
    """Hints are compacted explicitly rather than allowing panel wrapping."""
    return clamp_line(hint, width)


def make_full_blocks(
    snap: dict[str, Any],
    *,
    selected: Optional[int],
    hull_max: dict[int, int],
    draft_text: Optional[str],
    hint: str,
    banner: str,
    footer: str,
    optional: Sequence[FrameBlock],
) -> list[FrameBlock]:
    """Build the pre-responsive frame in the exact legacy order."""
    blocks: list[FrameBlock] = []
    if banner:
        blocks.append(FrameBlock("terminal_banner", banner))
    blocks.append(
        FrameBlock(
            "snapshot",
            "\n".join(
                text
                for _, text in format_tactical_blocks(
                    snap, selected=selected, hull_max=hull_max
                )
            ),
        )
    )
    blocks.extend(optional)
    phase = str(snap.get("phase") or "")
    draft_title = {
        "allocate": "ALLOCATE DRAFT (local until commit)",
        "movement": "PATH DRAFT (local until commit_path)",
        "firing": "VOLLEY DRAFT (local until commit_volley)",
    }.get(phase, "DRAFT (local until commit)")
    if draft_text:
        blocks.append(FrameBlock("draft", panel(draft_title, draft_text, width=72)))
    if hint:
        from style import muted

        blocks.append(FrameBlock("hint", muted(hint)))
    from style import muted

    blocks.append(FrameBlock("footer", muted(footer) if footer else muted("play frame · log=history · cls=redraw · session log under frontend/repl/local/")))
    return blocks


def make_compact_blocks(
    snap: dict[str, Any],
    *,
    selected: Optional[int],
    hull_max: dict[int, int],
    draft_text: Optional[str],
    hint: str,
    banner: str,
    footer: str,
    optional: Sequence[FrameBlock],
    width: int,
) -> list[FrameBlock]:
    """Build phase-aware compact candidates; selection remains in choose_layout."""
    phase = str(snap.get("phase") or "")
    focus = ship_by_id(snap, selected) if selected is not None else None
    if focus is None:
        players = [ship for ship in living_ships(snap) if ship.get("controller") == "player"]
        focus = players[0] if players else None
    active = movement_focus_id(snap) if phase == "movement" else None
    blocks: list[FrameBlock] = []
    header = format_header(snap, selected=selected)
    if banner:
        blocks.append(FrameBlock("banner", banner, required=True))
    blocks.append(FrameBlock("banner", header, required=True))
    if focus is not None:
        blocks.append(FrameBlock("player", panel("YOUR SHIP", render_compact_player(focus, hull_max=hull_max.get(int(focus.get("id") or 0)), selected=selected == focus.get("id"), active=active == focus.get("id"), width=max(24, width - 4)), width=min(width, 72)), required=True))
    contacts = render_compact_contacts(snap, selected=selected, hull_max=hull_max, width=max(24, width - 4))
    if contacts:
        blocks.append(FrameBlock("contacts", panel("CONTACTS", contacts, width=min(width, 72))))
    map_text = render_compact_map(snap, selected=selected, active=active, width=max(24, width - 4))
    if map_text:
        blocks.append(FrameBlock("map", panel("MAP", map_text, width=min(width, 72))))
    draft_title = {
        "allocate": "ALLOCATE DRAFT",
        "movement": "PATH DRAFT",
        "firing": "VOLLEY DRAFT",
    }.get(phase, "DRAFT")
    if draft_text:
        blocks.append(FrameBlock("draft", panel(draft_title, render_compact_draft(draft_text, width=max(24, width - 4)), width=min(width, 72)), required=phase == "allocate"))
    if hint:
        blocks.append(FrameBlock("hint", render_compact_hint(hint, width=width), required=True))
    blocks.extend(optional)
    if footer:
        blocks.append(FrameBlock("footer", clamp_line(footer, width), priority=500))
    return blocks
