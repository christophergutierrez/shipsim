"""Hex geometry helpers (mirror core arc/hex — display only, not rules authority)."""

from __future__ import annotations

from typing import Optional

# Same axial neighbor order as src/hex.rs DIRECTIONS (facing 0..5).
# Board display is q→ across, r↓ down — glyphs match *that* screen orientation
# so "forward" and the arrow agree (facing 0 is +q = right, not up).
DIRS = (
    (1, 0),  # 0 →
    (1, -1),  # 1 ↗
    (0, -1),  # 2 ↑
    (-1, 0),  # 3 ←
    (-1, 1),  # 4 ↙
    (0, 1),  # 5 ↓
)

FACING_GLYPH = {0: "→", 1: "↗", 2: "↑", 3: "←", 4: "↙", 5: "↓"}
FACING_LEGEND = "0→ 1↗ 2↑ 3← 4↙ 5↓  (screen: q right, r down; arrow = forward)"
# Relative shield labels (0 = ship's own forward face).
SHIELD_LABELS = ["F", "FR", "RR", "R", "RL", "FL"]

# Side letter for callsigns until scenarios carry real fleet/side ids.
# Same letter = same side; player fleet is always controllable "A".
SIDE_LETTER = {
    "player": "A",
    "ai": "B",
    "scripted": "C",
}


def ship_callsign(ship: dict) -> str:
    """Stable side+id label, e.g. A1 (player), B2 (ai). Map and lists use this."""
    ctrl = str(ship.get("controller") or "?").lower()
    side = SIDE_LETTER.get(ctrl, "X")
    return f"{side}{int(ship.get('id') or 0)}"


def distance(aq: int, ar: int, bq: int, br: int) -> int:
    return (abs(aq - bq) + abs(aq + ar - bq - br) + abs(ar - br)) // 2


def neighbor(q: int, r: int, facing: int) -> tuple[int, int]:
    dq, dr = DIRS[facing % 6]
    return q + dq, r + dr


def nearest_bearings(from_q: int, from_r: int, to_q: int, to_r: int) -> list[int]:
    if from_q == to_q and from_r == to_r:
        return [0]
    best = min(
        distance(*neighbor(from_q, from_r, f), to_q, to_r) for f in range(6)
    )
    return [
        f
        for f in range(6)
        if distance(*neighbor(from_q, from_r, f), to_q, to_r) == best
    ]


def bearing_to(from_q: int, from_r: int, to_q: int, to_r: int) -> int:
    return nearest_bearings(from_q, from_r, to_q, to_r)[0]


def relative_bearing(
    origin_facing: int, from_q: int, from_r: int, to_q: int, to_r: int
) -> int:
    return (bearing_to(from_q, from_r, to_q, to_r) - (origin_facing % 6)) % 6


def legal_shield_facings(
    attacker_q: int,
    attacker_r: int,
    target_q: int,
    target_r: int,
    target_facing: int,
) -> list[int]:
    """Target-relative shield faces (0=F .. 5=FL) that face the attacker."""
    abs_bearings = nearest_bearings(target_q, target_r, attacker_q, attacker_r)
    out: list[int] = []
    for b in abs_bearings:
        rel = (b - (target_facing % 6)) % 6
        if rel not in out:
            out.append(rel)
    return out


def turn_toward(current: int, target: int) -> str:
    """One turn order mode (turn_port | turn_starboard) toward absolute facing target."""
    current %= 6
    target %= 6
    delta = (target - current) % 6
    if delta == 0:
        return "forward"  # already aligned; caller should move
    if delta <= 3:
        return "turn_starboard"
    return "turn_port"


def steps_to_face(current: int, target: int) -> int:
    delta = (target - current) % 6
    return min(delta, 6 - delta)


def bar(filled: int, total: int, width: Optional[int] = None) -> str:
    """Text bar like [####....]. filled/total; width defaults to total (capped)."""
    filled = max(0, int(filled))
    total = max(0, int(total))
    if total <= 0:
        return "[—]"
    w = width if width is not None else min(total, 16)
    if total <= w:
        return "[" + "#" * filled + "." * (total - filled) + "]"
    # scale
    nf = round(filled * w / total) if total else 0
    nf = min(w, max(0, nf))
    return "[" + "#" * nf + "." * (w - nf) + "]"
