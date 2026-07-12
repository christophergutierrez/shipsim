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
    HELP,
    Action,
    ReplContext,
    build_action,
    interactive_fire,
    phase_hint,
)
from screen import TerminalUI, default_session_path
from hexutil import ship_callsign
from view import (
    format_board,
    format_combat_events,
    format_error,
    format_ship_line,
    format_snapshot,
    format_terminal_banner,
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
        raw = input(f"pick [{default_idx}]: ").strip()
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
    if ctx.draft is None:
        return None
    return ctx.draft.summary()


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
    sid = ctx.ensure_selected(snap)
    if sid is None:
        return None
    ship = next((s for s in snap.get("ships") or [] if s.get("id") == sid), None)
    if ship is None or ship.get("controller") != "player":
        return None
    ready = set(snap.get("ships_ready_fire") or [])
    if sid in ready:
        return None
    charged = [
        w
        for w in (ship.get("weapons") or [])
        if w.get("operational", True)
        and not w.get("fired")
        and int(w.get("charge") or 0) > 0
    ]
    # Exclude already-queued commits so we don't re-offer spent slots.
    already = {
        str(c.get("weapon"))
        for c in (snap.get("fire_commits") or [])
        if int(c.get("ship") or -1) == int(sid)
    }
    charged = [
        w
        for w in charged
        if str(w.get("id")) not in already
    ]
    if not charged:
        if already:
            ui.log(
                "  shots already queued — type r / ready / nofire / done "
                "to leave fire phase (not f)"
            )
        else:
            ui.log("  no charged weapons on focus — r/nofire to leave fire phase")
        return None
    ui.log(
        "  firing: pick weapons to fire, [-1] Done to stop. "
        "You may fire multiple weapons this phase."
    )
    while True:
        with ui.dialog():
            paint_frame(ui, session, ctx)
            order = interactive_fire(session.snapshot or {}, sid)
        if order is None:
            ui.log(
                "  done firing — type f to add more, or r/ready/done "
                "to finish fire phase"
            )
            return log_len
        log_len = send_orders(ui, session, ctx, [order], prev_log_len=log_len)
        # "Done" from the weapon menu returns a ready_fire order: the ship is
        # now committed and the fire phase is ending for it, so stop looping.
        if order.get("type") == "ready_fire":
            return log_len
        # Refresh and check for remaining chargeable weapons on this ship.
        snap = session.snapshot
        if not snap:
            return log_len
        ship = next((s for s in snap.get("ships") or [] if s.get("id") == sid), None)
        if ship is None:
            return log_len
        already = {
            str(c.get("weapon"))
            for c in (snap.get("fire_commits") or [])
            if int(c.get("ship") or -1) == int(sid)
        }
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
                "  no more charged weapons — r/ready/done to finish fire phase"
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
                    f"mov={sh.get('movement_allocated')}  weapons: {weps}  "
                    f"shields={sh.get('shields_powered')}"
                )
                if int(sh.get("movement_allocated") or 0) == 0 and weps == "(none charged)":
                    ui.log(
                        "  note: zero move + zero weapons → movement skipped, "
                        "fire has nothing charged"
                    )
                ctx.draft = None
                ctx.draft_group = None
                break
        new_log = msg.get("combat_log") or []
        if len(new_log) < log_len:
            log_len = 0
        if len(new_log) > log_len:
            events = new_log[log_len:]
            ui.log(format_combat_events(events, msg, hull_max=ctx.hull_max))
            log_len = len(new_log)
        if order.get("type") == "commit_fire":
            # Fire is queued, not resolved yet — tell the player so they know
            # the HIT/MISS line arrives after ready_fire ends the phase.
            ship = next(
                (s for s in (msg.get("ships") or []) if s.get("id") == order.get("ship")),
                None,
            )
            cs = ship_callsign(ship) if ship else f"#{order.get('ship')}"
            ui.log(
                f"  {cs} shot queued — resolves at end of fire phase "
                f"(r/ready/done to fire)"
            )
        delta = snapshot_delta(before, msg)
        if delta:
            ui.log(delta)
        if order.get("type") == "move" and str(order.get("mode", "")).startswith("turn"):
            ship = next(
                (s for s in (msg.get("ships") or []) if s.get("id") == order.get("ship")),
                None,
            )
            if ship:
                ui.log(
                    f"  …turned → face={ship.get('facing')} "
                    f"@({ship.get('q')},{ship.get('r')})"
                )
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


def run_repl(session: ShipsimSession, ui: TerminalUI) -> int:
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
        else:
            paint_frame(ui, session, ctx)

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
                    ui.log(format_terminal_banner(str(status)))
                    terminal_announced = True
            phase = str(snap.get("phase") or "?")
            turn = snap.get("turn", "?")
            active = snap.get("active_ship")

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
                last_phase = phase
                if not ui.scroll:
                    paint_frame(ui, session, ctx)
                # Auto weapon menu only when *entering* firing from another phase.
                if phase == "firing" and prev_phase != "firing":
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
                ready = snap.get("ships_ready_fire") or []
                if focus is not None and focus in ready:
                    prompt += "/ready"
                else:
                    prompt += "/r=done"
            if ctx.draft is not None:
                prompt += f" draft{ctx.draft.used()}/{ctx.draft.power}"
                if ctx.draft_group:
                    prompt += f"/{ctx.draft_group}"

            try:
                # Prompt always live (dialog-ish) so the user sees it under the frame.
                with ui.dialog():
                    line = input(f"{prompt}> ")
            except (EOFError, KeyboardInterrupt):
                print()
                break

            # Local UI commands (not engine orders)
            low = line.strip().lower()
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
                break
            if act.side == "help":
                ui.log(HELP)
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
                ui.log(format_board(snap, selected=ctx.selected, active=snap.get("active_ship")))
                paint_frame(ui, session, ctx)
                continue
            if act.side == "ships":
                active_id = snap.get("active_ship")
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
                                "active_ship",
                                "move_order",
                                "ships_moved_this_phase",
                                "ships_ready_fire",
                                "end_turn_warning",
                                "prng_state",
                            )
                        },
                        indent=2,
                    )
                )
                paint_frame(ui, session, ctx)
                continue
            if act.side == "empty":
                # Draft edits etc. — refresh frame so bars stay under the map.
                paint_frame(ui, session, ctx)
                continue
            if act.side == "unknown":
                ui.log("  unknown command; try help")
                paint_frame(ui, session, ctx)
                continue
            if act.side == "fire_loop":
                # Looping fire menu: fire multiple weapons, refresh each commit.
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
            log_len = send_orders(ui, session, ctx, act.orders, prev_log_len=log_len)
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
        if ui.session_path:
            ui.log(f"session log saved: {ui.session_path}")
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
                    "protocol_version": 1,
                    "type": "allocate",
                    "ship": int(ship["id"]),
                    "movement": 0,
                    "weapons": {
                        str(w["id"]): 0 for w in (ship.get("weapons") or [])
                    },
                    "shields": [0] * 6,
                }
            )
        return orders

    if phase == "movement":
        active = snap.get("active_ship")
        if active is None:
            return []
        active_ship = next(
            (s for s in living if int(s["id"]) == int(active)), None
        )
        if active_ship is None or active_ship.get("controller") != "scripted":
            return []
        return [
            {
                "protocol_version": 1,
                "type": "pass_move",
                "ship": int(active_ship["id"]),
            }
        ]

    if phase == "firing":
        ready = set(snap.get("ships_ready_fire") or [])
        pending_players = [
            s
            for s in living
            if s.get("controller") == "player" and int(s["id"]) not in ready
        ]
        if pending_players:
            return []
        orders = []
        for ship in living:
            if ship.get("controller") != "scripted":
                continue
            if int(ship["id"]) in ready:
                continue
            orders.append(
                {
                    "protocol_version": 1,
                    "type": "ready_fire",
                    "ship": int(ship["id"]),
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
        for order in orders:
            ui.log(
                f"  (scripted auto) {order.get('type')} ship=#{order.get('ship')}"
            )
        log_len = send_orders(ui, session, ctx, orders, prev_log_len=log_len)
        status = (session.snapshot or {}).get("status")
        if status in ("Won", "Lost"):
            break
    return log_len


def main(argv: list[str] | None = None) -> int:
    setup_readline()
    parser = argparse.ArgumentParser(description="shipsim interactive REPL (frontend/repl)")
    parser.add_argument("scenario", nargs="?", help="scenario path relative to repo")
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

        scenario = pick_scenario(repo, args.scenario, ui)
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
                return run_repl(session, ui)
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
