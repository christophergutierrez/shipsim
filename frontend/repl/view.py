"""Human-readable snapshot / board formatting for the REPL."""

from __future__ import annotations

from typing import Any, Optional

FACING_GLYPH = {0: "↑", 1: "↗", 2: "↘", 3: "↓", 4: "↙", 5: "↖"}
SHIELD_LABELS = ["F", "FR", "RR", "R", "RL", "FL"]


def _ship_by_id(snap: dict[str, Any], ship_id: int) -> Optional[dict[str, Any]]:
    for ship in snap.get("ships") or []:
        if ship.get("id") == ship_id:
            return ship
    return None


def format_header(snap: dict[str, Any]) -> str:
    status = snap.get("status", "?")
    phase = snap.get("phase", "?")
    turn = snap.get("turn", "?")
    active = snap.get("active_ship")
    warn = "  ⚠ leftover useful actions" if snap.get("end_turn_warning") else ""
    active_s = f" active=#{active}" if active is not None else ""
    return f"turn {turn}  phase={phase}  status={status}{active_s}{warn}"


def format_ship_line(ship: dict[str, Any], *, active: bool = False) -> str:
    mark = "*" if active else " "
    dead = " [DEAD]" if ship.get("destroyed") else ""
    face = FACING_GLYPH.get(int(ship.get("facing", 0)), "?")
    ctrl = ship.get("controller", "?")
    return (
        f"{mark}#{ship.get('id')} {ship.get('class', '?')} ({ctrl}) "
        f"@({ship.get('q')},{ship.get('r')}) {face} "
        f"pwr={ship.get('power')} mov={ship.get('move_remaining')}/"
        f"{ship.get('movement_allocated')} hull={ship.get('structure')}{dead}"
    )


def format_weapons(ship: dict[str, Any]) -> str:
    lines = []
    for w in ship.get("weapons") or []:
        if not w.get("operational", True):
            state = "dead"
        elif w.get("fired"):
            state = "fired"
        elif int(w.get("charge") or 0) > 0:
            state = f"chg={w.get('charge')}/{w.get('max_charge')}"
        else:
            state = "uncharged"
        lines.append(
            f"    {w.get('id')}: {w.get('kind')} arc={w.get('arc')} "
            f"rng≤{w.get('max_range')} [{state}]"
        )
    return "\n".join(lines) if lines else "    (no weapons)"


def format_shields(ship: dict[str, Any]) -> str:
    powered = ship.get("shields_powered") or [0] * 6
    remaining = ship.get("shields_remaining") or [0] * 6
    parts = [
        f"{lab}={remaining[i]}/{powered[i]}"
        for i, lab in enumerate(SHIELD_LABELS)
        if i < len(powered)
    ]
    return "    shields " + " ".join(parts)


def format_board(snap: dict[str, Any]) -> str:
    """Compact hex occupancy dump (text, not pretty)."""
    m = snap.get("map") or {}
    width = int(m.get("width") or 0)
    height = int(m.get("height") or 0)
    if width <= 0 or height <= 0 or width * height > 400:
        # Too large for a full dump; list ship positions only.
        rows = []
        for ship in snap.get("ships") or []:
            if ship.get("destroyed"):
                continue
            face = FACING_GLYPH.get(int(ship.get("facing", 0)), "?")
            rows.append(
                f"  ship #{ship.get('id')} ({ship.get('q')},{ship.get('r')}) {face}"
            )
        return "positions:\n" + ("\n".join(rows) if rows else "  (none)")

    occ: dict[tuple[int, int], str] = {}
    for ship in snap.get("ships") or []:
        if ship.get("destroyed"):
            continue
        key = (int(ship["q"]), int(ship["r"]))
        face = FACING_GLYPH.get(int(ship.get("facing", 0)), "?")
        occ[key] = f"{ship.get('id')}{face}"

    lines = ["board (q across, r down):"]
    for r in range(height):
        cells = []
        for q in range(width):
            cells.append(f"{occ.get((q, r), '..'):>4}")
        lines.append(f"  r{r:02d} " + "".join(cells))
    return "\n".join(lines)


def format_commits(snap: dict[str, Any]) -> str:
    commits = snap.get("fire_commits") or []
    if not commits:
        return ""
    lines = ["pending fire:"]
    for c in commits:
        lines.append(
            f"  ship #{c.get('ship')} {c.get('weapon')} → "
            f"#{c.get('target')} shield={c.get('shield_facing')}"
        )
    return "\n".join(lines)


def format_combat_log(snap: dict[str, Any], *, last_n: int = 8) -> str:
    log = snap.get("combat_log") or []
    if not log:
        return ""
    lines = [f"combat log (last {min(last_n, len(log))}):"]
    for e in log[-last_n:]:
        lines.append(
            f"  #{e.get('attacker')} → #{e.get('target')} "
            f"shield={e.get('shield')} dmg={e.get('damage')} ({e.get('kind')})"
        )
    return "\n".join(lines)


def format_snapshot(snap: dict[str, Any], *, verbose: bool = True) -> str:
    parts = [format_header(snap)]
    if snap.get("move_order"):
        parts.append(f"move_order={snap.get('move_order')} moved={snap.get('ships_moved_this_phase')}")
    if snap.get("ships_ready_fire"):
        parts.append(f"ready_fire={snap.get('ships_ready_fire')}")
    active = snap.get("active_ship")
    for ship in snap.get("ships") or []:
        parts.append(format_ship_line(ship, active=ship.get("id") == active))
        if verbose and not ship.get("destroyed"):
            parts.append(format_shields(ship))
            parts.append(format_weapons(ship))
    board = format_board(snap)
    if board:
        parts.append(board)
    commits = format_commits(snap)
    if commits:
        parts.append(commits)
    clog = format_combat_log(snap)
    if clog:
        parts.append(clog)
    return "\n".join(parts)


def format_error(err: dict[str, Any]) -> str:
    code = err.get("code", "error")
    msg = err.get("message", "")
    return f"! {code}: {msg}"


def living_player_ships(snap: dict[str, Any]) -> list[dict[str, Any]]:
    return [
        s
        for s in (snap.get("ships") or [])
        if not s.get("destroyed") and s.get("controller") == "player"
    ]


def living_ships(snap: dict[str, Any]) -> list[dict[str, Any]]:
    return [s for s in (snap.get("ships") or []) if not s.get("destroyed")]
