#!/usr/bin/env python3
"""Screen-level UI audit: run the REPL under a real PTY, emulate the terminal
with pyte, and assert invariants on WHAT IS VISIBLE — not on the byte stream.

This is the layer self-play never tests. Self-play reads the transcript;
users read a character grid. These checks live on the grid.

Requires: pexpect, pyte for the PTY matrix. The pure `audit()` checks remain
dependency-free and are covered by unit tests.

Invariants (add more; each must be falsifiable against a rendered screen):
  I1  bar-label integrity: every "[##..] N" bar visibly agrees with its number.
      Unscaled bars: hashes == N. Scaled bars MUST carry a "/M" denominator.
  I2  no duplicate panels: each panel title appears at most once on screen.
  I3  frame fits: if any frame content is visible, the shipsim banner is visible
      in the top row of the terminal.
  I4  allocate safety: an allocate prompt keeps a player representation and
      draft visible, with the prompt on the final terminal row.

Run from repo root:
  python3 frontend/repl/screen_audit.py
"""
from __future__ import annotations

import re
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]


def capture(
    cmd: str,
    keys: list[str],
    *,
    rows: int,
    cols: int,
    settle: float = 2.5,
) -> list[list[str]]:
    import pexpect
    import pyte

    screen = pyte.Screen(cols, rows)
    stream = pyte.Stream(screen)
    child = pexpect.spawn(cmd, cwd=str(REPO), dimensions=(rows, cols), timeout=20)

    def drain(t: float) -> None:
        end = time.time() + t
        while time.time() < end:
            try:
                stream.feed(
                    child.read_nonblocking(4096, timeout=0.3).decode(
                        "utf-8", "replace"
                    )
                )
            except pexpect.TIMEOUT:
                pass
            except pexpect.EOF:
                return

    drain(settle)
    frames = [screen.display[:]]  # visible grid after launch
    for k in keys:
        child.sendline(k.encode() if isinstance(k, str) else k)
        drain(1.5)
        frames.append(screen.display[:])
    child.sendline(b"quit")
    drain(0.8)
    try:
        child.sendline(b"y")
    except Exception:
        pass
    child.close(force=True)
    return frames


BAR = re.compile(r"\[([#.]+)\]\s*(\d+)(?:\s*/\s*(\d+))?")


def audit(frame: list[str], label: str, *, rows: int | None = None) -> list[str]:
    violations: list[str] = []
    text = "\n".join(frame)
    rows = rows or len(frame)
    # I1 bar-label integrity
    for line in frame:
        for m in BAR.finditer(line):
            fill, n, denom = m.group(1), int(m.group(2)), m.group(3)
            hashes, width = fill.count("#"), len(fill)
            if denom is None:
                if hashes != n:
                    violations.append(
                        f"I1 {label}: bar shows {hashes} but label says {n} "
                        f"(no denominator, so the user reads hashes==number): "
                        f"{line.strip()!r}"
                    )
            else:
                d = int(denom)
                if width == d and hashes != n:
                    violations.append(
                        f"I1 {label}: unscaled bar {hashes}#/{width} vs {n}/{d}: "
                        f"{line.strip()!r}"
                    )
                if width != d and hashes != round(n * width / d) if d else hashes:
                    expected = round(n * width / d) if d else 0
                    if hashes != expected:
                        violations.append(
                            f"I1 {label}: scaled bar wrong "
                            f"(got {hashes}# expected ~{expected}): {line.strip()!r}"
                        )
    # I2 duplicate panels
    for title in ("YOUR SHIP", "CONTACTS", "MAP", "ALLOCATE DRAFT"):
        c = text.count(f"─ {title} ")
        if c > 1:
            violations.append(
                f"I2 {label}: panel {title!r} visible {c}× on one screen"
            )
    # I3 frame fits the terminal. Any visible panel means the banner must be
    # at the top of the viewport; do not make this conditional on YOUR SHIP.
    has_frame_content = any("│" in row or "┌" in row or "└" in row for row in frame)
    if has_frame_content and (not frame or "shipsim" not in frame[0].lower()):
        violations.append(
            f"I3 {label}: visible frame content has no shipsim banner in row 0; "
            f"top row: {frame[0].strip() if frame else '<empty>'!r}"
        )

    # I4 phase-critical allocate content. The prompt is deliberately detected
    # by its command marker rather than the phase header, which also contains
    # the word allocate.
    prompt_rows = [
        i
        for i, row in enumerate(frame)
        if "allocate" in row.lower() and ">" in row
    ]
    if prompt_rows:
        if not any("YOUR SHIP" in row or "hull=" in row for row in frame):
            violations.append(f"I4 {label}: allocate prompt has no player representation")
        if not any("ALLOCATE DRAFT" in row or "draft " in row.lower() for row in frame):
            violations.append(f"I4 {label}: allocate prompt has no draft representation")
        nonempty = [i for i, row in enumerate(frame) if row.strip()]
        if not nonempty or nonempty[-1] != rows - 1:
            violations.append(f"I4 {label}: allocate prompt is not on final terminal row")
    return violations


def main() -> int:
    try:
        import pexpect  # noqa: F401
        import pyte  # noqa: F401
    except ImportError as exc:
        print(f"FAIL screen_audit: need pexpect+pyte ({exc})", file=sys.stderr)
        return 2

    all_v: list[str] = []
    total_frames = 0
    for rows, cols in ((24, 80), (40, 100), (50, 120), (60, 100)):
        frames = capture(
            "python3 frontend/repl/repl.py scenarios/ai.toml --no-session-log",
            keys=["a", "engine 4"],
            rows=rows,
            cols=cols,
        )
        total_frames += len(frames)
        for i, f in enumerate(frames):
            all_v += audit(f, f"{rows}x{cols}/frame{i}", rows=rows)
    for v in all_v:
        print("VIOLATION", v)
    print(f"\n{len(all_v)} violation(s) across {total_frames} rendered screens")
    return 1 if all_v else 0


if __name__ == "__main__":
    raise SystemExit(main())
