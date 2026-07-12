"""Human-readable snapshot / board / combat formatting for the REPL.

Presentation follows terminal-roguelike practice (see frontend/repl/README.md):
model stays the NDJSON snapshot; this module is a pure view. Axial coords match
the core (Red Blob / src/hex.rs). Rendering uses odd-r stagger + double-width
cells — the cheap, readable hex-on-ASCII option.
"""

from __future__ import annotations

from typing import Any, Optional

from hexutil import (
    FACING_GLYPH,
    FACING_LEGEND,
    SHIELD_LABELS,
    bar,
    distance,
    legal_shield_facings,
    relative_bearing,
    ship_callsign,
)
from style import (
    active as sty_active,
    available as sty_available,
    dead as sty_dead,
    enemy as sty_enemy,
    err as sty_err,
    fired as sty_fired,
    focus as sty_focus,
    hit as sty_hit,
    miss as sty_miss,
    muted,
    queued as sty_queued,
    ok as sty_ok,
    paint,
    panel,
    player as sty_player,
    rule,
    warn as sty_warn,
)

# Re-export for commands
__all__ = [
    "FACING_GLYPH",
    "SHIELD_LABELS",
    "format_header",
    "format_ship_line",
    "format_ship_card",
    "format_board",
    "format_commits",
    "format_combat_log",
    "format_combat_events",
    "format_snapshot",
    "format_error",
    "living_player_ships",
    "living_ships",
    "ship_by_id",
    "format_tactical",
]


def ship_by_id(snap: dict[str, Any], ship_id: int) -> Optional[dict[str, Any]]:
    for ship in snap.get("ships") or []:
        if ship.get("id") == ship_id:
            return ship
    return None


def format_header(snap: dict[str, Any], *, selected: Optional[int] = None) -> str:
    status = snap.get("status", "?")
    phase = snap.get("phase", "?")
    turn = snap.get("turn", "?")
    active = snap.get("active_ship")
    warn_s = (
        sty_warn("  ⚠ leftover useful actions") if snap.get("end_turn_warning") else ""
    )
    active_s = f" active=#{active}" if active is not None else ""
    sel_s = sty_focus(f"  focus=#{selected}") if selected is not None else ""
    phase_s = paint(str(phase), "bold", "bright_white")
    status_s = str(status)
    if status == "Won":
        status_s = sty_ok(status_s)
    elif status == "Lost":
        status_s = sty_err(status_s)
    return (
        f"{rule('shipsim')}\n"
        f"turn {turn}  phase={phase_s}  status={status_s}{active_s}{sel_s}{warn_s}"
    )


def weapon_outcomes_for_ship(
    snap: dict[str, Any], ship_id: int
) -> dict[str, dict[str, Any]]:
    """weapon_id → last combat-log entry for this attacker (kind, damage, target)."""
    out: dict[str, dict[str, Any]] = {}
    for e in snap.get("combat_log") or []:
        if int(e.get("attacker") or -1) != int(ship_id):
            continue
        wid = e.get("weapon")
        if not wid:
            continue
        out[str(wid)] = e
    return out


def queued_weapons_for_ship(snap: dict[str, Any], ship_id: int) -> dict[str, dict[str, Any]]:
    """weapon_id → pending fire_commit (not resolved yet)."""
    out: dict[str, dict[str, Any]] = {}
    for c in snap.get("fire_commits") or []:
        if int(c.get("ship") or -1) != int(ship_id):
            continue
        wid = c.get("weapon")
        if wid:
            out[str(wid)] = c
    return out


def format_ship_line(
    ship: dict[str, Any],
    *,
    active: bool = False,
    focused: bool = False,
    hull_max: Optional[int] = None,
) -> str:
    mark = "*" if active else ("@" if focused else " ")
    dead = sty_dead(" [DEAD]") if ship.get("destroyed") else ""
    face_i = int(ship.get("facing", 0))
    face = f"{face_i}{FACING_GLYPH.get(face_i, '?')}"
    ctrl = ship.get("controller", "?")
    hull = int(ship.get("structure") or 0)
    hmax = hull_max or hull
    cs = ship_callsign(ship)
    name = f"{cs} #{ship.get('id')} {ship.get('class', '?')}"
    if ship.get("controller") == "player":
        name = sty_player(name)
    elif not ship.get("destroyed"):
        name = sty_enemy(name)
    if focused:
        name = sty_focus(name)
    if active:
        mark = sty_active(mark)
    return (
        f"{mark}{name} ({ctrl}) "
        f"@({ship.get('q')},{ship.get('r')}) face={face} "
        f"(fwd={FACING_GLYPH.get(face_i, '?')}) "
        f"pwr={ship.get('power')} mov={ship.get('move_remaining')}/"
        f"{ship.get('movement_allocated')} "
        f"hull={hull}/{hmax} {bar(hull, hmax)}{dead}"
    )


def format_weapons(
    ship: dict[str, Any],
    *,
    indent: str = "    ",
    snap: Optional[dict[str, Any]] = None,
) -> str:
    """Weapon lines from live snapshot.

    Timing (easy to misread):
    - CHG = still available to commit this fire phase
    - QUEUED = commit_fire done; charge still on the ship until the phase resolves
    - FIRED HIT/MISS = phase resolved; charge spent (always empty bar)
    """
    sid = int(ship.get("id") or 0)
    outcomes = weapon_outcomes_for_ship(snap, sid) if snap is not None else {}
    queued = queued_weapons_for_ship(snap, sid) if snap is not None else {}
    lines = []
    for w in ship.get("weapons") or []:
        max_c = int(w.get("max_charge") or 0)
        ch = int(w.get("charge") or 0)
        mx = max(max_c, 1)
        wid = str(w.get("id"))
        log_e = outcomes.get(wid)
        is_fired = bool(w.get("fired")) or log_e is not None
        is_queued = wid in queued and not is_fired

        if is_fired:
            # Prefer combat-log outcome (covers every weapon that resolved).
            kind = str((log_e or {}).get("kind") or "").lower()
            dmg = int((log_e or {}).get("damage") or 0)
            if kind == "hit":
                tail = (
                    sty_fired("FIRED")
                    + " "
                    + sty_hit(f"HIT{f' {dmg}' if dmg else ''}")
                )
            elif kind == "miss":
                tail = sty_fired("FIRED") + " " + sty_miss("MISS")
            else:
                tail = sty_fired("FIRED")
            if not w.get("operational", True):
                tail += sty_dead(" (box)")
            # Never show leftover charge after a resolved shot.
            b = bar(0, mx)
            ch_note = muted(" chg=0")
            lines.append(
                f"{indent}{wid:10} {w.get('kind'):6} arc={w.get('arc')} "
                f"rng≤{w.get('max_range')} {b} {tail}{ch_note}"
            )
        elif is_queued:
            c = queued[wid]
            tgt = c.get("target")
            face = int(c.get("shield_facing") or 0)
            lab = SHIELD_LABELS[face] if 0 <= face < 6 else str(face)
            tail = sty_queued(
                f"QUEUED →#{tgt} sh{face}:{lab}  (resolves when all ready)"
            )
            # Charge still present until resolve — do not call it available CHG.
            b = bar(ch, mx)
            lines.append(
                f"{indent}{wid:10} {w.get('kind'):6} arc={w.get('arc')} "
                f"rng≤{w.get('max_range')} {b} {tail}"
            )
        elif not w.get("operational", True):
            tail = sty_dead("DEAD")
            b = bar(0, mx)
            lines.append(
                f"{indent}{wid:10} {w.get('kind'):6} arc={w.get('arc')} "
                f"rng≤{w.get('max_range')} {b} {tail}"
            )
        elif ch > 0:
            tail = sty_available(f"CHG {ch}/{max_c}  (available)")
            b = bar(ch, mx)
            lines.append(
                f"{indent}{wid:10} {w.get('kind'):6} arc={w.get('arc')} "
                f"rng≤{w.get('max_range')} {b} {tail}"
            )
        else:
            tail = muted(f"0/{max_c}")
            b = bar(0, mx)
            lines.append(
                f"{indent}{wid:10} {w.get('kind'):6} arc={w.get('arc')} "
                f"rng≤{w.get('max_range')} {b} {tail}"
            )

    # Explicit shot list so multi-weapon volleys aren't lost in RECENT truncation.
    if snap is not None and outcomes:
        lines.append(indent + muted("shots resolved this turn:"))
        for wid, e in outcomes.items():
            kind = str(e.get("kind") or "").lower()
            tag = sty_hit("HIT") if kind == "hit" else sty_miss("MISS")
            lines.append(
                indent
                + f"  {wid} → #{e.get('target')} {tag} "
                f"dmg={e.get('damage')} sh={e.get('shield')}"
            )
    return "\n".join(lines) if lines else f"{indent}(no weapons)"


def format_shields(
    ship: dict[str, Any],
    *,
    indent: str = "    ",
    highlight: Optional[list[int]] = None,
) -> str:
    """Bar scale = max per face so remaining always moves when damaged."""
    powered = ship.get("shields_powered") or [0] * 6
    remaining = ship.get("shields_remaining") or [0] * 6
    max_face = max(int(ship.get("max_shield_per_facing") or 0), 1)
    hi = set(highlight or [])
    lines = []
    for i, lab in enumerate(SHIELD_LABELS):
        rem = int(remaining[i]) if i < len(remaining) else 0
        pwr = int(powered[i]) if i < len(powered) else 0
        mark = "←" if i in hi else " "
        # rem against max_face so hits visibly shrink the bar
        lines.append(
            f"{indent}{mark}{i}:{lab:2} {bar(rem, max_face)} "
            f"rem={rem} pwr={pwr}"
        )
    return "\n".join(lines)


def format_ship_card(
    ship: dict[str, Any],
    *,
    title: Optional[str] = None,
    hull_max: Optional[int] = None,
    vs: Optional[dict[str, Any]] = None,
    focused: bool = False,
    active: bool = False,
    snap: Optional[dict[str, Any]] = None,
) -> str:
    """Detailed status for own ship or a target."""
    parts = []
    head = title or format_ship_line(
        ship, active=active, focused=focused, hull_max=hull_max
    )
    parts.append(head)
    if ship.get("destroyed"):
        return "\n".join(parts)

    hull = int(ship.get("structure") or 0)
    hmax = hull_max or hull
    parts.append(f"    hull {bar(hull, hmax)} {hull}/{hmax}  "
                 f"bridge={ship.get('bridge')} eng={ship.get('engine')} "
                 f"pwr_sys={ship.get('power_sys')}  keel={ship.get('keel')}")

    highlight = None
    if vs is not None and not vs.get("destroyed"):
        highlight = legal_shield_facings(
            int(vs["q"]),
            int(vs["r"]),
            int(ship["q"]),
            int(ship["r"]),
            int(ship.get("facing") or 0),
        )
        dist = distance(int(vs["q"]), int(vs["r"]), int(ship["q"]), int(ship["r"]))
        rel = relative_bearing(
            int(ship.get("facing") or 0),
            int(ship["q"]),
            int(ship["r"]),
            int(vs["q"]),
            int(vs["r"]),
        )
        face_labels = ", ".join(
            f"{i}:{SHIELD_LABELS[i]}" for i in highlight
        ) or "?"
        parts.append(
            f"    vs {ship_callsign(vs)}: range={dist}  "
            f"shields facing them: {face_labels}  "
            f"(rel bearing from this ship: {rel})"
        )

    parts.append("    shields (rem/powered; ← faces observer if vs set):")
    parts.append(format_shields(ship, highlight=highlight))
    parts.append("    weapons:")
    parts.append(format_weapons(ship, snap=snap))
    return "\n".join(parts)


def format_tactical(
    snap: dict[str, Any],
    *,
    selected: Optional[int] = None,
    hull_max: Optional[dict[int, int]] = None,
) -> str:
    """Focus card + enemies with facing-shield info."""
    hull_max = hull_max or {}
    me = ship_by_id(snap, selected) if selected is not None else None
    if me is None:
        players = living_player_ships(snap)
        me = players[0] if players else None
    if me is None:
        return format_snapshot(snap, selected=selected, hull_max=hull_max, verbose=True)

    parts = [format_header(snap, selected=selected)]
    if snap.get("move_order"):
        moved = set(snap.get("ships_moved_this_phase") or [])
        queue = " → ".join(
            (ship_callsign(ship_by_id(snap, int(sid)) or {"id": sid, "controller": "?"})
             + (" done" if sid in moved else ""))
            for sid in snap.get("move_order") or []
        )
        parts.append(
            muted(f"movement: {queue}")
        )
    if snap.get("ships_ready_fire"):
        ready = ", ".join(
            ship_callsign(ship_by_id(snap, int(sid)) or {"id": sid, "controller": "?"})
            for sid in snap.get("ships_ready_fire") or []
        )
        parts.append(muted(f"fire ready: {ready}"))

    you_body = format_ship_card(
        me,
        hull_max=hull_max.get(int(me["id"])),
        focused=True,
        active=snap.get("active_ship") == me.get("id"),
        snap=snap,
    )
    parts.append(panel("YOUR SHIP", you_body))

    enemies = [
        s
        for s in living_ships(snap)
        if s.get("id") != me.get("id")
    ]
    if enemies:
        contact_chunks = []
        for e in enemies:
            contact_chunks.append(
                format_ship_card(
                    e,
                    hull_max=hull_max.get(int(e["id"])),
                    vs=me,
                    active=snap.get("active_ship") == e.get("id"),
                    snap=snap,
                )
            )
        parts.append(panel("CONTACTS", "\n\n".join(contact_chunks)))

    board = format_board(
        snap,
        selected=selected,
        active=snap.get("active_ship"),
    )
    if board:
        parts.append(panel("MAP", board, width=48))
    commits = format_commits(snap)
    if commits:
        parts.append(panel("PENDING FIRE", commits))
    return "\n".join(parts)


def format_terminal_banner(status: str) -> str:
    """Panel-weight, monochrome-readable terminal outcome announcement."""
    label = f"SCENARIO {status.upper()}"
    styled = sty_ok(label) if status == "Won" else sty_err(label)
    return panel("GAME OVER", styled + "\nOrders disabled; use quit or log.", width=56)


def _cell_glyph(
    ship: Optional[dict[str, Any]],
    *,
    selected: Optional[int],
    active: Optional[int],
) -> str:
    """Map cell: callsign + facing arrow (e.g. A1→). Empty sea is muted dots."""
    if ship is None:
        return muted("····")
    face_i = int(ship.get("facing", 0)) % 6
    face = FACING_GLYPH.get(face_i, "?")
    sid = int(ship.get("id") or 0)
    cs = ship_callsign(ship)
    raw = f"{cs}{face}"
    # pad to 4 cols for rough alignment
    raw = f"{raw:<4}"[:4]
    if ship.get("destroyed"):
        return muted(" x  ")
    if selected is not None and sid == selected:
        return sty_focus(raw)
    if active is not None and sid == active:
        return sty_active(raw)
    if ship.get("controller") == "player":
        return sty_player(raw)
    return sty_enemy(raw)


def format_board(
    snap: dict[str, Any],
    *,
    selected: Optional[int] = None,
    active: Optional[int] = None,
) -> str:
    """
    Axial map as odd-r staggered, double-width cells.

    Coordinates stay axial (q, r) — same as the core / Red Blob axial.
    Odd rows indent by one character so hex neighbors look adjacent
    (classic terminal hex "offset trick"). Not a full hex outline draw.
    """
    m = snap.get("map") or {}
    width = int(m.get("width") or 0)
    height = int(m.get("height") or 0)
    ships: dict[tuple[int, int], dict[str, Any]] = {}
    # Keep one wreck as battlefield history, but never let it cover a living ship.
    for ship in snap.get("ships") or []:
        coord = (int(ship["q"]), int(ship["r"]))
        previous = ships.get(coord)
        if previous is None or (previous.get("destroyed") and not ship.get("destroyed")):
            ships[coord] = ship

    if width <= 0 or height <= 0 or width * height > 600:
        rows = []
        for ship in snap.get("ships") or []:
            face_i = int(ship.get("facing", 0))
            face = f"{face_i}{FACING_GLYPH.get(face_i, '?')}"
            if ship.get("destroyed"):
                face = "wreck"
            rows.append(
                f"  ship #{ship.get('id')} ({ship.get('q')},{ship.get('r')}) {face}"
            )
        return "positions:\n" + ("\n".join(rows) if rows else "  (none)")

    legend = muted(FACING_LEGEND)
    sides = muted("callsign: A#=player  B#=ai  C#=scripted   cell=callsign+fwd arrow")
    # Column ruler (q) — 4 chars per cell
    q_labels = "".join(f"{q % 10}   " for q in range(width))
    lines = [legend, sides, "     " + q_labels]

    for r in range(height):
        # Odd-r horizontal offset (~ half cell) for hex adjacency feel.
        indent = "  " if (r % 2) else ""
        cells = []
        for q in range(width):
            cells.append(_cell_glyph(ships.get((q, r)), selected=selected, active=active))
        lines.append(f" r{r:02d} {indent}{''.join(cells)}")

    lines.append(muted("     " + "".join(f"{q % 10}   " for q in range(width))))
    return "\n".join(lines)


def format_commits(snap: dict[str, Any]) -> str:
    commits = snap.get("fire_commits") or []
    if not commits:
        return ""
    lines = [
        "queued shots (NOT resolved yet — charge still listed on ship until ready):"
    ]
    for c in commits:
        face = int(c.get("shield_facing") or 0)
        lab = SHIELD_LABELS[face] if 0 <= face < 6 else "?"
        atk = ship_by_id(snap, int(c.get("ship") or -1))
        cs = ship_callsign(atk) if atk else f"#{c.get('ship')}"
        lines.append(
            f"  {cs} {c.get('weapon')} → #{c.get('target')} "
            f"shield={face}:{lab}"
        )
    return "\n".join(lines)


def format_combat_events(
    events: list[dict[str, Any]],
    snap: dict[str, Any],
    *,
    hull_max: Optional[dict[int, int]] = None,
) -> str:
    """HIT/MISS report with post-shot target status (visual weight on outcomes)."""
    if not events:
        return ""
    hull_max = hull_max or {}
    body_lines: list[str] = []
    for e in events:
        kind = str(e.get("kind") or "").lower()
        dmg = int(e.get("damage") or 0)
        face = int(e.get("shield") or 0)
        lab = SHIELD_LABELS[face] if 0 <= face < 6 else str(face)
        if kind == "hit" and dmg > 0:
            tag = sty_hit(f"HIT for {dmg}")
        elif kind == "hit":
            tag = sty_hit("HIT (0 dmg)")
        else:
            tag = sty_miss("MISS")
        atk = ship_by_id(snap, int(e.get("attacker") or -1))
        tgt = ship_by_id(snap, int(e.get("target") or -1))
        atk_cs = ship_callsign(atk) if atk else f"#{e.get('attacker')}"
        tgt_cs = ship_callsign(tgt) if tgt else f"#{e.get('target')}"
        wpn = e.get("weapon") or "?"
        body_lines.append(
            f"{atk_cs} {wpn} → {tgt_cs}  {tag}  on shield {face}:{lab}"
        )
        if tgt:
            body_lines.append(
                format_ship_card(
                    tgt,
                    title=f"target {tgt_cs} after shot:",
                    hull_max=hull_max.get(int(tgt["id"])),
                    vs=atk,
                    snap=snap,
                )
            )
    return panel("FIRE RESOLUTION", "\n".join(body_lines))


def format_combat_log(snap: dict[str, Any], *, last_n: int = 8) -> str:
    log = snap.get("combat_log") or []
    if not log:
        return ""
    lines = [f"combat log (last {min(last_n, len(log))}):"]
    for e in log[-last_n:]:
        kind = e.get("kind")
        face = int(e.get("shield") or 0)
        lab = SHIELD_LABELS[face] if 0 <= face < 6 else "?"
        wpn = e.get("weapon") or "?"
        lines.append(
            f"  #{e.get('attacker')} {wpn} → #{e.get('target')} "
            f"{kind} dmg={e.get('damage')} shield={face}:{lab}"
        )
    return "\n".join(lines)


def format_snapshot(
    snap: dict[str, Any],
    *,
    selected: Optional[int] = None,
    hull_max: Optional[dict[int, int]] = None,
    verbose: bool = True,
) -> str:
    hull_max = hull_max or {}
    if verbose:
        return format_tactical(snap, selected=selected, hull_max=hull_max)
    parts = [format_header(snap, selected=selected)]
    active = snap.get("active_ship")
    for ship in snap.get("ships") or []:
        parts.append(
            format_ship_line(
                ship,
                active=ship.get("id") == active,
                focused=ship.get("id") == selected,
                hull_max=hull_max.get(int(ship["id"])),
            )
        )
    return "\n".join(parts)


def format_error(err: dict[str, Any]) -> str:
    code = err.get("code", "error")
    msg = err.get("message", "")
    hint = ""
    low = str(msg).lower()
    if "phase firing" in low and "movement" in low:
        hint = (
            "\n  → You are still in movement: pass or move the ACTIVE ship first, "
            "then fire when phase is firing."
        )
    elif "phase movement" in low and "firing" in low:
        hint = (
            "\n  → You are in firing: use f/ready (not m). "
            "A single move already used this ship's movement decision."
        )
    elif "already moved" in low:
        hint = "\n  → This ship already moved/passed this movement phase."
    return sty_err(f"! {code}: {msg}") + hint


def snapshot_delta(before: Optional[dict[str, Any]], after: dict[str, Any]) -> str:
    """Short line of what changed (hull, charges, phase) so the board feels live."""
    if not before:
        return ""
    bits: list[str] = []
    if before.get("phase") != after.get("phase"):
        bits.append(f"phase {before.get('phase')}→{after.get('phase')}")
    if before.get("turn") != after.get("turn"):
        bits.append(f"turn {before.get('turn')}→{after.get('turn')}")
    bships = {int(s["id"]): s for s in (before.get("ships") or [])}
    for s in after.get("ships") or []:
        sid = int(s["id"])
        old = bships.get(sid)
        if not old:
            continue
        oh, nh = int(old.get("structure") or 0), int(s.get("structure") or 0)
        if nh != oh:
            bits.append(sty_hit(f"#{sid} hull {oh}→{nh}"))
        orem = old.get("shields_remaining") or []
        nrem = s.get("shields_remaining") or []
        for i in range(min(len(orem), len(nrem))):
            if int(orem[i]) != int(nrem[i]):
                bits.append(
                    f"#{sid} sh{i}:{SHIELD_LABELS[i]} {orem[i]}→{nrem[i]}"
                )
        ow = {w["id"]: w for w in (old.get("weapons") or [])}
        for w in s.get("weapons") or []:
            prev = ow.get(w["id"])
            if not prev:
                continue
            if bool(prev.get("fired")) != bool(w.get("fired")) and w.get("fired"):
                bits.append(sty_warn(f"#{sid} {w['id']} FIRED"))
            elif int(prev.get("charge") or 0) != int(w.get("charge") or 0):
                bits.append(
                    f"#{sid} {w['id']} chg {prev.get('charge')}→{w.get('charge')}"
                )
    if not bits:
        return muted("  (no ship field deltas)")
    return "  Δ " + " · ".join(bits)


def living_player_ships(snap: dict[str, Any]) -> list[dict[str, Any]]:
    return [
        s
        for s in (snap.get("ships") or [])
        if not s.get("destroyed") and s.get("controller") == "player"
    ]


def living_ships(snap: dict[str, Any]) -> list[dict[str, Any]]:
    return [s for s in (snap.get("ships") or []) if not s.get("destroyed")]
