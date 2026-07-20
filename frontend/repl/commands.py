"""Interactive command parsing and order construction (protocol v4).

Ship-centric: select a focus ship once; allocate draft / path / volley / status
use it. Facing is always 0..5 (same numbering as the core).
"""

from __future__ import annotations

import shlex
import difflib
from dataclasses import dataclass, field
from typing import Any, Optional

from hexutil import (
    SHIELD_LABELS,
    distance,
    damage_preview,
    format_bar,
    hit_preview,
    legal_shield_facings,
    motion_status_bits,
    path_action_short,
    ship_callsign,
    weapon_in_arc,
)
from view import living_player_ships, living_ships, ship_by_id

PROTOCOL_VERSION = 4

# Wire path actions (protocol v4). Each costs exactly one motion point.
PATH_SHORTHAND: dict[str, str] = {
    "f": "move_f",
    "move_f": "move_f",
    "fr": "move_fr",
    "move_fr": "move_fr",
    "fl": "move_fl",
    "move_fl": "move_fl",
    "tr": "turn_right",
    "r": "turn_right",
    "right": "turn_right",
    "turn_right": "turn_right",
    "tl": "turn_left",
    "l": "turn_left",
    "left": "turn_left",
    "turn_left": "turn_left",
}

COMMAND_REGISTRY = {
    "status": ("status | s", "show the current board, turn, phase, focus, and ship state"),
    "board": ("board | b", "show the hex map and coordinate legend"),
    "ships": ("ships", "list every ship and its callsign, position, facing, and hull"),
    "help": ("help [command] | ?", "show commands, or detailed syntax and an example"),
    "allocate": (
        "allocate [ship-id] | a [ship-id]",
        "spend a ship's power on engine (motion), weapons, and shields",
    ),
    "path": (
        "path [f|fr|fl|tr|tl ...] | path commit",
        "draft a movement path, then commit once (engine path_preview for legality)",
    ),
    "fire": ("fire | attack | f", "add a shot to this ship's volley draft"),
    "ready": (
        "ready | nofire | r | commit",
        "submit the volley draft (empty = hold fire)",
    ),
    "quit": ("quit | q", "leave the game; confirms during an unfinished game"),
    "log": ("log", "toggle the session history panel"),
    "hint": ("hint", "repeat the next-action hint for the current phase"),
}

HELP_TOPICS = {
    **COMMAND_REGISTRY,
    "engine": ("engine N", "allocate N power to motion for this turn"),
    "weapon": ("w [weapon] N", "allocate charge to a weapon in the current ship draft"),
    "shield": ("sh [face] N", "allocate power to shield facing 0..5 in the current ship draft"),
    "commit": (
        "commit | c | ok",
        "apply allocate draft, commit_path, or commit_volley (by phase)",
    ),
    "preview": ("preview", "ask the engine path_preview for the drafted path"),
    "undo": ("undo", "drop the last path action or last volley shot"),
    "clear": ("clear", "clear the path or volley draft"),
    "hold": ("hold | p | pass", "commit an empty path (stay put; facing may still turn)"),
    "move": ("move | m | motion | path", "show motion pool and path drafting help"),
    "coast": ("hold | p | pass", "commit an empty path (stationary)"),
}

HELP_ALIASES = {
    "?": "help",
    "h": "help",
    "commands": "help",
    "s": "status",
    "b": "board",
    "a": "allocate",
    "alloc": "allocate",
    "m": "move",
    "motion": "move",
    "maneuvers": "move",
    "movement": "move",
    "p": "hold",
    "pass": "hold",
    "stay": "hold",
    "attack": "fire",
    "atk": "fire",
    "f": "fire",
    "r": "ready",
    "nofire": "ready",
    "no-fire": "ready",
    "q": "quit",
    "exit": "quit",
    "hist": "log",
    "history": "log",
    "w": "weapon",
    "weapons": "weapon",
    "sh": "shield",
    "shields": "shield",
    "c": "commit",
    "ok": "commit",
}


def _path_primer() -> str:
    """Static teaching block for help path / motion (protocol 4)."""
    return (
        "  Path flight (protocol 4):\n"
        "    • No velocity or course — only position + facing persist.\n"
        "    • One path per ship per turn; each action costs 1 motion point.\n"
        "    • Actions: f=move_f, fr=move_fr, fl=move_fl, tr/r=turn_right, tl/l=turn_left.\n"
        "    • Draft with `path f fr tl` or bare tokens; undo / clear / preview / commit.\n"
        "    • Empty path (hold/p/pass) stays put. Engine path_preview is authoritative."
    )


def render_help(command: str | None = None) -> str:
    """Generate help from the same registry used by the command surface."""
    if command:
        key = HELP_ALIASES.get(command.lower(), command.lower())
        if key not in HELP_TOPICS:
            suggestion = difflib.get_close_matches(
                key,
                list(HELP_TOPICS) + list(HELP_ALIASES),
                n=1,
                cutoff=0.45,
            )
            suffix = f" Did you mean '{suggestion[0]}'?" if suggestion else ""
            return f"  unknown help topic {command!r}.{suffix} Try: help"
        syntax, description = HELP_TOPICS[key]
        examples = {
            "allocate": "a 1, then engine 4, w b1 2, sh 0 3, commit",
            "move": "path f fr tl  then  commit",
            "path": "path f fr  |  path commit",
            "fire": "fire b1 B2   then  ready",
            "ready": "r",
            "quit": "quit",
            "engine": "engine 6",
            "weapon": "w b1 4",
            "shield": "sh 0 6",
            "commit": "commit",
            "preview": "preview",
            "undo": "undo",
            "clear": "clear",
            "hold": "hold",
            "coast": "hold",
        }
        example = examples.get(key, syntax.split(" | ")[0])
        body = f"  {syntax}\n    {description}\n    example: {example}"
        if key in ("move", "path", "hold", "coast", "preview"):
            body += "\n" + _path_primer()
        return body
    lines = [
        "shipsim REPL — objective: destroy the opposing fleet.",
        "The prompt shows turn, phase, focus, and remaining actions. Type help <command> for details.",
        "Commands:",
    ]
    for syntax, description in COMMAND_REGISTRY.values():
        lines.append(f"  {syntax:34} {description}")
    lines.append(
        "Allocate draft: engine N | w [weapon] N | sh [face] N | show | reset | commit | cancel"
    )
    lines.append(
        "Directions: 0→ 1↗ 2↖ 3← 4↙ 5↘; shields 0:F 1:FR 2:RR 3:R 4:RL 5:FL"
    )
    lines.append(
        "Turn flow: allocate → movement (path) → firing (volley) → next allocate "
        "(auto after all volleys; no end_turn)"
    )
    lines.append(
        "Flight: path f|fr|fl|tr|tl … | undo | clear | preview | commit | hold (empty path)"
    )
    lines.append(
        "Combat: fire adds shots to a volley; ready/nofire submits commit_volley once. "
        "d20 to-hit uses range and target size; charge affects damage"
    )
    return "\n".join(lines)


HELP = render_help()


@dataclass
class Action:
    """Result of parsing a line."""

    orders: list[dict[str, Any]] = field(default_factory=list)
    side: Optional[str] = None  # help, status, quit, empty, unknown, hint, ...
    note: Optional[str] = None
    # Optional read-only engine request (path_preview / fire_preview / …).
    request: Optional[dict[str, Any]] = None


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
    # Protocol v4 local drafts (not on the engine until commit).
    path_draft: list[str] = field(default_factory=list)
    path_ship: Optional[int] = None
    volley_draft: list[dict[str, Any]] = field(default_factory=list)
    volley_ship: Optional[int] = None

    def clear_path_draft(self) -> None:
        self.path_draft = []
        self.path_ship = None

    def clear_volley_draft(self) -> None:
        self.volley_draft = []
        self.volley_ship = None

    def ensure_path_ship(self, ship_id: int) -> None:
        if self.path_ship is not None and self.path_ship != ship_id and self.path_draft:
            # Switching ships discards the previous path draft.
            self.path_draft = []
        self.path_ship = ship_id

    def ensure_volley_ship(self, ship_id: int) -> None:
        if self.volley_ship is not None and self.volley_ship != ship_id and self.volley_draft:
            self.volley_draft = []
        self.volley_ship = ship_id

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
        self.selected = None
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
            + "\n  tip: engine 6 (power -> thrust) | w then b1 2 | sh then 0 3 | commit\n"
            + "  (a lone number sets movement; it does NOT re-pick the ship)"
        )

    def select(self, snap: dict[str, Any], ship_id: int) -> str:
        ship = ship_by_id(snap, ship_id)
        if ship is None:
            return f"  no ship #{ship_id}"
        if (
            snap.get("phase") == "allocate"
            and ship.get("controller") == "player"
            and not ship.get("destroyed")
            and ship_id not in set(snap.get("ships_allocated_this_turn") or [])
        ):
            return self.open_alloc_draft(snap, ship_id)
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
                f"engine/w/sh … commit  (pending ships={pending})"
            )
        return (
            f"allocate:{foc}  a = pick ship {pending} then engine/w/sh … commit"
        )
    if phase == "movement":
        pending = movement_pending_player_ids(snap)
        ship = ship_by_id(snap, focus) if focus is not None else None
        draft_s = ""
        if ctx.path_draft:
            short = " ".join(path_action_short(a) for a in ctx.path_draft)
            draft_s = f"  path draft[{len(ctx.path_draft)}]: {short}"
        if ship:
            sticky = motion_status_bits(ship)
            lines = [
                f"movement:{foc}  {sticky}  pending player ships={pending}",
                f"  position @({ship.get('q')},{ship.get('r')})",
                "  draft a path (f/fr/fl/tr/tl), then commit once; hold/p = empty path",
            ]
            if draft_s:
                lines.append(draft_s)
            return "\n".join(lines)
        return (
            f"movement: select a pending player ship {pending}; "
            "path f fr … then commit (motion = help)"
        )
    if phase == "firing":
        committed = snap.get("ships_committed_volley") or []
        pending = volley_pending_player_ids(snap)
        me = ship_by_id(snap, focus) if focus is not None else None
        has_charge = False
        if me:
            has_charge = any(
                int(w.get("charge") or 0) > 0 and not w.get("fired")
                for w in (me.get("weapons") or [])
            )
        draft_n = len(ctx.volley_draft)
        draft_s = f"  volley draft={draft_n} shot(s)" if draft_n else ""
        if not has_charge and draft_n == 0:
            return (
                f"firing:{foc}  no charged weapons — r/ready/nofire submits empty volley "
                f"(hold fire) | pending={pending} committed={committed}"
                f"{draft_s}"
            )
        return (
            f"firing:{foc}  fire/f adds shots to the volley; r/nofire/commit submits once | "
            f"pending={pending} committed={committed}"
            f"{draft_s}"
        )
    return f"phase={phase}{foc}"


def movement_pending_player_ids(snap: dict[str, Any]) -> list[int]:
    committed = {int(sid) for sid in snap.get("ships_committed_path") or []}
    return [
        int(s["id"])
        for s in living_player_ships(snap)
        if int(s["id"]) not in committed
    ]


def volley_pending_player_ids(snap: dict[str, Any]) -> list[int]:
    committed = {int(sid) for sid in snap.get("ships_committed_volley") or []}
    return [
        int(s["id"])
        for s in living_player_ships(snap)
        if int(s["id"]) not in committed
    ]


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
    # Protocol 4: charge carried into this allocate (cannot strip below).
    carried_weapons: dict[str, int] = field(default_factory=dict)
    shields: list[int] = field(default_factory=lambda: [0, 0, 0, 0, 0, 0])
    dead_weapons: set[str] = field(default_factory=set)

    @classmethod
    def from_ship(cls, ship: dict[str, Any]) -> "AllocDraft":
        # Seed draft from live charge so bars show carried power (protocol 4).
        weapons: dict[str, int] = {}
        carried: dict[str, int] = {}
        for w in ship.get("weapons") or []:
            if not w.get("operational", True):
                continue
            wid = str(w.get("id"))
            ch = int(w.get("charge") or 0)
            weapons[wid] = ch
            carried[wid] = ch
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
            carried_weapons=carried,
            dead_weapons=dead_weapons,
        )

    def resolve_weapon(self, token: str) -> Optional[str]:
        return self.aliases.get(token.lower().replace("-", "_"))

    def weapon_power_spent(self) -> int:
        """Power spent this draft on *new* weapon charge only (not carried)."""
        total = 0
        for wid, ch in self.weapons.items():
            base = int(self.carried_weapons.get(wid, 0))
            total += max(0, int(ch) - base)
        return total

    def used(self) -> int:
        # Protocol 4: carried charge does not consume the power pool.
        return (
            int(self.movement)
            + self.weapon_power_spent()
            + sum(int(v) for v in self.shields)
        )

    def free(self) -> int:
        return self.power - self.used()

    def reset(self) -> None:
        self.movement = 0
        self.weapons = {k: int(self.carried_weapons.get(k, 0)) for k in self.weapons}
        self.shields = [0, 0, 0, 0, 0, 0]

    def _weapon_line(self, wid: str, short: str, mx: int, kind: str = "") -> str:
        ch = int(self.weapons.get(wid, 0))
        base = int(self.carried_weapons.get(wid, 0))
        bar = format_bar(ch, max(mx, 1))
        if base > 0 and ch == base:
            tag = "  (carried — no pool cost)"
        elif base > 0 and ch > base:
            tag = f"  (carried {base} +{ch - base} new)"
        else:
            tag = f"  ({kind})" if kind else ""
        return f"    {short:4} {wid:10} {bar}{tag}"

    def weapon_menu(self) -> str:
        lines = ["  weapons (shortcut → charge; carried charge already shown):"]
        for m in self.weapon_meta:
            wid = m["id"]
            short = weapon_short_alias(wid, str(m.get("kind") or ""))
            mx = max(int(m["max_charge"]), 1)
            lines.append(self._weapon_line(wid, short, mx, str(m.get("kind") or "")))
        lines.append("  set: t1 1   or   b1 2   | cannot go below carried | done | sh = shields")
        return "\n".join(lines)

    def shield_menu(self) -> str:
        lines = ["  shields (face power 0..max):"]
        for i, lab in enumerate(SHIELD_LABELS):
            v = self.shields[i]
            mx = max(self.max_shield, 1)
            lines.append(f"    {i}:{lab:2} {format_bar(v, mx)}")
        lines.append("  set: 0 3   or   F 2   |  done leaves group  |  w = weapons")
        return "\n".join(lines)

    def summary(self) -> str:
        used, free = self.used(), self.free()
        over = "  ** OVER **" if free < 0 else ""
        pool = max(self.power, 1)
        lines = [
            f"  draft #{self.ship_id} {self.ship_class}  "
            f"pool={self.power} used={used} free={free}{over}",
            # format_bar always prints filled/total so scaled bars (pool>16) stay honest
            f"  total  {format_bar(used, self.power)}",
            f"  engine {format_bar(self.movement, pool)}  (→ motion pool for path)",
            "  weapons:",
        ]
        for m in self.weapon_meta:
            wid = m["id"]
            short = weapon_short_alias(wid, str(m.get("kind") or ""))
            mx = max(int(m["max_charge"]), 1)
            lines.append(self._weapon_line(wid, short, mx, str(m.get("kind") or "")))
        lines.append("  shields:")
        for i, lab in enumerate(SHIELD_LABELS):
            v = self.shields[i]
            mx = max(self.max_shield, 1)
            lines.append(f"    {i}:{lab:2} {format_bar(v, mx)}")
        carried_any = any(int(v) > 0 for v in self.carried_weapons.values())
        if carried_any:
            lines.append(
                "  pool math: used = engine + shields + new weapon charge only "
                "(carried weapon # do not spend the pool)"
            )
        if free > 0:
            lines.append(
                f"  ⚠ {free} free in pool — spend on engine / shields / topping weapons"
            )
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
        carried = int(self.carried_weapons.get(wid, 0))
        clamped = max(0, min(int(n), max_c))
        if clamped != int(n):
            print(f"  clamp: {token} charge {n} → max {max_c}")
        if clamped < carried:
            print(
                f"  cannot strip {token}: already carries {carried} from last turn "
                f"(requested {clamped}). Charge carries; only top-ups spend power."
            )
            return False
        current = int(self.weapons.get(wid, 0))
        # Power already counted for this weapon's increase can be reused when lowering.
        current_increase = max(0, current - carried)
        available = self.free() + current_increase
        new_increase = max(0, clamped - carried)
        if new_increase > available:
            print(
                f"  not enough free power for {token}: need +{new_increase} new charge, "
                f"only {available} available. Lower another allocation first."
            )
            return False
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
        available = self.free() + int(self.shields[face])
        if clamped > available:
            print(
                f"  not enough free power for shield {face}: requested {clamped}, "
                f"only {available} available. Lower another allocation first."
            )
            return False
        self.shields[face] = clamped
        return True

    def set_movement(self, n: int) -> bool:
        value = max(0, int(n))
        available = self.free() + int(self.movement)
        if value > available:
            print(
                f"  not enough free power for engine: requested {value}, "
                f"only {available} available. Lower another allocation first."
            )
            return False
        self.movement = value
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
        try:
            if hint:
                raw = input(f"{msg} {hint} [{default}]: ").strip()
            else:
                raw = input(f"{msg} [{default}]: ").strip()
        except (EOFError, KeyboardInterrupt):
            print()
            return default
        if raw == "":
            return default
        if raw.lower() in ("done", "r", "ready", "nofire"):
            return -1
        try:
            return int(raw)
        except ValueError:
            print(f"  enter an integer (or type done/-1); expected: {msg}")


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
    """Face an absolute direction via turn_left/turn_right path actions."""
    ship = ship_by_id(snap, ship_id)
    if ship is None:
        return [], f"ship #{ship_id} not found"
    face = int(ship.get("facing") or 0) % 6
    target = abs_dir % 6
    if face == target:
        return [
            _order("commit_path", ship=ship_id, actions=["move_f"])
        ], f"already facing {target}; move_f along nose"
    # After the engine handedness fix: turn_left = +1 (ccw, visual left),
    # turn_right = -1 (cw, visual right). (target - face) % 6 counts +1 steps.
    left_steps = (target - face) % 6
    right_steps = (face - target) % 6
    if left_steps <= right_steps:
        actions = ["turn_left"] * left_steps
        note = f"turn_left ×{left_steps} to face {target}"
    else:
        actions = ["turn_right"] * right_steps
        note = f"turn_right ×{right_steps} to face {target}"
    return [_order("commit_path", ship=ship_id, actions=actions)], note


def parse_path_token(token: str) -> Optional[str]:
    """Map shorthand / wire name to canonical path action, or None."""
    return PATH_SHORTHAND.get(token.strip().lower())


def _pending_movement_ship(
    snap: dict[str, Any], ctx: ReplContext, requested: Optional[int] = None
) -> Optional[dict[str, Any]]:
    committed = {int(sid) for sid in snap.get("ships_committed_path") or []}
    ship_id = requested
    if ship_id is None:
        pending = [
            s for s in living_player_ships(snap) if int(s["id"]) not in committed
        ]
        ship_id = int(pending[0]["id"]) if pending else ctx.ensure_selected(snap)
    ship = ship_by_id(snap, ship_id) if ship_id is not None else None
    if ship is None or ship.get("controller") != "player" or ship.get("destroyed"):
        return None
    if int(ship["id"]) in committed:
        return None
    ctx.selected = int(ship["id"])
    return ship


def _pending_volley_ship(
    snap: dict[str, Any], ctx: ReplContext, requested: Optional[int] = None
) -> Optional[dict[str, Any]]:
    committed = {int(sid) for sid in snap.get("ships_committed_volley") or []}
    ship_id = requested if requested is not None else ctx.ensure_selected(snap)
    ship = ship_by_id(snap, ship_id) if ship_id is not None else None
    if ship is None or ship.get("controller") != "player" or ship.get("destroyed"):
        return None
    if int(ship["id"]) in committed:
        return None
    ctx.selected = int(ship["id"])
    return ship


def movement_summary(ship: dict[str, Any], path_draft: Optional[list[str]] = None) -> str:
    """Full path-drafting help (`motion` / `m` / help path)."""
    motion = int(ship.get("motion_available") or 0)
    cap = int(ship.get("max_maneuver_actions") or 0)
    lines = [
        f"  ship #{ship['id']} path drafting",
        f"  {motion_status_bits(ship)}",
        f"  position @({ship.get('q')},{ship.get('r')})",
        f"  motion pool={motion}" + (f" (hull cap {cap})" if cap else ""),
        "  Each path action costs 1 motion: f | fr | fl | tr (r) | tl (l).",
        "  Draft with `path f fr tl`, bare tokens, then `commit` (or `path commit`).",
        "  `hold` / `p` / `pass` commits an empty path (stay put).",
        "  `preview` asks the engine path_preview; do not invent legality here.",
        "  `undo` pops last action; `clear` empties the draft.",
    ]
    if path_draft:
        short = " ".join(path_action_short(a) for a in path_draft)
        lines.append(f"  current draft ({len(path_draft)}): {short}")
        lines.append(f"  wire actions: {path_draft}")
    else:
        lines.append("  current draft: (empty)")
    return "\n".join(lines)


def path_draft_summary(ctx: ReplContext) -> str:
    if not ctx.path_draft:
        return "  path draft empty — hold/p commits stay; or path f fr …"
    short = " ".join(path_action_short(a) for a in ctx.path_draft)
    return f"  path draft ship=#{ctx.path_ship}: {short}  ({len(ctx.path_draft)} actions)"


def volley_draft_summary(ctx: ReplContext) -> str:
    if not ctx.volley_draft:
        return "  volley draft empty — r/nofire holds fire"
    lines = [f"  volley draft ship=#{ctx.volley_ship} ({len(ctx.volley_draft)} shots):"]
    for shot in ctx.volley_draft:
        face = int(shot.get("shield_facing") or 0)
        lab = SHIELD_LABELS[face] if 0 <= face < 6 else "?"
        lines.append(
            f"    {shot.get('weapon')} → #{shot.get('target')} shield={face}:{lab}"
        )
    return "\n".join(lines)


def _commit_path_order(ship_id: int, actions: list[str]) -> dict[str, Any]:
    return _order("commit_path", ship=ship_id, actions=list(actions))


def _commit_volley_order(ship_id: int, shots: list[dict[str, Any]]) -> dict[str, Any]:
    return _order(
        "commit_volley",
        ship=ship_id,
        shots=[
            {
                "weapon": str(s["weapon"]),
                "target": int(s["target"]),
                "shield_facing": int(s["shield_facing"]),
            }
            for s in shots
        ],
    )


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
                "  Set engine / w / sh first, or type yes to commit zeros:"
            )
            # `commit yes` is deliberately supported in addition to the
            # prompt.  It is clearer for people and does not consume an
            # unrelated queued command in a scripted/piped session.
            confirm = args[0].lower() if args else ""
            if confirm not in ("y", "yes"):
                try:
                    confirm = input("  commit empty allocate? [yes/N]: ").strip().lower()
                except (EOFError, KeyboardInterrupt):
                    print()
                    confirm = ""
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
            "  draft: engine N | w [alias N] | sh [face N] | show | reset | commit | cancel\n"
            "  in weapons group: b1 2 / t1 1  (no leading w) | done\n"
            "  in shields group: 0 3 / F 2 | done\n"
            "  group names switch directly: sh from weapons, w from shields, engine from either\n"
            "  bare number at draft root = engine power (not ship select)\n"
            "  for global help (status, board, quit): type 'help' again or 'quit' to exit"
        )
        return Action(side="empty")

    # Group names navigate from inside any group: `sh` while in weapons
    # jumps to shields, `w` back to weapons, `engine` to engine power —
    # clear the group and let the draft-root handlers below take the line.
    if ctx.draft_group is not None and cmd in (
        "w", "weapon", "weap", "weapons",
        "sh", "shield", "shields",
        "engine", "mov", "movement",
    ):
        if not (ctx.draft_group == "w" and d.resolve_weapon(cmd) is not None):
            ctx.draft_group = None

    # ── movement sub-mode (mov then number on next line) ─────────────
    if ctx.draft_group == "mov":
        if cmd.isdigit():
            d.set_movement(int(cmd))
            ctx.draft_group = None
            print(d.summary())
            return Action(side="empty")
        if cmd in ("done", "..", "back", "cancel"):
            ctx.draft_group = None
            print(d.summary())
            return Action(side="empty")
        print("  enter movement as a number, or done")
        return Action(side="empty")

    # Bare number at draft root = engine power (NOT ship select).
    if cmd.isdigit() and ctx.draft_group is None:
        d.set_movement(int(cmd))
        print(f"  engine power set to {d.movement}  (use ship N / a N to change focus)")
        print(d.summary())
        return Action(side="empty")

    # ── nested weapons group ──────────────────────────────────────────
    if ctx.draft_group == "w":
        if cmd.isdigit():
            print("  need a weapon id first (e.g. b1 2), not a bare number")
            print("  (done = back to draft root | sh = shields | engine = engine power)")
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
                d.set_shield(cmd, n)
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

    # engine alone → await bare number on next line; engine N / mov N immediate
    if cmd in ("engine", "mov", "movement") and not args:
        ctx.draft_group = "mov"
        print(
            f"  engine power → becomes this ship's motion pool for the path. "
            f"Type a number (currently {d.movement}, free pool {d.free() + d.movement})"
        )
        return Action(side="empty")

    # engine N / mov N (integer only — not map move)
    if cmd in ("engine", "mov", "movement") or (
        cmd in ("m", "move") and args and args[0].lstrip("-").isdigit()
    ):
        if not args or not args[0].lstrip("-").isdigit():
            print("  usage: engine N   (or: engine  →  then a number; sets motion power for the path)")
            return Action(side="empty")
        val = int(args[0])
        if val < 0:
            print("  negative movement is not allowed (clamped to 0)")
        d.set_movement(val)
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
            d.set_shield(args[0], n)
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


def interactive_fire(
    snap: dict[str, Any],
    ship_id: int,
    ctx: Optional[ReplContext] = None,
) -> Optional[dict[str, Any]]:
    """Interactive weapon picker for the volley draft.

    Returns:
      - a shot dict ``{weapon, target, shield_facing}`` to append to the draft
      - a full ``commit_volley`` order when the player chooses Done (-1)
      - None to leave the menu without committing
    """
    ship = ship_by_id(snap, ship_id)
    if ship is None:
        print(f"  ship #{ship_id} not found")
        return None
    already: set[str] = set()
    if ctx is not None:
        ctx.ensure_volley_ship(ship_id)
        already = {str(s.get("weapon")) for s in ctx.volley_draft}
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
            "  already in volley draft: "
            + ", ".join(sorted(already))
            + "  (submit with r/ready when finished)"
        )
    if not charged:
        if already:
            print("  no more weapons to queue — submitting volley")
            shots = list(ctx.volley_draft) if ctx else []
            if ctx:
                ctx.clear_volley_draft()
            return _commit_volley_order(ship_id, shots)
        print("  no charged weapons — use ready/nofire to hold fire (empty volley)")
        return None
    print("  FIRE WEAPON — choose a weapon for the volley draft, then a target")
    print("  tip: after a shot is drafted, choose another weapon or type r/ready/nofire")
    print("  one-line form works here too: fire b1 #2")
    enemies = [
        s for s in living_ships(snap)
        if s.get("id") != ship_id and s.get("controller") != ship.get("controller")
    ]
    if not enemies:
        print("  no targets")
        return None

    usable: list[dict[str, Any]] = []
    blocked: list[tuple[dict[str, Any], str]] = []
    for weapon in charged:
        reasons = []
        for target in enemies:
            rng = distance(int(ship["q"]), int(ship["r"]), int(target["q"]), int(target["r"]))
            if rng == 0:
                reasons.append("TOO CLOSE")
            elif rng > int(weapon.get("max_range") or 0):
                reasons.append("OUT OF RANGE")
            elif not weapon_in_arc(
                weapon,
                int(ship.get("q") or 0),
                int(ship.get("r") or 0),
                int(ship.get("facing") or 0),
                int(target.get("q") or 0),
                int(target.get("r") or 0),
            ):
                reasons.append("OUT OF ARC")
            else:
                reasons = []
                break
        if reasons:
            blocked.append((weapon, "/".join(sorted(set(reasons)))))
        else:
            usable.append(weapon)
    for weapon, reason in blocked:
        print(f"  {weapon.get('id')} unavailable — [{reason}] against all contacts")
    if not usable:
        print("  no charged weapons have a legal target; use r/ready/nofire to submit volley")
        return None
    charged = usable

    print("  weapons available for the volley draft:")
    for i, w in enumerate(charged):
        ch, mx = int(w.get("charge") or 0), int(w.get("max_charge") or 0)
        mount = str(w.get("mount") or w.get("arc") or "?")
        print(
            f"    [{i}] {w.get('id')} {format_bar(ch, max(mx,1))} "
            f"rng≤{w.get('max_range')} arc={mount}"
        )
    aliases = build_weapon_aliases([
        {"id": w.get("id"), "kind": w.get("kind")} for w in charged
    ])
    while True:
        choices = ", ".join(str(i) for i in range(len(charged)))
        try:
            raw = input(
                f"  Enter weapon number ({choices}), weapon name (b1), or -1 when done: "
            ).strip().lower()
        except (EOFError, KeyboardInterrupt):
            print()
            raw = "-1"
        if raw in ("", "-1", "done", "r", "ready", "nofire"):
            wi = -1
            break
        if raw in ("hold", "p", "help", "?", "status", "s", "board", "b"):
            print(f"  leaving weapon picker — {raw!r} is handled at the firing prompt")
            return None
        try:
            inline = shlex.split(raw)
        except ValueError:
            inline = []
        if len(inline) >= 3 and inline[0] in ("fire", "attack", "f"):
            shot = direct_fire(snap, ship_id, inline[1], inline[2], ctx=ctx)
            if shot:
                return shot
            continue
        if raw in ("f", "fire", "attack"):
            print("  weapon menu is already open; enter 0, b1, or -1 when done")
            continue
        if raw.isdigit() and int(raw) < len(charged):
            wi = int(raw)
            break
        wid = aliases.get(raw)
        if wid:
            wi = next(i for i, w in enumerate(charged) if str(w.get("id")) == wid)
            break
        names = ", ".join(
            weapon_short_alias(str(w.get("id")), str(w.get("kind") or ""))
            for w in charged
        )
        print(
            f"  invalid weapon choice {raw!r}; choose number {choices} or weapon {names}; "
            "one-line form: fire <weapon> <target>; -1 = submit volley"
        )
    if wi < 0 or wi >= len(charged):
        # Done: submit the volley draft (possibly empty).
        shots = list(ctx.volley_draft) if ctx else []
        if ctx:
            ctx.clear_volley_draft()
        print(
            f"  commit_volley #{ship_id} with {len(shots)} shot(s) "
            f"(empty = hold fire)"
        )
        return _commit_volley_order(ship_id, shots)
    weapon = str(charged[wi]["id"])
    chosen = charged[wi]
    print(f"  selected {weapon} — now choosing a target")
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
        if rng == 0:
            flag = "TOO CLOSE"
        elif rng > max_range:
            flag = "OUT OF RANGE"
        elif not in_arc:
            flag = "OUT OF ARC"
        else:
            flag = "in arc"
            legal_indices.add(i)
        target_size = int(t.get("size") or 2)
        preview = hit_preview(
            str(chosen.get("kind") or ""),
            rng,
            target_size,
            int(ship.get("attack_accuracy_bonus") or 0),
        )
        damage = damage_preview(
            str(chosen.get("kind") or ""), int(chosen.get("charge") or 0), rng
        )
        print(
            f"    [{i}] {ship_callsign(t)} {t.get('class')} "
            f"@({t.get('q')},{t.get('r')}) rng={rng} "
            f"face={t.get('facing')} size={target_size}  "
            f"[{flag}]  shields facing you: {labs}"
            + (
                f"  to-hit d20≤{preview[0]} ({preview[1]}%), damage≈{damage}"
                if preview and damage is not None
                else ""
            )
        )
    if not legal_indices:
        print("  no targets in range and arc for this weapon")
        return None
    legal_enemies = [enemies[i] for i in legal_indices]
    if len(enemies) == 1 and len(legal_enemies) == 1:
        target = legal_enemies[0]
        rng = distance(
            int(ship["q"]), int(ship["r"]), int(target["q"]), int(target["r"])
        )
        print(
            f"  sole target: {ship_callsign(target)} {target.get('class')} "
            f"@({target.get('q')},{target.get('r')}) rng={rng} — auto-selected"
        )
    else:
        choices = ", ".join(str(i) for i in sorted(legal_indices))
        while True:
            try:
                raw = input(
                    f"  Enter target number ({choices}), callsign (e.g. {ship_callsign(enemies[sorted(legal_indices)[0]])}), "
                    "or -1 when done: "
                ).strip().lower()
            except (EOFError, KeyboardInterrupt):
                print()
                raw = "-1"
            if raw in ("", "-1", "done", "r", "ready", "nofire"):
                ti = -1
                break
            if raw.isdigit() and int(raw) < len(enemies):
                ti = int(raw)
                break
            token = raw.lstrip("#").upper()
            match = next(
                (i for i, t in enumerate(enemies) if ship_callsign(t).upper() == token),
                None,
            )
            if match is not None:
                ti = match
                break
            print(f"  invalid target choice {raw!r}; type one of {choices}, a callsign, or -1")
        if ti < 0 or ti >= len(enemies):
            shots = list(ctx.volley_draft) if ctx else []
            if ctx:
                ctx.clear_volley_draft()
            return _commit_volley_order(ship_id, shots)
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
    if len(legal) == 1:
        face = legal[0]
        print(f"  shield facing {face}:{SHIELD_LABELS[face]} (only legal facing) — auto-selected")
        print(f"  drafted {weapon} at {ship_callsign(target)}; type r/ready/nofire to submit volley")
        return {
            "weapon": weapon,
            "target": int(target["id"]),
            "shield_facing": face,
        }
    print(
        "  shield faces (legal marked *): "
        + " ".join(
            f"{'*' if i in legal else ' '}{i}:{SHIELD_LABELS[i]}"
            for i in range(6)
        )
    )
    for i in legal:
        print(f"    legal {i}:{SHIELD_LABELS[i]} (strength unknown)")
    facing = _prompt_int("  shield_facing", default_face)
    print(f"  drafted {weapon} at {ship_callsign(target)}; type r/ready/nofire to submit volley")
    return {
        "weapon": weapon,
        "target": int(target["id"]),
        "shield_facing": facing,
    }


def _target_token(snap: dict[str, Any], token: str) -> Optional[dict[str, Any]]:
    """Resolve #2, 2, or B2 consistently with the callsign shown on screen."""
    raw = token.strip().lstrip("#").upper()
    for ship in living_ships(snap):
        if raw == str(ship.get("id")) or raw == ship_callsign(ship).upper():
            return ship
    return None


def direct_fire(
    snap: dict[str, Any],
    ship_id: int,
    weapon_token: str,
    target_token: str,
    ctx: Optional[ReplContext] = None,
) -> Optional[dict[str, Any]]:
    """Build a volley shot from the one-line form: fire b1 #2.

    Returns a shot dict (not a wire order). Caller appends to the volley draft.
    """
    attacker = ship_by_id(snap, ship_id)
    if attacker is None:
        print(f"  ship #{ship_id} not found")
        return None
    weapon = next(
        (
            w
            for w in (attacker.get("weapons") or [])
            if str(w.get("id")).lower() == weapon_token.lower()
            or weapon_short_alias(str(w.get("id")), str(w.get("kind") or "")).lower()
            == weapon_token.lower()
        ),
        None,
    )
    target = _target_token(snap, target_token)
    if weapon is None:
        known = ", ".join(
            weapon_short_alias(str(w.get("id")), str(w.get("kind") or ""))
            for w in attacker.get("weapons") or []
        )
        print(f"  unknown weapon {weapon_token!r}; use one of: {known}")
        return None
    if target is None or target.get("controller") == attacker.get("controller"):
        print(f"  unknown or friendly target {target_token!r}; use a contact such as B2 or #2")
        return None
    if not weapon.get("operational", True):
        print(f"  {weapon_token} is destroyed and cannot fire or recharge")
        return None
    if weapon.get("fired"):
        print(
            f"  {weapon_token} already fired this turn; "
            "recharge it during the next allocation phase"
        )
        return None
    if ctx is not None and any(
        str(s.get("weapon")) == str(weapon.get("id")) for s in ctx.volley_draft
    ):
        print(f"  {weapon_token} is already in the volley draft; r/ready to submit")
        return None
    rng = distance(
        int(attacker["q"]), int(attacker["r"]), int(target["q"]), int(target["r"])
    )
    if int(weapon.get("charge") or 0) <= 0:
        print(f"  {weapon_token} is not charged; allocate weapon power first")
        return None
    if rng == 0:
        print(
            f"  {weapon_token} cannot fire at range 0; "
            "overlapping ships are too close to target"
        )
        return None
    if rng > int(weapon.get("max_range") or 0):
        print(
            f"  {weapon_token} cannot reach {ship_callsign(target)}: "
            f"range {rng} > max {weapon.get('max_range')}"
        )
        return None
    if not weapon_in_arc(
        weapon,
        int(attacker["q"]),
        int(attacker["r"]),
        int(attacker.get("facing") or 0),
        int(target["q"]),
        int(target["r"]),
    ):
        print(
            f"  {weapon_token} cannot bear on {ship_callsign(target)} "
            f"from facing {attacker.get('facing')}; adjust path first"
        )
        return None
    legal = legal_shield_facings(
        int(attacker["q"]),
        int(attacker["r"]),
        int(target["q"]),
        int(target["r"]),
        int(target.get("facing") or 0),
    )
    if not legal:
        print(f"  no legal shield facing on {ship_callsign(target)}")
        return None
    target_size = int(target.get("size") or 2)
    preview = hit_preview(
        str(weapon.get("kind") or ""),
        rng,
        target_size,
        int(attacker.get("attack_accuracy_bonus") or 0),
    )
    if preview:
        damage = damage_preview(
            str(weapon.get("kind") or ""), int(weapon.get("charge") or 0), rng
        )
        damage_text = f", damage≈{damage}" if damage is not None else ""
        print(
            f"  {weapon.get('id')} → {ship_callsign(target)} size={target_size} "
            f"range={rng}: d20 ≤ {preview[0]} ({preview[1]}% preview{damage_text}); "
            "target size and range affect accuracy; charge affects damage"
        )
    return {
        "weapon": str(weapon["id"]),
        "target": int(target["id"]),
        "shield_facing": legal[0],
    }


def build_action(line: str, snap: dict[str, Any], ctx: ReplContext) -> Action:
    line = line.strip()
    if not line:
        print("  no command entered. Type help (or ?) to see commands; try hint for the next action.")
        return Action(side="empty")

    try:
        tokens = shlex.split(line)
    except ValueError as exc:
        print(f"  parse error: {exc}")
        return Action(side="empty")

    phase = str(snap.get("phase") or "")

    # Help is global, including while a draft or fire sub-dialog is active.
    # Keeping this before draft dispatch prevents the old context trap where
    # typing help twice could never reach the command index.
    if tokens and tokens[0].lower() in ("help", "?", "h", "commands"):
        topic = tokens[1] if len(tokens) > 1 else None
        if ctx.draft is not None:
            print("  global help: status, board, ships, log, hint, quit; use help <command> for syntax")
        return Action(side="help", note=topic)

    # Draft commands first — bare numbers mean movement/shields, not ship pick.
    draft_act = _handle_draft_line(ctx, tokens)
    if draft_act is not None:
        return draft_act

    # Bare ship id → select (+ open draft in allocate if needed).
    # Only when NOT drafting (draft handler already claimed bare digits).
    if len(tokens) == 1 and tokens[0].isdigit():
        if phase == "firing":
            print(
                "  numeric input is not ship selection during firing. "
                "Use fire/attack/f to choose a weapon, or r/ready/nofire to submit the volley. "
                "One-line example: fire b1 #2"
            )
            return Action(side="empty")
        if phase == "movement":
            # Bare digit during movement could be ship select; keep that.
            pass
        print(ctx.select(snap, int(tokens[0])))
        return Action(side="empty")

    # Allocate-phase shortcuts: open draft automatically if user starts assigning.
    if phase == "allocate" and ctx.draft is None:
        cmd0 = tokens[0].lower()
        if cmd0 in (
            "engine",
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
                print("  (open a ship draft first, then engine/w/sh)")
                return Action(side="empty")

    cmd = tokens[0].lower()
    rest = tokens[1:]

    if phase == "allocate" and cmd == "e" and rest and rest[0].lstrip("-").isdigit():
        print(f"  'e {rest[0]}' ends a turn. Did you mean: engine {rest[0]}?")
        return Action(side="empty")

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
    if cmd in ("allocate", "a", "alloc") and phase == "firing" and not rest:
        cmd = "fire"

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

    # ── Movement: path draft ──────────────────────────────────────────
    if cmd in ("motion", "maneuvers", "pathhelp"):
        if phase != "movement":
            print(f"  motion is available during movement (now {phase})")
            return Action(side="empty")
        ship = _pending_movement_ship(snap, ctx)
        if ship is None:
            print("  no pending player ship; movement is waiting on another controller")
            return Action(side="empty")
        print(movement_summary(ship, ctx.path_draft if ctx.path_ship == int(ship["id"]) else None))
        return Action(side="empty")

    if phase == "movement" and cmd in ("move", "m", "path"):
        ship = _pending_movement_ship(snap, ctx)
        if ship is None:
            print(f"  cannot path: no pending living player ship (phase={phase!r})")
            return Action(side="empty")
        sid = int(ship["id"])
        ctx.ensure_path_ship(sid)
        if not rest:
            print(movement_summary(ship, ctx.path_draft))
            print(path_draft_summary(ctx))
            return Action(side="empty")
        # path commit / path hold
        if rest[0].lower() in ("commit", "c", "ok", "apply"):
            actions = list(ctx.path_draft)
            order = _commit_path_order(sid, actions)
            note = f"commit_path #{sid} actions={actions or '[]'}"
            return Action(orders=[order], note=note)
        if rest[0].lower() in ("hold", "pass", "stay", "empty"):
            order = _commit_path_order(sid, [])
            return Action(orders=[order], note=f"commit_path #{sid} empty (hold)")
        if rest[0].lower() in ("clear", "reset"):
            ctx.path_draft = []
            print("  path draft cleared")
            return Action(side="empty")
        if rest[0].lower() in ("undo", "u", "pop"):
            if ctx.path_draft:
                dropped = ctx.path_draft.pop()
                print(f"  undid {path_action_short(dropped)}; " + path_draft_summary(ctx))
            else:
                print("  path draft empty")
            return Action(side="empty")
        if rest[0].lower() == "preview":
            return Action(
                side="path_preview",
                request={
                    "protocol_version": PROTOCOL_VERSION,
                    "request": "path_preview",
                    "ship": sid,
                    "actions": list(ctx.path_draft),
                },
                note=path_draft_summary(ctx),
            )
        # Append path tokens: path f fr tl
        appended = []
        for tok in rest:
            action = parse_path_token(tok)
            if action is None:
                print(
                    f"  unknown path action {tok!r}; use f|fr|fl|tr|tl "
                    f"(or move_f/move_fr/…)"
                )
                return Action(side="empty")
            ctx.path_draft.append(action)
            appended.append(path_action_short(action))
        print(f"  path +{' '.join(appended)}; " + path_draft_summary(ctx))
        return Action(side="empty")

    # Bare path tokens during movement (f, fr, fl, tr, tl, and r/l as turns)
    if phase == "movement" and parse_path_token(cmd) is not None:
        # During movement, lone `r` is turn_right (not ready). `f` is move_f
        # only when not ambiguous with fire — fire is firing-phase only.
        ship = _pending_movement_ship(snap, ctx)
        if ship is None:
            print("  no pending living player ship")
            return Action(side="empty")
        sid = int(ship["id"])
        ctx.ensure_path_ship(sid)
        tokens_to_add = [cmd] + list(rest)
        for tok in tokens_to_add:
            action = parse_path_token(tok)
            if action is None:
                print(f"  unknown path action {tok!r}")
                return Action(side="empty")
            ctx.path_draft.append(action)
        print(path_draft_summary(ctx))
        return Action(side="empty")

    if cmd in ("hold", "pass", "pass_move", "p", "stay", "coast"):
        if phase != "movement":
            print(f"  hold/pass is unavailable in {phase}; use fire/f or ready/r")
            return Action(side="empty")
        requested = int(rest[0]) if rest and rest[0].isdigit() else None
        ship = _pending_movement_ship(snap, ctx, requested)
        if ship is None:
            print("  no pending living player ship")
            return Action(side="empty")
        sid = int(ship["id"])
        return Action(
            orders=[_commit_path_order(sid, [])],
            note=f"commit_path #{sid} empty (hold station)",
        )

    if phase == "movement" and cmd in ("undo", "u", "pop"):
        if not ctx.path_draft:
            print("  path draft empty")
            return Action(side="empty")
        dropped = ctx.path_draft.pop()
        print(f"  undid {path_action_short(dropped)}; " + path_draft_summary(ctx))
        return Action(side="empty")

    if phase == "movement" and cmd in ("clear", "reset"):
        ctx.path_draft = []
        print("  path draft cleared")
        return Action(side="empty")

    if phase == "movement" and cmd == "preview":
        ship = _pending_movement_ship(snap, ctx)
        if ship is None:
            print("  no pending living player ship")
            return Action(side="empty")
        sid = int(ship["id"])
        ctx.ensure_path_ship(sid)
        return Action(
            side="path_preview",
            request={
                "protocol_version": PROTOCOL_VERSION,
                "request": "path_preview",
                "ship": sid,
                "actions": list(ctx.path_draft),
            },
            note=path_draft_summary(ctx),
        )

    if phase == "movement" and cmd in ("commit", "c", "ok", "apply"):
        ship = _pending_movement_ship(snap, ctx)
        if ship is None:
            print("  no pending living player ship")
            return Action(side="empty")
        sid = int(ship["id"])
        actions = list(ctx.path_draft)
        order = _commit_path_order(sid, actions)
        return Action(
            orders=[order],
            note=f"commit_path #{sid} actions={actions or '[]'}",
        )

    if cmd in ("accel", "accelerate", "decel", "decelerate", "course", "rotate", "turn"):
        print(
            f"  {cmd!r} is a retired protocol-v3 maneuver. "
            "Protocol 4 uses path actions: f fr fl tr tl, then commit. "
            "Type motion or help path."
        )
        return Action(side="empty")

    # ── Firing: volley draft ──────────────────────────────────────────
    if cmd in ("fire", "attack", "atk", "a", "f", "commit_fire"):
        if phase == "movement" and parse_path_token(cmd) == "move_f" and cmd == "f":
            # Already handled above; keep for safety.
            pass
        if phase != "firing":
            if phase == "movement" and cmd == "f":
                # Bare `f` during movement is move_f (handled above). If we
                # get here, treat as fire-phase error.
                pass
            print(
                f"  fire/attack is available during firing only (now {phase}). "
                "Finish the path with commit/hold, then fire in the volley stage."
            )
            return Action(side="empty")
        sid = ctx.ensure_selected(snap)
        fire_args = list(rest)
        if fire_args and fire_args[0].isdigit():
            sid = int(fire_args[0])
            ctx.selected = sid
            fire_args = fire_args[1:]
        if sid is None:
            print("  select ship first")
            return Action(side="empty")
        ship = ship_by_id(snap, sid)
        if ship is None or ship.get("controller") != "player" or ship.get("destroyed"):
            print(f"  cannot fire ship #{sid}: not a living player ship")
            return Action(side="empty")
        committed = set(snap.get("ships_committed_volley") or [])
        if sid in committed:
            print(f"  ship #{sid} already committed a volley this stage")
            return Action(side="empty")
        ctx.ensure_volley_ship(sid)
        if len(fire_args) >= 2:
            shot = direct_fire(snap, sid, fire_args[0], fire_args[1], ctx=ctx)
            if shot is None:
                return Action(side="empty")
            ctx.volley_draft.append(shot)
            print(volley_draft_summary(ctx))
            return Action(side="empty", note=f"drafted {shot['weapon']}→#{shot['target']}")
        return Action(side="fire_loop")

    if cmd in ("ready", "r", "ready_fire", "nofire", "no-fire", "skipfire", "skip", "done", "-1"):
        if phase == "movement" and cmd == "r":
            # `r` during movement is turn_right — handled by bare path tokens.
            # If we somehow land here, redirect.
            ship = _pending_movement_ship(snap, ctx)
            if ship is not None:
                sid = int(ship["id"])
                ctx.ensure_path_ship(sid)
                ctx.path_draft.append("turn_right")
                print(path_draft_summary(ctx))
                return Action(side="empty")
        sid = ctx.ensure_selected(snap)
        if rest and rest[0].isdigit():
            sid = int(rest[0])
        if sid is None:
            print("  select ship first")
            return Action(side="empty")
        ship = ship_by_id(snap, sid)
        if ship is None or ship.get("controller") != "player" or ship.get("destroyed"):
            print(f"  cannot submit volley for ship #{sid}: not a living player ship")
            return Action(side="empty")
        if phase != "firing":
            print(
                f"  ready/nofire submits a volley during firing only (now {phase}). "
                f"Finish the path with commit/hold first."
            )
            return Action(side="empty")
        committed = set(snap.get("ships_committed_volley") or [])
        if sid in committed:
            print(
                f"  ship #{sid} already committed a volley — waiting for other ships. "
                f"committed={sorted(committed)}"
            )
            return Action(side="empty")
        ctx.ensure_volley_ship(sid)
        shots = list(ctx.volley_draft)
        print(
            f"  commit_volley #{sid} with {len(shots)} shot(s) "
            f"(empty = hold fire; resolves when every living ship has committed)"
        )
        return Action(orders=[_commit_volley_order(sid, shots)])

    if phase == "firing" and cmd in ("undo", "u", "pop"):
        if not ctx.volley_draft:
            print("  volley draft empty")
            return Action(side="empty")
        dropped = ctx.volley_draft.pop()
        print(f"  undid {dropped.get('weapon')}; " + volley_draft_summary(ctx))
        return Action(side="empty")

    if phase == "firing" and cmd in ("clear", "reset"):
        ctx.volley_draft = []
        print("  volley draft cleared")
        return Action(side="empty")

    if phase == "firing" and cmd in ("commit", "c", "ok", "apply"):
        ship = _pending_volley_ship(snap, ctx)
        if ship is None:
            print("  no pending living player ship for volley")
            return Action(side="empty")
        sid = int(ship["id"])
        ctx.ensure_volley_ship(sid)
        shots = list(ctx.volley_draft)
        return Action(
            orders=[_commit_volley_order(sid, shots)],
            note=f"commit_volley #{sid} shots={len(shots)}",
        )

    if cmd in ("end", "e", "end_turn"):
        print(
            "  end_turn was removed in protocol v4. "
            "After all ships commit_volley, the turn advances to allocate automatically."
        )
        return Action(side="empty")

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
        retired = {
            "move",
            "pass_move",
            "commit_maneuver",
            "commit_fire",
            "ready_fire",
            "end_turn",
        }
        if obj.get("type") in retired:
            print(
                f"  {obj.get('type')!r} is retired in protocol v4; "
                "use commit_path / commit_volley (no ready_fire or end_turn)"
            )
            return Action(side="empty")
        obj.setdefault("protocol_version", PROTOCOL_VERSION)
        return Action(orders=[obj])

    if phase == "movement" and cmd in ("thrust", "speed", "power", "engine", "motion_pool"):
        ship = _pending_movement_ship(snap, ctx) or (
            ship_by_id(snap, ctx.selected) if ctx.selected is not None else None
        )
        sticky = f"\n  now: {motion_status_bits(ship)}" if ship else ""
        print(
            "  motion points are the path budget from engine power this turn.\n"
            "  Spend them with path actions f|fr|fl|tr|tl (1 each), then commit."
            f"{sticky}\n"
            "  Type motion for the full path help block."
        )
        return Action(side="empty")

    if cmd in ("play", "next", "continue", "go", "proceed", "advance"):
        if phase == "movement":
            print(
                f"  no {cmd!r} command — draft a path (f/fr/fl/tr/tl) then commit, "
                "or hold/p for an empty path."
            )
        elif phase == "firing":
            print(
                f"  no {cmd!r} command — fire/attack to draft shots, or ready (r) "
                "to submit the volley (empty = hold fire)."
            )
        elif phase == "allocate":
            print(
                f"  no {cmd!r} command — allocate power first: a [ship], then "
                "engine N / w / sh, then commit."
            )
        else:
            print(f"  no {cmd!r} command — type hint for the next action.")
        return Action(side="empty")

    suggestion = difflib.get_close_matches(
        cmd,
        list(COMMAND_REGISTRY) + ["attack", "path", "quit", "status", "hold"],
        n=1,
        cutoff=0.45,
    )
    if suggestion:
        print(
            f"  unknown command {cmd!r}. Did you mean '{suggestion[0]}'? "
            "Type help for commands."
        )
    else:
        print(
            f"  unknown command {cmd!r}. Type help for commands; "
            "try status, path, fire, or quit."
        )
    return Action(side="unknown")
