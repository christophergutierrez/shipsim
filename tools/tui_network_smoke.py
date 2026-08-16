#!/usr/bin/env python3
"""Human-like PTY smoke for a TUI-hosted Human-vs-Greedy lobby."""

from __future__ import annotations

import argparse
import fcntl
import os
import pty
import re
import select
import struct
import subprocess
import sys
import termios
import threading
import time
from pathlib import Path

ANSI = re.compile(rb"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\)|[@-_])")


def wait_text(fd: int, transcript: bytearray, needle: str, timeout: float = 12.0) -> str:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        ready, _, _ = select.select([fd], [], [], 0.1)
        if ready:
            try:
                transcript.extend(os.read(fd, 65536))
            except OSError:
                pass
        visible = ANSI.sub(b"", bytes(transcript)).decode("utf-8", "replace")
        if needle in visible:
            return visible
    raise RuntimeError(f"TUI did not show {needle!r}; transcript tail:\n{visible[-3000:]}")


def drain_stderr(stream: object) -> None:
    for _ in stream:  # type: ignore[union-attr]
        pass


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--no-build", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    if not args.no_build:
        subprocess.run(["cargo", "build", "--bin", "shipsim-session"], cwd=root, check=True)
        subprocess.run(
            ["cargo", "build", "--manifest-path", "frontend/tui/Cargo.toml"],
            cwd=root,
            check=True,
        )

    server = subprocess.Popen(
        [str(root / "target/debug/shipsim-session"), "--listen", "127.0.0.1:0"],
        cwd=root,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
    )
    tui: subprocess.Popen[bytes] | None = None
    master = -1
    transcript = bytearray()
    try:
        assert server.stderr is not None
        line = server.stderr.readline()
        match = re.search(r"127\.0\.0\.1:\d+", line)
        if not match:
            raise RuntimeError(f"session server did not report an address: {line!r}")
        address = match.group(0)
        threading.Thread(target=drain_stderr, args=(server.stderr,), daemon=True).start()

        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
        tui = subprocess.Popen(
            [
                str(root / "frontend/tui/target/debug/shipsim-tui"),
                "--connect",
                address,
            ],
            cwd=root,
            stdin=slave,
            stdout=slave,
            stderr=slave,
            close_fds=True,
        )
        os.close(slave)
        wait_text(master, transcript, "Create match")
        os.write(master, b"jjj")  # Shipyard Assault in the server catalog
        os.write(master, b"2")  # Human vs Bot
        os.write(master, b"\r")
        visible = wait_text(master, transcript, "turn 1")
        if "Greedy" not in visible and "greedy" not in visible:
            raise RuntimeError("selected Greedy policy was never visible")
        os.write(master, b"q")
        time.sleep(0.1)
        os.write(master, b"y")
        tui.wait(timeout=5)
        if tui.returncode != 0:
            raise RuntimeError(f"TUI exited with {tui.returncode}")
        server.wait(timeout=5)
        if server.returncode != 0:
            raise RuntimeError(f"session server exited with {server.returncode}")
        print("PASS: visible lobby -> Human vs Greedy -> first battle screen -> clean quit")
        return 0
    finally:
        if master >= 0:
            os.close(master)
        for process in (tui, server):
            if process is not None and process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()


if __name__ == "__main__":
    sys.exit(main())
