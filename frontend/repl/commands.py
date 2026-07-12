"""Interactive command parsing and order construction (protocol v1).

Ship-centric: select a focus ship once; allocate draft / fire / status use it.
Facing is always 0..5 (same numbering as the core).
"""

from __future__ import annotations

import shlex
from dataclasses import dataclass, field
from typing import Any, Optional

from hexutil import (
    SHIELD_LABELS,
    bar,
    distance,
    legal_shield_facings,
    ship_callsign,
    turn_toward,
    weapon_in_arc,
)
from view import living_player_ships, living_ships, ship_by_id

PROTOCOL_VERSION = 1

HELP = """
shipsim REPL — ship-centric Combat Model v2

  Facing 0..5:  0→ 1↗ 2↑ 3← 4↙ 5↓
  Shields:      0=F 1=FR 2=RR 3=R 4=RL 5=FL

  status | s     refresh play frame (live snapshot)
  board | b      hex map
  ships          ship list
  log            toggle scrollback history panel
  cls            redraw frame
  hint           phase tip
  quit

Allocate (local draft until commit — pick ship first)
  a              list unallocated player ships and open a draft
  a <id>         draft that ship (if not yet allocated)
  During draft:
    mov N | m N            movement power
    w                      enter weapons group (then b1 2, t1 1, …)
    w t1 1 | w b1 2        set charge (shortcuts: b1=beam_1, t1=torp_1, p1=plasma_1)
    sh                     enter shields group (then 0 3, F 2, …)
    sh 0 3                 set one shield face
    show | reset | commit | cancel
    done | ..              leave weapons/shields group

Movement: m <0-5> (one turn OR one step — one decision) | m f/r/port/stbd | p pass
Firing:   f fire | r/nofire ready without shots | e end WHOLE turn
""".strip()


@dataclass
class Action:
    """Result of parsing a line."""

    orders: list[dict[str, Any]] = field(default_factory=list)
    side: Optional[str] = None  # help, status, quit, empty, unknown, hint, ...
    note: Optional[str] = None


def weapon_short_alias(weapon_id: str, kind: str = "") -> str:
    """beam_1 → b1, torp_1 → t1, plasma_1 → p1."""
    wid = weapon_id.lower()
    parts = wid.split("_")
    num = parts[-1] if parts and parts[-1].isdigit() else ""
    kind_l = (kind or "").lower()
    if kind_l:
        letter = kind_l[0]
    elif parts:
        letter = parts[0][0]
    else:
        letter = wid[0] if wid else "?"
    return f"{letter}{num}" if num else letter


def build_weapon_aliases(weapon_meta: list[dict[str, Any]]) -> dict[str, str]:
    """Map shortcut → full weapon id (lowercase keys)."""
    aliases: dict[str, str] = {}
    for m in weapon_meta:
        wid = str(m["id"])
        kind = str(m.get("kind") or "")
        aliases[wid.lower()] = wid
        aliases[wid.replace("_", "").lower()] = wid
        short = weapon_short_alias(wid, kind)
        aliases[short.lower()] = wid
        # also kind+number: beam1
        parts = wid.split("_")
        if len(parts) >= 2 and parts[-1].isdigit():
            aliases[(parts[0] + parts[-1]).lower()] = wid
    return aliases


def ships_still_to_allocate(snap: dict[str, Any]) -> list[dict[str, Any]]:
    """Player ships that have not finished allocate this turn."""
    done = set(snap.get("ships_allocated_this_turn") or [])
    return [
        s
        for s in living_player_ships(snap)
        if int(s["id"]) not in done
    ]


@dataclass
class ReplContext:
    """Sticky UI state (not engine state)."""

    selected: Optional[int] = None
    draft: Optional["AllocDraft"] = None
    # Nested draft group: None | "w" | "sh" | "mov"
    draft_group: Optional[str] = None
    hull_max: dict[int, int] = field(default_factory=dict)
    # For delta lines after orders
    last_fingerprint: Optional[str] = None

    def note_hull(self, snap: dict[str, Any]) -> None:
        for s in snap.get("ships") or []:
            sid = int(s["id"])
            hull = int(s.get("structure") or 0)
            prev = self.hull_max.get(sid, 0)
            if hull > prev:
                self.hull_max[sid] = hull

    def ensure_selected(self, snap: dict[str, Any]) -> Optional[int]:
        if self.selected is not None:
            ship = ship_by_id(snap, self.selected)
            if ship and not ship.get("destroyed"):
                return self.selected
        players = living_player_ships(snap)
        if players:
            self.selected = int(players[0]["id"])
            return self.selected
        return None

    def open_alloc_draft(self, snap: dict[str, Any], ship_id: int) -> str:
        ship = ship_by_id(snap, ship_id)
        if ship is None:
            return f"  no ship #{ship_id}"
        if ship.get("controller") != "player":
            return f"  #{ship_id} is {ship.get('controller')}, not player"
        done = set(snap.get("ships_allocated_this_turn") or [])
        if ship_id in done:
            return f"  #{ship_id} already allocated this turn"
        if snap.get("phase") != "allocate":
            return f"  not allocate phase (phase={snap.get('phase')})"

        # Never wipe a draft that already has points (bare "1" used to re-open empty).
        if self.draft is not None:
            if self.draft.ship_id == ship_id:
                return (
                    f"  draft already open for #{ship_id} "
                    f"(used={self.draft.used()}/{self.draft.power}) — "
                    f"edit, commit, cancel, or reset\n"
                    + self.draft.summary()
                )
            if self.draft.used() > 0:
                return (
                    f"  finish or cancel draft for #{self.draft.ship_id} first "
                    f"(used={self.draft.used()})"
                )

        self.selected = ship_id
        self.draft = AllocDraft.from_ship(ship)
        self.draft_group = None
        return (
            f"  allocate ship #{ship_id} {ship.get('class')} "
            f"(local draft — commit to apply)\n"
            + self.draft.summary()
            + "\n  tip: mov 6 | w then b1 2 | sh then 0 3 | commit\n"
            + "  (a lone number sets movement; it does NOT re-pick the ship)"
        )

    def select(self, snap: dict[str, Any], ship_id: int) -> str:
        ship = ship_by_id(snap, ship_id)
        if ship is None:
            return f"  no ship #{ship_id}"
        # Re-selecting the already-focused ship is a clean no-op (UX_ANALYSIS.md
        # §3b): no warning, no draft re-open, just confirm current focus.
        if self.selected == ship_id and self.draft is None:
            return f"  focus already on {ship_callsign(ship)}"
        # While drafting, bare ids must not clobber the draft — use ship/a explicitly.
        if self.draft is not None and self.draft.used() > 0:
            self.selected = ship_id
            return (
                f"  focus noted {ship_callsign(ship)}, but draft for "
                f"{self.draft.ship_id} is still open (used={self.draft.used()}). "
                f"commit/cancel/reset that draft first."
            )
        self.selected = ship_id
        msg = f"  focus → {ship_callsign(ship)} {ship.get('class')} ({ship.get('controller')})"
        if ship.get("controller") != "player":
            msg += "  (observer: cannot order)"
        if (
            snap.get("phase") == "allocate"
            and ship.get("controller") == "player"
            and not ship.get("destroyed")
        ):
            done = set(snap.get("ships_allocated_this_turn") or [])
            if ship_id not in done:
                return self.open_alloc_draft(snap, ship_id)
            msg += " (already allocated this turn)"
        return msg

    def begin_allocate_picker(self, snap: dict[str, Any]) -> str:
        """List ships needing allocate; auto-open if exactly one."""
        pending = ships_still_to_allocate(snap)
        if not pending:
            return "  no player ships left to allocate"
        if len(pending) == 1:
            return self.open_alloc_draft(snap, int(pending[0]["id"]))
        lines = ["  allocate which ship?"]
        for s in pending:
            lines.append(f"    {s.get('id')}: {s.get('class')} pwr={s.get('power')}")
        lines.append("  type a ship id, or: a <id>")
        return "\n".join(lines)


def phase_hint(snap: dict[str, Any], ctx: ReplContext) -> str:
    if snap.get("status") in ("Won", "Lost"):
        return f"scenario {snap.get('status')}: quit exits; log shows session history"
    phase = str(snap.get("phase") or "")
    focus = ctx.selected
    foc = f" focus=#{focus}" if focus is not None else ""
    if phase == "allocate":
        pending = [int(s["id"]) for s in ships_still_to_allocate(snap)]
        if ctx.draft:
            g = f" group={ctx.draft_group}" if ctx.draft_group else ""
            return (
                f"allocate draft #{ctx.draft.ship_id}{g}: "
                f"mov/w/sh … commit  (pending ships={pending})"
            )
        return (
            f"allocate:{foc}  a = pick ship {pending} then mov/w/sh … commit"
        )
    if phase == "movement":
        active = snap.get("active_ship")
        return (
            f"movement: ACTIVE=#{active}{foc}  "
            f"m <0-5> | m f/r/port/stbd | pass"
        )
    if phase == "firing":
        ready = snap.get("ships_ready_fire") or []
        # Detect whether the focused ship has any charged weapons, so the hint
        # can steer the player toward `r` when there's nothing to fire
        # (UX_ANALYSIS.md §3d/§2a — avoid a dead-end `f` prompt).
        me = ship_by_id(snap, focus) if focus is not None else None
        has_charge = False
        if me:
            has_charge = any(
                int(w.get("charge") or 0) > 0 and not w.get("fired")
                for w in (me.get("weapons") or [])
            )
        if not has_charge:
            return (
                f"firing:{foc}  no charged weapons — r to ready (skip fire) | "
                f"ready={ready}  (e = whole turn)"
            )
        return (
            f"firing:{foc}  weapon menu opens if you have charge; "
            f"f again for more shots | r/nofire when done | ready={ready}  "
            f"(e = whole turn)"
        )
    if phase == "turn_end":
        # turn_end is a brief bookkeeping phase: the engine regenerates shields
        # and clears weapon charge for the next turn. The delta line above shows
        # what changed; here we just prompt to advance (UX_ANALYSIS.md §7d).
        return (
            f"turn_end:{foc}  fire resolved — shields/charge reset for next turn. "
            f"end to advance"
        )
    return f"phase={phase}{foc}"


@dataclass
class AllocDraft:
    ship_id: int
    ship_class: str
    power: int
    max_shield: int
    weapon_meta: list[dict[str, Any]] = field(default_factory=list)
    aliases: dict[str, str] = field(default_factory=dict)
    movement: int = 0
    weapons: dict[str, int] = field(default_factory=dict)
    shields: list[int] = field(default_factory=lambda: [0, 0, 0, 0, 0, 0])
    dead_weapons: set[str] = field(default_factory=set)

    @classmethod
    def from_ship(cls, ship: dict[str, Any]) -> "AllocDraft":
        weapons = {
            str(w.get("id")): 0
            for w in (ship.get("weapons") or [])
            if w.get("operational", True)
        }
        meta = [
            {
                "id": str(w.get("id")),
                "kind": w.get("kind"),
                "max_charge": int(w.get("max_charge") or 0),
            }
            for w in (ship.get("weapons") or [])
            if w.get("operational", True)
        ]
        dead_weapons = {
            str(w.get("id"))
            for w in (ship.get("weapons") or [])
            if not w.get("operational", True)
        }
        aliases = build_weapon_aliases(meta)
        # Add aliases for dead weapons so we can resolve them and report "destroyed"
        for wid in dead_weapons:
            kind = next(
                (w.get("kind", "") for w in (ship.get("weapons") or []) if str(w.get("id")) == wid),
                ""
            )
            aliases[wid.lower()] = wid
            aliases[wid.replace("_", "").lower()] = wid
            short = weapon_short_alias(wid, kind)
            aliases[short.lower()] = wid
        pa = ship.get("power_available")
        pool = int(pa if pa is not None else (ship.get("power") or 0))
        return cls(
            ship_id=int(ship["id"]),
            ship_class=str(ship.get("class") or "?"),
            power=pool,
            max_shield=int(ship.get("max_shield_per_facing") or 0),
            weapon_meta=meta,
            aliases=aliases,
            weapons=weapons,
            dead_weapons=dead_weapons,
        )

    def resolve_weapon(self, token: str) -> Optional[str]:
        return self.aliases.get(token.lower().replace("-", "_"))

    def used(self) -> int:
        return (
            int(self.movement)
            + sum(int(v) for v in self.weapons.values())
            + sum(int(v) for v in self.shields)
        )

    def free(self) -> int:
        return self.power - self.used()

    def reset(self) -> None:
        self.movement = 0
        self.weapons = {k: 0 for k in self.weapons}
        self.shields = [0, 0, 0, 0, 0, 0]

    def weapon_menu(self) -> str:
        lines = ["  weapons (shortcut → charge):"]
        for m in self.weapon_meta:
            wid = m["id"]
            short = weapon_short_alias(wid, str(m.get("kind") or ""))
            ch = int(self.weapons.get(wid, 0))
            mx = max(int(m["max_charge"]), 1)
            lines.append(
                f"    {short:4} {wid:10} {bar(ch, mx)} {ch}/{m['max_charge']}  "
                f"({m.get('kind')})"
            )
        lines.append("  set: t1 1   or   b1 2   |  done leaves group")
        return "\n".join(lines)

    def shield_menu(self) -> str:
        lines = ["  shields (face power 0..max):"]
        for i, lab in enumerate(SHIELD_LABELS):
            v = self.shields[i]
            mx = max(self.max_shield, 1)
            lines.append(f"    {i}:{lab:2} {bar(v, mx)} {v}/{self.max_shield}")
        lines.append("  set: 0 3   or   F 2   |  done leaves group")
        return "\n".join(lines)

    def summary(self) -> str:
        used, free = self.used(), self.free()
        over = "  ** OVER **" if free < 0 else ""
        lines = [
            f"  draft #{self.ship_id} {self.ship_class}  "
            f"pool={self.power} used={used} free={free}{over}",
            f"  total {bar(used, self.power)}",
            f"  mov    {bar(self.movement, max(self.power, 1))} {self.movement}",
            "  weapons:",
        ]
        for m in self.weapon_meta:
            wid = m["id"]
            short = weapon_short_alias(wid, str(m.get("kind") or ""))
            ch = int(self.weapons.get(wid, 0))
            mx = max(int(m["max_charge"]), 1)
            lines.append(
                f"    {short:4} {wid:10} {bar(ch, mx)} {ch}/{m['max_charge']}"
            )
        lines.append("  shields:")
        for i, lab in enumerate(SHIELD_LABELS):
            v = self.shields[i]
            mx = max(self.max_shield, 1)
            lines.append(f"    {i}:{lab:2} {bar(v, mx)} {v}/{self.max_shield}")
        if free > 0:
            lines.append(f"  ⚠ {free} unspent power — ready a weapon, move, or shield")
        return "\n".join(lines)

    def to_order(self) -> Optional[dict[str, Any]]:
        if self.free() < 0:
            print(
                f"  cannot commit: used {self.used()} > pool {self.power}. "
                "reset or lower values."
            )
            return None
        return {
            "protocol_version": PROTOCOL_VERSION,
            "type": "allocate",
            "ship": self.ship_id,
            "movement": int(self.movement),
            "weapons": {k: int(v) for k, v in self.weapons.items()},
            "shields": [int(x) for x in self.shields],
        }

    def set_weapon(self, token: str, n: int) -> bool:
        wid = self.resolve_weapon(token)
        if wid is None:
            print(f"  unknown weapon {token!r}")
            print(self.weapon_menu())
            return False
        # Check if it's a destroyed/non-operational weapon
        if wid in self.dead_weapons:
            print(f"  weapon {token!r} is destroyed or not operational")
            return False
        meta = next((m for m in self.weapon_meta if m["id"] == wid), None)
        max_c = int(meta["max_charge"]) if meta else n
        clamped = max(0, min(int(n), max_c))
        if clamped != int(n):
            print(f"  clamp: {token} charge {n} → max {max_c}")
        self.weapons[wid] = clamped
        return True

    def set_shield(self, face_tok: str, n: int) -> bool:
        face = _face_index(face_tok)
        if face is None:
            print("  bad face; use 0-5 or F/FR/RR/R/RL/FL")
            return False
        clamped = max(0, min(int(n), self.max_shield))
        if clamped != int(n):
            print(f"  clamp: shield {face} power {n} → max {self.max_shield}")
        self.shields[face] = clamped
        return True


def _face_index(token: str) -> Optional[int]:
    t = token.strip().upper()
    if t.isdigit():
        i = int(t)
        return i if 0 <= i <= 5 else None
    try:
        return SHIELD_LABELS.index(t)
    except ValueError:
        return None


def _prompt_int(msg: str, default: int = 0, hint: str | None = None) -> int:
    """Prompt for an integer.

    Renders as ``{msg} {hint} [{default}]:`` when a hint is given, else
    ``{msg} [{default}]:``. Pass ``msg`` as the full desired prefix (including
    any indent); the hint replaces nothing — both hint and default are shown.
    """
    while True:
        if hint:
            raw = input(f"{msg} {hint} [{default}]: ").strip()
        else:
            raw = input(f"{msg} [{default}]: ").strip()
        if raw == "":
            return default
        try:
            return int(raw)
        except ValueError:
            print("  need an integer")


def _order(typ: str, **fields: Any) -> dict[str, Any]:
    body = {"protocol_version": PROTOCOL_VERSION, "type": typ}
    body.update(fields)
    return body


def default_allocate(snap: dict[str, Any], ship_id: int) -> Optional[dict[str, Any]]:
    ship = ship_by_id(snap, ship_id)
    if ship is None:
        print(f"  ship #{ship_id} not found")
        return None
    draft = AllocDraft.from_ship(ship)
    pool = draft.power

    # Charge the first beam to its max
    for m in draft.weapon_meta:
        if str(m.get("kind", "")).lower() == "beam" and int(m.get("max_charge") or 0) >= 1:
            max_c = int(m.get("max_charge") or 0)
            draft.weapons[m["id"]] = max_c
            pool -= max_c
            break

    # Allocate meaningful forward shield (face 0)
    max_shield = draft.max_shield
    forward_shield = min(2, max_shield, pool)
    draft.shields[0] = forward_shield
    pool -= forward_shield

    # Rest goes to movement (graceful degradation on small pools)
    draft.movement = max(0, pool)

    print(draft.summary())
    return draft.to_order()


def plan_absolute_move(
    snap: dict[str, Any], ship_id: int, abs_dir: int
) -> tuple[list[dict[str, Any]], str]:
    """
    Absolute map direction 0..5 — **one engine order only**.

    Combat v2 allows a single Move/Pass decision per ship per movement phase.
    Batching turn+turn+forward in one `m N` was a regression: the first turn
    consumed the decision, AI finished the phase, and later steps failed or
    left the player in firing with a confusing soft-error.

    If not aligned: emit one turn toward the target dir and tell the player to
    press `m N` again. If aligned: one forward or reverse step.
    """
    ship = ship_by_id(snap, ship_id)
    if ship is None:
        return [], f"ship #{ship_id} not found"
    facing = int(ship.get("facing") or 0) % 6
    abs_dir %= 6
    rev = (facing + 3) % 6

    if abs_dir == facing:
        return (
            [_order("move", ship=ship_id, mode="forward")],
            f"step absolute {abs_dir} via forward (face={facing})",
        )
    if abs_dir == rev:
        return (
            [_order("move", ship=ship_id, mode="reverse")],
            f"step absolute {abs_dir} via reverse (face={facing})",
        )

    mode = turn_toward(facing, abs_dir)
    if mode == "forward":
        return [], f"could not plan turn from face {facing} toward {abs_dir}"
    # Predict face after this single turn for the note.
    if mode == "turn_starboard":
        new_face = (facing + 1) % 6
    else:
        new_face = (facing + 5) % 6
    return (
        [_order("move", ship=ship_id, mode=mode)],
        f"turn toward absolute {abs_dir} ({mode}); face {facing}→{new_face}. "
        f"This uses your movement decision for this phase — next phase or "
        f"next turn to step; or if you still have another movement phase, "
        f"m {abs_dir} again when active.",
    )


# Relative move aliases (single order).
REL_MOVE = {
    "f": "forward",
    "fwd": "forward",
    "forward": "forward",
    "0rel": "forward",
    "rel0": "forward",
    "r": "reverse",
    "rev": "reverse",
    "reverse": "reverse",
    "3rel": "reverse",
    "rel3": "reverse",
    "port": "turn_port",
    "p": "turn_port",
    "left": "turn_port",
    "l": "turn_port",
    "starboard": "turn_starboard",
    "stbd": "turn_starboard",
    "sb": "turn_starboard",
    "right": "turn_starboard",
}


def _handle_draft_line(ctx: ReplContext, tokens: list[str]) -> Optional[Action]:
    """If a draft is open, try to handle as draft command. None = not draft cmd."""
    if ctx.draft is None:
        return None
    d = ctx.draft
    cmd = tokens[0].lower()
    args = tokens[1:]

    # Leave nested group
    if cmd in ("done", "..", "back", "up") and ctx.draft_group:
        ctx.draft_group = None
        print("  (back to draft root)")
        print(d.summary())
        return Action(side="empty")

    if cmd in ("show", "status", "st"):
        print(d.summary())
        if ctx.draft_group == "w":
            print(d.weapon_menu())
        elif ctx.draft_group == "sh":
            print(d.shield_menu())
        return Action(side="empty")
    if cmd in ("reset", "undo", "clear", "u"):
        d.reset()
        print("  draft cleared")
        print(d.summary())
        return Action(side="empty")
    if cmd in ("cancel", "abort", "x"):
        ctx.draft = None
        ctx.draft_group = None
        print("  draft discarded")
        return Action(side="empty")
    if cmd in ("commit", "ok", "c", "apply"):
        if d.used() == 0:
            print(
                "  draft is empty (all zeros). That skips movement and leaves "
                "weapons uncharged.\n"
                "  Set mov / w / sh first, or type yes to commit zeros:"
            )
            confirm = input("  commit empty allocate? [yes/N]: ").strip().lower()
            if confirm not in ("y", "yes"):
                print("  commit cancelled — draft still open")
                print(d.summary())
                return Action(side="empty")
        order = d.to_order()
        if order is None:
            return Action(side="empty")
        print("  committing to engine:\n" + d.summary())
        return Action(orders=[order])

    if cmd in ("help",):
        print(
            "  draft: mov N | w [alias N] | sh [face N] | show | reset | commit | cancel\n"
            "  in weapons group: b1 2 / t1 1  (no leading w) | done\n"
            "  in shields group: 0 3 / F 2 | done\n"
            "  bare number at draft root = movement power (not ship select)\n"
            "  for global help (status, board, quit): type 'help' again or 'quit' to exit"
        )
        return Action(side="empty")

    # ── movement sub-mode (mov then number on next line) ─────────────
    if ctx.draft_group == "mov":
        if cmd.isdigit():
            d.movement = max(0, int(cmd))
            ctx.draft_group = None
            print(d.summary())
            return Action(side="empty")
        if cmd in ("done", "..", "back", "cancel"):
            ctx.draft_group = None
            print(d.summary())
            return Action(side="empty")
        print("  enter movement as a number, or done")
        return Action(side="empty")

    # Bare number at draft root = movement power (NOT ship select).
    if cmd.isdigit() and ctx.draft_group is None:
        d.movement = max(0, int(cmd))
        print(f"  movement set to {d.movement}  (use ship N / a N to change focus)")
        print(d.summary())
        return Action(side="empty")

    # ── nested weapons group ──────────────────────────────────────────
    if ctx.draft_group == "w":
        if cmd.isdigit():
            print("  need a weapon id first (e.g. b1 2), not a bare number")
            print(d.weapon_menu())
            return Action(side="empty")
        # bare alias → prompt value; alias N → set
        if d.resolve_weapon(cmd) is not None:
            if not args:
                wid = d.resolve_weapon(cmd)
                meta = next((m for m in d.weapon_meta if m["id"] == wid), None)
                mx = int(meta["max_charge"]) if meta else 0
                n = _prompt_int(f"  charge {cmd} (0..{mx})", int(d.weapons.get(wid or "", 0)))
                d.set_weapon(cmd, n)
            else:
                if not args[0].lstrip("-").isdigit():
                    print("  usage: t1 2")
                    return Action(side="empty")
                d.set_weapon(cmd, int(args[0]))
            print(d.summary())
            print(d.weapon_menu())
            return Action(side="empty")
        if cmd in ("w", "weapon") and args:
            # still allow w t1 1 inside group
            if len(args) >= 2 and args[1].lstrip("-").isdigit():
                d.set_weapon(args[0], int(args[1]))
                print(d.summary())
                print(d.weapon_menu())
                return Action(side="empty")
        print("  weapons group — set with b1 2, or done")
        print(d.weapon_menu())
        return Action(side="empty")

    # ── nested shields group ──────────────────────────────────────────
    if ctx.draft_group == "sh":
        face = _face_index(cmd)
        if face is not None:
            if not args:
                n = _prompt_int(
                    f"  shield {face}:{SHIELD_LABELS[face]} (0..{d.max_shield})",
                    d.shields[face],
                )
                d.shields[face] = max(0, min(n, d.max_shield))
            else:
                if not args[0].lstrip("-").isdigit():
                    print("  usage: 0 3")
                    return Action(side="empty")
                d.set_shield(cmd, int(args[0]))
            print(d.summary())
            print(d.shield_menu())
            return Action(side="empty")
        print("  shields group — set with 0 3 or F 2, or done")
        print(d.shield_menu())
        return Action(side="empty")

    # ── draft root ────────────────────────────────────────────────────
    # Enter groups
    if cmd in ("w", "weapon", "weap", "weapons") and not args:
        ctx.draft_group = "w"
        print(d.weapon_menu())
        return Action(side="empty")
    if cmd in ("sh", "shield", "shields") and not args:
        ctx.draft_group = "sh"
        print(d.shield_menu())
        return Action(side="empty")

    # mov alone → await bare number on next line; mov N / m N immediate
    if cmd in ("mov", "movement") and not args:
        ctx.draft_group = "mov"
        print(
            f"  movement power? type a number (currently {d.movement}, "
            f"free pool {d.free() + d.movement})"
        )
        return Action(side="empty")

    # mov N / m N (integer only — not map move)
    if cmd in ("mov", "movement") or (
        cmd in ("m", "move") and args and args[0].lstrip("-").isdigit()
    ):
        if not args or not args[0].lstrip("-").isdigit():
            print("  usage: mov N   (or: mov  →  then a number)")
            return Action(side="empty")
        val = int(args[0])
        if val < 0:
            print("  negative movement is not allowed (clamped to 0)")
        d.movement = max(0, val)
        print(d.summary())
        return Action(side="empty")

    # w t1 1 / w beam_1 2 at root
    if cmd in ("w", "weapon", "weap", "charge") and args:
        if len(args) == 1:
            # enter group focused, or set with prompt
            if d.resolve_weapon(args[0]):
                ctx.draft_group = "w"
                wid = d.resolve_weapon(args[0])
                meta = next((m for m in d.weapon_meta if m["id"] == wid), None)
                mx = int(meta["max_charge"]) if meta else 0
                n = _prompt_int(f"  charge {args[0]} (0..{mx})", 0)
                d.set_weapon(args[0], n)
                print(d.summary())
                print(d.weapon_menu())
                return Action(side="empty")
            print("  usage: w t1 1  or just  w  to list")
            return Action(side="empty")
        if len(args) >= 2 and args[1].lstrip("-").isdigit():
            if d.set_weapon(args[0], int(args[1])):
                print(d.summary())
            return Action(side="empty")
        print("  usage: w t1 1")
        return Action(side="empty")

    # sh 0 3 at root
    if cmd in ("sh", "shield", "shields") and args:
        if len(args) == 1:
            face = _face_index(args[0])
            if face is None:
                print("  usage: sh 0 3")
                return Action(side="empty")
            n = _prompt_int(f"  shield {face} (0..{d.max_shield})", 0)
            d.shields[face] = max(0, min(n, d.max_shield))
            print(d.summary())
            return Action(side="empty")
        if len(args) >= 2 and args[1].lstrip("-").isdigit():
            if d.set_shield(args[0], int(args[1])):
                print(d.summary())
            return Action(side="empty")

    # Bare weapon shortcut at root (t1 2 without w) when unambiguous
    if d.resolve_weapon(cmd) is not None:
        if args and args[0].lstrip("-").isdigit():
            d.set_weapon(cmd, int(args[0]))
            print(d.summary())
            return Action(side="empty")
        if not args:
            ctx.draft_group = "w"
            wid = d.resolve_weapon(cmd)
            meta = next((m for m in d.weapon_meta if m["id"] == wid), None)
            mx = int(meta["max_charge"]) if meta else 0
            n = _prompt_int(f"  charge {cmd} (0..{mx})", int(d.weapons.get(wid or "", 0)))
            d.set_weapon(cmd, n)
            print(d.summary())
            print(d.weapon_menu())
            return Action(side="empty")

    return None


def interactive_fire(snap: dict[str, Any], ship_id: int) -> Optional[dict[str, Any]]:
    ship = ship_by_id(snap, ship_id)
    if ship is None:
        print(f"  ship #{ship_id} not found")
        return None
    # Already committed this phase (still show charge on ship until resolve).
    already = {
        str(c.get("weapon"))
        for c in (snap.get("fire_commits") or [])
        if int(c.get("ship") or -1) == int(ship_id)
    }
    charged = [
        w
        for w in (ship.get("weapons") or [])
        if w.get("operational", True)
        and not w.get("fired")
        and int(w.get("charge") or 0) > 0
        and str(w.get("id")) not in already
    ]
    if already:
        print(
            "  already queued this phase: "
            + ", ".join(sorted(already))
            + "  (still show charge until all ships ready_fire)"
        )
    if not charged:
        if already:
            print("  no more weapons to queue — ready/nofire when done committing")
        else:
            print("  no charged weapons — use ready/nofire to leave fire phase")
        return None
    enemies = [
        s for s in living_ships(snap)
        if s.get("id") != ship_id and s.get("controller") != ship.get("controller")
    ]
    if not enemies:
        print("  no targets")
        return None

    print("  weapons available to queue (not yet resolved):")
    for i, w in enumerate(charged):
        ch, mx = int(w.get("charge") or 0), int(w.get("max_charge") or 0)
        mount = str(w.get("mount") or w.get("arc") or "?")
        print(
            f"    [{i}] {w.get('id')} {bar(ch, max(mx,1))} {ch}/{mx} "
            f"rng≤{w.get('max_range')} arc={mount}"
        )
    wi = _prompt_int(" ", 0, hint="[-1] Done")
    if wi < 0 or wi >= len(charged):
        # "Done" finishes the fire phase for this ship directly: emit a
        # ready_fire order so the caller sends it and the phase ends,
        # instead of dropping the player back to the main prompt where they
        # would have to separately type r/ready/done.
        return _order("ready_fire", ship=ship_id)
    weapon = str(charged[wi]["id"])
    chosen = charged[wi]
    max_range = int(chosen.get("max_range") or 0)

    print("  targets:")
    legal_indices = set()
    for i, t in enumerate(enemies):
        rng = distance(
            int(ship["q"]), int(ship["r"]), int(t["q"]), int(t["r"])
        )
        in_arc = weapon_in_arc(
            chosen,
            int(ship["q"]),
            int(ship["r"]),
            int(ship.get("facing") or 0),
            int(t["q"]),
            int(t["r"]),
        )
        legal = legal_shield_facings(
            int(ship["q"]),
            int(ship["r"]),
            int(t["q"]),
            int(t["r"]),
            int(t.get("facing") or 0),
        )
        labs = ",".join(f"{x}:{SHIELD_LABELS[x]}" for x in legal)
        # Advisory legality flag from snapshot + geometry only.
        if rng > max_range:
            flag = "OUT OF RANGE"
        elif not in_arc:
            flag = "OUT OF ARC"
        else:
            flag = "in arc"
            legal_indices.add(i)
        # Sensor-limited: show position + facing + which shields bear on you,
        # but NOT the enemy's hull value or shield strength (internal state the
        # player cannot observe — UX_ANALYSIS.md §2f, §2g).
        print(
            f"    [{i}] {ship_callsign(t)} {t.get('class')} "
            f"@({t.get('q')},{t.get('r')}) rng={rng} "
            f"face={t.get('facing')}  "
            f"[{flag}]  shields facing you: {labs}"
        )
    # Auto-skip the target prompt when there's exactly one legal enemy
    # (UX_ANALYSIS.md §5b): no useful choice, so don't make them press enter.
    if not legal_indices:
        print("  no targets in range and arc for this weapon")
        return None
    legal_enemies = [enemies[i] for i in legal_indices]
    if len(legal_enemies) == 1:
        target = legal_enemies[0]
        rng = distance(
            int(ship["q"]), int(ship["r"]), int(target["q"]), int(target["r"])
        )
        print(
            f"  sole target: {ship_callsign(target)} {target.get('class')} "
            f"@({target.get('q')},{target.get('r')}) rng={rng} — auto-selected"
        )
    else:
        ti = _prompt_int(" ", 0, hint="[-1] Done")
        if ti < 0 or ti >= len(enemies):
            # "Done" finishes the fire phase for this ship directly (same as
            # the weapon-menu Done): emit ready_fire instead of dropping back.
            return _order("ready_fire", ship=ship_id)
        if ti not in legal_indices:
            print("  that target is not in range or arc")
            return None
        target = enemies[ti]
    legal = legal_shield_facings(
        int(ship["q"]),
        int(ship["r"]),
        int(target["q"]),
        int(target["r"]),
        int(target.get("facing") or 0),
    )
    default_face = legal[0] if legal else 0
    # Auto-skip the prompt when exactly one facing is legal (UX_ANALYSIS.md §3c):
    # no useful choice for the player, so don't make them press enter.
    if len(legal) == 1:
        face = legal[0]
        print(f"  shield facing {face}:{SHIELD_LABELS[face]} (only legal facing) — auto-selected")
        return _order(
            "commit_fire",
            ship=ship_id,
            weapon=weapon,
            target=int(target["id"]),
            shield_facing=face,
        )
    print(
        "  shield faces (legal marked *): "
        + " ".join(
            f"{'*' if i in legal else ' '}{i}:{SHIELD_LABELS[i]}"
            for i in range(6)
        )
    )
    for i in legal:
        # Sensor-limited: show which facings bear on you (geometry) but NOT the
        # enemy's shield remaining/powered values (internal state — §2g).
        print(f"    legal {i}:{SHIELD_LABELS[i]} (strength unknown)")
    facing = _prompt_int("  shield_facing", default_face)
    return _order(
        "commit_fire",
        ship=ship_id,
        weapon=weapon,
        target=int(target["id"]),
        shield_facing=facing,
    )


def build_action(line: str, snap: dict[str, Any], ctx: ReplContext) -> Action:
    line = line.strip()
    if not line:
        return Action(side="empty")

    try:
        tokens = shlex.split(line)
    except ValueError as exc:
        print(f"  parse error: {exc}")
        return Action(side="empty")

    phase = str(snap.get("phase") or "")

    # Draft commands first — bare numbers mean movement/shields, not ship pick.
    draft_act = _handle_draft_line(ctx, tokens)
    if draft_act is not None:
        return draft_act

    # Bare ship id → select (+ open draft in allocate if needed).
    # Only when NOT drafting (draft handler already claimed bare digits).
    if len(tokens) == 1 and tokens[0].isdigit():
        print(ctx.select(snap, int(tokens[0])))
        return Action(side="empty")

    # Allocate-phase shortcuts: open draft automatically if user starts assigning.
    if phase == "allocate" and ctx.draft is None:
        cmd0 = tokens[0].lower()
        if cmd0 in (
            "mov",
            "movement",
            "w",
            "weapon",
            "sh",
            "shield",
            "shields",
            "commit",
            "c",
            "ok",
        ) or (
            cmd0 in ("m", "move")
            and len(tokens) > 1
            and tokens[1].lstrip("-").isdigit()
        ):
            pending = ships_still_to_allocate(snap)
            if len(pending) == 1:
                print(ctx.open_alloc_draft(snap, int(pending[0]["id"])))
                draft_act = _handle_draft_line(ctx, tokens)
                if draft_act is not None:
                    return draft_act
            elif pending:
                print(ctx.begin_allocate_picker(snap))
                print("  (open a ship draft first, then mov/w/sh)")
                return Action(side="empty")

    cmd = tokens[0].lower()
    rest = tokens[1:]

    if cmd in ("help", "?", "h"):
        return Action(side="help")
    if cmd in ("hint", "what"):
        return Action(side="hint")
    if cmd in ("status", "s", "tactical", "tac"):
        return Action(side="status")
    if cmd in ("board", "b"):
        return Action(side="board")
    if cmd == "ships":
        return Action(side="ships")
    if cmd == "raw":
        return Action(side="raw")
    if cmd in ("quit", "exit"):
        return Action(side="quit")
    if cmd == "q" and not rest:
        return Action(side="quit")

    if cmd in ("ship", "sel", "focus", "select"):
        if not rest or not rest[0].isdigit():
            print("  usage: ship <id>")
            return Action(side="empty")
        print(ctx.select(snap, int(rest[0])))
        return Action(side="empty")

    # Allocate: pick ship, then draft — not a free-floating mode token
    if cmd in ("allocate", "a", "alloc"):
        if phase != "allocate":
            print(f"  allocate only in allocate phase (now {phase})")
            return Action(side="empty")
        if rest and rest[0].isdigit():
            print(ctx.open_alloc_draft(snap, int(rest[0])))
            return Action(side="empty")
        print(ctx.begin_allocate_picker(snap))
        return Action(side="empty")

    if cmd in ("alloc-default", "ad", "allocd"):
        if phase != "allocate":
            print(f"  allocate only in allocate phase (now {phase})")
            return Action(side="empty")
        if ctx.draft is not None and ctx.draft.used() > 0:
            print(f"  finish or cancel draft for #{ctx.draft.ship_id} first")
            return Action(side="empty")
        pending = ships_still_to_allocate(snap)
        sid = int(pending[0]["id"]) if len(pending) == 1 else ctx.ensure_selected(snap)
        if rest and rest[0].isdigit():
            sid = int(rest[0])
            ctx.selected = sid
        if sid is None:
            print("  no ship")
            return Action(side="empty")
        ship = ship_by_id(snap, sid)
        if ship is None or ship.get("controller") != "player" or ship.get("destroyed"):
            print(f"  cannot allocate ship #{sid}: not a living player ship")
            return Action(side="empty")
        order = default_allocate(snap, sid)
        return Action(orders=[order]) if order else Action(side="empty")

    if cmd in ("move", "m"):
        if phase != "movement":
            print(f"  cannot move: phase is {phase!r} (need movement)")
            return Action(side="empty")
        if not rest:
            print("  usage: m <0-5> | m f|r|port|stbd")
            return Action(side="empty")
        # Movement acts on ACTIVE ship (rules); focus is informational.
        active = snap.get("active_ship")
        ship_id = int(active) if active is not None else ctx.ensure_selected(snap)
        if ship_id is None:
            print("  no active ship")
            return Action(side="empty")
        ship = ship_by_id(snap, ship_id)
        if ship is None or ship.get("controller") != "player" or ship.get("destroyed"):
            print(f"  cannot move ship #{ship_id}: not a living player ship")
            return Action(side="empty")
        if active is not None and ctx.selected not in (None, int(active)):
            print(f"  note: moving ACTIVE #{active} (focus was #{ctx.selected})")

        token = rest[0].lower()
        if token.isdigit() and 0 <= int(token) <= 5:
            if phase != "movement":
                print(
                    f"  cannot move: phase is {phase!r} (need movement). "
                    f"Finish fire with ready/nofire, or end turn."
                )
                return Action(side="empty")
            orders, note = plan_absolute_move(snap, ship_id, int(token))
            print(f"  {note}")
            if not orders:
                return Action(side="empty")
            # One decision only — never batch turn+step (engine marks moved after one).
            return Action(orders=orders[:1], note=note)

        mode = REL_MOVE.get(token)
        if mode is None:
            print(f"  unknown move {token!r}; use 0-5 or f/r/port/stbd")
            return Action(side="empty")
        return Action(orders=[_order("move", ship=ship_id, mode=mode)])

    if cmd in ("pass", "pass_move"):
        if phase != "movement":
            print(f"  pass only in movement phase (now {phase})")
            return Action(side="empty")
        # bare "p" is pass; "p" alone
        active = snap.get("active_ship")
        ship_id = int(active) if active is not None else ctx.ensure_selected(snap)
        if rest and rest[0].isdigit():
            ship_id = int(rest[0])
        if ship_id is None:
            print("  no ship")
            return Action(side="empty")
        ship = ship_by_id(snap, ship_id)
        if ship is None or ship.get("controller") != "player" or ship.get("destroyed"):
            print(f"  cannot pass ship #{ship_id}: not a living player ship")
            return Action(side="empty")
        return Action(orders=[_order("pass_move", ship=ship_id)])

    if cmd == "p" and (not rest or rest[0].isdigit()):
        if phase != "movement":
            print(f"  pass only in movement phase (now {phase})")
            return Action(side="empty")
        # pass_move (not turn port — use m port for that)
        active = snap.get("active_ship")
        ship_id = int(active) if active is not None else ctx.ensure_selected(snap)
        if rest and rest[0].isdigit():
            ship_id = int(rest[0])
        if ship_id is None:
            print("  no ship")
            return Action(side="empty")
        ship = ship_by_id(snap, ship_id)
        if ship is None or ship.get("controller") != "player" or ship.get("destroyed"):
            print(f"  cannot pass ship #{ship_id}: not a living player ship")
            return Action(side="empty")
        return Action(orders=[_order("pass_move", ship=ship_id)])

    if cmd in ("fire", "f", "commit_fire"):
        if phase != "firing":
            print(f"  fire only in firing phase (now {phase})")
            return Action(side="empty")
        sid = ctx.ensure_selected(snap)
        if rest and rest[0].isdigit():
            sid = int(rest[0])
            ctx.selected = sid
        if sid is None:
            print("  select ship first")
            return Action(side="empty")
        ship = ship_by_id(snap, sid)
        if ship is None or ship.get("controller") != "player" or ship.get("destroyed"):
            print(f"  cannot fire ship #{sid}: not a living player ship")
            return Action(side="empty")
        # Defer to the REPL's looping fire offer so the player can fire
        # multiple weapons in one go (each commit refreshes the snapshot).
        return Action(side="fire_loop")

    if cmd in ("ready", "r", "ready_fire", "nofire", "no-fire", "skipfire", "skip", "done"):
        sid = ctx.ensure_selected(snap)
        if rest and rest[0].isdigit():
            sid = int(rest[0])
        if sid is None:
            print("  select ship first")
            return Action(side="empty")
        ship = ship_by_id(snap, sid)
        if ship is None or ship.get("controller") != "player" or ship.get("destroyed"):
            print(f"  cannot ready ship #{sid}: not a living player ship")
            return Action(side="empty")
        if phase != "firing":
            print(
                f"  ready_fire only in firing phase (now {phase}). "
                f"Finish movement first, or use m/p during movement."
            )
            return Action(side="empty")
        ready_already = set(snap.get("ships_ready_fire") or [])
        if sid in ready_already:
            print(
                f"  ship #{sid} is already ready — waiting for other ships. "
                f"ready so far={sorted(ready_already)}"
            )
            return Action(side="empty")
        print(
            f"  ready_fire #{sid} — done committing this fire phase "
            f"(phase ends when every living ship has readied)"
        )
        return Action(orders=[_order("ready_fire", ship=sid)])

    if cmd in ("end", "e", "end_turn"):
        if phase not in ("firing", "turn_end"):
            print(f"  end_turn only in firing or turn_end phase (now {phase})")
            return Action(side="empty")
        if phase == "firing":
            print(
                "  end_turn ends the WHOLE turn, not the fire phase.\n"
                "  To leave firing without shots: ready / nofire"
            )
            confirm = input("  type yes to end whole turn: ").strip().lower()
            if confirm not in ("y", "yes"):
                print("  cancelled")
                return Action(side="empty")
        return Action(orders=[_order("end_turn")])

    if cmd == "order":
        import json

        raw = line[len(tokens[0]) :].strip()
        try:
            obj = json.loads(raw)
        except json.JSONDecodeError as exc:
            print(f"  bad json: {exc}")
            return Action(side="empty")
        if not isinstance(obj, dict):
            print("  need object")
            return Action(side="empty")
        obj.setdefault("protocol_version", PROTOCOL_VERSION)
        return Action(orders=[obj])

    return Action(side="unknown")
