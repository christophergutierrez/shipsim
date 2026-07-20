#!/usr/bin/env python3
"""Interactive shipsim dev client (Combat Model v2) — ship-centric.

Play mode (default): fixed frame — map + ships refresh in place; RECENT strip
for the last few events; `log` toggles longer history. No endless scroll.

Session log (default on): frontend/repl/local/session-*.log
  --debug          verbose file log (timestamps + ORDER JSON)
  --log-file PATH  override session path
  --no-session-log disable file log
  --scroll         old long-log on-screen UI

Usage:
  python3 frontend/repl/repl.py scenarios/ai.toml
  python3 frontend/repl/repl.py scenarios/ai.toml --debug
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

_HERE = Path(__file__).resolve().parent
if str(_HERE) not in sys.path:
    sys.path.insert(0, str(_HERE))

from client import (
    ShipsimSession,
    TransportError,
    ensure_local,
    find_repo_root,
    find_shipsim_bin,
    list_scenarios,
)
from commands import (
    Action,
    ReplContext,
    build_action,
    interactive_fire,
    phase_hint,
    render_help,
)
from screen import TerminalUI, default_session_path
from tutorial import Tutorial, load_tutorial
from hexutil import ship_callsign
from view import (
    format_board,
    format_combat_events,
    format_error,
    format_event_highlights,
    format_ship_line,
    format_snapshot,
    format_terminal_banner,
    movement_focus_id,
    movement_pending_ids,
    snapshot_delta,
)


def setup_readline() -> None:
    try:
        import readline
    except ImportError:
        return
    hist = ensure_local() / "history"
    try:
        if hist.is_file():
            readline.read_history_file(str(hist))
        readline.set_history_length(500)
    except OSError:
        pass

    def _save() -> None:
        try:
            readline.write_history_file(str(hist))
        except OSError:
            pass

    import atexit

    atexit.register(_save)
    try:
        readline.parse_and_bind("set editing-mode emacs")
        readline.parse_and_bind("\\e[A: previous-history")
        readline.parse_and_bind("\\e[B: next-history")
    except Exception:
        pass


def pick_scenario(repo: Path, preferred: str | None, ui: TerminalUI) -> str:
    if preferred:
        candidates = [
            preferred,
            preferred if preferred.endswith(".toml") else f"{preferred}.toml",
            f"scenarios/{preferred}",
            f"scenarios/{preferred}.toml" if not preferred.endswith(".toml") else None,
        ]
        for c in candidates:
            if c and (repo / c).is_file():
                return c.replace("\\", "/")
        raise SystemExit(f"scenario not found: {preferred}")

    scenarios = list_scenarios(repo)
    if not scenarios:
        raise SystemExit("no scenarios/*.toml found")
    with ui.dialog():
        print("scenarios:")
        for i, s in enumerate(scenarios):
            print(f"  [{i}] {s}")
        default_idx = 0
        for i, s in enumerate(scenarios):
            if s.endswith("ai.toml"):
                default_idx = i
                break
        try:
            raw = input(f"pick [{default_idx}]: ").strip()
        except (EOFError, KeyboardInterrupt):
            print()
            raise SystemExit("no scenario selected")
    if raw == "":
        idx = default_idx
    else:
        try:
            idx = int(raw)
        except ValueError:
            return pick_scenario(repo, raw, ui)
    if idx < 0 or idx >= len(scenarios):
        raise SystemExit("bad scenario index")
    return scenarios[idx]


def draft_panel(ctx: ReplContext) -> str | None:
    parts: list[str] = []
    if ctx.draft is not None:
        parts.append(ctx.draft.summary())
    if ctx.path_draft:
        short = " ".join(a for a in ctx.path_draft)
        parts.append(
            f"  path draft ship=#{ctx.path_ship}: {short} "
            f"({len(ctx.path_draft)} actions) — commit or hold"
        )
    if ctx.volley_draft:
        lines = [f"  volley draft ship=#{ctx.volley_ship}:"]
        for shot in ctx.volley_draft:
            lines.append(
                f"    {shot.get('weapon')} → #{shot.get('target')} "
                f"face={shot.get('shield_facing')}"
            )
        parts.append("\n".join(lines))
    return "\n".join(parts) if parts else None


def paint_frame(
    ui: TerminalUI,
    session: ShipsimSession,
    ctx: ReplContext,
    *,
    banner: str = "",
) -> None:
    snap = session.snapshot
    if snap is None:
        return
    if ui.scroll:
        # long-log mode: only print full state when caller asks via log()
        return
    ui.redraw(
        snap,
        selected=ctx.selected,
        hull_max=ctx.hull_max,
        draft_text=draft_panel(ctx),
        hint=phase_hint(snap, ctx),
        banner=banner,
        footer=(
            f"orders→{session.orders_log.name}"
            + (f"  session→{ui.session_path}" if ui.session_path else "  session log off")
            + ("  verbose" if ui.verbose else "")
        ),
    )


def _auto_fire_offer(
    ui: TerminalUI,
    session: ShipsimSession,
    ctx: ReplContext,
    log_len: int,
) -> int | None:
    snap = session.snapshot
    if not snap:
        return None
    # In scripted (non-interactive) mode, do not auto-open the fire menu:
    # it would read the next piped line (e.g. "fire b1 2") as a weapon-name
    # answer and desync the rest of the command sequence. Let piped one-liners
    # flow through the normal input loop and build_action instead.
    if not sys.stdin.isatty():
        return None
    sid = ctx.ensure_selected(snap)
    if sid is None:
        return None
    ship = next((s for s in snap.get("ships") or [] if s.get("id") == sid), None)
    if ship is None or ship.get("controller") != "player":
        return None
    committed = set(snap.get("ships_committed_volley") or [])
    if sid in committed:
        return None
    charged = [
        w
        for w in (ship.get("weapons") or [])
        if w.get("operational", True)
        and not w.get("fired")
        and int(w.get("charge") or 0) > 0
    ]
    already = {str(s.get("weapon")) for s in ctx.volley_draft}
    charged = [w for w in charged if str(w.get("id")) not in already]
    if not charged:
        if already:
            ui.log(
                "  shots already in volley draft — type r / ready / nofire / done "
                "to submit commit_volley"
            )
        else:
            ui.log("  no charged weapons on focus — r/nofire to hold fire (empty volley)")
        return None
    ui.log(
        "  firing: pick weapons for the volley draft, [-1] Done submits commit_volley. "
        "You may draft multiple weapons before submitting."
    )
    while True:
        with ui.dialog():
            paint_frame(ui, session, ctx)
            try:
                result = interactive_fire(session.snapshot or {}, sid, ctx)
            except (EOFError, KeyboardInterrupt):
                ui.log("  fire input ended; volley draft kept — type r to submit or clear")
                return log_len
        if result is None:
            ui.log(
                "  left weapon menu — type f to add more, or r/ready/done "
                "to submit the volley"
            )
            return log_len
        if result.get("type") == "commit_volley":
            log_len = send_orders(ui, session, ctx, [result], prev_log_len=log_len)
            return log_len
        # Shot dict for the local draft.
        if "weapon" in result and "type" not in result:
            ctx.ensure_volley_ship(sid)
            ctx.volley_draft.append(result)
            ui.log(
                f"  drafted {result.get('weapon')} → #{result.get('target')} "
                f"({len(ctx.volley_draft)} in volley)"
            )
        snap = session.snapshot
        if not snap:
            return log_len
        ship = next((s for s in snap.get("ships") or [] if s.get("id") == sid), None)
        if ship is None:
            return log_len
        already = {str(s.get("weapon")) for s in ctx.volley_draft}
        remaining = [
            w
            for w in (ship.get("weapons") or [])
            if w.get("operational", True)
            and not w.get("fired")
            and int(w.get("charge") or 0) > 0
            and str(w.get("id")) not in already
        ]
        if not remaining:
            ui.log(
                "  no more charged weapons — r/ready/done to submit the volley"
            )
            return log_len


def send_orders(
    ui: TerminalUI,
    session: ShipsimSession,
    ctx: ReplContext,
    orders: list[dict],
    *,
    prev_log_len: int,
) -> int:
    log_len = prev_log_len
    for i, order in enumerate(orders):
        status = (session.snapshot or {}).get("status")
        if status in ("Won", "Lost"):
            ui.log(f"*** scenario {status} — orders are disabled; use quit or log ***")
            break
        before = session.snapshot
        ui.log_order(order)
        msg = session.send_order(order)
        if msg.get("type") == "error":
            if hasattr(ui, "dialog"):
                with ui.dialog():
                    print(format_error(msg))
            else:
                ui.log(format_error(msg))
            if i > 0:
                if hasattr(ui, "dialog"):
                    with ui.dialog():
                        print("  (stopped multi-step move after error)")
                else:
                    ui.log("  (stopped multi-step move after error)")
            break
        ctx.note_hull(msg)
        if order.get("type") == "allocate":
            sid = int(order.get("ship") or 0)
            for sh in msg.get("ships") or []:
                if int(sh.get("id") or 0) != sid:
                    continue
                weps = ", ".join(
                    f"{w.get('id')}={w.get('charge')}"
                    for w in (sh.get("weapons") or [])
                    if int(w.get("charge") or 0) > 0
                ) or "(none charged)"
                ui.log(
                    f"  engine accepted allocate #{sid}: "
                    f"engine={sh.get('movement_allocated')} power → "
                    f"motion={sh.get('motion_available')}  weapons: {weps}  "
                    f"shields={sh.get('shields_powered')}"
                )
                if int(sh.get("movement_allocated") or 0) == 0 and weps == "(none charged)":
                    ui.log(
                        "  note: zero engine power means empty motion pool; "
                        "commit an empty path (hold) and hold fire if unarmed"
                    )
                ctx.draft = None
                ctx.draft_group = None
                break
        if order.get("type") == "commit_path":
            ctx.clear_path_draft()
        if order.get("type") == "commit_volley":
            ctx.clear_volley_draft()
        new_log = msg.get("combat_log") or []
        if len(new_log) < log_len:
            log_len = 0
        if len(new_log) > log_len:
            events = new_log[log_len:]
            # The engine exposes its PRNG checkpoint, not the private roll.
            # Replaying the documented SplitMix64 step from the pre-resolution
            # checkpoint lets the view report the exact roll without changing
            # rules or the persisted protocol shape.
            state = int((before or {}).get("prng_state") or 0)
            for event in events:
                state = (state + 0x9E3779B97F4A7C15) & ((1 << 64) - 1)
                value = state
                value = ((value ^ (value >> 30)) * 0xBF58476D1CE4E5B9) & ((1 << 64) - 1)
                value = ((value ^ (value >> 27)) * 0x94D049BB133111EB) & ((1 << 64) - 1)
                event["roll"] = ((value ^ (value >> 31)) % 20) + 1
            fire_text = format_combat_events(events, msg, hull_max=ctx.hull_max)
            ui.log(fire_text)
            # Persist the full FIRE RESOLUTION block (plus explicit damage
            # highlights) across repaints. The RECENT strip truncates the
            # compact Δ line and the live "shots resolved" block is scoped to
            # the focused ship's own weapons — so without this panel the
            # enemy's fire, weapon destruction, and power-pool loss vanish
            # after the next paint_frame. Reuse the existing formatted text;
            # do not reinvent it.
            highlights = format_event_highlights(before, msg)
            ui.recent_events_text = (
                (highlights + "\n" + fire_text) if highlights else fire_text
            )
            log_len = len(new_log)
        if order.get("type") == "commit_path":
            ship = next(
                (s for s in (msg.get("ships") or []) if s.get("id") == order.get("ship")),
                None,
            )
            cs = ship_callsign(ship) if ship else f"#{order.get('ship')}"
            actions = order.get("actions") or []
            ui.log(
                f"  {cs} path committed ({len(actions)} actions) — "
                f"resolves once every living ship has committed a path"
            )
        if order.get("type") == "commit_volley":
            ship = next(
                (s for s in (msg.get("ships") or []) if s.get("id") == order.get("ship")),
                None,
            )
            cs = ship_callsign(ship) if ship else f"#{order.get('ship')}"
            n = len(order.get("shots") or [])
            ui.log(
                f"  {cs} volley committed ({n} shot(s)) — "
                f"resolves once every living ship has committed; "
                f"then allocate begins automatically"
            )
        delta = snapshot_delta(before, msg)
        if delta:
            ui.log(delta)
        if before and before.get("phase") != msg.get("phase"):
            if before.get("turn") != msg.get("turn"):
                ui.log(f"=== END OF TURN {before.get('turn')} — START TURN {msg.get('turn')} ===")
            else:
                ui.log(f"phase complete: {before.get('phase')} → {msg.get('phase')}")
        # Always repaint frame from latest snapshot so bars update in place.
        if not ui.scroll:
            paint_frame(ui, session, ctx)
        elif i == len(orders) - 1:
            ui.log(
                format_snapshot(
                    msg,
                    selected=ctx.selected,
                    hull_max=ctx.hull_max,
                    verbose=True,
                )
            )
    return log_len


def run_repl(
    session: ShipsimSession,
    ui: TerminalUI,
    tutorial: Tutorial | None = None,
) -> int:
    assert session.snapshot is not None
    ctx = ReplContext()
    ctx.note_hull(session.snapshot)
    ctx.ensure_selected(session.snapshot)

    # print hook already installed by main(); keep idempotent if called alone
    restore_print = ui.install_print_hook()
    try:
        ui.log(f"shipsim REPL  bin={session.bin}")
        ui.log(f"scenario={session.scenario}")
        if ui.session_path:
            ui.log(f"session log → {ui.session_path}")
        if ui.verbose:
            ui.log("verbose transcript (--debug): timestamps + ORDER lines")
        ui.log("play frame · log=history · cls=redraw · --scroll=long log")
        ui.log("objective: destroy the opposing fleet. Type help to see commands; ? also works.")
        if tutorial is not None:
            ui.log(f"tutorial={tutorial.name} (strict choices; incorrect commands are blocked)")
            ui.tutorial_text = tutorial.panel_text(session.snapshot)
            if ui.scroll:
                ui.log(ui.tutorial_text)
        if ui.scroll:
            print(
                format_snapshot(
                    session.snapshot,
                    selected=ctx.selected,
                    hull_max=ctx.hull_max,
                    verbose=True,
                )
            )
            print(phase_hint(session.snapshot, ctx))
        # Play mode: do NOT paint here. The main loop's phase-transition hook
        # paints once when last_phase is None → allocate. Painting both here
        # and there stacked two full frames in scrollback on short TTYs (I2/I3).

        last_phase: str | None = None
        terminal_announced = False
        log_len = len(session.snapshot.get("combat_log") or [])
        # Unstick scripted-only blockers before first prompt (e.g. resume mid-phase).
        log_len = pump_scripted(ui, session, ctx, log_len)

        while True:
            snap = session.snapshot
            if snap is None:
                ui.log("no snapshot; exiting")
                return 1
            # Drive scripted ships when phase is blocked only on them (combat.toml).
            log_len = pump_scripted(ui, session, ctx, log_len)
            snap = session.snapshot
            if snap is None:
                ui.log("no snapshot; exiting")
                return 1
            ctx.note_hull(snap)
            status = snap.get("status")
            if status in ("Won", "Lost"):
                if not terminal_announced:
                    ui.log(format_terminal_banner(str(status), snap))
                    terminal_announced = True
            phase = str(snap.get("phase") or "?")
            turn = snap.get("turn", "?")
            active = movement_focus_id(snap) if phase == "movement" else None
            if tutorial is not None:
                drift = tutorial.state_error(snap)
                if drift:
                    ui.log(drift)
                    return 1
                ui.tutorial_text = tutorial.panel_text(snap)

            # Phase transition hooks — run once per entry into a phase.
            # Do NOT clear last_phase after every order (that re-opened auto-fire
            # forever so r/done/ready could never escape the fire menu).
            if phase != last_phase:
                prev_phase = last_phase
                ui.log(phase_hint(snap, ctx))
                if phase == "allocate" and ctx.draft is None:
                    msg = ctx.begin_allocate_picker(snap)
                    ui.log(msg)
                if phase != "allocate" and ctx.draft is not None:
                    ctx.draft = None
                    ctx.draft_group = None
                if phase != "movement":
                    ctx.clear_path_draft()
                if phase != "firing":
                    ctx.clear_volley_draft()
                last_phase = phase
                if not ui.scroll:
                    paint_frame(ui, session, ctx)
                # Auto weapon menu only when *entering* firing from another phase.
                if tutorial is None and phase == "firing" and prev_phase != "firing":
                    auto = _auto_fire_offer(ui, session, ctx, log_len)
                    if auto is not None:
                        log_len = auto
                        log_len = pump_scripted(ui, session, ctx, log_len)
                        # Keep last_phase = "firing" if still there; if phase
                        # advanced, reset last_phase so the next iteration runs
                        # the new phase's entry hooks once.
                        after = str(
                            (session.snapshot or {}).get("phase") or phase
                        )
                        if after != phase:
                            last_phase = phase  # force transition detection
                        continue

            focus = ctx.selected
            prompt = f"t{turn}/{phase}"
            if focus is not None:
                prompt += f"@{focus}"
            if phase == "movement" and active is not None:
                prompt += f"*{active}"
            if phase == "firing":
                committed = set(snap.get("ships_committed_volley") or [])
                if focus is not None and focus in committed:
                    prompt += "/volley_ok"
                else:
                    prompt += f"/v={len(ctx.volley_draft)}"
            if ctx.draft is not None:
                prompt += f" draft{ctx.draft.used()}/{ctx.draft.power}"
                if ctx.draft_group:
                    prompt += f"/{ctx.draft_group}"
            elif ctx.path_draft:
                prompt += f" path{len(ctx.path_draft)}"
            else:
                ship = next((s for s in snap.get("ships") or [] if s.get("id") == focus), None)
                if phase == "movement" and ship is not None:
                    prompt += f" actions=motion:{int(ship.get('motion_available') or 0)}"
                elif phase == "firing" and ship is not None:
                    charged = sum(
                        1 for w in (ship.get("weapons") or [])
                        if int(w.get("charge") or 0) > 0 and not w.get("fired")
                    )
                    prompt += f" actions=charged:{charged}"

            try:
                # Tutorial guidance must be adjacent to input. The tactical frame
                # can exceed a short terminal and push its top panel out of view.
                with ui.dialog():
                    if tutorial is not None:
                        print(tutorial.prompt_text())
                    line = input(f"{prompt}> ")
            except (EOFError, KeyboardInterrupt):
                print()
                break

            # Local UI commands (not engine orders)
            low = line.strip().lower()
            tutorial_advances = False
            if tutorial is not None:
                if not tutorial.accepts(line):
                    ui.log(tutorial.reject_text(line))
                    paint_frame(ui, session, ctx)
                    continue
                tutorial_advances = tutorial.advances_for(line)
            if low in ("log", "hist", "history"):
                ui.show_history = not ui.show_history
                ui.log(f"  history panel {'ON' if ui.show_history else 'OFF'}")
                paint_frame(ui, session, ctx)
                continue
            if low in ("cls", "redraw", "refresh"):
                paint_frame(ui, session, ctx)
                continue

            with ui.dialog():
                # Subcommands that print menus stay visible during the dialog.
                act: Action = build_action(line, snap, ctx)

            if act.side == "quit":
                if snap.get("status") not in ("Won", "Lost"):
                    with ui.dialog():
                        try:
                            confirm = input("  unfinished game — type yes to quit: ").strip().lower()
                        except (EOFError, KeyboardInterrupt):
                            print()
                            confirm = "yes"
                    if confirm not in ("y", "yes"):
                        ui.log("  quit cancelled")
                        paint_frame(ui, session, ctx)
                        continue
                break
            if act.side == "help":
                ui.log(render_help(act.note))
                paint_frame(ui, session, ctx)
                continue
            if act.side == "hint":
                ui.log(phase_hint(snap, ctx))
                paint_frame(ui, session, ctx)
                continue
            if act.side == "status":
                paint_frame(ui, session, ctx)
                continue
            if act.side == "board":
                ui.log(format_board(snap, selected=ctx.selected, active=movement_focus_id(snap)))
                paint_frame(ui, session, ctx)
                continue
            if act.side == "ships":
                active_id = movement_focus_id(snap)
                for ship in snap.get("ships") or []:
                    ui.log(
                        format_ship_line(
                            ship,
                            active=ship.get("id") == active_id,
                            focused=ship.get("id") == ctx.selected,
                            hull_max=ctx.hull_max.get(int(ship["id"])),
                        )
                    )
                paint_frame(ui, session, ctx)
                continue
            if act.side == "raw":
                ui.log(
                    json.dumps(
                        {
                            k: snap.get(k)
                            for k in (
                                "protocol_version",
                                "turn",
                                "phase",
                                "status",
                                "ships_allocated_this_turn",
                                "ships_committed_path",
                                "ships_committed_volley",
                                "prng_state",
                            )
                        },
                        indent=2,
                    )
                )
                paint_frame(ui, session, ctx)
                continue
            if act.side == "path_preview":
                if act.note:
                    ui.log(act.note)
                if act.request is not None:
                    try:
                        preview = session.send_request(act.request)
                    except TransportError as exc:
                        ui.log(f"  path_preview failed: {exc}")
                    else:
                        if preview.get("type") == "error":
                            ui.log(format_error(preview))
                        else:
                            ui.log(
                                "  path_preview: "
                                + json.dumps(
                                    {
                                        k: preview.get(k)
                                        for k in (
                                            "ok",
                                            "cost",
                                            "remaining_motion",
                                            "final_q",
                                            "final_r",
                                            "final_facing",
                                            "error",
                                            "error_index",
                                        )
                                        if k in preview
                                    },
                                    separators=(",", ":"),
                                )
                            )
                paint_frame(ui, session, ctx)
                continue
            if act.side == "empty":
                # Draft edits etc. — refresh frame so bars stay under the map.
                if tutorial_advances:
                    tutorial.advance()
                    ui.tutorial_text = tutorial.panel_text(session.snapshot or snap)
                    if ui.scroll:
                        ui.log(ui.tutorial_text)
                paint_frame(ui, session, ctx)
                continue
            if act.side == "unknown":
                ui.log(f"  invalid command for phase={phase}; use hint or help for valid commands")
                paint_frame(ui, session, ctx)
                continue
            if act.side == "fire_loop":
                # Looping fire menu: draft multiple weapons, then submit volley.
                log_len = _auto_fire_offer(ui, session, ctx, log_len) or log_len
                phase_after = str(
                    (session.snapshot or {}).get("phase") or phase
                )
                if phase_after == phase:
                    last_phase = phase_after
                else:
                    last_phase = phase
                continue
            if not act.orders:
                paint_frame(ui, session, ctx)
                continue

            phase_before = phase
            if act.note:
                ui.log(f"  {act.note}")
            log_len = send_orders(ui, session, ctx, act.orders, prev_log_len=log_len)
            if tutorial_advances:
                tutorial.advance()
                ui.tutorial_text = tutorial.panel_text(session.snapshot or snap)
                if ui.scroll:
                    ui.log(ui.tutorial_text)
                paint_frame(ui, session, ctx)
            # After player acts, advance any scripted-only tail of the phase.
            log_len = pump_scripted(ui, session, ctx, log_len)
            # If phase changed, leave last_phase as phase_before so the next
            # loop runs entry hooks once. If unchanged (e.g. ready while still
            # firing), keep last_phase == current so auto-fire does not re-open.
            phase_after = str((session.snapshot or {}).get("phase") or phase_before)
            if phase_after == phase_before:
                last_phase = phase_after
            else:
                last_phase = phase_before

        ui.log(f"session orders: {session.orders_log}")
        return 0
    finally:
        restore_print()


def plan_scripted_orders(snap: dict | None) -> list[dict]:
    """Build passive orders for scripted ships when the phase is blocked only on them.

    Does not send. Callers apply via send_order / send_orders.
    Never drives AI (harness) or pending player ships.
    """
    if snap is None:
        return []
    if snap.get("status") in ("Won", "Lost"):
        return []

    phase = str(snap.get("phase") or "")
    ships = list(snap.get("ships") or [])
    living = [s for s in ships if not s.get("destroyed")]

    if phase == "allocate":
        allocated = set(snap.get("ships_allocated_this_turn") or [])
        pending_players = [
            s
            for s in living
            if s.get("controller") == "player" and int(s["id"]) not in allocated
        ]
        if pending_players:
            return []
        orders = []
        for ship in living:
            if ship.get("controller") != "scripted":
                continue
            if int(ship["id"]) in allocated:
                continue
            orders.append(
                {
                    "protocol_version": 4,
                    "type": "allocate",
                    "ship": int(ship["id"]),
                    "movement": 0,
                    "weapons": {
                        str(w["id"]): 0
                        for w in (ship.get("weapons") or [])
                        if w.get("operational", True)
                    },
                    "shields": [0] * 6,
                }
            )
        return orders

    if phase == "movement":
        committed = {int(sid) for sid in snap.get("ships_committed_path") or []}
        pending_players = [
            s for s in living
            if s.get("controller") == "player" and int(s["id"]) not in committed
        ]
        if pending_players:
            return []
        return [
            {
                "protocol_version": 4,
                "type": "commit_path",
                "ship": int(ship["id"]),
                "actions": [],
            }
            for ship in living
            if ship.get("controller") == "scripted"
            and int(ship["id"]) not in committed
        ]

    if phase == "firing":
        committed = set(snap.get("ships_committed_volley") or [])
        pending_players = [
            s
            for s in living
            if s.get("controller") == "player" and int(s["id"]) not in committed
        ]
        if pending_players:
            return []
        orders = []
        for ship in living:
            if ship.get("controller") != "scripted":
                continue
            if int(ship["id"]) in committed:
                continue
            orders.append(
                {
                    "protocol_version": 4,
                    "type": "commit_volley",
                    "ship": int(ship["id"]),
                    "shots": [],
                }
            )
        return orders

    return []


def auto_drive_scripted(
    ui: TerminalUI,
    session: ShipsimSession,
    ctx: ReplContext,
) -> None:
    """Send passive orders for scripted ships when the phase is blocked only on them.

    Used by unit tests and as a single-shot driver. Interactive loop uses
    pump_scripted() which re-plans after each batch so multi-step progress works.
    """
    del ui, ctx  # API stable for tests; session is the order sink
    for order in plan_scripted_orders(session.snapshot):
        session.send_order(order)


def pump_scripted(
    ui: TerminalUI,
    session: ShipsimSession,
    ctx: ReplContext,
    log_len: int,
    *,
    max_steps: int = 64,
) -> int:
    """Drive scripted ships until the phase needs a player (or no more work).

    Re-plans after each batch so allocate → movement → firing can chain when
    only scripted ships are pending. Returns updated combat-log cursor.
    """
    for _ in range(max_steps):
        orders = plan_scripted_orders(session.snapshot)
        if not orders:
            break
        before = session.snapshot
        for order in orders:
            ui.log(
                f"  (scripted auto) {order.get('type')} ship=#{order.get('ship')}"
            )
        log_len = send_orders(ui, session, ctx, orders, prev_log_len=log_len)
        # A rejected passive order leaves the snapshot unchanged. Stop here so
        # one bad scripted order cannot flood the terminal until max_steps.
        if session.snapshot == before:
            break
        status = (session.snapshot or {}).get("status")
        if status in ("Won", "Lost"):
            break
    return log_len


def main(argv: list[str] | None = None) -> int:
    setup_readline()
    parser = argparse.ArgumentParser(description="shipsim interactive REPL (frontend/repl)")
    parser.add_argument("scenario", nargs="?", help="scenario path relative to repo")
    parser.add_argument(
        "--tutorial",
        metavar="NAME",
        help="strict narrated tutorial (available: rear-attack)",
    )
    parser.add_argument("--bin", dest="bin_path", help="shipsim binary path")
    parser.add_argument("--save", dest="save_path", help="optional save path under local/")
    parser.add_argument(
        "--debug",
        action="store_true",
        help="verbose session log (timestamps + full ORDER JSON lines)",
    )
    parser.add_argument(
        "--log-file",
        dest="log_file",
        help="session transcript path (default: frontend/repl/local/session-*.log)",
    )
    parser.add_argument(
        "--debug-file",
        dest="debug_file",
        help="alias for --log-file (deprecated name)",
    )
    parser.add_argument(
        "--no-session-log",
        action="store_true",
        help="do not write a session transcript file",
    )
    parser.add_argument(
        "--scroll",
        action="store_true",
        help="old long scrolling log UI instead of fixed play frame",
    )
    args = parser.parse_args(argv)

    session_path = None
    if not args.no_session_log:
        override = args.log_file or args.debug_file
        session_path = Path(override) if override else default_session_path()

    ui = TerminalUI(
        session_path=session_path,
        verbose=bool(args.debug),
        scroll=args.scroll,
    )
    restore = ui.install_print_hook()
    try:
        repo = find_repo_root()
        try:
            bin_path = Path(args.bin_path) if args.bin_path else find_shipsim_bin(repo)
        except FileNotFoundError as exc:
            print(exc, file=sys.stderr)
            return 1

        tutorial = None
        if args.tutorial:
            try:
                tutorial = load_tutorial(args.tutorial)
            except ValueError as exc:
                parser.error(str(exc))
        preferred_scenario = args.scenario or (tutorial.scenario if tutorial else None)
        scenario = pick_scenario(repo, preferred_scenario, ui)
        if tutorial is not None and scenario != tutorial.scenario:
            parser.error(
                f"tutorial {tutorial.name} requires {tutorial.scenario}, got {scenario}"
            )
        save_path = None
        if args.save_path:
            save_path = Path(args.save_path)
            if not save_path.is_absolute():
                save_path = ensure_local() / save_path.name

        session = ShipsimSession(
            scenario, repo=repo, bin_path=bin_path, save_path=save_path
        )
        try:
            try:
                session.start()
                return run_repl(session, ui, tutorial=tutorial)
            except TransportError as exc:
                print(f"engine terminated: {exc}", file=sys.stderr)
                return 1
        finally:
            session.close()
    finally:
        restore()
        ui.close()


if __name__ == "__main__":
    raise SystemExit(main())
