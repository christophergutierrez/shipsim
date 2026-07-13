"""Hex geometry helpers (mirror core arc/hex — display only, not rules authority)."""

from __future__ import annotations

from typing import Optional

# Same axial neighbor order as src/hex.rs DIRECTIONS (facing 0..5).
# Board display is q→ across, r↓ down — glyphs match *that* screen orientation
# so "forward" and the arrow agree (facing 0 is +q = right, not up).
DIRS = (
    (1, 0),  # 0 →
    (1, -1),  # 1 ↗
    (0, -1),  # 2 ↖
    (-1, 0),  # 3 ←
    (-1, 1),  # 4 ↙
    (0, 1),  # 5 ↘
)

FACING_GLYPH = {0: "→", 1: "↗", 2: "↖", 3: "←", 4: "↙", 5: "↘"}
FACING_LEGEND = "0→ 1↗ 2↖ 3← 4↙ 5↘  (q right, r down; port=↗, starboard=↘)"
# Relative shield labels (0 = ship's own forward face).
SHIELD_LABELS = ["F", "FR", "RR", "R", "RL", "FL"]

# Presentation-only preview of the engine's documented d20 threshold tables.
# The engine remains authoritative; this lets the picker explain a result
# before the irreversible fire commit.
_TO_HIT = {
    "beam": (18, 17, 15, 13, 11, 10, 8, 7, 5, 4),
    "plasma": (16, 14, 12, 10, 8, 6, 5, 4, 3, 2, 2, 2, 1, 1),
    "torp": (14, 13, 12, 11, 10, 9, 7, 6, 5, 4, 3, 3),
}


def hit_preview(kind: str, range_: int) -> tuple[int, int] | None:
    """Return (d20 threshold, percent) for the engine's range table."""
    values = _TO_HIT.get(str(kind).lower())
    if not values or range_ < 1 or range_ > len(values):
        return None
    threshold = values[range_ - 1]
    return threshold, threshold * 5


def damage_preview(kind: str, charge: int, range_: int) -> int | None:
    """Return the engine-table damage preview for a charged shot."""
    k = str(kind).lower()
    if k == "beam":
        factors = (2.0, 1.9, 1.7, 1.6, 1.4, 1.3, 1.2, 1.1, 1.0, 1.0)
        if 1 <= range_ <= len(factors) and charge > 0:
            return int(charge * factors[range_ - 1] + 0.5)
    if k == "torp" and 1 <= range_ <= 12:
        return 4
    if k == "plasma":
        values = (8, 6, 5, 4, 3, 3, 2, 2, 1, 1, 1, 1, 1, 1)
        if 1 <= range_ <= len(values):
            return values[range_ - 1]
    return None

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


# Weapon mount → relative facings it can bear (mirrors src/arc.rs Mount).
# Snapshot weapons carry `mount` as the snake_case name below.
MOUNT_FACINGS: dict[str, tuple[int, ...]] = {
    "forward": (0,),
    "forward_starboard": (5, 0),
    "aft_starboard": (3, 4),
    "aft": (3,),
    "aft_port": (2, 3),
    "forward_port": (0, 1),
}


def weapon_in_arc(
    weapon: dict,
    attacker_q: int,
    attacker_r: int,
    attacker_facing: int,
    target_q: int,
    target_r: int,
) -> bool:
    """True if the target lies in this weapon's firing arc (pure geometry).

    Uses the snapshot `mount` field (snake_case) when present; falls back to
    the broad `arc` name (Forward/Rear/Left/Right/All) for older snapshots.
    """
    if attacker_q == target_q and attacker_r == target_r:
        return False
    mount = str(weapon.get("mount") or "").lower()
    facings = MOUNT_FACINGS.get(mount)
    if facings is None:
        arc = str(weapon.get("arc") or "").lower()
        facings = _ARC_FACINGS.get(arc, tuple(range(6)))
    rel = relative_bearing(attacker_facing, attacker_q, attacker_r, target_q, target_r)
    return rel in facings


# Broad arc name → relative facings (fallback when `mount` is absent).
_ARC_FACINGS: dict[str, tuple[int, ...]] = {
    "forward": (0, 5, 1),
    "rear": (2, 3, 4),
    "left": (4, 5),
    "right": (1, 2),
    "all": (0, 1, 2, 3, 4, 5),
}


def threats_to_ship(
    snap: dict, ship_id: int
) -> list[dict]:
    """Enemy ships that can bear on `ship_id` with at least one charged weapon.

    Advisory only — derived from snapshot fields + pure geometry. Does not
    consult engine rules authority. Each entry: {ship, weapon, range, in_arc}.
    """
    target = None
    for s in snap.get("ships") or []:
        if int(s.get("id") or -1) == int(ship_id):
            target = s
            break
    if target is None or target.get("destroyed"):
        return []
    tq, tr = int(target.get("q") or 0), int(target.get("r") or 0)
    out: list[dict] = []
    for s in snap.get("ships") or []:
        if s is target or s.get("destroyed"):
            continue
        if s.get("controller") == target.get("controller"):
            continue
        sq, sr = int(s.get("q") or 0), int(s.get("r") or 0)
        sf = int(s.get("facing") or 0)
        rng = distance(sq, sr, tq, tr)
        for w in s.get("weapons") or []:
            if not w.get("operational", True) or w.get("fired"):
                continue
            if int(w.get("charge") or 0) <= 0:
                continue
            if int(w.get("max_range") or 0) < rng:
                continue
            if not weapon_in_arc(w, sq, sr, sf, tq, tr):
                continue
            out.append({"ship": s, "weapon": w, "range": rng, "in_arc": True})
    return out


def bar(filled: int, total: int, width: Optional[int] = None) -> str:
    """Text bar like [####....]. filled/total; width defaults to total (capped)."""
    total = max(0, int(total))
    filled = min(total, max(0, int(filled)))
    if total <= 0:
        return "[—]"
    w = width if width is not None else min(total, 16)
    if total <= w:
        return "[" + "#" * filled + "." * (total - filled) + "]"
    # scale
    nf = round(filled * w / total) if total else 0
    nf = min(w, max(0, nf))
    return "[" + "#" * nf + "." * (w - nf) + "]"
