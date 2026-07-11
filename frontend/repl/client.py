"""Spawn and talk to the shipsim NDJSON harness (protocol v1).

All paths for logs and session orders resolve under frontend/repl/local/.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Any, Optional

PROTOCOL_VERSION = 1

HERE = Path(__file__).resolve().parent
LOCAL = HERE / "local"


def find_repo_root(start: Optional[Path] = None) -> Path:
    cur = (start or HERE).resolve()
    for candidate in [cur, *cur.parents]:
        if (candidate / "Cargo.toml").is_file() and (candidate / "src").is_dir():
            return candidate
    raise FileNotFoundError("could not find shipsim repo root (Cargo.toml + src/)")


def find_shipsim_bin(repo: Path) -> Path:
    env = os.environ.get("SHIPSIM_BIN")
    if env:
        p = Path(env)
        if p.is_file():
            return p
    debug = repo / "target" / "debug" / "shipsim"
    release = repo / "target" / "release" / "shipsim"
    if debug.is_file():
        return debug
    if release.is_file():
        return release
    raise FileNotFoundError(
        f"shipsim binary not found under {repo}/target/{{debug,release}}/. "
        "Run `cargo build` or set SHIPSIM_BIN."
    )


def ensure_local() -> Path:
    LOCAL.mkdir(parents=True, exist_ok=True)
    return LOCAL


class ShipsimSession:
    """Long-lived `shipsim --scenario … --stdin` process."""

    def __init__(
        self,
        scenario: str,
        *,
        repo: Optional[Path] = None,
        bin_path: Optional[Path] = None,
        save_path: Optional[Path] = None,
    ) -> None:
        self.repo = (repo or find_repo_root()).resolve()
        self.bin = (bin_path or find_shipsim_bin(self.repo)).resolve()
        self.scenario = scenario  # relative to repo, e.g. scenarios/ai.toml
        self.snapshot: Optional[dict[str, Any]] = None
        self.last_error: Optional[dict[str, Any]] = None
        self.orders: list[dict[str, Any]] = []
        self._proc: Optional[subprocess.Popen[str]] = None
        ensure_local()
        stamp = time.strftime("%Y%m%d-%H%M%S")
        self.orders_log = LOCAL / f"orders-{stamp}.jsonl"
        self.stderr_log = LOCAL / f"stderr-{stamp}.log"
        self.save_path = save_path

    def start(self) -> dict[str, Any]:
        if self._proc is not None:
            raise RuntimeError("session already started")
        cmd = [
            str(self.bin),
            "--scenario",
            self.scenario,
            "--stdin",
        ]
        if self.save_path is not None:
            cmd.extend(["--save", str(self.save_path)])
        stderr_f = open(self.stderr_log, "w", encoding="utf-8")
        self._proc = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=stderr_f,
            text=True,
            bufsize=1,  # line-buffered text
            cwd=str(self.repo),
        )
        # Harness emits a post-load snapshot before reading any orders.
        msg = self._read_message()
        if msg is None:
            self.close()
            raise RuntimeError(
                f"shipsim produced no post-load snapshot; see {self.stderr_log}"
            )
        if msg.get("type") == "error":
            self.last_error = msg
            raise RuntimeError(f"post-load error: {msg}")
        self.snapshot = msg
        return msg

    def send_order(self, order: dict[str, Any]) -> dict[str, Any]:
        if self._proc is None or self._proc.stdin is None:
            raise RuntimeError("session not started")
        if "protocol_version" not in order:
            order = {**order, "protocol_version": PROTOCOL_VERSION}
        line = json.dumps(order, separators=(",", ":"))
        with open(self.orders_log, "a", encoding="utf-8") as log:
            log.write(line + "\n")
        self._proc.stdin.write(line + "\n")
        self._proc.stdin.flush()
        msg = self._read_message()
        if msg is None:
            code = self._proc.poll()
            raise RuntimeError(
                f"shipsim closed after order (exit={code}); see {self.stderr_log}"
            )
        if msg.get("type") == "error":
            self.last_error = msg
            return msg
        self.last_error = None
        self.orders.append(order)
        self.snapshot = msg
        return msg

    def _read_message(self) -> Optional[dict[str, Any]]:
        assert self._proc is not None and self._proc.stdout is not None
        while True:
            line = self._proc.stdout.readline()
            if line == "":
                return None
            line = line.strip()
            if not line:
                continue
            try:
                return json.loads(line)
            except json.JSONDecodeError as exc:
                raise RuntimeError(f"non-JSON from shipsim: {line!r}") from exc

    def close(self) -> None:
        if self._proc is None:
            return
        try:
            if self._proc.stdin and not self._proc.stdin.closed:
                self._proc.stdin.close()
        except OSError:
            pass
        try:
            self._proc.wait(timeout=2)
        except subprocess.TimeoutExpired:
            self._proc.kill()
            self._proc.wait(timeout=2)
        self._proc = None

    def __enter__(self) -> "ShipsimSession":
        self.start()
        return self

    def __exit__(self, *args: object) -> None:
        self.close()


def list_scenarios(repo: Optional[Path] = None) -> list[str]:
    root = (repo or find_repo_root()) / "scenarios"
    if not root.is_dir():
        return []
    return sorted(
        p.relative_to(root.parent).as_posix()
        for p in root.glob("*.toml")
    )


def main_smoke() -> int:
    """Non-interactive sanity check used by README / CI-adjacent checks."""
    repo = find_repo_root()
    scenario = "scenarios/combat.toml"
    with ShipsimSession(scenario, repo=repo) as session:
        snap = session.snapshot
        assert snap is not None
        assert snap.get("protocol_version") == PROTOCOL_VERSION
        assert snap.get("phase") == "allocate"
        err = session.send_order(
            {
                "type": "allocate",
                "ship": 1,
                "movement": 4,
                "weapons": {"beam_1": 1},
                "shields": [0, 0, 0, 0, 0, 0],
            }
        )
        if err.get("type") == "error":
            print("allocate soft-error:", err, file=sys.stderr)
            return 1
        print(f"ok phase={session.snapshot.get('phase')} turn={session.snapshot.get('turn')}")
        print(f"orders log: {session.orders_log}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main_smoke())
