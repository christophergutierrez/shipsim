#!/usr/bin/env python3
"""Interactive shipsim dev client (Combat Model v2).

Everything for this client lives under frontend/repl/:
  repl.py client.py view.py commands.py README.md .gitignore
  local/   (gitignored session logs)

Usage:
  python3 frontend/repl/repl.py
  python3 frontend/repl/repl.py scenarios/ai.toml
  python3 frontend/repl/client.py   # non-interactive smoke
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

# Allow `python3 frontend/repl/repl.py` without installing a package.
_HERE = Path(__file__).resolve().parent
if str(_HERE) not in sys.path:
    sys.path.insert(0, str(_HERE))

from client import ShipsimSession, find_repo_root, find_shipsim_bin, list_scenarios
from commands import HELP, build_order
from view import (
    format_board,
    format_error,
    format_ship_line,
    format_snapshot,
)


def pick_scenario(repo: Path, preferred: str | None) -> str:
    if preferred:
        # Accept bare name or path relative to repo.
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
    print("scenarios:")
    for i, s in enumerate(scenarios):
        print(f"  [{i}] {s}")
    # Prefer ai.toml for player-vs-AI playtesting when present.
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
            # treat as path fragment
            return pick_scenario(repo, raw)
    if idx < 0 or idx >= len(scenarios):
        raise SystemExit("bad scenario index")
    return scenarios[idx]


def print_state(snap: dict, *, full: bool = True) -> None:
    print()
    print(format_snapshot(snap, verbose=full))
    print()


def run_repl(session: ShipsimSession) -> int:
    assert session.snapshot is not None
    print(f"shipsim REPL  bin={session.bin}")
    print(f"scenario={session.scenario}  orders→{session.orders_log}")
    print("type help for commands; quit to exit")
    print_state(session.snapshot)

    while True:
        snap = session.snapshot
        if snap is None:
            print("no snapshot; exiting")
            return 1
        status = snap.get("status")
        if status in ("Won", "Lost"):
            print(f"*** scenario {status} ***")
            # still allow inspection / quit
        phase = snap.get("phase", "?")
        turn = snap.get("turn", "?")
        active = snap.get("active_ship")
        prompt = f"t{turn}/{phase}"
        if active is not None:
            prompt += f"#{active}"
        try:
            line = input(f"{prompt}> ")
        except (EOFError, KeyboardInterrupt):
            print()
            break

        order, side = build_order(line, snap)
        if side == "quit":
            break
        if side == "help":
            print(HELP)
            continue
        if side == "status":
            print_state(snap)
            continue
        if side == "board":
            print(format_board(snap))
            continue
        if side == "ships":
            active_id = snap.get("active_ship")
            for ship in snap.get("ships") or []:
                print(format_ship_line(ship, active=ship.get("id") == active_id))
            continue
        if side == "raw":
            print(json.dumps({k: snap.get(k) for k in (
                "protocol_version", "turn", "phase", "status", "active_ship",
                "move_order", "ships_moved_this_phase", "ships_ready_fire",
                "end_turn_warning", "prng_state",
            )}, indent=2))
            continue
        if side == "empty":
            continue
        if side == "unknown":
            print("  unknown command; try help")
            continue
        if order is None:
            continue

        msg = session.send_order(order)
        if msg.get("type") == "error":
            print(format_error(msg))
            continue
        print_state(msg)

    print(f"session orders: {session.orders_log}")
    if session.stderr_log.exists() and session.stderr_log.stat().st_size:
        print(f"harness stderr: {session.stderr_log}")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="shipsim interactive REPL (frontend/repl)")
    parser.add_argument(
        "scenario",
        nargs="?",
        help="scenario path relative to repo (default: interactive picker)",
    )
    parser.add_argument(
        "--bin",
        dest="bin_path",
        help="path to shipsim binary (default: target/debug/shipsim)",
    )
    parser.add_argument(
        "--save",
        dest="save_path",
        help="optional --save path passed to shipsim (written under frontend/repl/local if relative)",
    )
    args = parser.parse_args(argv)

    repo = find_repo_root()
    try:
        bin_path = Path(args.bin_path) if args.bin_path else find_shipsim_bin(repo)
    except FileNotFoundError as exc:
        print(exc, file=sys.stderr)
        return 1

    scenario = pick_scenario(repo, args.scenario)
    save_path = None
    if args.save_path:
        save_path = Path(args.save_path)
        if not save_path.is_absolute():
            from client import ensure_local

            save_path = ensure_local() / save_path.name

    session = ShipsimSession(
        scenario,
        repo=repo,
        bin_path=bin_path,
        save_path=save_path,
    )
    try:
        session.start()
        return run_repl(session)
    finally:
        session.close()


if __name__ == "__main__":
    raise SystemExit(main())
