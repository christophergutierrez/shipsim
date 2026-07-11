"""Play-mode fixed frame + always-on session transcript.

Play mode: clear and redraw ships/map/status each step so the board updates
in place (shields/weapons/hull stay visible without scrolling the world away).

Session log (default on): tee history + frames to
`frontend/repl/local/session-*.log` (gitignored). Override with --log-file;
disable with --no-session-log.

--debug: verbose transcript (timestamps, full order JSON, every frame marked).

History: also kept in memory; `log` toggles a scrollback panel in the frame.
"""

from __future__ import annotations

import sys
import time
from collections import deque
from contextlib import contextmanager
from datetime import datetime
from pathlib import Path
from typing import Any, Callable, Optional, TextIO

from style import muted, panel
from view import format_snapshot

# frontend/repl/local/ — same tree as orders logs
_LOCAL = Path(__file__).resolve().parent / "local"


class TerminalUI:
    def __init__(
        self,
        *,
        session_path: Optional[Path] = None,
        verbose: bool = False,
        scroll: bool = False,
        recent: int = 10,
        history_cap: int = 500,
    ) -> None:
        self.scroll = scroll  # old long-log behavior if True
        self.verbose = verbose  # --debug: richer file transcript
        self.recent = max(1, recent)
        self.history: deque[str] = deque(maxlen=history_cap)
        self.show_history = False  # toggled by `log`
        self._dialog = False  # interactive sub-prompts print live
        self._file: Optional[TextIO] = None
        self.session_path = session_path
        self._real_print = print
        if session_path is not None:
            session_path.parent.mkdir(parents=True, exist_ok=True)
            self._file = open(session_path, "a", encoding="utf-8")
            self._file.write(
                f"\n===== shipsim REPL session {datetime.now().isoformat()} =====\n"
            )
            if verbose:
                self._file.write("mode=verbose (--debug)\n")
            self._file.flush()

    # Back-compat alias used by older call sites
    @property
    def debug_path(self) -> Optional[Path]:
        return self.session_path

    def close(self) -> None:
        if self._file is not None:
            self._file.write(f"===== end {datetime.now().isoformat()} =====\n")
            self._file.close()
            self._file = None

    def _write_file(self, text: str) -> None:
        if self._file is not None:
            if self.verbose:
                ts = datetime.now().strftime("%H:%M:%S.") + f"{datetime.now().microsecond // 1000:03d}"
                for line in (text.splitlines() or [text]):
                    self._file.write(f"{ts} | {line}\n")
            else:
                self._file.write(text)
                if not text.endswith("\n"):
                    self._file.write("\n")
            self._file.flush()

    def log(self, text: str = "", *, important: bool = False) -> None:
        """Record a message; in play mode it appears in the recent strip after redraw."""
        if text is None:
            text = ""
        for line in str(text).splitlines() or [""]:
            self.history.append(line)
            self._write_file(line)
            if self.scroll or self._dialog:
                self._real_print(line)

    def log_order(self, order: dict[str, Any]) -> None:
        """Verbose: record outbound order JSON."""
        if not self.verbose:
            return
        import json

        self._write_file("ORDER " + json.dumps(order, separators=(",", ":")))

    def log_block(self, text: str) -> None:
        self.log(text)

    @contextmanager
    def dialog(self):
        """Interactive multi-step prompts (fire picker, empty-commit confirm)."""
        prev = self._dialog
        self._dialog = True
        try:
            yield
        finally:
            self._dialog = prev

    def clear(self) -> None:
        if self.scroll or not sys.stdout.isatty():
            return
        self._real_print("\033[2J\033[H", end="")

    def redraw(
        self,
        snap: dict[str, Any],
        *,
        selected: Optional[int],
        hull_max: dict[int, int],
        draft_text: Optional[str] = None,
        hint: str = "",
        banner: str = "",
        footer: str = "",
    ) -> None:
        """Full play frame from the *current* snapshot (shields/weapons live)."""
        if self.scroll:
            return

        self.clear()
        lines: list[str] = []
        if banner:
            lines.append(banner)
        lines.append(
            format_snapshot(
                snap, selected=selected, hull_max=hull_max, verbose=True
            )
        )
        recent = list(self.history)[-self.recent :]
        if recent:
            lines.append(panel("RECENT", "\n".join(recent), width=56))
        if self.show_history:
            hist = list(self.history)[-40:]
            lines.append(
                panel(
                    f"LOG (last {len(hist)}; type log to hide)",
                    "\n".join(hist) if hist else "(empty)",
                    width=56,
                )
            )
        if draft_text:
            lines.append(panel("ALLOCATE DRAFT (local until commit)", draft_text, width=56))
        if hint:
            lines.append(muted(hint))
        if footer:
            lines.append(muted(footer))
        else:
            lines.append(
                muted(
                    "play frame · log=history · cls=redraw · "
                    "session log under frontend/repl/local/"
                )
            )
        frame = "\n".join(lines)
        self._real_print(frame)
        # Always persist frames to the session file so post-mortems match the screen.
        self._write_file("--- frame ---\n" + frame)

    def install_print_hook(self) -> Callable[[], None]:
        """Route builtin print into the UI log (for commands.py)."""
        ui = self
        real = self._real_print

        def hooked_print(*args: Any, **kwargs: Any) -> None:
            sep = kwargs.get("sep", " ")
            end = kwargs.get("end", "\n")
            file = kwargs.get("file", sys.stdout)
            if file is not sys.stdout and file is not None:
                real(*args, **kwargs)
                return
            text = sep.join(str(a) for a in args) + (end if end else "")
            body = text[:-1] if text.endswith("\n") else text
            if body == "" and end == "\n":
                ui.history.append("")
                ui._write_file("")
                if ui.scroll or ui._dialog:
                    real()
                return
            for line in body.splitlines() or [body]:
                ui.history.append(line)
                ui._write_file(line)
            if ui.scroll or ui._dialog:
                real(*args, **kwargs)
            elif end != "\n":
                real(*args, **kwargs)

        import builtins

        builtins.print = hooked_print  # type: ignore[assignment]

        def restore() -> None:
            builtins.print = real  # type: ignore[assignment]

        return restore


def default_session_path() -> Path:
    """Gitignored session transcript under this client tree."""
    stamp = time.strftime("%Y%m%d-%H%M%S")
    return _LOCAL / f"session-{stamp}.log"


def default_debug_path() -> Path:
    """Deprecated alias — same as default_session_path()."""
    return default_session_path()
