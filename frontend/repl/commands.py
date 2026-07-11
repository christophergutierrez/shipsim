"""Interactive command parsing and order construction (protocol v1)."""

from __future__ import annotations

import shlex
from typing import Any, Optional

from view import SHIELD_LABELS, living_player_ships, living_ships

PROTOCOL_VERSION = 1

HELP = """
Commands (Combat Model v2 — see docs/PLAY-V2.md)

  help | ?                 this text
  status | s               reprint snapshot
  board | b                board only
  ships                    ship list (compact)
  raw                      dump last snapshot JSON keys summary
  quit | q | exit          leave REPL

  allocate | a [ship]      interactive allocate for a player ship
  alloc-default [ship]     dump remaining power into movement (quick test)
  move | m <mode> [ship]   forward|reverse|port|starboard  (ACTIVE ship default)
  pass | p [ship]          pass_move
  fire | f [ship]          interactive commit_fire
  ready | r [ship]         ready_fire
  end | e                  end_turn
  order <json>             send raw JSON order (protocol_version filled if missing)

Phase flow: allocate all ships → move/pass each ACTIVE → commit fire + ready all
→ (loop move/fire) → end turn.
""".strip()


def _default_ship(snap: dict[str, Any], phase: str) -> Optional[int]:
    if phase == "movement" and snap.get("active_ship") is not None:
        return int(snap["active_ship"])
    players = living_player_ships(snap)
    if len(players) == 1:
        return int(players[0]["id"])
    if phase == "allocate":
        for s in players:
            # Heuristic: not yet allocated if movement_allocated==0 and no weapon charge
            # and no shield power — imperfect but good enough for the menu default.
            if int(s.get("movement_allocated") or 0) == 0 and not any(
                int(w.get("charge") or 0) > 0 for w in (s.get("weapons") or [])
            ):
                return int(s["id"])
    if phase == "firing":
        ready = set(snap.get("ships_ready_fire") or [])
        for s in players:
            if int(s["id"]) not in ready:
                return int(s["id"])
    if players:
        return int(players[0]["id"])
    return None


def _parse_ship(tokens: list[str], snap: dict[str, Any], phase: str) -> tuple[Optional[int], list[str]]:
    if tokens and tokens[0].isdigit():
        return int(tokens[0]), tokens[1:]
    return _default_ship(snap, phase), tokens


def _prompt(msg: str, default: Optional[str] = None) -> str:
    suffix = f" [{default}]" if default is not None else ""
    raw = input(f"{msg}{suffix}: ").strip()
    if raw == "" and default is not None:
        return default
    return raw


def _prompt_int(msg: str, default: int = 0) -> int:
    while True:
        raw = _prompt(msg, str(default))
        try:
            return int(raw)
        except ValueError:
            print("  need an integer")


def interactive_allocate(snap: dict[str, Any], ship_id: int) -> Optional[dict[str, Any]]:
    ship = next((s for s in living_ships(snap) if s.get("id") == ship_id), None)
    if ship is None:
        print(f"  ship #{ship_id} not found / destroyed")
        return None
    power = int(ship.get("power") or 0)
    print(f"  allocate for #{ship_id} {ship.get('class')} — power pool {power}")
    movement = _prompt_int("  movement power", min(4, power))
    remaining = max(0, power - movement)
    weapons: dict[str, int] = {}
    for w in ship.get("weapons") or []:
        if not w.get("operational", True):
            continue
        wid = str(w.get("id"))
        max_c = int(w.get("max_charge") or 0)
        if max_c <= 0 or remaining <= 0:
            weapons[wid] = 0
            continue
        ch = _prompt_int(f"  charge {wid} (0..{min(max_c, remaining)})", 0)
        ch = max(0, min(ch, max_c, remaining))
        weapons[wid] = ch
        remaining -= ch
    shields = [0] * 6
    for i, lab in enumerate(SHIELD_LABELS):
        if remaining <= 0:
            break
        max_face = int(ship.get("max_shield_per_facing") or 0)
        val = _prompt_int(f"  shield {lab} (0..{min(max_face, remaining)})", 0)
        val = max(0, min(val, max_face, remaining))
        shields[i] = val
        remaining -= val
    if remaining > 0:
        print(f"  (leaving {remaining} unallocated)")
    return {
        "protocol_version": PROTOCOL_VERSION,
        "type": "allocate",
        "ship": ship_id,
        "movement": movement,
        "weapons": weapons,
        "shields": shields,
    }


def default_allocate(snap: dict[str, Any], ship_id: int) -> Optional[dict[str, Any]]:
    ship = next((s for s in living_ships(snap) if s.get("id") == ship_id), None)
    if ship is None:
        print(f"  ship #{ship_id} not found / destroyed")
        return None
    power = int(ship.get("power") or 0)
    weapons = {str(w.get("id")): 0 for w in (ship.get("weapons") or [])}
    # Put one point into first beam if any, rest into movement (capped by common sense).
    for w in ship.get("weapons") or []:
        if str(w.get("kind", "")).lower() == "beam" and int(w.get("max_charge") or 0) >= 1:
            weapons[str(w["id"])] = 1
            power = max(0, power - 1)
            break
    return {
        "protocol_version": PROTOCOL_VERSION,
        "type": "allocate",
        "ship": ship_id,
        "movement": power,
        "weapons": weapons,
        "shields": [0, 0, 0, 0, 0, 0],
    }


MOVE_ALIASES = {
    "f": "forward",
    "forward": "forward",
    "fwd": "forward",
    "r": "reverse",
    "rev": "reverse",
    "reverse": "reverse",
    "port": "turn_port",
    "p": "turn_port",
    "l": "turn_port",
    "left": "turn_port",
    "starboard": "turn_starboard",
    "stbd": "turn_starboard",
    "sb": "turn_starboard",
    "right": "turn_starboard",
}


def interactive_fire(snap: dict[str, Any], ship_id: int) -> Optional[dict[str, Any]]:
    ship = next((s for s in living_ships(snap) if s.get("id") == ship_id), None)
    if ship is None:
        print(f"  ship #{ship_id} not found / destroyed")
        return None
    charged = [
        w
        for w in (ship.get("weapons") or [])
        if w.get("operational", True)
        and not w.get("fired")
        and int(w.get("charge") or 0) > 0
    ]
    if not charged:
        print("  no charged unfired weapons")
        return None
    print("  weapons:")
    for i, w in enumerate(charged):
        print(f"    [{i}] {w.get('id')} chg={w.get('charge')} rng≤{w.get('max_range')}")
    wi = _prompt_int("  weapon index", 0)
    if wi < 0 or wi >= len(charged):
        print("  bad weapon index")
        return None
    weapon = str(charged[wi]["id"])
    enemies = [s for s in living_ships(snap) if s.get("id") != ship_id]
    if not enemies:
        print("  no targets")
        return None
    print("  targets:")
    for i, t in enumerate(enemies):
        print(f"    [{i}] #{t.get('id')} {t.get('class')} @({t.get('q')},{t.get('r')})")
    ti = _prompt_int("  target index", 0)
    if ti < 0 or ti >= len(enemies):
        print("  bad target index")
        return None
    target = int(enemies[ti]["id"])
    print("  shield facings: " + " ".join(f"{i}={lab}" for i, lab in enumerate(SHIELD_LABELS)))
    facing = _prompt_int("  shield_facing", 0)
    return {
        "protocol_version": PROTOCOL_VERSION,
        "type": "commit_fire",
        "ship": ship_id,
        "weapon": weapon,
        "target": target,
        "shield_facing": facing,
    }


def build_order(
    line: str, snap: dict[str, Any]
) -> tuple[Optional[dict[str, Any]], Optional[str]]:
    """
    Parse a REPL line into an order dict, or a side-effect command.

    Returns (order, side_command). Exactly one of them is set for handled input.
    side_command is one of: help, status, board, ships, raw, quit, empty, unknown.
    """
    line = line.strip()
    if not line:
        return None, "empty"
    try:
        tokens = shlex.split(line)
    except ValueError as exc:
        print(f"  parse error: {exc}")
        return None, "empty"

    cmd = tokens[0].lower()
    rest = tokens[1:]
    phase = str(snap.get("phase") or "")

    if cmd in ("help", "?", "h"):
        return None, "help"
    if cmd in ("status", "s"):
        return None, "status"
    if cmd in ("board", "b"):
        return None, "board"
    if cmd == "ships":
        return None, "ships"
    if cmd == "raw":
        return None, "raw"
    if cmd in ("quit", "q", "exit"):
        return None, "quit"

    if cmd in ("allocate", "a", "alloc"):
        ship, _ = _parse_ship(rest, snap, "allocate")
        if ship is None:
            print("  no default ship; pass ship id")
            return None, "empty"
        order = interactive_allocate(snap, ship)
        return order, None

    if cmd in ("alloc-default", "ad", "allocd"):
        ship, _ = _parse_ship(rest, snap, "allocate")
        if ship is None:
            print("  no default ship; pass ship id")
            return None, "empty"
        return default_allocate(snap, ship), None

    if cmd in ("move", "m"):
        if not rest:
            print("  usage: move <forward|reverse|port|starboard> [ship]")
            return None, "empty"
        mode_raw = rest[0].lower()
        mode = MOVE_ALIASES.get(mode_raw)
        if mode is None:
            print(f"  unknown move mode {mode_raw!r}")
            return None, "empty"
        ship, _ = _parse_ship(rest[1:], snap, "movement")
        if ship is None:
            print("  no active/default ship")
            return None, "empty"
        return {
            "protocol_version": PROTOCOL_VERSION,
            "type": "move",
            "ship": ship,
            "mode": mode,
        }, None

    if cmd in ("pass", "p", "pass_move"):
        ship, _ = _parse_ship(rest, snap, "movement")
        if ship is None:
            print("  no active/default ship")
            return None, "empty"
        return {
            "protocol_version": PROTOCOL_VERSION,
            "type": "pass_move",
            "ship": ship,
        }, None

    if cmd in ("fire", "f", "commit", "commit_fire"):
        ship, _ = _parse_ship(rest, snap, "firing")
        if ship is None:
            print("  no default ship")
            return None, "empty"
        order = interactive_fire(snap, ship)
        return order, None

    if cmd in ("ready", "r", "ready_fire"):
        ship, _ = _parse_ship(rest, snap, "firing")
        if ship is None:
            print("  no default ship")
            return None, "empty"
        return {
            "protocol_version": PROTOCOL_VERSION,
            "type": "ready_fire",
            "ship": ship,
        }, None

    if cmd in ("end", "e", "end_turn"):
        return {"protocol_version": PROTOCOL_VERSION, "type": "end_turn"}, None

    if cmd == "order":
        raw = line[len(tokens[0]) :].strip()
        if not raw:
            print("  usage: order {json…}")
            return None, "empty"
        import json

        try:
            obj = json.loads(raw)
        except json.JSONDecodeError as exc:
            print(f"  bad json: {exc}")
            return None, "empty"
        if not isinstance(obj, dict):
            print("  order must be a JSON object")
            return None, "empty"
        obj.setdefault("protocol_version", PROTOCOL_VERSION)
        return obj, None

    return None, "unknown"
